# #7343: GP-07: Dropping an improved (instanced) item silently destroys its quality

- **Severity**: LOW
- **Dimension**: 1 (Inventory & equipment, ItemInstancePool)
- **Location**: `byroredux/src/inventory/actions.rs:296-306` (`drop_item`), `:311` (`create_scripted`)
- **Status**: NEW
- **Trigger**: Skyrim; `crafting::improve_item` at a grindstone/armor table (the improved copy gets `ItemInstance { quality }`), then `inv.drop` or the menu's Drop on that row, then pick it back up.
- **Description**: `drop_item` allows an instanced row (it refuses only an *unresolvable* instance), decrements the count, and at 0 does `row.instance.take()` plus `ItemInstancePool::release`. The created reference it spawns (`create_scripted(reference, form_id, PLAYER_REFERENCE, false)`) carries only the base form id; there is no instance field on a created reference, and `take_placed_item` adds `ItemStack::new(base, count)`. The tempered armor or weapon comes back plain. Barter documents the same rule ("per-instance items are traded as their base", `barter.rs:41`), but nothing documents or tests it for drop.
- **Evidence**:
  ```rust
  let released = { let row = &mut inventory.items[index as usize]; row.count -= 1;
      if row.count == 0 { row.instance.take() } else { None } };
  if let Some(instance) = released { world.resource_mut::<ItemInstancePool>().release(instance); }
  ```
- **Impact**: silent loss of a crafted upgrade on drop-and-repickup. Low: Skyrim only, player-initiated.
- **Related**: #6544 (removal order for improved copies), M43 crafting.
- **Suggested Fix**: refuse the drop for an instanced row until a created reference can carry an instance, or document "dropped items revert to base" beside the barter note and pin it with a test.

---
- **Dimension agent ID**: GP-D1-01

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other action systems / movers / park paths / add paths named in the finding)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the system's `Access` row matches
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_GAMEPLAY_2026-11-23.md`
