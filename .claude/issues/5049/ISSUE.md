# #5049 — GAME-D7-2026-09-29-02: `NpcEquipmentPart.inventory_index` is written but never read; `0182fc5e8` adds a helper and a test to populate it

**Labels**: low,gameplay,inventory,tech-debt,bug

**Source report**: `docs/audits/AUDIT_GAMEPLAY_2026-09-29.md`
**Severity**: LOW
**Dimension**: 7 — Gameplay-state coverage

## Location
- `byroredux/src/npc_spawn.rs` — `NpcEquipmentPart.inventory_index`
- writers: `byroredux/src/npc_spawn/resumable.rs`, `byroredux/src/npc_spawn/loot_appearance.rs`
- `byroredux/src/npc_spawn/loot_appearance.rs` — `inventory_index_for` + its doc

## Description
No production code reads the field; hide/reveal and ownership both match on `form_id`. The new `inventory_index_for` claims to point "at the same row the equip events name", but `EquipmentChange` carries only `item_form_id`, and the helper takes the first same-base row, which may be a zeroed row or an instance row.

## Evidence
`grep -rn 'inventory_index' byroredux/src` — every read is on `EquippedWeapon` / `EquipmentSlots`; the only `inventory_index_for` callers are production writers and its own test.

## Impact
Dead state plus a misleading doc; a future consumer would read a possibly wrong row.

## Related
GAME-D1-2026-09-29-01 (#5031).

## Suggested Fix
Drop the field and the helper, or give the field the consumer its doc implies; if kept, resolve the row from `EquipmentSlots`, not the first same-base row.

Validated at HEAD 9fcfdc3fc: grep of `inventory_index` across `byroredux/src` finds no reader of `NpcEquipmentPart.inventory_index`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
