# FO3-2026-10-27-02: The FO3 leg of #6252 (XRGD corpse poses) has no real-data pin, and the module doc's FO3 census is not reproduced

Labels: low, bug, test-gap, game:fo3, legacy-compat, physics
Source: docs/audits/AUDIT_FO3_2026-10-27.md

**Source**: `docs/audits/AUDIT_FO3_2026-10-27.md` — finding `FO3-2026-10-27-02` (LOW, test-gap, Dim 5; mechanism `/audit-physics` `parks_authored_pose`, `/audit-character`)
**Location (current)**: `byroredux/src/ragdoll_pose.rs` (module doc, ~lines 165-181); `byroredux/src/ragdoll_pose_tests.rs` (`fnv_xrgd_corpse_poses_line_up_with_the_humanoid_ragdoll`, ~line 930, FNV only).

## Description
#6252 widened authored-pose (`XRGD`) posing to "the FO3/FNV humanoid rig", and the module doc asserts "202 FNV and 307 FO3 dead-base humanoid corpses (every one with 18 entries but three FO3 19-entry poses, which the count gate refuses)". Only FNV is pinned: the ignored real-data test loads `FalloutNV.esm` and asserts `(parked, posable) == (202, 202)`. No test loads `Fallout3.esm`, so an FO3-only regression (the 19-entry count gate, the `Bip01` rig check against FO3's skeleton) passes both CI and the `--ignored` lane. The documented FO3 number is also not reproduced anywhere in the tree.

## Evidence
Raw census of `Fallout3.esm` ACHR `XRGD` (28-byte entries) against NPC_ `DATA` base health <= 0 (the rule in `crates/plugin/src/esm/records/actor/npc.rs`): **335 eighteen-entry + 3 nineteen-entry** dead-base ACHR poses. The three 19-entry poses match the doc; the doc's 307 does not match 335.

## Impact
FO3's 300+ authored corpse poses (Megaton, Super-Duper Mart, Springvale ...) are guarded only by the shared FNV pin, and the doc number is unverifiable.

## Related
#6252 (closed), PHYS-D3-2026-10-27-02 (same `GameKind::Fallout3NV` branch in `parks_authored_pose`), FNV-2026-10-21-02 (closed leg), #6354 (real-data pin list).

## Suggested Fix
Add `fo3_xrgd_corpse_poses_line_up_with_the_humanoid_ragdoll` mirroring the FNV test against `Fallout3.esm` + its `skeleton.nif`, asserting the measured parked / posable / refused counts, and correct the module-doc number to what the test measures. Register it in the real-data pin list if one applies (#6354).

## Completeness Checks
- [ ] **SIBLING**: Other FNV-only real-data pins that should have an FO3 twin (the shared `GameKind::Fallout3NV` paths) are checked
- [ ] **TESTS**: The new test self-skips without game data and asserts under `BYROREDUX_REQUIRE_GAME_DATA=1`, like its FNV sibling

