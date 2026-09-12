# FO4-D9-01: attach_container_inventory never expands leveled-list (LVLI) entries — placed containers with leveled loot get unresolvable phantom items

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4248

**Severity**: MEDIUM
**Dimension**: 9 — Real-Data Validation + Forward Scope
**Location**: `byroredux/src/cell_loader/references/attach.rs:196-220` (`attach_container_inventory`)
**Status**: NEW (searched open issues for LVLI/leveled/container-inventory keywords — no match; #1359 built this function but never added leveled-list expansion)

**Description**: `attach_container_inventory` pushes each `ContainerRecord.contents` entry's `item_form_id` directly into the ECS `Inventory` with no leveled-list expansion. Vanilla Bethesda CONT records commonly reference an `LVLI` form ID in place of a concrete item — the same pattern `npc_spawn.rs`'s `build_npc_equip_state` already handles correctly via `byroredux_plugin::equip::expand_leveled_form_id`. No equivalent call exists in the container path.

**Evidence**: Confirmed in current code — `attach.rs:213-220` pushes `entry.item_form_id` directly (`inventory.push(ItemStack::new(entry.item_form_id, runtime_count))`) with no leveled-list resolution, while `expand_leveled_form_id` (`crates/plugin/src/equip.rs:628`) is called only from `byroredux/src/inventory.rs:231,253` and `byroredux/src/npc_spawn.rs:914,943` — never from the container attach path. `LVLI` records index separately (`crates/plugin/src/esm/records/index.rs:564`, `index.leveled_items` vs. `index.items`); any downstream item-metadata lookup keyed by `index.items.get(&form_id)` on an unresolved LVLI form ID returns `None`.

**Impact**: Any placed container (FO4, and this function is game-agnostic so also FNV/FO3/Skyrim) whose loot table uses a leveled list — a common vanilla pattern for ammo boxes, footlockers, safes — spawns with at least one unresolvable phantom inventory entry instead of real looted items. Does not crash the engine; silently produces wrong loot contents across a potentially large fraction of the world's containers.

**Related**: Closed #1359 (built the base function); distinct from the already-closed NPC-side leveled-list bugs (#1180, #1182, #3069, #3217, #3341, #3356, #3365).

**Suggested Fix**: Route `entry.item_form_id` through `expand_leveled_form_id`, mirroring `build_npc_equip_state`. Open design question: containers have no natural `actor_level` the way NPCs do (loot tables conventionally roll against player level) — needs either the player's level threaded to the cell-loader attach path, or a documented default-level policy for cold cell-load before the player entity exists.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked against `inventory.rs`'s and `npc_spawn.rs`'s existing `expand_leveled_form_id` call sites for the correct calling convention
- [ ] **TESTS**: A regression test on a container with an `LVLI`-referencing CNTO entry asserts the spawned `Inventory` contains a resolved concrete item, not the raw LVLI form ID
