# #7344: GP-08: Sibling of #7231, a scripted or console add of count 0 still adds a zero row and emits a zero-count `ItemTransfer`

- **Severity**: LOW
- **Dimension**: 1 (Inventory::add paths, item events)
- **Location**: `crates/scripting/src/fragment/effects/inventory.rs:299-335` (`add_items`), `byroredux/src/inventory/mod.rs:84-102` (`add_item`, console)
- **Status**: NEW
- **Trigger**: a Papyrus fragment `AddItem` whose `ItemCount` resolves to 0 (`resolve_item_count` accepts `0.0..=u32::MAX`, e.g. a `Global` or `Value` count that is 0), or the console `inv.add <form> 0`.
- **Description**: #7231 (2e535ba49) established that a zero-count transfer is phantom: it runs the item's `OnAdd`/`OnDrop`, and gives the receiver a zero-count row for a form it does not hold. `add_items` does the same on the add side: `Inventory::add(ItemStack::new(item, 0))` appends a (form, 0) row when the container has none, then `emit_item_transfers` publishes `ItemTransfer { count: 0, added: true }` unconditionally (it filters `count > 0` only for the challenge counter, not for the `ItemEventBatch`). `obscript_item_system` then runs `BLOCK_ON_ADD`. The ObScript caller clamps with `.max(1.0)` (`obscript_more/actions.rs:170`), so only the fragment and console doors are open.
- **Evidence**: `add_items` has no `count == 0` early return; `remove_items` does (`if removed == 0 { return; }`), so the two directions are asymmetric.
- **Impact**: a spurious item `OnAdd` (re-running scripted-item side effects) and a zero row when a script computes a 0 count. Low reach.
- **Related**: #7231.
- **Suggested Fix**: `if count == 0 { return; }` at the head of `add_items` (after the leveled-list expansion, which recurses per entry) and in `add_item`.

---
- **Dimension agent ID**: GP-D1-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other action systems / movers / park paths / add paths named in the finding)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the system's `Access` row matches
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_GAMEPLAY_2026-11-23.md`
