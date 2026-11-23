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
    if let Some(dir) = final_path.parent()
        && let Ok(dir_file) = fs::File::open(dir) {
            dir_file.sync_all()?;
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

/// A fresh file in the temp directory that no one else can be holding: `create_new` (`O_EXCL`)
/// refuses an existing path, a planted symlink included, instead of following and truncating it,
/// and on Unix the file is the owner's alone (`0600`). The name,
/// `{stem}-{pid}-{nanos}-{counter}.{extension}`, separates processes and concurrent writers; a
/// name that is taken anyway is retried with a new one, up to 8 times.
///
/// #7143 — the one temp-file creator, shared by the `.fuz` voice decoder (#6307) and the movie
/// soundtrack, which had re-grown `File::create` on a predictable name. The path comes back as a
/// [`TempPath`], so the file is removed on every way out the caller does not [`keep`](TempPath::keep).
pub fn create_temp_file(stem: &str, extension: &str) -> std::io::Result<(fs::File, TempPath)> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut last = None;
    for _ in 0..8 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "{stem}-{}-{nanos}-{counter}.{extension}",
            std::process::id()
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        match options.open(&path) {
            Ok(file) => return Ok((file, TempPath(Some(path)))),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => last = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::from(std::io::ErrorKind::AlreadyExists)))
}

/// How old a [`create_temp_file`] file must be before [`reclaim_stale_temp_files`] may delete it. The longest
/// legitimate holder is a movie soundtrack, capped at one hour of audio and written once at open, so a file two
/// hours old belongs to a process that died; a younger one may belong to a live engine sharing the temp
/// directory and is left alone.
pub const STALE_TEMP_AGE: std::time::Duration = std::time::Duration::from_secs(2 * 60 * 60);

/// Whether `name` is `{stem}-{pid}-{nanos}-{counter}.{extension}` as [`create_temp_file`] writes it, made by a
/// process other than `own_pid`.
fn is_foreign_temp_name(name: &str, stem: &str, extension: &str, own_pid: u32) -> bool {
    let Some(rest) = name
        .strip_prefix(stem)
        .and_then(|rest| rest.strip_prefix('-'))
        .and_then(|rest| rest.strip_suffix(extension))
        .and_then(|rest| rest.strip_suffix('.'))
    else {
        return false;
    };
    let parts: Vec<&str> = rest.split('-').collect();
    parts.len() == 3
        && parts.iter().all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        && parts[0].parse::<u32>().is_ok_and(|pid| pid != own_pid)
}

/// Delete the temp files a process death left in `dir` (#7366). [`TempPath`] removes its file on a return and on
/// an unwinding panic, but SIGKILL, an OOM kill, a power cut or a driver crash runs no destructor, so a movie
/// soundtrack WAV (up to ~1.4 GB, alive for the whole playback) or a `.fuz` xWMA image stays behind and nothing
/// reclaims it: the names never repeat. Removes only regular files named `{stem}-{pid}-{nanos}-{counter}.{extension}`
/// for one of `kinds` (`(stem, extension)` pairs), made by another process than `own_pid` and at least `older_than`
/// old, best-effort (an entry that cannot be read or removed is skipped). Returns how many it removed.
pub fn reclaim_stale_temp_files(
    dir: &Path,
    kinds: &[(&str, &str)],
    own_pid: u32,
    older_than: std::time::Duration,
) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let now = std::time::SystemTime::now();
    let mut removed = 0;
    for entry in entries.filter_map(|entry| entry.ok()) {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !kinds.iter().any(|&(stem, extension)| is_foreign_temp_name(&name, stem, extension, own_pid)) {
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

/// [`reclaim_stale_temp_files`] over the system temp directory, as this process.
pub fn reclaim_stale_temp_files_in_temp_dir(kinds: &[(&str, &str)]) -> usize {
    reclaim_stale_temp_files(&std::env::temp_dir(), kinds, std::process::id(), STALE_TEMP_AGE)
}

/// Where a console or debug command may write `name`: a single file name (one normal component —
/// no separator, no `..`, not absolute) under the working-directory folder `dir`, which is created
/// if missing and must still resolve inside the working directory once canonicalised, so a `dir`
/// that is a symlink to somewhere else is refused. #7182 — the one confinement rule for the
/// screenshot, `tex.dump` and `depth.dump` writers; `depth.dump` had re-grown its own copy without
/// the canonical-root check.
pub fn confined_output_path(dir: &str, name: &str) -> std::io::Result<PathBuf> {
    confined_output_path_in(&std::env::current_dir()?, dir, name)
}

fn confined_output_path_in(base: &Path, dir: &str, name: &str) -> std::io::Result<PathBuf> {
    use std::path::Component;
    let refuse = |what: String| std::io::Error::new(std::io::ErrorKind::InvalidInput, what);
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(refuse(format!("'{name}' must be a file name inside {dir}/")));
    }
    let root = base.join(dir);
    fs::create_dir_all(&root)?;
    let root = root.canonicalize()?;
    if !root.starts_with(base.canonicalize()?) {
        return Err(refuse(format!("{dir}/ must stay inside the working directory")));
    }
    Ok(root.join(name))
}

/// Write `bytes` to a new file at [`confined_output_path`]: refused if one exists (`create_new`,
/// which also refuses a planted symlink), removed again if the write fails. Answers the path.
pub fn write_new_confined(dir: &str, name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    write_new_at(confined_output_path(dir, name)?, bytes)
}

fn write_new_at(path: PathBuf, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
    if let Err(error) = file.write_all(bytes) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(error);
    }
    Ok(path)
}

