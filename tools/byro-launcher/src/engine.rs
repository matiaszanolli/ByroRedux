//! Finding and running the engine.
//!
//! The launcher stays resident behind the engine rather than exec-ing and
//! exiting (`docs/engine/launcher.md` §12 Q1). That is the same argument that
//! put the launcher in its own process at all: if the engine dies during
//! startup, someone has to be left holding a window that can say so. A user who
//! double-clicked an icon has no terminal to read a panic out of.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

/// How many trailing log lines to keep for the crash report.
const LOG_TAIL_LINES: usize = 200;

/// Engine executable name for this platform.
pub const ENGINE_EXE: &str = if cfg!(windows) {
    "byroredux.exe"
} else {
    "byroredux"
};

/// Find the engine binary given the directory the launcher is running from.
///
/// Checked in order: beside the launcher (how a release ships, and also how
/// `cargo build` lays out `target/<profile>/`), then one level up, which is
/// what a `bin/` subdirectory layout would need. Pure so the search order is
/// testable without installing anything.
pub fn locate_engine(exe_dir: &Path) -> Option<PathBuf> {
    let candidates = [
        exe_dir.join(ENGINE_EXE),
        exe_dir.join("..").join(ENGINE_EXE),
    ];
    candidates.into_iter().find(|path| path.is_file())
}

/// [`locate_engine`] against the running executable's own directory.
pub fn locate_engine_beside_self() -> Option<PathBuf> {
    locate_engine(&byroredux_core::paths::exe_dir()?)
}

/// A running engine, plus the tail of what it has said.
pub struct EngineProcess {
    child: Child,
    log: Arc<Mutex<VecDeque<String>>>,
    /// The thread draining the engine's stderr into `log`; taken once the
    /// process has exited and the thread has been given time to finish.
    reader: Option<std::thread::JoinHandle<()>>,
}

/// How long, after the engine has exited, to wait for the stderr reader to reach
/// end-of-file before reporting what it has. The exit is routinely seen before
/// the reader thread has taken the last lines out of the pipe; a descendant that
/// inherited stderr and outlives the engine keeps the pipe open, so the wait has
/// to be bounded.
const READER_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

/// Longest line kept whole; the rest of a longer line is dropped. Bounds the
/// reader's buffer against an engine (or a native component writing to fd 2)
/// that emits a huge run with no newline.
const MAX_LINE_BYTES: usize = 16 * 1024;

/// Feed `sink` one lossily-decoded line at a time until `reader` reaches
/// end-of-file (#6279).
///
/// This must never stop before the child's end of the pipe closes. Dropping the
/// read end early makes the engine's next `eprintln!` fail with `EPIPE` — the
/// Rust runtime ignores `SIGPIPE` — and `eprintln!` panics on a write error, so
/// a diagnostic line would take the engine down (exit 101) and hide its cause.
/// `BufRead::lines` ended the loop at the first non-UTF-8 line; reading bytes
/// and decoding lossily cannot fail on content, and a genuine read error falls
/// back to draining into a sink.
fn pump_lines<R: BufRead>(mut reader: R, mut sink: impl FnMut(String)) {
    let mut line: Vec<u8> = Vec::new();
    let mut truncated = false;
    let mut emit = |line: &mut Vec<u8>, truncated: &mut bool| {
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        let mut text = String::from_utf8_lossy(line).into_owned();
        if std::mem::take(truncated) {
            text.push_str(" […truncated]");
        }
        line.clear();
        sink(text);
    };
    loop {
        let (used, complete) = match reader.fill_buf() {
            Ok([]) => break,
            Ok(buf) => {
                let newline = buf.iter().position(|&b| b == b'\n');
                let chunk = &buf[..newline.unwrap_or(buf.len())];
                let room = MAX_LINE_BYTES.saturating_sub(line.len());
                line.extend_from_slice(&chunk[..chunk.len().min(room)]);
                truncated |= chunk.len() > room;
                (newline.map_or(buf.len(), |i| i + 1), newline.is_some())
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => {
                let _ = std::io::copy(&mut reader, &mut std::io::sink());
                break;
            }
        };
        reader.consume(used);
        if complete {
            emit(&mut line, &mut truncated);
        }
    }
    if !line.is_empty() {
        emit(&mut line, &mut truncated);
    }
}

/// What happened to a launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineStatus {
    Running,
    /// Exited cleanly. Nothing to report.
    Finished,
    /// Exited non-zero, or was killed by a signal. Carries the log tail,
    /// because this is the case the user cannot otherwise see.
    Failed {
        code: Option<i32>,
        tail: Vec<String>,
    },
}

