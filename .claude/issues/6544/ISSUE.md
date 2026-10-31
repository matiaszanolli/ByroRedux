# GAME-D1-2026-10-31-01: Form-keyed removal consumes an improved item ahead of a plain sibling when the improved row has the lower index

Labels: medium,gameplay,inventory,bug,game:skyrim

Filed from `docs/audits/AUDIT_GAMEPLAY_2026-10-31.md` (finding `GAME-D1-2026-10-31-01`).

- **Severity**: MEDIUM
- **Dimension**: Inventory & Equipment (Dim 1, M43 improving)
- **Location**: `crates/scripting/src/fragment/effects/inventory.rs:47-74` (`remove_items`); producers `byroredux/src/crafting.rs:394-398` (in-place instance), `crates/core/src/ecs/components/inventory.rs:123-135` (`Inventory::add`); callers `byroredux/src/barter.rs:427-446`, `DropObject`, scripted `RemoveItem`
- **Status**: NEW (`2c55546f4`, 2026-10-30)
- **Trigger**: Skyrim. The player holds one plain copy of an item, improves it at a grindstone, then acquires another plain copy, and then sells, drops or spends one copy by form.
- **Description**: `improve_item` improves a single-copy row in place, so that row now carries an `ItemInstance` (`(true, None)` arm). `Inventory::add` folds a plain stack into an existing plain row of the form and otherwise appends, so the later plain copy lands in a new row after the improved one. `remove_items(world, source, form, count, ...)` walks `items` in index order and takes from the first row of the form with `count > 0`. Selling "one Iron Sword" through `barter::apply_action` calls it by form only, and so do `DropObject` and a scripted `RemoveItem`. The improved row is first, so it goes: its instance moves to the merchant (or its `ItemInstancePool` entry is freed on a drop), and the plain copy stays. Barter prices a per-instance item "as its base", as documented, so the player also gets the unimproved price. When the improved copy is appended later (the split-off `(false, _)` arm) the plain row precedes it and is consumed first, so the loss only appears when the improved row is the lower index.
- **Evidence**:
  ```rust
  for (index, stack) in inventory.items.iter_mut().enumerate() {
      ...
      if stack.base_form_id != item_form_id || stack.count == 0 { continue; }
  ```
  There is no preference for an uninstanced row. `Inventory::add` gives an instanced row no plain twin to merge with.
- **Impact**: An improved weapon or armor piece is silently lost, with its quality, when its plain sibling is sold, dropped or removed by a script. The player cannot choose the copy: `BarterAction` carries only a form. This is not a crash and the loss is bounded, but it is irreversible and only appears once improved items exist, which began with M43.
- **Related**: #5871 (the add-fold rule), #5863 (`remove_items` unequip), barter doc "per-instance items are traded as their base".
- **Suggested Fix**: In `remove_items`, take plain rows before instanced ones (a two-pass walk over the matching rows), then instanced rows in descending quality or index. Add a case to the `crafting.rs` or `script_requests.rs` tests that improves one copy, adds a plain copy, removes one, and expects the improved one to survive.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games / siblings)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
