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
    fs::create_dir_all(dir)?;
    let final_path = slot_path(dir, slot);
    let tmp_path = final_path.with_extension(format!("{SAVE_EXT}.tmp"));

    {
        let mut f = fs::File::create(&tmp_path)?;
        f.write_all(bytes)?;
        f.flush()?;
        f.sync_all()?;
    }

    // Read-back verification: the bytes on disk must equal what we wrote.
    let mut readback = Vec::with_capacity(bytes.len());
    fs::File::open(&tmp_path)?.read_to_end(&mut readback)?;
    if readback != bytes {
        // Don't leave a corrupt temp lying around.
        let _ = fs::remove_file(&tmp_path);
        return Err(SaveError::Io(std::io::Error::other(
            "save read-back verification failed (short or corrupt write)",
        )));
    }

    fs::rename(&tmp_path, &final_path)?;

    // SAVE-D3-01 — a successful `rename` isn't durable until the parent
    // directory's own metadata is fsynced: a crash immediately after can
    // otherwise lose the new directory entry (slot points at the old or no
    // inode) even though we returned Ok. Opening a directory as a `File`
    // and fsyncing it is a Unix capability; platforms that can't open a
    // directory (Windows) journal the rename, so skip there.
    if let Ok(dir_file) = fs::File::open(dir) {
        dir_file.sync_all()?;
    }
    Ok(final_path)
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
        let dir = std::env::temp_dir().join(format!("byro_save_resume_empty_{}", std::process::id()));
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
    fn write_read_round_trip_and_atomic_rename() {
        let dir = std::env::temp_dir().join(format!(
            "byro_save_disk_test_{}",
            std::process::id()
        ));
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

        let _ = fs::remove_dir_all(&dir);
    }
}
