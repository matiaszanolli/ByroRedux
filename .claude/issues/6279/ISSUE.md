# #6279: TOOL-D4-2026-10-21-01: The launcher's stderr reader stops at the first non-UTF-8 line and closes the pipe, after which the engine's next `eprintln!` panics

**Labels**: low,tech-debt,bug

**Source**: docs/audits/AUDIT_TOOLING_2026-10-21.md · **Severity**: LOW · **Dimension**: Boot Handoff & Persistence (launcher supervision) · **Location**: `tools/byro-launcher/src/engine.rs:105-116` (reader thread; loop body from `c53527ff7`, 2026-08-30, wrapped by `da3f62128` this window)

- **Severity**: LOW
- **Dimension**: Boot Handoff & Persistence (launcher supervision)
- **Exposure**: launcher user (any engine started by `byro-launcher`)
- **Location**: `tools/byro-launcher/src/engine.rs:105-116` (reader thread; loop body from `c53527ff7`, 2026-08-30, wrapped by `da3f62128` this window)
- **Status**: NEW
- **Description**: The reader loop is `for line in BufReader::new(stderr).lines().map_while(Result::ok)`. `BufRead::lines` yields `Err(InvalidData)` for a line that is not valid UTF-8, and `map_while` then ends the loop for good. The thread exits and drops the `ChildStderr`, which closes the read end of the pipe. The engine's stderr is that pipe, and the Rust runtime ignores `SIGPIPE`. Its next stderr write therefore fails with `EPIPE`, and `eprintln!` panics on a write error ("failed printing to stderr"). There are 53 production `eprintln!` sites in `byroredux/src`. `env_logger` drops write errors, so logging alone survives, but the first `eprintln!` takes the engine down with exit 101. The launcher then shows a crash whose tail ends at the line before the bad bytes, so the report hides both the trigger and the panic.
- **Evidence**: `engine.rs:107-114`; the `da3f62128` change only waits for this thread to finish, so it does not change how the loop ends. First-party Rust code does not emit non-UTF-8 text: there are no raw `stderr()` writes in `byroredux/src` or `crates/*/src`, and paths are printed through `display()`. The ffmpeg `.fuz` decoder pipes its own stderr (`crates/audio/src/fuz.rs:241`). The realistic trigger is a native component that writes straight to fd 2 inside the process, such as a driver, a Vulkan layer or the FSR3 runtime, emitting a non-UTF-8 byte.
- **Impact**: when it triggers, the engine crashes over a diagnostic line and the crash report loses its cause. Before that, nothing records what the engine writes. The same gap applies to a single very long line with no newline, which buffers without bound in the reader. The likelihood is low and the fix is cheap.
- **Related**: `da3f62128`, #5294 (launcher-spawned engine contract)
- **Suggested Fix**: Read with `read_until(b'\n', &mut buf)` and push `String::from_utf8_lossy(&buf)`, capping the line length. On any non-EOF read error, keep draining into a sink instead of returning, so the pipe stays open for the life of the child. A test helper that writes `\xff\n` and then more lines should see the later lines in the tail.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_TOOLING_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
