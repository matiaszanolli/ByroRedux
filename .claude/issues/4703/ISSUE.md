# GAME-D5-2026-09-21-02: StartCombat does not suspend the ambient package — two movers drive one actor, and a seated actor chases in its sit pose

**Issue**: #4703
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: 5 — Locomotion
**Location**: `byroredux/src/systems/combat_ai.rs` (Update-stage chase); the seven PostUpdate ambient movers e.g. `systems/wander.rs`; `systems/sandbox.rs`; `systems/walk_anim.rs` (Seated gate)

## Description
None of the seven ambient movers reads `AiCombatState`, so an actor's Wander/Travel/Follow/Escort/Guard/Patrol/seat mover keeps moving it in PostUpdate even while `npc_combat_ai_system` also moves it in Update. `sandbox_seat_system` never re-snaps; `walk_anim` refuses to take over `Seated` actors.

## Evidence
`rg AiCombatState byroredux/src/systems` finds matches only in `combat_ai.rs`.

## Impact
Actors in scripted combat jitter between chase and package leg, or chase while seated.

## Related
GAME-D4-2026-09-21-01 (#4693, same lack-of-awareness pattern); #4414.

## Suggested Fix
Have movers skip `AiCombatState` actors, or have `StartCombat` clear ambient behavior and let re-selection happen when combat ends. Unseat on combat start.
