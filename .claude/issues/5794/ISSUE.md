# TD2-2026-10-15-03: The runtime and pre-baked NPC spawn arms each carry a ~100-line `Finalize` tail; #5312 shared the Armor/Skeleton units but not this, and #4700 was the Draugr marker missing from one copy

Labels: medium,gameplay,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-03`).

- **Severity**: MEDIUM
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/npc_spawn/resumable/prebaked.rs:354-449` (`finalize_prebaked`) vs `runtime.rs:797-936` (`RuntimePhase::Finalize`); also the armor-phase arm `prebaked.rs:318-348` vs `runtime.rs:765-795`
- **Status**: NEW (partial: #5312 "share the Armor and Skeleton spawn units" closed the unit bodies, not the phase glue or finalize)
- **Age**: `ec3a18d2f` (2026-09-21, Draugr marker on runtime path only) -> `3978b5184` (#4700, 2026-09-23, prebaked catch-up); `774560dce` walk speed; `37db35cca` (#5091, 2026-10-01) split the two arms into files and froze the copy
- **Effort**: small
- **Description**: both tails insert `AnimationTarget`, `NpcSkeletonBones(skel_map.clone())`, the `DraugrCombatAnim` marker, the `WalkAnimation` + `WalkSpeed` pair (identical 20 lines: registry duration, `last_pos` from `Transform`, `captured: None`, `transition_secs: 0.0`), the `ResolvedNpc::resolve` + `apply_ai_package_behavior` + `arm_combat_disposition` triple, `tag_descendants_as_actor`, and `loot_appearance::install` with `mem::take(&mut state.appearance)`. The armor-phase arm (call `spawn_armor_unit`, the 8-argument call, the identical `log::debug!` "armor ... not in archives", the `next = index + 1` transition) is a second 30-line twin; the prebaked comment even says "(see the runtime Finalize twin)".
- **Evidence**: the Draugr combat marker landed in `ec3a18d2f` on the runtime arm only; `3978b5184` (#4700) states the prebaked path "stayed silent on Skyrim, the only game with Draugr" until two days later, and its inserted comment reads "beside `AnimationTarget` as on the runtime path".
- **Impact**: every new per-actor component (the M42 behaviors, M47.2 behavior-graph state, the next AI marker) is a two-arm edit in spawn glue; the prebaked arm is the Skyrim+/FO4 path, so a miss shows on exactly the newest games.
- **Related**: #5312, #5091, #4700, #5535 (touched both in a lock-order fix).
- **Suggested Fix**: in `resumable/mod.rs` add `fn attach_skeleton_runtime(world, root, skeleton, skel_map, draugr: bool)`, `fn attach_walk_animation(world, root, handle)` and `fn finish_actor(world, root, npc, index, player_body, appearance, skel_map) -> UnitOutcome`; both finalize arms keep only their path-specific head (inventory/equipment slots, idle KF, walk-clip selection) and call the three helpers. A shared `advance_armor_phase(state, index, ...) -> Option<next_index>` removes the armor twin.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
