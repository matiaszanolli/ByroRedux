# #4826 — GAME-D4-2026-09-24-05: `faction_hostility` tests don't pin the edges most likely to regress

**Labels**: low,gameplay,combat,test-gap,bug
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: LOW
- **Dimension**: 4 — Test gap
- **Location**: `byroredux/src/systems/faction_hostility.rs:399-589`. The four tests cover the aggression table, standing, range and throttle, and the missing GMST.
- **Status**: NEW
- **Description**: nothing covers:
  - a dead perceiver, dead target or dead player (the skip at `:258`);
  - keeping a scripted `StartCombat` target (`:328`, `:380`);
  - excluding the player capsule from line of sight;
  - the mid-spawn window (GAME-D4-2026-09-24-03).

  All of these are correct today except the spawn window.
- **Suggested Fix**: add one fixture test for each.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
