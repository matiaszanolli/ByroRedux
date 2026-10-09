# #5468: TD1-2026-10-08-02: `crates/scripting/src/fragment/effects.rs` crossed 2000 production LOC (1940 → 2018)

**Labels**: low,scripting,tech-debt,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5468

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-08.md` — `TD1-2026-10-08-02` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/scripting/src/fragment/effects.rs` (2018 lines, all production; tests live in `fragment/tests.rs`)
- **Status**: NEW (first crossing; it was on the 10-05 watch list)
- **Age**: `59980e64e` (#5298, 10-05), `9813af435` (#4470/#4415, 10-06) and `f82b4a0de` (#5071, 10-06).
- **Effort**: medium
- **Description**: the file has three layers:
  - target resolvers (`resolve_quest*`, `resolve_object`, `resolve_actor`, `resolve_npc_actor`, …, lines 11–230);
  - the deferred-effect queue (`DeferredFragmentEffects` plus its two enums, the guard-free apply and the poll, 233–620);
  - one applier per effect family: global, inventory, placement, scene, lock, player-control, vehicle/cinematic,
    AI/combat and quest-scoped (713–1856), driven by `apply_effects` (1857).
- **Evidence**: `prod_loc` → 2018. On the baseline tree it was 1940.
- **Suggested Fix**:
  - Split into `fragment/effects/{resolve,deferred}.rs` plus `fragment/effects/apply_<family>.rs`.
  - Repoint the two source scans that read the file whole: `crates/scripting/src/fragment.rs:87` and
    `byroredux/src/boot/schedule/mod.rs:551`.

## Completeness Checks
- [ ] **SIBLING**: The two whole-file source scans (`crates/scripting/src/fragment.rs`, `byroredux/src/boot/schedule/mod.rs`) re-pointed to the new submodules
