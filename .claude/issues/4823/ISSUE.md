# #4823 — GAME-D7-2026-09-24-04: Save-classification guard drift — the hand-written `NPC_SPAWN_STAMPED` list omits today's spawn stamps, and the `NavmeshResidency` rationale is inaccurate

**Labels**: low,gameplay,save-load,test-gap,bug
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: LOW
- **Dimension**: 7 — Test gap / doc
- **Location**: `byroredux/src/save_io/round_trip_tests.rs:1417-1435`; `byroredux/src/save_io/registry_completeness_tests.rs:708`
- **Status**: NEW
- **Description**:
  - (a) `NPC_SPAWN_STAMPED` omits the two new stamps in `spawn_placement_root` (`resumable.rs:2089-2091`): `CombatDisposition` (#4414) and `SpellList` (#4415). The general guard classifies both, but this guard's check for a new runtime mutator doesn't run on them.
  - (b) The `NavmeshResidency` row says the counter "restarts at 0 after a load". An in-session load keeps the world, so the counter keeps incrementing. The conclusion (any cached `NavPath` is stale) still holds.
- **Suggested Fix**: add the two names and reword (b). The `AiCombatState` row is covered by GAME-D4-2026-09-24-04.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
