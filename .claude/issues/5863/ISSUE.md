# #5863: GAME-D1-2026-10-15-01: `remove_items` (barter, `RemoveItem`, `DropObject`, `GetCaps`) leaves `EquippedWeapon` stale and emits no unequip event

- **Labels**: medium,bug,gameplay,inventory
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Severity**: MEDIUM
- **Dimension**: Inventory & Equipment Model (Dim 1)
- **Location**: `crates/scripting/src/fragment/effects/inventory.rs:17-114` (slot release `:84-92`). Callers: `byroredux/src/barter.rs:438-482`, the `DropObject` native (`crates/scripting/src/fragment/vm.rs:3302-3325`), `GetCaps` (`crates/scripting/src/fragment/effects.rs:366-376`)
- **Status**: NEW (from `61ad95cf3` and the M47.2 item natives)
- **Trigger**: FNV/FO3/Skyrim. Sell the player's equipped weapon at a vendor; `grouped()` does not exclude equipped items. Or a script `RemoveItem` of an NPC's equipped weapon or armor.
- **Description**: Rows are zeroed in place and `EquipmentSlots::release` frees the slots, which is correct. But `EquippedWeapon` is not re-derived: only `byroredux/src/inventory.rs` calls `reconcile_equipped_weapon`. No `emit_equipment_changes` is sent either; the only emit in this file is in the equip path at `:234`. `transfer_loot` does both.
- **Impact**:
  - `attack_damage` (`combat.rs:818-820`) keeps the sold weapon's damage until a reload. The load-time reconcile covers the player only.
  - An NPC's removed armor stays visible for the session, because no `EquipmentEventBatch` is sent.
  - `OnObjectUnequipped` never fires.
  - The `loot_appearance.rs:282-293` doc calling the `added == false` release path "forward-latent" is now stale: barter sell, `GetCaps` and player `RemoveItem` reach it.
- **Suggested Fix**: Have `remove_items` collect the emptied equipped indices, emit the unequip changes and re-derive `EquippedWeapon`. That means moving `reconcile_equipped_weapon` (or its core) to where both crates can call it, rather than duplicating it. Test: barter-sell the equipped weapon, and assert `EquippedWeapon` is gone and an unequip event was emitted.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
