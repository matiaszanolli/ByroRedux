# #3739 — TD1-2026-08-30-04: `build_scheduler` is 818 LOC — one registration wall, the largest non-renderer function in the workspace

**Labels**: bug, ecs, low, tech-debt

---

- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `byroredux/src/boot.rs` — `build_scheduler` (`pub(crate) fn build_scheduler() -> Scheduler`)
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD1-2026-08-30-04`), HEAD `64f64480`

## Description

The audit skill already names `boot.rs` "the single scheduler-registration wall"; the
measurable form of that is one **818-LOC** function listing every `add_to_with_access` /
`add_exclusive` call in stage order, plus the three release-level `assert_eq!`
access-report guards at its tail, inside a 1797-production-LOC `boot.rs`.

Not urgent — a flat registration list is legitimately linear and the access-report
assertions make mistakes loud at boot. But at 818 LOC it is the **largest non-renderer
function in the workspace**, and a stage-ordering mistake inside it is reviewed by eye.

## Suggested Fix

One `register_<stage>_systems(&mut scheduler)` per `Stage`
(`Early` / `Update` / `PostUpdate` / `Physics` / `Late`), with `build_scheduler` reduced
to five calls plus the guard block — which also makes the stage-ordering test in
`crates/core/src/ecs/scheduler.rs` map 1:1 onto five reviewable functions.

Effort: small (mechanical, `sed`-extract per stage; diff-check because `cargo fmt`
reformats the whole crate).

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix — the three release `assert_eq!` access-report guards must still fire
