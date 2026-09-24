# #4825 — GAME-D4-2026-09-24-04: Docs and the save allowlist still say combat is `StartCombat`-only

**Labels**: low,gameplay,combat,documentation,doc-rot
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: LOW
- **Dimension**: 4 — Doc drift from #4414
- **Location**:
  - `byroredux/src/npc_spawn.rs:189-190`: "Creatures still start no combat of their own — there is no ambient AI aggro".
  - `crates/scripting/src/combat.rs:19`: `AiCombatState` is "forced into combat … by `Effect::StartCombat`".
  - `byroredux/src/save_io/registry_completeness_tests.rs:613`: the `AiCombatState` rationale.
- **Status**: NEW
- **Description**: `faction_hostility_system` is now a second producer. After a load it re-creates ambient combat within about 0.5 s when the pair is still in range and in sight. Only scripted combat against a non-hostile target is lost.
- **Suggested Fix**: name the second producer in all three places, and state the re-derivation in the allowlist row.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
