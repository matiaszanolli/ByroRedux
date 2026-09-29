# #5034 — GAME-D1-2026-09-29-02: Worn meshes are never reconciled against loaded `EquipmentSlots` — after a load the player's body shows the pre-load session's gear (NPCs re-attach wearing the record outfit)

**Labels**: medium,gameplay,inventory,save-load,bug

**Source report**: `docs/audits/AUDIT_GAMEPLAY_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: 1 — Player body + mid-life gear

## Location
- `byroredux/src/player_body.rs` — `attach_player_body` (single, idempotent attach from `NPC_ 0x7`; early-returns when `PlayerBodyRootEntity` is set)
- `byroredux/src/save_io.rs` — post-`apply_deltas` reconcilers (only `EquippedWeapon` is re-derived)
- `byroredux/src/cell_loader/reference_state.rs` — `restore` (inserts `EquipmentSlots` only)

## Description
The player body is assembled once from `NPC_ 0x7`'s outfit and survives the reload untouched. The load overlays `EquipmentSlots` and re-derives only `EquippedWeapon`; it emits no `EquipmentEventBatch`. None of the visual state is saved, by design (`NpcAppearanceHidden`, `HiddenFirstPerson`, mid-life `NpcEquipmentPart` roots, `PendingGearImport`). So the body shows whatever the pre-load session last showed: saved mid-life equips draw no mesh; saved unequips of default outfit pieces still draw; after a boot `--load`, the `NPC_ 0x7` default outfit draws regardless of the saved slots.

NPCs have the same problem: `NpcSpawnJob` assembles `armor_to_spawn` from the record, then `reference_state::restore` overwrites `EquipmentSlots` with the parked row; nothing re-derives the meshes from the restored slots.

Extension (from the save audit): a player `PendingGearImport` in flight at load time (not saved, survives on the player) keeps attaching the pre-load session's gear.

## Evidence
Trigger — player: equip a never-worn item and/or unequip an outfit piece, save, change gear again, load (or boot with `--load`), switch to third person. NPC: script-unequip a spawn-time piece or mid-life-equip a new one, leave the cell, return (the parked row restores the slots).

## Impact
Third-person gear disagrees with inventory/equipment state after any load or cell return that follows a gear change. Combat, `GetEquipped` and the save itself are correct; only the visuals diverge.

## Related
GAME-D1-2026-09-29-01 (#5031); GAME-D7-2026-09-29-01 (#5027), SAVE-D1-2026-09-29-01 (#5052), SAVE-D5-2026-09-29-02 (#5056), SAVE-D1-2026-09-29-02 (#5058), SAVE-D5-2026-09-29-01 (#5054) (same save-load-teardown survival theme); #3488.

## Suggested Fix
After the overlay (player) and after `restore` (NPC), diff live `NpcEquipmentPart` roots against `EquipmentSlots`: hide roots whose form is no longer equipped, reveal roots whose form is equipped, queue a `PendingGearImport` for equipped forms with no root — reusing `equipment_appearance_system`'s paths via a synthesized change list. Clear a stale player `PendingGearImport` on load.

Validated at HEAD 9fcfdc3fc: `attach_player_body` early-returns once `PlayerBodyRootEntity` is set; the load drain's post-overlay block re-derives only `EquippedWeapon`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
