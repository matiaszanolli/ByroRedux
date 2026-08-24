# 3259: SAFE-BUILD-2026-08-24-01: cargo test --workspace fails to build - fragment_coverage.rs missing 3 new Effect variants

**Severity**: HIGH · **Report**: `docs/audits/AUDIT_SAFETY_2026-08-24.md` (SAFE-BUILD-2026-08-24-01)

## Description

Commits `cee35507` and `5f38402e` added three new `Effect` variants — `Conditional { .. }`, `SetGlobalValue { .. }`, and `Disable { .. }` — today (quest-fragment work). `fragment_coverage.rs`'s exhaustive `match e` was not updated, so it now fails to compile with `E0004` (non-exhaustive patterns). `cargo test --workspace` builds every workspace target — including examples — before running any test binary; a build failure in one target aborts the whole invocation with zero tests executed, for any crate, anywhere in the workspace.

## Location

`crates/scripting/examples/fragment_coverage.rs:59` (the non-exhaustive `match e { … }`); enum at `crates/scripting/src/translate/effects.rs:68` (`pub enum Effect`).

## Evidence

```
$ cargo test --workspace --quiet
error[E0004]: non-exhaustive patterns: `&Effect::Conditional { .. }`,
  `&Effect::SetGlobalValue { .. }` and `&Effect::Disable { .. }` not covered
  --> crates/scripting/examples/fragment_coverage.rs:59:11
```

## Impact

The project's two documented top-level verification commands (`cargo test`, `cargo test -p <crate>`) are broken workspace-wide. Not a logic regression — `cargo test -p byroredux-scripting --lib` (311 tests) passes clean — but anyone/CI invoking the documented command sees only a compile error, not the real suite state.

Cross-referenced (not re-filed) by `AUDIT_ECS_2026-08-24.md`, `AUDIT_CONCURRENCY_2026-08-24.md`, and `AUDIT_SCRIPTING_2026-08-24.md`.

## Suggested Fix

Add the three missing arms to `fragment_coverage.rs`'s `match`, or fold them under a documented wildcard.

## Completeness Checks
- [ ] **TESTS**: `cargo test --workspace` builds clean after the fix (this IS the regression test)
