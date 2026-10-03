//! Durable, crash-safe file replacement.
//!
//! Lives in `core` rather than beside the save container because it is plain
//! `std` file IO with no save-format knowledge, and three separate writers now
//! need the same durability contract: the save ring, the settings registry, and
//! the launcher. Two writers with two different contracts — only one of them
//! documented — is exactly what #3472 removed.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Unique sibling staging path for [`atomic_write`]:
/// `.{name}.{pid}.{counter}.tmp` beside the destination. The dot keeps the
/// temp out of casual directory listings, the pid separates concurrent
/// processes, and the in-process counter separates concurrent writers within
/// one process. #5143 — one shared helper instead of the three drifting
/// copies the boot-request, overrides and settings writers each carried.
pub fn atomic_temp_path(path: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!(".{name}.{}.{}.tmp", std::process::id(), id))
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
///
/// Any failure up to and including the `rename` removes the temp (best
/// effort) before returning. #5163 — with [`atomic_temp_path`]'s unique
/// names a leftover is never overwritten by the next attempt, so on the
/// disk-full path every failed save would otherwise strand one more partial
/// hidden file.
pub fn atomic_write(
    final_path: &Path,
    tmp_path: &Path,
    bytes: &[u8],
) -> Result<(), std::io::Error> {
    if let Err(error) = stage_and_rename(final_path, tmp_path, bytes) {
        let _ = fs::remove_file(tmp_path);
        return Err(error);
    }

    // SAVE-D3-01 — see the durability note above.
    if let Some(dir) = final_path.parent() {
        if let Ok(dir_file) = fs::File::open(dir) {
            dir_file.sync_all()?;
        }
    }
    Ok(())
}

/// Source-scan guard for a writer that stages through [`atomic_write`] (#5164).
///
/// `source` is the writer's whole file (`include_str!`); only its production
/// text is scanned, so the needles in the calling test cannot satisfy it. The
/// writer must call `atomic_file::atomic_write(` exactly once and mention its
/// staging binding `temp_path` exactly twice — bound from
/// [`atomic_temp_path`], then passed to the write. #5143 removed a "Windows
/// rename" fallback that ran on any `atomic_write` error and copied the
/// temp's possibly-partial bytes over the good file; any such fallback has to
/// read the temp again, which a third `temp_path` mention exposes whatever its
/// error binding is called. A plain `fs::write`/`fs::rename` of the
/// destination is rejected outright, since nothing in a writer should replace
/// the file except the atomic path.
///
/// The failure such a fallback mishandles (disk full, EIO on sync) is
/// OS-level and cannot be fault-injected portably, hence the static pin.
#[doc(hidden)]
pub fn assert_no_clobber_fallback(source: &str) {
    use crate::source_scan::{count_identifier, production_text};
    let production = production_text(source);
    assert_eq!(
        production.matches("atomic_file::atomic_write(").count(),
        1,
        "the save must go through the shared durable writer exactly once"
    );
    assert_eq!(
        count_identifier(production, "temp_path"),
        2,
        "`temp_path` may only be bound and handed to `atomic_write`; another use \
         is a fallback that reads the temp again (#5143)"
    );
    for forbidden in ["fs::write(", "fs::rename("] {
        assert!(
            !production.contains(forbidden),
            "`{forbidden}` in a durable writer bypasses `atomic_write` (#5143)"
        );
    }
}

/// Every [`atomic_write`] step that leaves the temp behind on failure.
fn stage_and_rename(
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
        return Err(std::io::Error::other(
            "atomic write read-back verification failed (short or corrupt write)",
        ));
    }

    fs::rename(tmp_path, final_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #5143 — the shared staging-path helper must give every concurrent
    /// writer its own hidden temp beside the destination.
    #[test]
    fn atomic_temp_paths_are_unique_hidden_siblings() {
        let destination = Path::new("/home/u/.byroredux/settings.toml");
        let first = atomic_temp_path(destination);
        let second = atomic_temp_path(destination);
        assert_ne!(
            first, second,
            "two writers in one process must not share a temp path"
        );
        for temp in [&first, &second] {
            let name = temp.file_name().unwrap().to_string_lossy().to_string();
            assert!(
                name.starts_with(".settings.toml."),
                "the temp hides as a sibling of the destination: {name}"
            );
            assert!(name.ends_with(".tmp"), "{name}");
            assert!(
                name.contains(std::process::id().to_string().as_str()),
                "the pid separates concurrent processes: {name}"
            );
        }
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "byroredux-atomic-file-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_round_trips_and_renames_the_temp_away() {
        let dir = scratch_dir("ok");
        let final_path = dir.join("out.toml");
        let temp = atomic_temp_path(&final_path);
        atomic_write(&final_path, &temp, b"hello").unwrap();
        assert_eq!(fs::read(&final_path).unwrap(), b"hello");
        assert!(!temp.exists(), "a successful write renames the temp away");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// #5163 — a failure after the temp exists must not strand it. The
    /// injected failure is the `rename`: the destination is a non-empty
    /// directory, which a file cannot be renamed over.
    #[test]
    fn failed_rename_removes_the_temp() {
        let dir = scratch_dir("rename-fail");
        let final_path = dir.join("out.toml");
        fs::create_dir_all(final_path.join("occupied")).unwrap();
        let temp = atomic_temp_path(&final_path);
        assert!(atomic_write(&final_path, &temp, b"hello").is_err());
        assert!(!temp.exists(), "a failed write cleans up its temp");
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name != "out.toml")
            .collect();
        assert!(leftovers.is_empty(), "stranded temps: {leftovers:?}");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// #5164 — the guard must reject a reintroduced fallback under any
    /// binding name, and pass the shape every writer actually has.
    #[test]
    fn clobber_guard_accepts_the_writer_shape_and_rejects_fallbacks() {
        let tail = "\n#[cfg(test)]\nmod tests {}\n";
        let good = "let temp_path = atomic_temp_path(path);\n\
                    byroredux_core::atomic_file::atomic_write(path, &temp_path, b).map_err(f)";
        assert_no_clobber_fallback(&format!("{good}{tail}"));

        let fallbacks = [
            format!("{good} match x {{ Err(e) if path.exists() => fs::copy(&temp_path, path) }}"),
            format!("{good}; Err(_) => std::fs::write(path, text)"),
            format!("{good}; fs::rename(other, path)"),
        ];
        for bad in fallbacks {
            let source = format!("{bad}{tail}");
            assert!(
                std::panic::catch_unwind(|| assert_no_clobber_fallback(&source)).is_err(),
                "{bad}"
            );
        }
        // A needle that only lives in the test module does not count.
        let source = format!("let temp_path = 1;{tail}atomic_file::atomic_write(&temp_path)");
        assert!(std::panic::catch_unwind(|| assert_no_clobber_fallback(&source)).is_err());
    }
}
