//! On-disk save slots — atomic writes and a slot ring.
//!
//! A save is written to `<dir>/save_<slot>.ess` via the standard
//! crash-safe dance: stage into a unique hidden
//! `.save_<slot>.ess.<pid>.<n>.tmp` sibling (#5247 — unique per attempt,
//! so two engine processes sharing a save directory never quicksave into
//! the same staging inode), `fsync`, re-read and verify the bytes match,
//! then atomically `rename` over the target. A power cut mid-write
//! leaves the old `save_<slot>.ess` intact; a failed attempt removes its
//! own staging file (#5163) — never a half-written live slot.
//!
//! [`SaveRing`] picks the next slot round-robin so a quicksave never
//! immediately clobbers the most recent good save (Bethesda's "F5 ate my
//! save" is a UX choice, not a constraint).

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::SaveError;

// Moved to `byroredux_core::atomic_file` so the settings registry and the
// launcher share one durability contract with the save ring. Re-exported here
// because `save::disk::atomic_write` is the established path.
pub use byroredux_core::atomic_file::atomic_write;

/// File extension for save slots (Elder-Scrolls-Save heritage).
const SAVE_EXT: &str = "ess";

/// Path of a numbered save slot under `dir`.
pub fn slot_path(dir: &Path, slot: u32) -> PathBuf {
    dir.join(format!("save_{slot}.{SAVE_EXT}"))
}

/// Write `bytes` to `slot` under `dir`, crash-safely.
///
/// Creates `dir` if absent. Stages into a unique hidden
/// `.save_<slot>.ess.<pid>.<n>.tmp` sibling, flushes + fsyncs it, re-reads
/// to confirm the bytes landed, then renames over the live slot. The
/// re-read catches a lying filesystem / short write before it can replace
/// a good save with a bad one.
pub fn write_slot(dir: &Path, slot: u32, bytes: &[u8]) -> Result<PathBuf, SaveError> {
    // Kept here, not in `atomic_write`: this function's contract promises it.
    fs::create_dir_all(dir)?;
    let final_path = slot_path(dir, slot);
    // #5247 — the fixed `.ess.tmp` sibling made two engine processes
    // sharing a save directory race on one staging inode: both ring
    // cursors resume from the same mtimes, so both can pick the same
    // slot, and one rename can then consume the inode the other is
    // still writing. The shared helper's pid + counter naming gives
    // every attempt its own.
    let temp_path = byroredux_core::atomic_file::atomic_temp_path(&final_path);
    byroredux_core::atomic_file::atomic_write(&final_path, &temp_path, bytes)
        .map_err(SaveError::Io)
        .map(|()| final_path)
}

/// How old a staging file must be before [`reclaim_stale_staging_files`] may delete it. A save takes milliseconds, so a
/// staging file this old belongs to a writer that died; a younger one may belong to a live process sharing the
/// directory (#5247) and is left alone.
pub const STALE_STAGING_AGE: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Whether `name` is the staging name [`write_slot`] creates: `.save_<slot>.ess.<pid>.<n>.tmp`.
fn is_slot_staging_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(".save_").and_then(|rest| rest.strip_suffix(".tmp")) else {
        return false;
    };
    let mut parts = rest.split('.');
    let numeric = |part: Option<&str>| part.is_some_and(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    numeric(parts.next())
        && parts.next() == Some(SAVE_EXT)
        && numeric(parts.next())
        && numeric(parts.next())
        && parts.next().is_none()
}

