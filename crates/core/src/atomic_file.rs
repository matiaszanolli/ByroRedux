//! Durable, crash-safe file replacement.
//!
//! Lives in `core` rather than beside the save container because it is plain
//! `std` file IO with no save-format knowledge, and three separate writers now
//! need the same durability contract: the save ring, the settings registry, and
//! the launcher. Two writers with two different contracts — only one of them
//! documented — is exactly what #3472 removed.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;

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
