# #4816 — GAME-D4-2026-09-24-02: Combat never disengages — a combatant chases a live target forever and its ambient package stays suspended

**Labels**: medium,gameplay,combat,ai,bug
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: MEDIUM
- **Dimension**: 4 — Combat & Death
- **Location**:
  - `byroredux/src/systems/combat_ai.rs:122-135`: the only drop is a dead target or a target with no `Transform`; the drop is applied at `:302`.
  - `byroredux/src/npc_spawn/ai_package.rs:684`: combatants are skipped.
  - No `StopCombat` consumer exists in `crates/scripting` or `byroredux/src`.
- **Status**: NEW (a consequence of #4414)
- **Description**:
  - `faction_hostility` starts combat behind a range and line-of-sight gate, but nothing ends it on range, lost sight or a timeout.
  - The chase is a straight line with no navmesh and no stuck-repick, so a blocked chaser grinds against geometry.
  - #4703 resumes the package only when `AiCombatState` is gone, so while the target lives the package never resumes: no Sandbox, no seat, no schedule.
- **Impact**: an actor that glimpses the player at exterior range (FNV/Skyrim 2,500 × 2.0–2.1 BU) follows them for the rest of the session. Guards and townsfolk engaged by a creature that runs off never return to their packages.
- **Trigger**: FNV or Skyrim exterior; let an Aggressive NPC see you, then walk out of range.
- **Related**: #4414, #4703; GAME-D5-2026-09-24-03.
- **Suggested Fix**: re-check range and line of sight on the existing 0.5 s hostility cadence, and drop ambient-started `AiCombatState` after a grace period. Add a `StopCombat` lowering.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
