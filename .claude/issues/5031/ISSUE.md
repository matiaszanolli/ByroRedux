# #5031 — GAME-D1-2026-09-29-01: The mid-life gear queue keeps one root-less equip per wearer per batch and drops the rest — a second never-worn item (or one equipped while an import is pending) never gets its worn mesh

**Labels**: medium,gameplay,inventory,bug

**Source report**: `docs/audits/AUDIT_GAMEPLAY_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: 1 — Inventory & Equipment Model (P3 mid-life gear import)

## Location
- `byroredux/src/npc_spawn/loot_appearance.rs` — `queue_midlife_imports`: the per-wearer request collapse (`if !requests.iter().any(|(w, _)| w == wearer)`), the pending skip (`world.get::<PendingGearImport>(wearer).is_some() → continue`), the empty-path `continue`
- `byroredux/src/npc_spawn/loot_appearance.rs` — the multi-path drain in the gear-import loader

## Description
- Only the **first** root-less `equipped: true` change per wearer is kept; later changes in the same batch are discarded.
- A new request is discarded while an earlier import for that wearer is still draining.
- No retry: Late `event_cleanup_system` clears `EquipmentEventBatch` the same frame and nothing rescans `EquipmentSlots`.

Wider than first reported (routed from `/audit-ecs`):
- **Non-armor equips take the slot.** `resolve_armor_meshes` (`crates/plugin/src/equip.rs`) returns empty for non-`Armor` items. If a weapon equip comes first in the batch, the request is dropped at the empty-path `continue`, and an armor equipped later in the same batch is never queued.
- **Unequips are ignored by the queue.** For `[equip A, equip B that displaces A]`, A (now unequipped) is imported and drawn, B is dropped.
- **Multi-path (ARMA) imports are not re-checked mid-drain** — if unequipped mid-drain, remaining paths still attach visibly.

## Evidence
Change batches merge per wearer per frame (`crates/scripting/src/equipment.rs`), so multiple equips per wearer per batch happen on: a quest fragment running `EquipItem` twice; several queued `inv.equip` / menu toggles draining in one `drain_pending_inventory_actions`; one equip displacing another; or a second equip while a multi-ARMA Skyrim import drains at one NIF per frame.

## Impact
The item is equipped in `EquipmentSlots` but invisible in third person, or a displaced item is drawn instead, until the player unequips and re-equips by hand. The feature is actor-generic, so NPC scripted equips are affected too.

## Related
REN-D5-2026-09-29-02 (gear imports never released), PERF-D7-2026-09-29-02 (third archive set on the main thread), CONC-D4-2026-09-29-01 (#5069) (missing access declarations — a distinct finding on the same feature); GAME-D1-2026-09-29-02 (#5034).

## Suggested Fix
Queue a list per wearer (`Vec<(form_id, paths)>`) and append instead of skipping; resolve changes in batch order so an unequip cancels a queued equip of the same form; have the loader re-check `EquipmentSlots` before each path. Add tests for a two-item batch, weapon-then-armor, and equip-while-pending.

Validated at HEAD 9fcfdc3fc: `queue_midlife_imports` still collapses requests with `requests.iter().any(|(w, _)| w == wearer)` and skips wearers with a live `PendingGearImport`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