/// A temp file's path that removes the file when dropped, unless [`keep`](Self::keep) hands the
/// path on: an error return and a panic unwinding past it both clean up. #7158 — the movie
/// soundtrack removed its WAV on its `Err` arms only, so a decoder panic left up to ~1.4 GB of PCM
/// in the temp directory. Close the file before the guard drops (Windows refuses to remove an
/// open file).
#[derive(Debug)]
pub struct TempPath(Option<PathBuf>);

impl TempPath {
    pub fn path(&self) -> &Path {
        self.0.as_deref().expect("a TempPath holds its path until kept or dropped")
    }

    /// Keep the file: the caller owns removing it from here on.
    pub fn keep(mut self) -> PathBuf {
        self.0.take().expect("a TempPath holds its path until kept or dropped")
    }
}

impl AsRef<Path> for TempPath {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_file(path);
        }
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

    /// #7143 — the shared temp-file creator gives each caller its own new file, private to the
    /// owner on Unix, named by stem, pid and extension.
    #[test]
    fn temp_files_are_new_private_and_unique() {
        let (mut first, first_path) = create_temp_file("byro-test", "bin").unwrap();
        let (_, second_path) = create_temp_file("byro-test", "bin").unwrap();
        let (first_path, second_path) = (first_path.keep(), second_path.keep());
        assert_ne!(first_path, second_path);
        first.write_all(b"one").unwrap();
        assert_eq!(fs::read(&first_path).unwrap(), b"one");
        let name = first_path.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.starts_with(&format!("byro-test-{}-", std::process::id())) && name.ends_with(".bin"), "{name}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&first_path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        for path in [first_path, second_path] {
            let _ = fs::remove_file(path);
        }
    }

    /// #7182 — a confined output is one file name under its folder, and the folder must resolve
    /// inside the working directory: a symlink out of it is refused, as are paths with a separator,
    /// `..` or a root; a second write of one name is refused rather than clobbering the first.
    #[test]
    fn confined_outputs_stay_inside_their_folder() {
        let base = std::env::temp_dir().join(format!("byro-confined-{}", std::process::id()));
        let outside = std::env::temp_dir().join(format!("byro-confined-out-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&base).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let path = confined_output_path_in(&base, "shots", "a.png").unwrap();
        assert_eq!(path, base.canonicalize().unwrap().join("shots").join("a.png"));
        for bad in ["../a.png", "/tmp/a.png", "nested/a.png", "", "."] {
            let err = confined_output_path_in(&base, "shots", bad).expect_err(bad);
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput, "{bad}");
        }
        assert!(write_new_at(path.clone(), b"one").is_ok());
        assert!(write_new_at(path.clone(), b"two").is_err(), "an existing file is not clobbered");
        assert_eq!(fs::read(&path).unwrap(), b"one");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, base.join("linked")).unwrap();
            let err = confined_output_path_in(&base, "linked", "a.png").expect_err("a symlinked folder");
            assert!(err.to_string().contains("must stay inside the working directory"), "{err}");
        }
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&outside);
    }

    /// #7158 — a temp file is removed when its guard drops, on an early return or a panic
    /// unwinding past it, and stays once kept.
    #[test]
    fn a_temp_file_is_removed_unless_kept() {
        let (file, dropped) = create_temp_file("byro-test", "bin").unwrap();
        drop(file);
        let path = dropped.path().to_path_buf();
        assert!(path.exists());
        drop(dropped);
        assert!(!path.exists(), "dropping the guard removes the file");
        let unwound = std::panic::catch_unwind(|| {
            let (_file, guard) = create_temp_file("byro-test", "bin").unwrap();
            let path = guard.path().to_path_buf();
            std::panic::panic_any(path);
        })
        .expect_err("the closure panics");
        let path = unwound.downcast::<PathBuf>().unwrap();
        assert!(!path.exists(), "a panic unwinding past the guard removes the file");
        let (_file, kept) = create_temp_file("byro-test", "bin").unwrap();
        let kept = kept.keep();
        assert!(kept.exists(), "a kept file stays");
        let _ = fs::remove_file(kept);
    }

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

    /// #7366 — the sweep removes only another process's old `create_temp_file` files of the named kinds.
    #[test]
    fn the_sweep_removes_only_dead_processes_old_temp_files_of_the_named_kinds() {
        let dir = std::env::temp_dir().join(format!("byro-sweep-test-{}-{}", std::process::id(), line!()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let kinds = [("byroredux-movie", "wav"), ("byro-fuz", "xwma")];
        let own = std::process::id();
        let dead = own.wrapping_add(1);
        let touch = |name: String| {
            fs::write(dir.join(&name), b"x").unwrap();
            name
        };
        let stale_wav = touch(format!("byroredux-movie-{dead}-1700000000000-0.wav"));
        let stale_fuz = touch(format!("byro-fuz-{dead}-1700000000000-3.xwma"));
        let own_wav = touch(format!("byroredux-movie-{own}-1700000000000-1.wav"));
        let other_stem = touch(format!("someone-else-{dead}-1700000000000-0.wav"));
        let other_ext = touch(format!("byroredux-movie-{dead}-1700000000000-0.txt"));
        let malformed = touch(format!("byroredux-movie-{dead}-nanos-0.wav"));
        let extra_part = touch(format!("byroredux-movie-{dead}-1-2-3.wav"));
        fs::create_dir(dir.join(format!("byroredux-movie-{dead}-1-2.wav"))).unwrap();

        // Too young for the age gate: nothing goes.
        assert_eq!(reclaim_stale_temp_files(&dir, &kinds, own, std::time::Duration::from_secs(3600)), 0);
        assert!(dir.join(&stale_wav).exists(), "a fresh file may belong to a live process");

        // Old enough: only the two dead-process files of the named kinds.
        assert_eq!(reclaim_stale_temp_files(&dir, &kinds, own, std::time::Duration::ZERO), 2);
        assert!(!dir.join(&stale_wav).exists() && !dir.join(&stale_fuz).exists());
        for kept in [&own_wav, &other_stem, &other_ext, &malformed, &extra_part] {
            assert!(dir.join(kept).exists(), "{kept} is not a stale temp file of ours");
        }
        assert!(dir.join(format!("byroredux-movie-{dead}-1-2.wav")).is_dir(), "directories are never removed");
        assert_eq!(reclaim_stale_temp_files(&dir, &kinds, own, std::time::Duration::ZERO), 0, "nothing left");
        assert_eq!(reclaim_stale_temp_files(&dir.join("absent"), &kinds, own, std::time::Duration::ZERO), 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// The names the sweep matches are the names the creator writes.
    #[test]
    fn the_sweep_matches_the_names_create_temp_file_writes() {
        let (file, path) = create_temp_file("byro-sweep-shape", "bin").unwrap();
        drop(file);
        let name = path.path().file_name().unwrap().to_string_lossy().into_owned();
        assert!(is_foreign_temp_name(&name, "byro-sweep-shape", "bin", std::process::id().wrapping_add(1)));
        assert!(!is_foreign_temp_name(&name, "byro-sweep-shape", "bin", std::process::id()));
    }
}
