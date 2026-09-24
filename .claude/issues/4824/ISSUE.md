# #4824 — GAME-D5-2026-09-24-03: A combatant's ambient runtime is "due" every frame — its candidate list is cloned every frame of a fight

**Labels**: low,gameplay,ai,performance,bug
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: LOW
- **Dimension**: 5 — Package selection cost
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:522` (suspension sets `last_evaluated_game_minute = None`), `:657` (the due gate), `:672` (the clone), `:684` (the combat `continue` skips the minute stamp)
- **Status**: NEW (#4703 interacting with #4414)
- **Description**: the #3353 "one evaluation per in-game minute" cost model does not hold for combatants. Each frame pays a `package_candidates` clone plus `Dead` and `AiCombatState` lookups per combatant. Ambient hostility makes this common, and GAME-D4-2026-09-24-02 makes it last indefinitely.
- **Suggested Fix**: stamp the current minute for skipped combatants, or filter combatants out before the clone.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
