# REG-2026-10-31-02: Regression of #6222: the blocking `cargo clippy --workspace -- -D warnings` step is red again

Labels: medium,scripting,tech-debt,bug
Source: docs/audits/ (2026-10-31 report)

**Severity**: MEDIUM
**Dimension**: CI gates / closed-fix regression
**Source**: `docs/audits/AUDIT_REGRESSION_2026-10-31.md` REG-2026-10-31-02
**Status**: regression of #6222 (closed 2026-10-27, "the blocking clippy gate is green again, on every crate"). #6222 was itself a regression of #5849.

## Location
- `crates/scripting/src/fragment/vm/natives/ownership_crime.rs:108` (`unnecessary_map_or`: `values.get(i).map_or(true, Value::truthy)`)
- `crates/scripting/src/jail.rs:305` and `:389` (`useless_conversion`: `i32::try_from(sentence_gold).unwrap_or(i32::MAX)` where `sentence_gold` is already `i32`)

Both come from the 10-30 JAIL work (`9c6c37cd0`, `36d7bfbd6`). The source sites were re-checked present at HEAD `670f81917`.

## Description
`cargo clippy --workspace --keep-going -- -D warnings` stops with `could not compile byroredux-scripting (lib) due to 3 previous errors`. Because the failing crate is a dependency of the binary, the step reports nothing about the crates downstream of it.

## Evidence
Audit-run output (rustc/clippy 1.96.0) and CI run 41215450288, step "cargo clippy". The step has been red on every CI run since 10-30 (first cause `bench_capture.rs:98` from `f7194719f`, then the JAIL lints). Verification limit for this issue: clippy was not re-run at publish time; the lint sites were confirmed by reading the source.

## Impact
The blocking gate has been red for a day and the same class has recurred three times (#5849, #6222, now). Any further lint in the other crates is masked.

## Suggested Fix
Use `is_none_or(Value::truthy)` at `ownership_crime.rs:108` and drop the two `i32::try_from(...).unwrap_or(i32::MAX)` wrappers in `jail.rs`. Note on the existing open #6492 (non-blocking `--all-targets` step): it now also lists `crates/scripting/src/condition/tests.rs:134` (unused imports `Oblivion`, `Starfield`), `jail.rs:658` (`GUARD` never used) and `obscript_more/tests.rs:684-685` (`field_reassign_with_default`), besides the `shore_probe.rs` sites it already tracks.

## Related
#6222, #5849, #6492

## Completeness Checks
- [ ] **SIBLING**: `cargo clippy --workspace --keep-going -- -D warnings` is green on every crate, not just `byroredux-scripting`
- [ ] **TESTS**: Local clippy run before pushing (the gate itself is the regression test)