impl EngineProcess {
    /// Launch the engine against a boot-request file.
    ///
    /// `profiles_path` is exported as `$BYRO_PROFILES` so the engine reads
    /// the same profiles file the launcher validated against and wrote
    /// `[roots]` into (#5294) — pre-fix the engine always fell back to
    /// `~/.byroredux/profiles.toml`, which a `--profiles` launcher could
    /// silently disagree with.
    ///
    /// stderr is piped and drained on a reader thread rather than inherited:
    /// an inherited stderr goes to a terminal that, for the launcher's actual
    /// audience, does not exist.
    pub fn spawn(
        engine: &Path,
        boot_request: &Path,
        profiles_path: &Path,
    ) -> std::io::Result<Self> {
        let mut child = Command::new(engine)
            .arg("--boot")
            .arg(boot_request)
            .env(byroredux_game_detect::profiles::PROFILES_ENV, profiles_path)
            // Run from the engine's own directory so its relative asset paths
            // (`assets/debug_profiles.toml`, shader blobs) resolve the same way
            // they do when started from a shell.
            .current_dir(engine.parent().unwrap_or(Path::new(".")))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;

        let log = Arc::new(Mutex::new(VecDeque::with_capacity(LOG_TAIL_LINES)));
        let reader = child.stderr.take().map(|stderr| {
            let log = Arc::clone(&log);
            std::thread::spawn(move || {
                pump_lines(BufReader::new(stderr), |line| {
                    // A poisoned log drops the line but keeps draining: the
                    // pipe must outlive the child (see `pump_lines`).
                    let Ok(mut log) = log.lock() else { return };
                    if log.len() == LOG_TAIL_LINES {
                        log.pop_front();
                    }
                    log.push_back(line);
                })
            })
        });
        Ok(Self { child, log, reader })
    }

    /// Give the stderr reader the chance to finish after the process has
    /// exited, so a crash report is not missing the lines the engine wrote last
    /// (the reason it died). Waits at most [`READER_DRAIN_TIMEOUT`], once.
    fn drain_reader(&mut self) {
        let Some(reader) = self.reader.take() else {
            return;
        };
        let deadline = std::time::Instant::now() + READER_DRAIN_TIMEOUT;
        while !reader.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        if reader.is_finished() {
            let _ = reader.join();
        }
        // Otherwise a descendant still holds the pipe: leave the thread to
        // append whatever else arrives.
    }

    /// Poll without blocking. Call once per frame.
    pub fn poll(&mut self) -> EngineStatus {
        match self.child.try_wait() {
            Ok(None) => EngineStatus::Running,
            Ok(Some(status)) if status.success() => EngineStatus::Finished,
            Ok(Some(status)) => {
                self.drain_reader();
                EngineStatus::Failed {
                    code: status.code(),
                    tail: self.tail(),
                }
            }
            // A process we cannot wait on is not one we can claim is running.
            Err(error) => EngineStatus::Failed {
                code: None,
                tail: vec![format!("could not wait on the engine process: {error}")],
            },
        }
    }

    /// Everything the engine has written to stderr, up to the tail limit.
    pub fn tail(&self) -> Vec<String> {
        self.log
            .lock()
            .map(|log| log.iter().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engine_is_found_beside_the_launcher_first() {
        let dir = tempfile::tempdir().unwrap();
        let beside = dir.path().join(ENGINE_EXE);
        std::fs::write(&beside, b"#!/bin/sh\n").unwrap();
        assert_eq!(locate_engine(dir.path()), Some(beside));
    }

    #[test]
    fn the_parent_directory_is_searched_second() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(dir.path().join(ENGINE_EXE), b"#!/bin/sh\n").unwrap();
        assert_eq!(
            locate_engine(&bin),
            Some(bin.join("..").join(ENGINE_EXE)),
            "the hit must be the parent directory's executable"
        );
    }

    /// A missing engine must be reported, never guessed at — the launcher has
    /// to be able to say "the engine is not installed beside me".
    #[test]
    fn a_missing_engine_is_none_rather_than_a_hopeful_path() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(locate_engine(dir.path()), None);
    }

    /// A directory named like the executable must not satisfy the search.
    /// The supervision contract, against a stub engine.
    ///
    /// Unix-only because it needs a script that is executable without a
    /// compiler; the logic under test (argv shape, exit classification, log
    /// capture) is platform-independent.
    #[cfg(unix)]
    mod supervision {
        use super::super::*;
        use std::os::unix::fs::PermissionsExt;
        use std::sync::{Mutex, MutexGuard};

        /// Serialises "write an executable, then spawn it".
        ///
        /// `Command::spawn` is fork+exec, and a fork inherits every open file
        /// descriptor — including another thread's *write* handle to the stub
        /// it is still creating. The inherited handle keeps that file
        /// write-open until the child execs, so a second test's `exec` of its
        /// own stub fails with `ETXTBSY` ("Text file busy"). It reproduced
        /// about one run in three.
        ///
        /// Serialising both halves means no fork ever happens while a write
        /// handle to any stub is open. Cheap: four tests, milliseconds each.
        static STUB_LOCK: Mutex<()> = Mutex::new(());

