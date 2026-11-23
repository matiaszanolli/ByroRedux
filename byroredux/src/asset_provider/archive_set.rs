//! An ordered pool of archives with one path index over it (PRD legacy-mod-compatibility LMC-PERF-003).
//!
//! A provider's pool is the archives it reads in last-listed-wins order (#3637). Looking a path up used to probe each
//! archive in turn from the last: one lowercase, one separator rewrite and one hash probe per archive, and a formatted
//! `io::Error` for every miss, so a vanilla asset behind 400 archives cost 64 µs (1.6 µs behind 10). Opening a pool cost
//! more: every archive's entries were probed against every archive before it for the "overrides N entries" log line,
//! quadratic in the archive count (21 s for 400 archives of 5,000 entries).
//!
//! [`ArchiveSet`] keeps the archives and, beside them, a [`PathIndex`]: the 64-bit hash of each normalized path to the
//! position of the last-listed archive that holds it. A lookup is one hash and one probe whatever the archive count.
//!
//! **The index is a hint, never the answer.** A hit is checked against its archive ([`Archive::contains`]) and a failed
//! check, which only a hash collision or a leading separator can cause, falls back to the linear scan the pool always
//! did, so a wrong archive is never served. An absent hash is trusted: every key an archive holds is in the index, and a
//! collision can only overwrite an entry, never remove one, so a path some archive holds always has an entry.
//!
//! Building: `push` marks the index stale and the next lookup builds it, from the running index when the pool's
//! shadow-count walk already covered every archive, from the archives' file tables otherwise, or from the memo of indexes
//! sealed for the same ordered pool (provider rebuilds repeat their pools, #5288, and must not pay for an index again).

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use super::archive::Archive;

/// Passes a key that is already a 64-bit hash straight through to the table.
#[derive(Default)]
struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 << 8) | u64::from(byte);
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = value;
    }
}

/// Hash of a path key, to the position of the last-listed archive that holds it.
type Winners = HashMap<u64, u32, BuildHasherDefault<KeyHasher>>;

/// The hash of an archive's own stored key. Every backend stores keys already normalized (lowercase, backslash
/// separators: `BsaArchive`, `Ba2Archive` and the loose index all run `normalize_path`), so none is normalized again.
fn stored_key_hash(key: &str) -> u64 {
    twox_hash::XxHash64::oneshot(0, key.as_bytes())
}

/// The hash of a path as a caller wrote it: the archive key it would match, which the loose index also reaches with a
/// leading separator trimmed.
fn lookup_hash(path: &str) -> u64 {
    stored_key_hash(&byroredux_bsa::normalize_path(path.trim_start_matches(['\\', '/'])))
}

/// The winning archive position of every path of `archives`, the last-listed archive holding a path winning it.
fn winners_of(archives: &[Archive]) -> Winners {
    let mut winners = Winners::default();
    insert_from(&mut winners, archives, 0);
    winners
}

/// Add the paths of `archives[from..]` to `winners`.
fn insert_from(winners: &mut Winners, archives: &[Archive], from: usize) {
    for (position, archive) in archives.iter().enumerate().skip(from) {
        for key in archive.all_paths() {
            winners.insert(stored_key_hash(key), position as u32);
        }
    }
}

/// An index of a pool, shared by every clone of the pool and memoised across provider rebuilds.
struct PathIndex {
    winners: Winners,
}

/// The paths of the archives pushed so far, kept while a pool is being opened so the shadow count of each new archive
/// costs its own entries, not every archive before it.
#[derive(Default)]
struct Running {
    winners: Winners,
    /// How many of the pool's archives `winners` covers.
    covered: usize,
}

/// A sealed index and the pool it was built for: the archives are held so a pointer identity cannot be reused by another
/// archive while the entry lives.
struct Sealed {
    identity: Vec<usize>,
    _archives: Vec<Archive>,
    index: Arc<PathIndex>,
}

/// Most pools whose index is memoised: a session reads a mesh, a texture, a sound, a material and a few loose pools.
const SEALED_MAX: usize = 16;

fn sealed_indexes() -> &'static Mutex<Vec<Sealed>> {
    static SEALED: OnceLock<Mutex<Vec<Sealed>>> = OnceLock::new();
    SEALED.get_or_init(|| Mutex::new(Vec::new()))
}

/// The archives of one provider pool in precedence order (the last listed wins), with a path index over them.
/// Reads like a slice of archives (`iter`, `len`, `is_empty`, indexing); `push` is the only way to add one.
#[derive(Default)]
pub(crate) struct ArchiveSet {
    archives: Vec<Archive>,
    index: OnceLock<Arc<PathIndex>>,
    running: Running,
}

