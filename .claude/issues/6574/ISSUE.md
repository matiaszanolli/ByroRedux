# SAVE-2026-10-31-02: Two more M43 transients escape the load sweep, and their allowlist reasons overstate it (`ActiveCrafting`, `PendingDroppedItems`)

Labels: low,save-load,bug

Source: `docs/audits/AUDIT_SAVE_2026-10-31.md` (SAVE-2026-10-31-02). Severity LOW. Dimension: Live Load-Apply (frame boundary) / Snapshot Completeness (allowlist reasons). Data-loss class: corruption-on-load, cosmetic (nothing is lost). The #6239 sweep-gap class, a sibling of open #6508.

**Location**:
- `byroredux/src/crafting.rs:431-442`
- `byroredux/src/inventory/actions.rs:312-343` (`PendingDroppedItems`, `dropped_item_story_event_system`)
- `byroredux/src/save_io/registry_completeness_tests.rs:572-574`
- `byroredux/src/save_io/live_load.rs:459-481` (the sweep)

## Description
1. **`ActiveCrafting`.** The allowlist row says "a menu session that a load closes". No code in the load path calls `end_crafting`; the only callers are `toggle_game_menu`, `open_inventory_menu` and `resume_from_game_menu` in `main.rs`. F5/F9 and the pause menu cannot load while the page is open, but the remote console `load` can (the same hole the `ActiveBarter` row already admits). After such a load the page stays open on the old bench `EntityId` (ids are not reclaimed); `recipes_at` finds no keywords for it, the page lists nothing, and `craft` answers "this bench does not make that". Nothing is lost but the reason is false. `CraftingRequest` ("drained the same frame") is taken in `render_one_frame` after the save/load drain of the same tick, so a bench activated that tick is still queued across a drain; its stale id is refused by `begin_crafting` (harmless).
2. **`PendingDroppedItems`.** Holds `(created-reference FormID, seconds waited)` for up to 10 s and is not in `discard_outgoing_session_requests`. After a load the outgoing timeline's pending ids stay queued; `CreatedReferences` is restored with its saved id allocator, so an id from the abandoned timeline can be handed out again, and a drop in the loaded session within the window can raise `REMP` twice for the same reference. The row's "only the not-yet-raised event is lost on a load" is the opposite of what happens.

## Impact
Cosmetic and very narrow. Filed so the next sweep fix covers these types, and because the generic guard proposed in the 10-21 and 10-27 audits (a source scan failing when a "drained"/"consumed" row is not discarded in the drain or justified as entity-keyed) would have caught both this and #6508.

## Suggested Fix
Call `end_crafting` and clear `CraftingRequest` in the load drain (`end_barter` is the same shape). Clear `PendingDroppedItems` in `discard_outgoing_session_requests`. Extend `live_load_drain_discards_the_outgoing_sessions_requests`. Land the generic guard.

Related: #6508.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
