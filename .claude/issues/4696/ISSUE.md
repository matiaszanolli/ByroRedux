# GAME-D2-2026-09-21-01: NPC inventories expand leveled lists without LVLO counts or chance-none — a corpse yields ~1/4 of what the same list yields in a container

**Issue**: #4696
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: 2 — Loot Persistence / leveled loot
**Location**: `byroredux/src/npc_spawn.rs` (OTFT/CNTO via `expand_leveled_form_id`); `crates/plugin/src/equip.rs` (`expand_leveled_form_id` vs `expand_leveled_loot`); `byroredux/src/cell_loader/references/attach.rs`

## Description
`expand_leveled_form_id` (NPC/player path) drops nested LVLO counts and ignores `chance_none`; `expand_leveled_loot` (container path) multiplies nested counts and respects `chance_none >= 100`. Same LVLI, different yield depending on whether it sits on an actor or in a chest.

## Evidence
Engine-parser census across 5 masters: FNV 25,708 items/corpses vs 105,905/containers; Skyrim SE 35,932 vs 152,180; similar ratios on FO3/FO4/Oblivion.

## Impact
Corpses in every game carry roughly a quarter of the authored ammo/gold/consumables a leveled list would yield in a container.

## Related
#4248 (attach_container_inventory LVLI gap — stale/fixed, separate comment posted); ESM-2026-09-21-D2-01 (#4638, Oblivion LVLO/LVLD decode, same consumers).

## Suggested Fix
Build NPC/player inventory rows from `expand_leveled_loot`'s `(form_id, count)` pairs. Keep `expand_leveled_form_id` for worn-gear picking only. Add a corpse-equals-container test.
