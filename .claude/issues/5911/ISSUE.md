# #5911: PAR-D6-2026-10-15-03: a non-UTF-8 `--profiles` path panics both tools instead of giving a usage error; the shared helper hard-codes `String`

- **Labels**: low,bug,tech-debt
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**:
  - `crates/game-detect/src/profiles.rs:81-89`: `parse_profiles_arg(args: &mut impl Iterator<Item = &'a String>)`.
  - `tools/byro-detect/src/main.rs:26` and `tools/byro-launcher/src/main.rs:27`: `std::env::args()`.
- **Status**: NEW. #5739 centralised the parse but kept the `String` interface.
- **Trigger Input**: `byro-detect --profiles $'/tmp/\xff/profiles.toml'`, or any argv element that is not valid UTF-8 on Linux.
- **Description**:
  - `std::env::args()` panics during iteration on non-Unicode arguments, before `parse_args` runs. A legal Linux path aborts the tool with a panic backtrace rather than "--profiles needs a path"-style usage handling.
  - Because the shared helper takes `&String`, a fix in either tool needs an API change in game-detect.
  - Minor related edge: `--profiles ""` passes the `starts_with('-')` filter and absolutises to the cwd *directory* (`:66-71`). The loader then warns "failed to read" on an `exists()` directory (`:325-326`, `:357-362`) instead of rejecting the value as a usage error.
- **Evidence**: code reading. The panic is documented `std::env::args` behaviour ("will panic during iteration if any argument to the process is not valid Unicode").
- **Impact**: robustness of a user-facing CLI. No data hazard, since nothing is written before the panic.
- **Related**: #5739, #5167, #5476.
- **Suggested Fix**: collect `std::env::args_os()`, make `parse_profiles_arg` take `&OsString` and return `PathBuf`, and reject an empty value as a usage error.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
