//! On-disk save slots — atomic writes and a slot ring.
//!
//! A save is written to `<dir>/save_<slot>.ess` via the standard
//! crash-safe dance: write to a `.tmp` sibling, `fsync`, re-read and
//! verify the bytes match, then atomically `rename` over the target.
//! A power cut mid-write leaves the old `save_<slot>.ess` intact and a
//! stray `.tmp` that the next save overwrites — never a half-written
//! live slot.
//!
//! [`SaveRing`] picks the next slot round-robin so a quicksave never
//! immediately clobbers the most recent good save (Bethesda's "F5 ate my
//! save" is a UX choice, not a constraint).

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::SaveError;

/// File extension for save slots (Elder-Scrolls-Save heritage).
const SAVE_EXT: &str = "ess";

/// Path of a numbered save slot under `dir`.
pub fn slot_path(dir: &Path, slot: u32) -> PathBuf {
    dir.join(format!("save_{slot}.{SAVE_EXT}"))
}

/// Write `bytes` to `slot` under `dir`, crash-safely.
///
/// Creates `dir` if absent. Writes `save_<slot>.ess.tmp`, flushes +
/// fsyncs it, re-reads to confirm the bytes landed, then renames over
/// the live slot. The re-read catches a lying filesystem / short write
/// before it can replace a good save with a bad one.
pub fn write_slot(dir: &Path, slot: u32, bytes: &[u8]) -> Result<PathBuf, SaveError> {
    // Kept here, not in `atomic_write`: this function's contract promises it.
    fs::create_dir_all(dir)?;
    let final_path = slot_path(dir, slot);
    let tmp_path = final_path.with_extension(format!("{SAVE_EXT}.tmp"));
    atomic_write(&final_path, &tmp_path, bytes)
        .map_err(SaveError::Io)
        .map(|()| final_path)
}

/// Write `bytes` to `final_path` crash-safely, staging through `tmp_path`.
///
/// The full durable sequence, in the order that makes each step meaningful:
/// create the temp → `write_all` → `flush` → `sync_all` (the data is now on
/// the platter, not just in the page cache) → read back and compare (catches a
/// lying filesystem or a short write *before* it can replace a good file with
/// a bad one) → `rename` (atomic on POSIX) → fsync the **parent directory**.
///
/// That last step is the one most often missed and is why this is a shared
/// helper rather than a comment: a successful `rename` is not durable until
/// the directory's own metadata is fsynced. A crash immediately after can
/// otherwise lose the new directory entry — the path points at the old inode
/// or none — even though the call returned `Ok`. Opening a directory as a
/// `File` is a Unix capability; platforms that cannot (Windows) journal the
/// rename, so the step is skipped there rather than failing.
///
/// #3472 — extracted from [`write_slot`] so `byroredux::settings_io` can share
/// it. That writer had `fs::write` + `fs::rename` with none of the three
/// durability steps, so a crash in the window between the rename hitting the
/// directory journal and the data reaching the platter left a zero-length or
/// truncated `settings.toml`. Two writers in one binary with two different
/// durability contracts, only one of them documented.
///
/// Does NOT create the parent directory — callers own that, since they know
/// whether an absent parent is an error or a first-run condition.
pub fn atomic_write(
    final_path: &Path,
    tmp_path: &Path,
    bytes: &[u8],
) -> Result<(), std::io::Error> {
    {
        let mut f = fs::File::create(tmp_path)?;
        f.write_all(bytes)?;
        f.flush()?;
        f.sync_all()?;
    }

    // Read-back verification: the bytes on disk must equal what we wrote.
    let mut readback = Vec::with_capacity(bytes.len());
    fs::File::open(tmp_path)?.read_to_end(&mut readback)?;
    if readback != bytes {
        // Don't leave a corrupt temp lying around.
        let _ = fs::remove_file(tmp_path);
        return Err(std::io::Error::other(
            "atomic write read-back verification failed (short or corrupt write)",
        ));
    }

    fs::rename(tmp_path, final_path)?;

    // SAVE-D3-01 — see the durability note above.
    if let Some(dir) = final_path.parent() {
        if let Ok(dir_file) = fs::File::open(dir) {
            dir_file.sync_all()?;
        }
    }
    Ok(())
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

/// Most recently modified valid save slot, used by the player-facing
/// quickload action. Invalid names and unreadable metadata are ignored.
pub fn latest_slot(dir: &Path) -> Option<u32> {
    slots_by_recency(dir).into_iter().next()
}

/// Valid slot filenames ordered newest-first. Modification-time ties are
/// broken by higher slot number so quickload behavior is deterministic.
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
    match slots.iter().max_by_key(|(_, mtime)| *mtime) {
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
        assert_eq!(parse_slot_filename("notes.txt"), None);
        assert_eq!(parse_slot_filename("save_x.ess"), None);
    }

    #[test]
    fn latest_slot_ignores_newer_tmp_and_empty_directory() {
        let dir =
            std::env::temp_dir().join(format!("byro_save_latest_filter_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(latest_slot(&dir), None);
        fs::write(dir.join("save_9.ess.tmp"), b"newer temp").unwrap();
        assert_eq!(latest_slot(&dir), None, "temp files are never live slots");
        fs::write(slot_path(&dir, 2), b"valid-name slot").unwrap();
        assert_eq!(latest_slot(&dir), Some(2));
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
        // No leftover temp file after a clean write.
        assert!(!path.with_extension("ess.tmp").exists());

        assert_eq!(read_slot(&dir, 2).unwrap(), payload);
        assert_eq!(list_slots(&dir), vec![2]);

        write_slot(&dir, 0, payload).unwrap();
        assert_eq!(list_slots(&dir), vec![0, 2]);
        assert_eq!(latest_slot(&dir), Some(0));

        let _ = fs::remove_dir_all(&dir);
    }
}
