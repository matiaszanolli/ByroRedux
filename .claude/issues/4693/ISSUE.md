# GAME-D4-2026-09-21-01: A dead combatant's corpse root is written to world origin by npc_combat_ai_system, and the zeroed Transform is saved

**Issue**: #4693
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: HIGH
**Dimension**: 4 — Combat & Death Pipeline
**Location**: `byroredux/src/systems/combat_ai.rs` (dead-attacker decision + unconditional translation write); `byroredux/src/combat.rs` (`reconcile_dead_actor`); `byroredux/src/npc_spawn/ai_package.rs` (`clear_ambient_behavior`)

## Description
When an `AiCombatState` actor is `Dead`, the read pass pushes a decision with `new_translation: Vec3::ZERO`. The write pass then assigns `transform.translation = decision.new_translation` for every decision, including that one. `AiCombatState` is not part of the death teardown anywhere else (`reconcile_dead_actor` / `clear_ambient_behavior` never reference it — grep-confirmed). Each scripted-combat actor that dies reaches this branch exactly once, on the next frame, and its placement root is moved to world origin.

## Evidence
`combat_ai.rs`'s dead-attacker branch still pushes `Vec3::ZERO`; the write pass still applies it unconditionally. `Transform` is a `MUTABLE_DELTA_COLUMNS` entry (`save_io.rs:84-89`). The existing test `clears_combat_state_when_attacker_dies` spawns the attacker at `Vec3::ZERO`, masking the wrong write.

## Impact
Within the session the root's `GlobalTransform`/`WorldBound` jump to the origin. A save taken after the kill stores the corpse root at `(0,0,0)`; on load the corpse respawns at/near the origin instead of where it fell.

## Related
GAME-D5-2026-09-21-02 (#4694, same actor also driven by its package); #4605 (closed, same system's lock-hold-stack fix, not a regression); #3708 (precedent that grew the death teardown).

## Suggested Fix
Remove `AiCombatState` in `reconcile_dead_actor`. In `npc_combat_ai_system`, carry the actor's current translation forward (or skip the write) for `state: None` decisions. Update the dead-attacker test to start away from the origin.