        fn serialised() -> MutexGuard<'static, ()> {
            STUB_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        }

        /// Write an executable stub in place of the engine.
        fn stub(dir: &Path, body: &str) -> PathBuf {
            let path = dir.join(ENGINE_EXE);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).unwrap();
            path
        }

        fn wait_for(process: &mut EngineProcess) -> EngineStatus {
            for _ in 0..200 {
                match process.poll() {
                    EngineStatus::Running => {
                        std::thread::sleep(std::time::Duration::from_millis(25))
                    }
                    settled => return settled,
                }
            }
            panic!("stub engine never exited");
        }

        /// The engine must be invoked as `--boot <path>` — the one thing the
        /// launcher and the engine's `expand_boot_request` seam have to agree
        /// on, and the only part of the handoff not covered by either side's
        /// own tests. #5294 — it must also receive the launcher's profiles
        /// file as `$BYRO_PROFILES`, or a `--profiles` launcher validates
        /// against a file the engine it starts never reads.
        #[test]
        fn the_engine_is_invoked_with_the_boot_request_path_and_profiles_env() {
            let _serialised = serialised();
            let dir = tempfile::tempdir().unwrap();
            let seen = dir.path().join("argv.txt");
            let engine = stub(
                dir.path(),
                &format!(
                    "printf '%s\\n' \"$@\" > {seen}\nprintf '%s\\n' \"${{BYRO_PROFILES:-unset}}\" >> {seen}",
                    seen = seen.display()
                ),
            );
            let request = dir.path().join("boot.toml");
            std::fs::write(&request, "version = 1\n").unwrap();
            let profiles = dir.path().join("alt-profiles.toml");

            let mut process = EngineProcess::spawn(&engine, &request, &profiles).unwrap();
            assert_eq!(wait_for(&mut process), EngineStatus::Finished);
            let argv = std::fs::read_to_string(&seen).unwrap();
            let argv: Vec<&str> = argv.lines().collect();
            assert_eq!(
                argv,
                [
                    "--boot",
                    request.to_str().unwrap(),
                    profiles.to_str().unwrap(),
                ]
            );
        }

        /// A clean exit is not a failure and must not open the crash screen.
        #[test]
        fn a_clean_exit_is_reported_as_finished() {
            let _serialised = serialised();
            let dir = tempfile::tempdir().unwrap();
            let engine = stub(dir.path(), "exit 0");
            let mut process =
                EngineProcess::spawn(&engine, &dir.path().join("boot.toml"), dir.path()).unwrap();
            assert_eq!(wait_for(&mut process), EngineStatus::Finished);
        }

        /// The case the resident launcher exists for: the engine dies, and the
        /// user has no terminal to read the reason out of, so the reason has to
        /// come back with the exit status.
        #[test]
        fn a_crash_carries_its_code_and_the_last_thing_the_engine_said() {
            let _serialised = serialised();
            let dir = tempfile::tempdir().unwrap();
            let engine = stub(
                dir.path(),
                "echo 'vkCreateDevice failed: ERROR_INITIALIZATION_FAILED' >&2\nexit 3",
            );
            let mut process =
                EngineProcess::spawn(&engine, &dir.path().join("boot.toml"), dir.path()).unwrap();
            match wait_for(&mut process) {
                EngineStatus::Failed { code, tail } => {
                    assert_eq!(code, Some(3));
                    assert!(
                        tail.iter().any(|line| line.contains("vkCreateDevice")),
                        "log tail lost the reason: {tail:?}"
                    );
                }
                other => panic!("expected Failed, got {other:?}"),
            }
        }

        /// The exit can be observed before the stderr reader thread has taken
        /// the last bytes out of the pipe, so a crash's reason used to come
        /// back empty now and then (CI's lock-order lane saw `tail == []`).
        /// `poll` must let the reader reach end-of-file before it reports
        /// `Failed`. A helper that inherited stderr and writes after the engine
        /// is gone makes the ordering deterministic: the exit is visible at once
        /// and the last line is still in flight.
        #[test]
        fn a_crash_tail_waits_for_the_stderr_reader_to_drain() {
            let _serialised = serialised();
            let dir = tempfile::tempdir().unwrap();
            let engine = stub(
                dir.path(),
                "echo 'early' >&2\n(sleep 0.05; echo 'late' >&2) &\nexit 3",
            );
            let mut process =
                EngineProcess::spawn(&engine, &dir.path().join("boot.toml"), dir.path()).unwrap();
            let EngineStatus::Failed { code, tail } = wait_for(&mut process) else {
                panic!("expected Failed");
            };
            assert_eq!(code, Some(3));
            assert_eq!(tail, ["early", "late"], "the report lost the engine's last lines");
        }

        /// The wait for the reader is bounded: a descendant that keeps stderr
        /// open for good must not hang the crash screen.
        #[test]
        fn a_crash_report_is_not_held_hostage_by_a_lingering_descendant() {
            let _serialised = serialised();
            let dir = tempfile::tempdir().unwrap();
            let engine = stub(dir.path(), "echo 'died' >&2\n(sleep 5) &\nexit 3");
            let mut process =
                EngineProcess::spawn(&engine, &dir.path().join("boot.toml"), dir.path()).unwrap();
            let started = std::time::Instant::now();
            let EngineStatus::Failed { tail, .. } = wait_for(&mut process) else {
                panic!("expected Failed");
            };
            assert!(started.elapsed() < std::time::Duration::from_secs(3), "poll hung on the pipe");
            assert_eq!(tail, ["died"]);
        }

        /// #6279: the reader must outlive a non-UTF-8 line. The stub writes the
        /// bad byte, lingers, then writes more; if the reader had returned, the
        /// stub's next write would hit a closed pipe and `echo` would fail
        /// (SIGPIPE) before `late` — and the exit status would not be 3.
        #[test]
        fn the_reader_survives_a_non_utf8_line_and_keeps_the_pipe_open() {
            let _serialised = serialised();
            let dir = tempfile::tempdir().unwrap();
            let engine = stub(
                dir.path(),
                "printf 'before\\n\\377\\n' >&2\nsleep 0.1\necho 'late' >&2\nexit 3",
            );
            let mut process =
                EngineProcess::spawn(&engine, &dir.path().join("boot.toml"), dir.path()).unwrap();
            let EngineStatus::Failed { code, tail } = wait_for(&mut process) else {
                panic!("expected Failed");
            };
            assert_eq!(code, Some(3));
            assert_eq!(tail, ["before", "\u{FFFD}", "late"]);
        }

        /// The tail is bounded, so a chatty engine cannot grow the buffer
        /// without limit — and what survives is the *end*, which is where the
        /// failure is.
        #[test]
        fn the_log_tail_is_bounded_and_keeps_the_most_recent_lines() {
            let _serialised = serialised();
            let dir = tempfile::tempdir().unwrap();
            let total = LOG_TAIL_LINES + 50;
            let engine = stub(
                dir.path(),
                &format!(
                    "i=0; while [ $i -lt {total} ]; do echo line$i >&2; i=$((i+1)); done; exit 1"
                ),
            );
            let mut process =
                EngineProcess::spawn(&engine, &dir.path().join("boot.toml"), dir.path()).unwrap();
            let EngineStatus::Failed { tail, .. } = wait_for(&mut process) else {
                panic!("expected Failed");
            };
            assert!(tail.len() <= LOG_TAIL_LINES, "tail grew to {}", tail.len());
            assert_eq!(
                tail.last().map(String::as_str),
                Some(&*format!("line{}", total - 1))
            );
        }
    }

    fn pumped(input: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        pump_lines(input, |line| out.push(line));
        out
    }

    /// #6279: a non-UTF-8 line used to end the reader, closing the pipe under a
    /// live engine. Every later line must still arrive.
    #[test]
    fn a_non_utf8_line_does_not_end_the_stream() {
        assert_eq!(
            pumped(b"before\n\xff\xfe bad\nafter\n"),
            ["before", "\u{FFFD}\u{FFFD} bad", "after"]
        );
    }

    #[test]
    fn crlf_and_an_unterminated_last_line_are_handled_like_lines() {
        assert_eq!(pumped(b"one\r\ntwo\n\nlast"), ["one", "two", "", "last"]);
        assert_eq!(pumped(b""), Vec::<String>::new());
    }

    /// An endless run with no newline must not buffer without bound.
    #[test]
    fn an_overlong_line_is_capped_and_the_next_line_is_intact() {
        let mut input = vec![b'x'; MAX_LINE_BYTES * 3];
        input.extend_from_slice(b"\nnext\n");
        let lines = pumped(&input);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with(&"x".repeat(MAX_LINE_BYTES)));
        assert!(lines[0].ends_with(" […truncated]"));
        assert_eq!(lines[0].len(), MAX_LINE_BYTES + " […truncated]".len());
        assert_eq!(lines[1], "next");
    }

    #[test]
    fn a_directory_with_the_engine_name_is_not_the_engine() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(ENGINE_EXE)).unwrap();
        assert_eq!(locate_engine(dir.path()), None);
    }
}
