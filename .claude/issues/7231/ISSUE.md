# #7231: GAME-2026-11-22-08: Take-all over a source with zeroed rows publishes zero-count item transfers (phantom OnAdd/OnDrop) and a zero-count theft row

**Labels**: low,gameplay,inventory,bug

From `docs/audits/AUDIT_GAMEPLAY_2026-11-22.md` (report ID **GP-08**), audited at HEAD 2b40f76b6.

- **Severity**: LOW
- **Dimension**: 1 (inventory)
- **Domain**: inventory · **Game**: all
- **Trigger**: A corpse or container with an emptied row (rows are zeroed in place by `remove_items`, `RemoveItem`, an NPC's Find/Eat use, or a single-stack loot), taken with take-all while another row still holds items. Only visible when the emptied form carries an `OnAdd`/`OnDrop` ObScript block.
- **Location**: `byroredux/src/inventory/loot.rs:192-210` (`LootSelection::All`), feeding `:238-262` (`emit_item_transfers`) and `:277` (`crime::report_theft`)
- **Status**: NEW
- **Description**: The `Stack` arm filters `count > 0` (`loot.rs:216`); the `All` arm does not. A zeroed source row yields `(form, 0)` in `outcome.rows`, and `emit_item_transfers` pushes `ItemTransfer { count: 0 }` onto both sides' `ItemEventBatch`. `obscript_item_system` (`obscript_objects.rs:604-620`) runs `OnAdd` / `OnDrop` with no count check. `report_theft` notes a stolen count of 0, and `destination.add` appends a zero-count row to the player.
- **Evidence**: No `inventory::tests` case runs `transfer_loot` over a zeroed row; every fixture has positive counts.
- **Impact**: A scripted item can run its `OnAdd` again on the player at corpse loot, which can repeat quest-stage side effects.
- **Related**: #5871, #5268
- **Suggested Fix**: Skip `count == 0` stacks in the `All` arm. Add a `transfer_loot` test over one zeroed and one live row, asserting the batch holds only the live one.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other procedures / other transients / other inventory paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
