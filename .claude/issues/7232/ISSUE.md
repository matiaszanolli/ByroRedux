# #7232: GAME-2026-11-22-09: `inv.equip` takes the first row of the form even when it is zeroed, so it refuses while a live row exists

**Labels**: low,gameplay,inventory,bug

From `docs/audits/AUDIT_GAMEPLAY_2026-11-22.md` (report ID **GP-09**), audited at HEAD 2b40f76b6.

- **Severity**: LOW
- **Dimension**: 1 (inventory)
- **Domain**: inventory · **Game**: all
- **Trigger**: The console or smoke command `inv.equip <form>` after the player's first row of that form was emptied while a later row (an instanced copy, or a re-add after instancing) still holds it.
- **Location**: `byroredux/src/inventory/mod.rs:112-119` (`queue_equip_by_form_id`)
- **Status**: NEW
- **Description**: This is the last hand-spelled "first row of form X" left in the inventory code. `Inventory::first_row_of` (#6965) requires `count > 0`; this site takes the first row of the form regardless of count. It queues `ToggleEquip`, which then refuses the zero row ("Can't equip …"), and the command still reports success.
- **Evidence**: `inv.use` (`mod.rs:155`) and `drain_script_weapon_equips` (`mod.rs:191`) both use `first_row_of`. The only test has a single positive row.
- **Impact**: Console and smoke scripts only.
- **Related**: #6965, #6363
- **Suggested Fix**: Use `inventory.first_row_of(form_id)`. Add a two-row test (zeroed, then live).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other procedures / other transients / other inventory paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