/// Delete the staging files a crash or power cut left in `dir` (#7354). `atomic_write` removes its temp on every
/// failure it can see, but a process that dies between creating the temp and renaming it leaves
/// `.save_<slot>.ess.<pid>.<n>.tmp` holding up to a full snapshot, and the unique name never repeats, so nothing
/// else ever reclaims it (`slots_by_recency` and [`SaveRing::resume`] correctly ignore it). Removes only regular
/// files with exactly that name and at least `older_than` old, best-effort (an entry that cannot be read or removed
/// is skipped), and never touches a slot file. Returns how many it removed.
pub fn reclaim_stale_staging_files(dir: &Path, older_than: std::time::Duration) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let now = std::time::SystemTime::now();
    let mut removed = 0;
    for entry in entries.filter_map(|entry| entry.ok()) {
        if !is_slot_staging_name(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else { continue };
        let old_enough = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= older_than);
        if metadata.is_file() && old_enough && fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Read the raw bytes of `slot` under `dir`.
pub fn read_slot(dir: &Path, slot: u32) -> Result<Vec<u8>, SaveError> {
    let path = slot_path(dir, slot);
    let mut bytes = Vec::new();
    fs::File::open(&path)?.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// List the slot numbers that currently have a `save_<n>.ess` file,
/// ascending.
pub fn list_slots(dir: &Path) -> Vec<u32> {
    let mut slots: Vec<u32> = match fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter_map(|e| parse_slot_filename(&e.file_name().to_string_lossy()))
            .collect(),
        Err(_) => Vec::new(),
    };
    slots.sort_unstable();
    slots
}

/// Valid slot filenames ordered newest-first, used by the player-facing
/// quickload action (its first entry is the newest slot). Modification-time
/// ties are broken by higher slot number so quickload behavior is
/// deterministic. Invalid names and unreadable metadata are ignored.
pub fn slots_by_recency(dir: &Path) -> Vec<u32> {
    let mut slots: Vec<(u32, std::time::SystemTime)> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let slot = parse_slot_filename(&entry.file_name().to_string_lossy())?;
                let modified = entry.metadata().ok()?.modified().ok()?;
                Some((slot, modified))
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    slots.sort_unstable_by(|(slot_a, time_a), (slot_b, time_b)| {
        time_b.cmp(time_a).then_with(|| slot_b.cmp(slot_a))
    });
    slots.into_iter().map(|(slot, _)| slot).collect()
}

/// Cursor for a resumed ring: one past the slot with the newest mtime, or
/// `0` when no slots exist. Pure so the resume policy is unit-testable
/// without touching the filesystem. SAVE-D3-02.
fn cursor_after_newest(slots: &[(u32, std::time::SystemTime)], size: u32) -> u32 {
    // Same order as `slots_by_recency`: newest mtime, ties broken by the higher slot number. A bare
    // `max_by_key` keeps the last equal maximum in `read_dir` order, which is arbitrary (SAVE-8).
    match slots.iter().max_by(|(slot_a, time_a), (slot_b, time_b)| time_a.cmp(time_b).then_with(|| slot_a.cmp(slot_b))) {
        Some((newest, _)) => (newest + 1) % size.max(1),
        None => 0,
    }
}

/// Extract `n` from `save_<n>.ess`, or `None` if the name doesn't match.
fn parse_slot_filename(name: &str) -> Option<u32> {
    let stem = name.strip_suffix(&format!(".{SAVE_EXT}"))?;
    let digits = stem.strip_prefix("save_")?;
    digits.parse().ok()
}

/// A fixed-size ring of save slots.
///
/// `next()` advances round-robin over `0..size`, so successive quicksaves
/// spread across the ring and the previous good save survives the next
/// write. Stateless on disk beyond the slot files themselves — the cursor
/// lives in memory for the session.
#[derive(Debug, Clone)]
pub struct SaveRing {
    size: u32,
    cursor: u32,
}

impl SaveRing {
    /// Create a ring of `size` slots (`size` is clamped to at least 1).
    pub fn new(size: u32) -> Self {
        Self {
            size: size.max(1),
            cursor: 0,
        }
    }

    /// Create a ring whose cursor resumes *past* the most-recently-written
    /// slot on disk (SAVE-D3-02).
    ///
    /// The cursor is in-memory, so a plain [`new`](Self::new) restarts it at
    /// 0 every launch — and if slot 0 held the newest save, the first
    /// quicksave after a restart clobbers it. Scanning the slot files' mtimes
    /// and starting one past the newest spreads the next write onto a fresh
    /// slot instead, preserving the latest save the same way mid-session
    /// round-robin already does.
    pub fn resume(size: u32, dir: &Path) -> Self {
        let size = size.max(1);
        let slots: Vec<(u32, std::time::SystemTime)> = match fs::read_dir(dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let slot = parse_slot_filename(&e.file_name().to_string_lossy())?;
                    if slot >= size {
                        return None; // a slot from a larger former ring
                    }
                    let mtime = e.metadata().ok()?.modified().ok()?;
                    Some((slot, mtime))
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        Self {
            size,
            cursor: cursor_after_newest(&slots, size),
        }
    }

    /// The slot the next [`advance`](Self::advance) will return.
    pub fn peek(&self) -> u32 {
        self.cursor
    }

    /// Return the current slot and advance the cursor round-robin.
    pub fn advance(&mut self) -> u32 {
        let slot = self.cursor;
        self.cursor = (self.cursor + 1) % self.size;
        slot
    }

    pub fn size(&self) -> u32 {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #3472 — the durable sequence, exercised through the shared helper both
    /// `write_slot` and `byroredux::settings_io` now use.
    ///
    /// Cannot assert the fsyncs happened (nothing in `std` observes that), so
    /// it pins the observable half: the temp file is consumed, the target
    /// holds exactly the bytes written, and an existing target is replaced
    /// rather than appended to or left half-written.
    #[test]
    fn atomic_write_replaces_the_target_and_consumes_the_temp() {
        let dir = std::env::temp_dir().join(format!(
            "byroredux-atomic-write-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create test dir");
        let target = dir.join("settings.toml");
        let tmp = dir.join("settings.toml.tmp");

        // First write onto a path that does not exist yet.
        atomic_write(&target, &tmp, b"version = 1\n").expect("first write");
        assert_eq!(fs::read(&target).unwrap(), b"version = 1\n");
        assert!(
            !tmp.exists(),
            "#3472: the temp must be renamed away, not left behind"
        );

        // Second write must REPLACE, not append — the failure mode a
        // non-atomic write-in-place produces when the new content is shorter.
        atomic_write(&target, &tmp, b"v = 2\n").expect("second write");
        assert_eq!(
            fs::read(&target).unwrap(),
            b"v = 2\n",
            "#3472: a shorter payload must fully replace the longer one"
        );
        assert!(!tmp.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    /// The read-back check must reject rather than rename a bad temp into
    /// place — that is the step which keeps a lying filesystem or a short
    /// write from replacing a good file with a corrupt one. Verified by
    /// pointing the helper at a target whose parent does not exist, the
    /// closest failure this API can be driven into deterministically.
    #[test]
    fn atomic_write_fails_without_renaming_when_the_temp_cannot_be_created() {
        let missing = std::env::temp_dir()
            .join("byroredux-atomic-write-absent-parent")
            .join("nested")
            .join("settings.toml");
        let tmp = missing.with_extension("toml.tmp");
        let err = atomic_write(&missing, &tmp, b"x").expect_err("must not succeed");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert!(
            !missing.exists(),
            "#3472: a failed write must leave no target behind"
        );
    }

    #[test]
    fn ring_wraps() {
        let mut ring = SaveRing::new(3);
        assert_eq!(ring.advance(), 0);
        assert_eq!(ring.advance(), 1);
        assert_eq!(ring.advance(), 2);
        assert_eq!(ring.advance(), 0);
    }

    #[test]
    fn ring_size_floored_to_one() {
        let mut ring = SaveRing::new(0);
        assert_eq!(ring.size(), 1);
        assert_eq!(ring.advance(), 0);
        assert_eq!(ring.advance(), 0);
    }

    #[test]
    fn cursor_after_newest_points_past_latest_mtime() {
        use std::time::{Duration, SystemTime};
        let t = |s: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        // Slot 1 is newest → resume one past it (slot 2): the next save lands
        // on a fresh slot, not the just-written newest. SAVE-D3-02.
        let slots = [(0u32, t(100)), (1, t(300)), (2, t(200))];
        assert_eq!(cursor_after_newest(&slots, 3), 2);
        // Newest is the last slot → wrap to 0 (the oldest), not clobber it.
        let slots = [(0u32, t(100)), (2, t(300))];
        assert_eq!(cursor_after_newest(&slots, 3), 0);
        // No slots → start at 0.
        assert_eq!(cursor_after_newest(&[], 3), 0);
        // SAVE-8: equal mtimes (FAT / exFAT, a mtime-normalizing copy) resume like `slots_by_recency`
        // (higher slot is newer), whatever order `read_dir` listed them in.
        let same = t(100);
        assert_eq!(cursor_after_newest(&[(0u32, same), (2, same), (1, same)], 5), 3);
        assert_eq!(cursor_after_newest(&[(2u32, same), (1, same), (0, same)], 5), 3);
    }

    #[test]
    fn resume_on_empty_dir_starts_at_zero() {
        let dir =
            std::env::temp_dir().join(format!("byro_save_resume_empty_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let ring = SaveRing::resume(3, &dir);
        assert_eq!(ring.peek(), 0, "no slots on disk → cursor starts at 0");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_slot_names() {
        assert_eq!(parse_slot_filename("save_0.ess"), Some(0));
        assert_eq!(parse_slot_filename("save_42.ess"), Some(42));
        assert_eq!(parse_slot_filename("save_42.ess.tmp"), None);
        assert_eq!(parse_slot_filename(".save_42.ess.7.0.tmp"), None);
        assert_eq!(parse_slot_filename("notes.txt"), None);
        assert_eq!(parse_slot_filename("save_x.ess"), None);
    }

    #[test]
    fn newest_slot_ignores_newer_tmp_and_empty_directory() {
        let dir =
            std::env::temp_dir().join(format!("byro_save_latest_filter_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(slots_by_recency(&dir).first(), None);
        // The staging shape `write_slot` actually creates (#5247).
        fs::write(
            dir.join(format!(".save_9.ess.{}.0.tmp", std::process::id())),
            b"newer temp",
        )
        .unwrap();
        assert_eq!(
            slots_by_recency(&dir).first(),
            None,
            "temp files are never live slots"
        );
        fs::write(slot_path(&dir, 2), b"valid-name slot").unwrap();
        assert_eq!(slots_by_recency(&dir).first(), Some(&2));
        let _ = fs::remove_dir_all(&dir);
    }

    /// #5247 — `write_slot` is the fourth durable writer moved onto the
    /// shared unique-temp staging helper, pinned like the settings,
    /// boot-request and game-detect writers (#5164): exactly one
    /// `atomic_write` call, the staging path bound and passed, no
    /// clobbering fallback.
    #[test]
    fn write_slot_has_no_clobber_fallback() {
        byroredux_core::atomic_file::assert_no_clobber_fallback(include_str!("disk.rs"));
    }

    /// #7354 — a crash between creating a staging file and renaming it leaves a `.save_<slot>.ess.<pid>.<n>.tmp`
    /// nothing ever deleted. The reclaim removes exactly those, once they are old enough to belong to a dead writer,
    /// and nothing else in the directory.
    #[test]
    fn stale_staging_files_are_reclaimed_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("byro_save_reclaim_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        let aged = |name: &str| {
            let path = dir.join(name);
            fs::write(&path, b"x").unwrap();
            fs::OpenOptions::new().write(true).open(&path).unwrap().set_modified(hour_ago).unwrap();
            path
        };
        // A dead writer's temps: removed.
        let stale = [aged(".save_3.ess.4242.0.tmp"), aged(".save_0.ess.1.17.tmp")];
        // Not the staging shape, however old: kept.
        let kept = [
            aged("save_1.ess"),
            aged(".save_9.ess.tmp"),
            aged(".save_x.ess.1.2.tmp"),
            aged(".save_1.ess.1.2.3.tmp"),
            aged(".save_1.dat.1.2.tmp"),
            aged(".settings.toml.1.2.tmp"),
            aged("save_1.ess.1.2.tmp"),
        ];
        // The right shape but young, as a live writer's would be: kept.
        let young = dir.join(".save_2.ess.77.5.tmp");
        fs::write(&young, b"in flight").unwrap();

        assert_eq!(reclaim_stale_staging_files(&dir, STALE_STAGING_AGE), 2);
        assert!(stale.iter().all(|path| !path.exists()), "a dead writer's staging files are gone");
        assert!(kept.iter().all(|path| path.exists()), "nothing else is touched");
        assert!(young.exists(), "a young staging file may belong to a live process");
        assert_eq!(slots_by_recency(&dir).first(), Some(&1), "the live slot still reads");
        assert_eq!(reclaim_stale_staging_files(&dir, STALE_STAGING_AGE), 0, "nothing left to reclaim");
        assert_eq!(reclaim_stale_staging_files(&dir.join("absent"), STALE_STAGING_AGE), 0, "a missing directory is fine");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recency_tie_breaks_by_slot_number() {
        use std::time::{Duration, SystemTime};
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let mut slots = vec![(1u32, t), (3, t), (2, t)];
        slots.sort_unstable_by(|(slot_a, time_a), (slot_b, time_b)| {
            time_b.cmp(time_a).then_with(|| slot_b.cmp(slot_a))
        });
        assert_eq!(
            slots.into_iter().map(|(slot, _)| slot).collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
    }

    #[test]
    fn write_read_round_trip_and_atomic_rename() {
        let dir = std::env::temp_dir().join(format!("byro_save_disk_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let payload = b"BYRSAVE\0 some bytes here";
        let path = write_slot(&dir, 2, payload).unwrap();
        assert!(path.exists());
        // No leftover staging files after a clean write: the unique
        // staging names are hidden dotfiles, so the honest check is that
        // every directory entry parses as a live slot (#5247).
        let names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.iter().all(|name| parse_slot_filename(name).is_some()),
            "stranded staging temps: {names:?}"
        );

        assert_eq!(read_slot(&dir, 2).unwrap(), payload);
        assert_eq!(list_slots(&dir), vec![2]);

        write_slot(&dir, 0, payload).unwrap();
        assert_eq!(list_slots(&dir), vec![0, 2]);
        assert_eq!(slots_by_recency(&dir).first(), Some(&0));

        let _ = fs::remove_dir_all(&dir);
    }
}
