# #5453: PHYS-D2-2026-10-08-01: The #5311 "move-only" world split dropped `CharacterMoveResult`'s `Debug`/`Clone`/`Copy` derive and two doc blocks, and left the step comment pointing at a `None` that moved into another function

**Labels**: low,physics,documentation,doc-rot
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5453

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-08.md` — `PHYS-D2-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: Step & Sync (doc and API hygiene)
- **Location**:
  - `crates/physics/src/world/queries.rs:12` (`pub struct CharacterMoveResult`, re-exported at
    `crates/physics/src/lib.rs:51`)
  - `crates/physics/src/world/recovery.rs:38-42` (`DynamicBodySnapshot`)
  - `crates/physics/src/world/mod.rs:761-764`
- **Status**: NEW. The derive and doc loss is a regression introduced by `12ca34a70` (#5311). The stale comment is
  pre-existing since #4685 (`"None" is passed`).
- **Description**: a line-multiset diff of `12ca34a70^:crates/physics/src/world.rs` against the three new files
  finds only these non-mechanical differences, besides `run_substep` and the re-pointed `include_str!`s:
  - Removed: `#[derive(Debug, Clone, Copy)]` and the "Result of a `move_character` step. Mirrors Rapier's
    `EffectiveCharacterMovement` … See M28.5" doc on the public `CharacterMoveResult`. The type is now not
    `Debug`, `Clone` or `Copy`.
  - Removed: `DynamicBodySnapshot`'s seven-line rationale. It recorded why the snapshot is per-substep, not
    per-frame ("a long catch-up frame must not roll a body back farther than the one solve that corrupted it").
    That invariant is load-bearing for the #4687 recovery design and now appears nowhere in the code.
  - Still present: the step rationale says the per-substep rebuild "is removed forty lines below (`None` is passed
    for the query pipeline)". Since #4685 the step passes `Some(&mut self.query_pipeline)`, and since #5311 that
    call is in `run_substep`, about 190 lines below. The guard
    `step_cost_rationale_is_scoped_to_history_and_names_the_real_cost_centre` does not check this sentence.
- **Impact**: none at runtime. A caller can no longer `{:?}`-log or copy a `CharacterMoveResult`, a public type in
  the controller API. The deleted snapshot rationale is the kind of doc an auditor re-derives from first principles
  each pass, and the stale `None` sentence contradicts the paragraph directly below it.
- **Related**: #5311 (closed), #4685, #4687.
- **Suggested Fix**: restore the derive and both doc blocks from `12ca34a70^`. Reword the stale sentence to "removed
  by `6e55b492`; today the step passes `Some(&mut self.query_pipeline)` (see `run_substep`)", and extend the
  existing rationale guard with `!rationale.contains("`None` is passed")`.

## Completeness Checks
- [ ] **SIBLING**: The rest of the #5311 split (`world/mod.rs`, `queries.rs`, `recovery.rs`) diffed against `12ca34a70^` for other dropped derives/docs
- [ ] **TESTS**: A regression test pins this specific fix
