# GAME-D4-2026-09-21-03: A Dead player still attacks, opens doors, loots and changes equipment

**Issue**: #4701
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: 4 — Combat & Death
**Location**: `byroredux/src/combat.rs` (`combat_input_system`, no aggressor Dead check); `byroredux/src/interaction.rs` (`interaction_system`, `activate_target`); `byroredux/src/inventory.rs` (`container_loot_system`, `apply_action` ToggleEquip)

## Description
`consume_item` correctly refuses when the player is `Dead`, but `combat_input_system`, `activate_target`, `container_loot_system` and `apply_action`'s ToggleEquip branch never check the player's own `Dead` state.

## Evidence
`rg 'Dead' combat.rs` inside `combat_input_system` hits only target checks, never the aggressor. The other three entry points read in full — none checks player `Dead`.

## Impact
A dead player can keep attacking, looting, equipping and walking through doors with no game-over flow.

## Related
#3119 (closed; water death reconcile, different mechanism).

## Suggested Fix
Add one `player_can_act(world)` gate used by all four entry points.
