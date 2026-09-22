# GAME-D2-2026-09-21-03: Disable()d doors and containers stay interactive — an invisible working door and a lootable invisible chest

**Issue**: #4698
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: 2 — Interaction
**Location**: `byroredux/src/cell_loader/spawn.rs` (`spawn_placement_root` stamps teleport/lock before the disabled-gate return); `byroredux/src/cell_loader/references/synth_child.rs` (`attach_container_inventory` unconditional); `byroredux/src/interaction.rs` (`populate_candidates`, `interaction_bound`)

## Description
#3278 skips meshes/colliders/lights for a disabled REFR but keeps the placement root with `DoorTeleport`/`Locked`. Interaction never re-checks disabled state, so a disabled door still teleports and a disabled container still loots.

## Evidence
`spawn_placement_root(…, teleport, lock)` runs before the disabled-gate return. `attach_container_inventory` has no `placement_disabled` filter (unlike the trigger/light branches a few lines away).

## Impact
A quest-disabled door still shows "[E] Open" and still transitions cells; a disabled container can still be looted.

## Related
#3278 (closed; this is the residual, not a regression); #4327.

## Suggested Fix
Skip `DoorTeleport` insertion and `attach_container_inventory` for disabled placements, or reject disabled candidates in `populate_candidates`.
