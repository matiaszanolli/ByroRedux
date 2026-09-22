# GAME-D2-2026-09-21-02: Loose-item pickup has no player input path, and the pickup/tombstone flow has no test

**Issue**: #4697
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: 2 — Interaction
**Location**: `byroredux/src/interaction.rs` (`populate_candidates`, no pickup arm; `InteractionKind` has no `Pickup`); `byroredux/src/inventory.rs` (`pickup_loot`, `is_pickup_target`, `mark_picked_up`)

## Description
`populate_candidates` offers only loot sources, doors and a few scripted activators — no arm for a plain pickup target. `pickup_loot`/`is_pickup_target`/`mark_picked_up` have zero test callers and are only reachable via the `script.activate` console command.

## Evidence
`rg 'pickup_loot|is_pickup_target|mark_picked_up'` finds no test caller. `InteractionKind` has `Activate`/`Door`/`Container`/`Corpse` only.

## Impact
In every game, the player cannot pick up loose items through normal input. Limits (but doesn't eliminate) reachability of GAME-D7-2026-09-21-01's duplication bug.

## Related
#4571 (closed; cited this gap only as mitigation); GAME-D7-2026-09-21-01 (#4695); GAME-D7-2026-09-21-03 (#4116); #4464.

## Suggested Fix
Add a `Pickup` candidate kind with the same bound/occlusion/lock gates. Add an input-path test.
