# #6252: FNV-2026-10-21-02: FO3/FNV authored corpse poses (`XRGD`) are dropped at two layers; 276 dead-base FNV `ACHR` placements collapse from their idle pose

**Labels**: medium,bug,game:fnv,game:fo3,legacy-compat,gameplay,physics

**Source**: docs/audits/AUDIT_FNV_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: 2 / 4 (ESM slice → spawn → ragdoll). **Owner of the mechanism**: `/audit-gameplay` (corpse spawn), `/audit-physics` (ragdoll). · **Location**: `crates/plugin/src/esm/cell/walkers.rs:990-992` (`starts_dead` is set only for `Tes5Plus` `ACHR`); `byroredux/src/cell_loader/references/mod.rs:775-782` (the pose is parked only when `placed_ref.starts_dead`, so the FO3/FNV `npc.starts_dead` route never parks it); `byroredux/src/ragdoll_pose.rs:602-620` (`pose_skeleton` accepts only `is_skyrim_posable_ragdoll` / `is_fo4_posable_ragdoll`).

- **Severity**: MEDIUM
- **Dimension**: 2 / 4 (ESM slice → spawn → ragdoll). **Owner of the mechanism**: `/audit-gameplay` (corpse spawn), `/audit-physics` (ragdoll).
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:990-992` (`starts_dead` is set only for `Tes5Plus` `ACHR`); `byroredux/src/cell_loader/references/mod.rs:775-782` (the pose is parked only when `placed_ref.starts_dead`, so the FO3/FNV `npc.starts_dead` route never parks it); `byroredux/src/ragdoll_pose.rs:602-620` (`pose_skeleton` accepts only `is_skyrim_posable_ragdoll` / `is_fo4_posable_ragdoll`).
- **Status**: NEW. The 10-19 report (FNV-SD-05) filed it under #5015. #5015 has since closed on its Skyrim/FO4 scope (`18b3d87a7`, `e1bd40bcf`, `dfbcf7e2c`), and the skill text says FO3/FNV `XRGD` posing has no tracking issue. Related: #6065 (FO4 creature rigs), #5005 (the FO3/FNV corpse marker is base health, so `XRGD` is pose only).
- **Description**: On FO3/FNV a corpse is recognised from the base record (`DATA` health ≤ 0 → `NpcRecord::starts_dead`), never from the placement flag. The `XRGD` stamp in `load_references_budgeted` is keyed on the placement flag, so `AuthoredRagdollPose` is never inserted for an FNV corpse. Even if it were, `pose_skeleton` would refuse the FNV humanoid (`Bip01 …`) body set, because no FO3/FNV entry order has been established.
- **Evidence**: census over `FalloutNV.esm` (`/tmp/audit/fnv/probe`, 28-byte entries, base health from `NPC_ DATA` offset 0): **278 `ACHR` carry an 18-entry `XRGD`, 276 of them on a base with health ≤ 0** (the other 2 are posed live actors, which must not be stamped). About 107 `ACRE` carry `XRGD` with 10–37 entries, nearly all on dead bases. The 18 matches the 18-body FNV humanoid ragdoll (`fnv_humanoid_skeleton_threads_ragdoll`, green).
- **Impact**: Every authored FNV corpse (Legion crucifixions, staged massacre scenes, quest bodies) ragdoll-collapses from the idle pose at its placement, instead of lying where the level designer posed it. FO3 shares both layers; it is not measured here.
- **Suggested Fix**: Park the pose on the FO3/FNV route too, gated on `npc.starts_dead && !ragdoll_pose.is_empty()`. Establish the FO3/FNV humanoid entry order the way #6068 did for Skyrim: compare the median per-bone offsets of the 276 poses against `skeleton.nif`'s `bhkRigidBody` order. Then add an `is_fallout_posable_ragdoll` arm. Leave creatures refused until each rig is verified.

**Publish note**: #5015 (closed) covered only Skyrim/FO4 `XRGD`; this is the FO3/FNV scope it never had, not a regression. #6065 tracks FO4 creature rigs only.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_FNV_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