impl Clone for ArchiveSet {
    /// A copy shares the index when this one has built it; the running index is the opener's scratch and is not copied.
    fn clone(&self) -> Self {
        let index = OnceLock::new();
        if let Some(built) = self.index.get() {
            let _ = index.set(Arc::clone(built));
        }
        Self { archives: self.archives.clone(), index, running: Running::default() }
    }
}

impl std::ops::Deref for ArchiveSet {
    type Target = [Archive];

    fn deref(&self) -> &[Archive] {
        &self.archives
    }
}

impl From<Vec<Archive>> for ArchiveSet {
    fn from(archives: Vec<Archive>) -> Self {
        Self { archives, index: OnceLock::new(), running: Running::default() }
    }
}

impl ArchiveSet {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Add `archive` as the highest-precedence one. The index is built again on the next lookup.
    pub(crate) fn push(&mut self, archive: Archive) {
        self.archives.push(archive);
        self.index = OnceLock::new();
    }

    /// The pool's index, built on first use.
    fn index(&self) -> &PathIndex {
        self.index.get_or_init(|| {
            let identity: Vec<usize> = self.archives.iter().map(Archive::identity).collect();
            if let Some(hit) = sealed_indexes()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .find(|sealed| sealed.identity == identity)
            {
                return Arc::clone(&hit.index);
            }
            // The walk that counted the last archive's shadowed entries already holds the whole pool.
            let winners = if self.running.covered == self.archives.len() {
                self.running.winners.clone()
            } else {
                winners_of(&self.archives)
            };
            let index = Arc::new(PathIndex { winners });
            let mut sealed = sealed_indexes().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if sealed.len() >= SEALED_MAX {
                sealed.remove(0);
            }
            sealed.push(Sealed { identity, _archives: self.archives.clone(), index: Arc::clone(&index) });
            index
        })
    }

    /// The position of the last-listed archive that holds `path`, the one a lookup in precedence order would stop at.
    pub(crate) fn locate(&self, path: &str) -> Option<usize> {
        if self.archives.is_empty() {
            return None;
        }
        let &position = self.index().winners.get(&lookup_hash(path))?;
        let position = position as usize;
        if self.archives.get(position).is_some_and(|archive| archive.contains(path)) {
            return Some(position);
        }
        // A collision or a leading separator: the index is a hint, the archives are the answer.
        self.archives.iter().rposition(|archive| archive.contains(path))
    }

    /// Whether any archive holds `path`.
    pub(crate) fn contains(&self, path: &str) -> bool {
        self.locate(path).is_some()
    }

    /// The last-listed archive that holds `path`.
    pub(crate) fn winner(&self, path: &str) -> Option<&Archive> {
        self.locate(path).map(|position| &self.archives[position])
    }

    /// The bytes of `path` from the last-listed archive that holds it and extracts it: a present entry that fails to
    /// read is named once and passed over for the next archive down (#4658), as [`super::extract_first`] does. Archives
    /// above the one [`Self::locate`] names do not hold the path, so none is probed.
    pub(crate) fn extract_first(&self, path: &str) -> Option<Vec<u8>> {
        let top = self.locate(path)?;
        self.archives[..=top].iter().rev().find_map(|archive| archive.extract_or_warn(path))
    }

    /// The decoded size of `path` in the last-listed archive that reports one.
    pub(crate) fn declared_size(&self, path: &str) -> Option<usize> {
        let top = self.locate(path)?;
        self.archives[..=top].iter().rev().find_map(|archive| archive.declared_size(path).ok())
    }

    /// How many of `new`'s entries an archive of this pool also holds, which `new` is about to outrank. The pool's own
    /// paths are kept in the running index, so the cost is `new`'s entries, and an archive pushed after is counted
    /// against everything before it (it was quadratic in the archive count when each probed every earlier archive).
    pub(crate) fn count_shadowed(&mut self, new: &Archive) -> usize {
        if self.running.covered < self.archives.len() {
            let from = self.running.covered;
            insert_from(&mut self.running.winners, &self.archives, from);
            self.running.covered = self.archives.len();
        }
        new.all_paths().into_iter().filter(|key| self.running.winners.contains_key(&stored_key_hash(key))).count()
    }

    /// A copy of this pool whose index names `position` for `path`, whatever archive holds it.
    #[cfg(test)]
    pub(crate) fn with_poisoned_index(&self, path: &str, position: usize) -> ArchiveSet {
        let mut winners = self.index().winners.clone();
        winners.insert(lookup_hash(path), position as u32);
        let copy = ArchiveSet { archives: self.archives.clone(), index: OnceLock::new(), running: Running::default() };
        let _ = copy.index.set(Arc::new(PathIndex { winners }));
        copy
    }

    /// Whether both pools read one index: a rebuilt pool of the same ordered archives takes the memoised one.
    #[cfg(test)]
    pub(crate) fn shares_index_with(&self, other: &ArchiveSet) -> bool {
        std::ptr::eq(self.index(), other.index())
    }
}
