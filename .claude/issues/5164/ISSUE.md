# #5164 — TOOL-D4-2026-10-02-03: The #5143 "pinned statically" guards are vacuous on their positive half, and boot-request has no negative pin

Labels: low,tech-debt,test-gap,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/5164

From `docs/audits/AUDIT_TOOLING_2026-10-02.md` (HEAD `e737f06bf`).

- **Severity**: LOW
- **Dimension**: Boot Handoff & Persistence
- **Exposure**: developer only (regression-guard quality)
- **Location**: `crates/settings-io/src/lib.rs:668-680` (`save_has_no_nonatomic_clobber_fallback`); `crates/game-detect/src/overrides.rs:159-172` (`merge_uses_the_shared_atomic_file_writer`); `crates/boot-request/src/lib.rs:475-477` (`save_uses_the_shared_atomic_file_writer`)
- **Status**: NEW (test-gap)
- **Description**: The tests scan their own file with whole-file `include_str!`. That is the #4604/#4842 pattern `_audit-common.md` warns about: the scan should use `production_text` instead.
  - The positive needles, `"atomic_file::atomic_write"` and `"byroredux_core::atomic_file::atomic_write"`, appear verbatim inside each test's own `assert!`. Deleting the production call does not fail the test.
  - The negative needle is assembled at run time, but it is only the old binding name `rename_error`. A fallback reintroduced as `Err(e) if path.exists() => fs::write(path, fs::read(&temp_path)?)` passes.
  - The boot-request guard is entirely the vacuous positive half. It has no negative check, although `2e95f0bbf` says the removal is pinned in each writer.
- **Evidence**: `boot-request/src/lib.rs:476`: `assert!(include_str!("lib.rs").contains("byroredux_core::atomic_file::atomic_write"));`. The needle is the literal on that same line.
- **Impact**: The three guards cannot catch the regression they were written for: the clobber fallback coming back.
- **Related**: #5143, #4604, TOOL-D4-02
- **Suggested Fix**: Strip `#[cfg(test)]` blocks before scanning, using the `source_scan::production_text` pattern. Make the negative check structural, for example "no `fs::write` whose argument reads `temp_path`", and add it to boot-request.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / frontends / request variants named above)
- [ ] **TESTS**: A regression test pins this specific fix

