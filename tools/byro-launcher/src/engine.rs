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
    let exe = std::env::current_exe().ok()?;
    locate_engine(exe.parent()?)
}

/// A running engine, plus the tail of what it has said.
pub struct EngineProcess {
    child: Child,
    log: Arc<Mutex<VecDeque<String>>>,
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
    /// stderr is piped and drained on a reader thread rather than inherited:
    /// an inherited stderr goes to a terminal that, for the launcher's actual
    /// audience, does not exist.
    pub fn spawn(engine: &Path, boot_request: &Path) -> std::io::Result<Self> {
        let mut child = Command::new(engine)
            .arg("--boot")
            .arg(boot_request)
            // Run from the engine's own directory so its relative asset paths
            // (`assets/debug_profiles.toml`, shader blobs) resolve the same way
            // they do when started from a shell.
            .current_dir(engine.parent().unwrap_or(Path::new(".")))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;

        let log = Arc::new(Mutex::new(VecDeque::with_capacity(LOG_TAIL_LINES)));
        if let Some(stderr) = child.stderr.take() {
            let log = Arc::clone(&log);
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let Ok(mut log) = log.lock() else { return };
                    if log.len() == LOG_TAIL_LINES {
                        log.pop_front();
                    }
                    log.push_back(line);
                }
            });
        }
        Ok(Self { child, log })
    }

    /// Poll without blocking. Call once per frame.
    pub fn poll(&mut self) -> EngineStatus {
        match self.child.try_wait() {
            Ok(None) => EngineStatus::Running,
            Ok(Some(status)) if status.success() => EngineStatus::Finished,
            Ok(Some(status)) => EngineStatus::Failed {
                code: status.code(),
                tail: self.tail(),
            },
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
        assert!(locate_engine(&bin).is_some());
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
        /// own tests.
        #[test]
        fn the_engine_is_invoked_with_the_boot_request_path() {
            let _serialised = serialised();
            let dir = tempfile::tempdir().unwrap();
            let seen = dir.path().join("argv.txt");
            let engine = stub(
                dir.path(),
                &format!("printf '%s\\n' \"$@\" > {}", seen.display()),
            );
            let request = dir.path().join("boot.toml");
            std::fs::write(&request, "version = 1\n").unwrap();

            let mut process = EngineProcess::spawn(&engine, &request).unwrap();
            assert_eq!(wait_for(&mut process), EngineStatus::Finished);
            let argv = std::fs::read_to_string(&seen).unwrap();
            let argv: Vec<&str> = argv.lines().collect();
            assert_eq!(argv, ["--boot", request.to_str().unwrap()]);
        }

        /// A clean exit is not a failure and must not open the crash screen.
        #[test]
        fn a_clean_exit_is_reported_as_finished() {
            let _serialised = serialised();
            let dir = tempfile::tempdir().unwrap();
            let engine = stub(dir.path(), "exit 0");
            let mut process = EngineProcess::spawn(&engine, &dir.path().join("boot.toml")).unwrap();
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
            let mut process = EngineProcess::spawn(&engine, &dir.path().join("boot.toml")).unwrap();
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
            let mut process = EngineProcess::spawn(&engine, &dir.path().join("boot.toml")).unwrap();
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

    #[test]
    fn a_directory_with_the_engine_name_is_not_the_engine() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(ENGINE_EXE)).unwrap();
        assert_eq!(locate_engine(dir.path()), None);
    }
}
