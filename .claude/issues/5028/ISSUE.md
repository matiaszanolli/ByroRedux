# REN-D5-2026-09-29-02: the player's mid-life gear imports are never released — each distinct item ever equipped stays resident until shutdown

**Labels**: low,bug,renderer,memory,gameplay,inventory

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-29.md`
**Severity**: LOW
**Dimension**: Memory/Lifecycle (owner overlaps `/audit-gameplay`)
**Location**: `GearImportLoader::step` (`byroredux/src/npc_spawn/loot_appearance.rs`). Its doc says: "the player has no `CellRoot` — their gear outlives cells exactly like the body it hangs from".

## Description
- Equipping an item the actor did not spawn wearing imports its worn NIF under the player body root (`0182fc5e8`).
- Unequipping hides that root (`NpcAppearanceHidden`), and re-equipping reveals it.
- The only path that despawns an `NpcEquipmentPart` root is cell teardown (`stamp_cell_root_range` → `CellRootIndex` → `unload_cell`), and the player never gets one. A failed player import likewise leaves its hidden partial range in place.

## Impact
For every distinct armour/clothing piece equipped in a session, its geometry (global and per-mesh buffers, plus its BLAS once drawn) and texture refcounts stay resident until shutdown. Dropping, selling or destroying the item frees nothing. Growth is bounded by the item catalogue and happens per user action, not per frame; not measured.

## Related
`0182fc5e8` (mid-life gear import); `/audit-gameplay` owns the equip path.

## Suggested Fix
When the imported item's form leaves the inventory, release its root through the same entity-despawn and GPU-handle release path that cell teardown uses. Alternatively, cap hidden gear roots with an LRU.

Validated at HEAD 9fcfdc3fc: `GearImportLoader` doc states player gear outlives cells; unequip only toggles `NpcAppearanceHidden`; no non-cell release path for player gear roots found.

## Completeness Checks
- [ ] **SIBLING**: NPC mid-life gear imports (cell-owned) still released via the cell range
- [ ] **DROP**: GPU handles (mesh, BLAS, textures) released through the existing deferred-destroy path, not immediately
- [ ] **TESTS**: A regression test pins release of an imported gear root when its item leaves the inventory
