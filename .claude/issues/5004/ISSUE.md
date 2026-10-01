# Issue #5004

**Title:** SF-2026-09-29-META-01: /audit-starfield Dim 5 first-step filter shader_tests::starfield matches zero tests and reports green
**State:** OPEN
**Labels:** bug, low, legacy-compat, tech-debt, game:starfield

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-29.md`
**Severity**: LOW
**Dimension**: NIF Shader Blocks (audit infrastructure)
**Location**: `.claude/commands/audit-starfield/SKILL.md` (Dimension 5, `**First step**:`)

## Description
`crates/nif/src/blocks/shader_tests/mod.rs` is mounted in `crates/nif/src/blocks/shader/mod.rs` via `#[path = "../shader_tests/mod.rs"] mod tests`, so the test paths are `blocks::shader::tests::starfield::*`, not `shader_tests::starfield`. The skill's documented first step, `cargo test -p byroredux-nif shader_tests::starfield`, filters to nothing.

## Evidence
`cargo test -p byroredux-nif --lib -- shader_tests::starfield` → `ok. 0 passed; 0 failed; 1371 filtered out` (per the audit run). `-- shader::tests::starfield` (or `-- starfield`) runs the 11 Dim 5 guards the skill names.

## Impact
An auditor following the skill literally gets a green zero-test run as the dimension's guard check — the vacuous-gate pattern `_audit-common.md` warns about.

## Related
`/audit-nif` (same module-path trap if any skill names `shader_tests::`); #4446 (closed, earlier skill path drift for the shader split).

## Suggested Fix
Change the first step to `cargo test -p byroredux-nif --lib -- shader::tests::starfield`, and consider a `_audit-validate.sh` check that each skill's `cargo test … <filter>` runs at least one test.

Validated at HEAD 9fcfdc3fc: SKILL.md Dim 5 first step reads `cargo test -p byroredux-nif shader_tests::starfield`; `blocks/shader/mod.rs` mounts the tests via `#[path = "../shader_tests/mod.rs"]` (tests not re-run per publish constraints).

## Completeness Checks
- [ ] **SIBLING**: other audit skills that name `shader_tests::` or other `#[path]`-mounted module filters
- [ ] **TESTS**: the corrected filter is confirmed to run >0 tests

