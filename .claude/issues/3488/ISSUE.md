# Issue #3488 — SAVE-D1-2026-08-27-01: `EquippedWeapon` is removed at runtime with no reconciler, so the additive-only live overlay cannot clear it from the process-lifetime player body

Source audit: `docs/audits/AUDIT_SAVE_2026-08-27.md`
Filed: 2026-08-27 (HEAD `969d81c8`)
Labels: high, save-load, inventory, combat, bug

---

Audit: `docs/audits/AUDIT_SAVE_2026-08-27.md` (SAVE-D1-2026-08-27-01)
Severity: **HIGH** · Dimension 1 — Snapshot Completeness & Determinism (removal semantics); manifests in Dimension 6 — M45.1 Live Load-Apply
Data-Loss Class: silent-drop → corruption-on-load

## Location
- `byroredux/src/inventory.rs:493` — the removal (`reconcile_equipped_weapon`'s `else` arm)
- `crates/save/src/driver.rs:310-317` — the additive-only contract it violates
- `crates/save/src/registry.rs:144-149` — the silent `filter_map` that makes it invisible
- `byroredux/src/save_io.rs:102` — `"EquippedWeapon"` in `MUTABLE_DELTA_COLUMNS`
- `byroredux/src/save_io.rs:1367-1384` — the one-shot `build_form_id_remap` + `apply_deltas` + `reconcile_dead_actor_runtime_state` tail; no equipment reconciler
- `byroredux/src/combat.rs:414-437` / `:450` (`reconcile_dead_actor_runtime_state`) — the contract-compliant counterexample

## Description
`apply_deltas`' own doc comment states the contract plainly:

```rust
/// This overlay is **additive-only** — it can update or insert a row via
/// `ApplyFn`, never remove one. Runtime removals that are consequences of a
/// persisted fact must therefore be rebuilt by the binary after this call.
/// Death uses that model: `Dead` is overlaid here, then the shared combat
/// reconciler removes respawned AI/animation state and reactivates ragdoll
/// (#3022). Future disable/delete persistence needs the same explicit
/// marker-plus-reconciler contract […]
```
(`crates/save/src/driver.rs:310-317`)

The P2 native inventory menu now performs exactly such a runtime removal. Unequipping the wielded weapon routes `apply_action` → `reconcile_equipped_weapon`, whose `else` arm is:

```rust
if let Some(weapon) = candidate {
    world.insert(player, weapon);
} else {
    let _ = world.remove::<EquippedWeapon>(player);
}
```
(`byroredux/src/inventory.rs:489-494`)

This is production-reachable, not a debug path: `main.rs:813-817` drives it straight off the pause-menu overlay's `outputs.inventory_actions`.

Three facts combine into the bug:

1. **The player body outlives the reload.** `unload_cell_inner` draws its victim set from `CellRootIndex` (`byroredux/src/cell_loader/unload.rs:139-142`); the player body is spawned in `scene::setup_scene` and never stamped with a `CellRoot` — the SAVE-D1-12 allowlist says so itself (*"`PlayerEntity`: points to the process-lifetime player body, which deliberately outlives cell unload; the entity remains valid across live reload"*). On a live load the player body keeps every component the current session gave it.
2. **A removed component leaves no trace in the snapshot.** `save_world` serialises the live `EquippedWeapon` column; an unarmed player simply has no row (and if no NPC has one either, the column is omitted entirely — `driver.rs:37-45`).
3. **`apply_deltas` cannot express a removal.** The component `ApplyFn` `filter_map`s saved rows through the remap and calls `insert_batch`; a saved id with no row is a no-op, and a *live* row with no saved counterpart is never touched (`crates/save/src/registry.rs:144-149`).

Net: quicksave while unarmed, equip a weapon, quickload that save — the player is still holding the weapon. Worse, the *sibling* columns **do** overlay: `Inventory` and `EquipmentSlots` are both in `MUTABLE_DELTA_COLUMNS` (`save_io.rs:86-87`), so the restored `EquipmentSlots.weapon == None` now contradicts the surviving `EquippedWeapon`, whose `inventory_index` indexes an inventory that was just wholesale-replaced.

## Evidence
- `byroredux/src/inventory.rs:493` — the only `world.remove::<EquippedWeapon>` site in the tree (verified by `grep -rn "remove::<EquippedWeapon>" byroredux/src crates/`).
- `byroredux/src/main.rs:813-817` — the production driver: `for action in outputs.inventory_actions { if inventory::apply_action(world, action) == …`.
- `byroredux/src/save_io.rs:1367-1384` — the entire post-`apply_deltas` tail is `reconcile_dead_actor_runtime_state(world)` and nothing else; `grep -rn "reconcile_equipped_weapon" byroredux/src` returns exactly one call site, inside `apply_action`, never from the load drain.
- `combat.rs:414-419` documents the correct pattern for the analogous case (*"Live-load deltas are intentionally additive, so absence of AI and animation components is not serialized as a second, generic tombstone format. Both the combat transition and save-load drain call this one reconciler"*). `EquippedWeapon` has no such shared reconciler.
- The gates do not catch it: `validate_equipment` (`crates/save/src/validate.rs:196-240`) checks `EquippedWeapon.inventory_index` bounds and its `base_form_id` against `inventory[index]`, but a stale weapon whose index is in range and whose form id matches the restored item at that index passes cleanly — and the whole post-load pass is diagnostic only (`log_validation_warnings`, no abort).
- No test covers the direction: `crates/save/tests/round_trip.rs:333-382` (`player_body_inventory_survives_live_load`) proves only that an *added* `Inventory` overlays onto the surviving player; the removal direction is untested for every column.

## Impact
Fires on the ordinary path — equip/unequip through the pause menu is the P2 slice's headline interaction, and quickload is a one-keystroke action. The player's wielded state silently desynchronises from the save they just loaded, and because `combat.rs` reads `EquippedWeapon` (not `EquipmentSlots.weapon`) to resolve melee damage, the reloaded session deals the *current* session's weapon damage rather than the saved unarmed damage — a gameplay-visible divergence with no log line anywhere. Blast radius is one component today, but the defect is contract-shaped, not instance-shaped: every future `world.remove::<T>()` on a `MUTABLE_DELTA_COLUMNS` type inherits it, and `EquippedWeapon` is the first to land since the contract was written.

## Related
#1847 / SAVE-04 (the additive-only overlay contract); #3022 (`reconcile_dead_actor`, the model the contract names); #3112 / `a5ed4bf5`, which reshaped `reconcile_equipped_weapon` this cycle without noticing the save interaction; `.claude/commands/_audit-common.md`'s "Gameplay slice (P2) — **NO owner audit skill**" row, which predicted exactly this class of miss.

## Suggested Fix
Add an equipment reconciler alongside `reconcile_dead_actor_runtime_state` in `execute_pending_save_loads`'s tail that re-derives `EquippedWeapon` from the just-overlaid `EquipmentSlots.weapon` + `Inventory` — `inventory::reconcile_equipped_weapon` already *is* that function; call it for the player after `apply_deltas` succeeds (it removes the component when `EquipmentSlots.weapon` is `None`, which is precisely the missing behaviour). Extend `crates/save/tests/round_trip.rs` with the removal-direction case (`player_body_unequipped_weapon_survives_live_load`), and add a tripwire beside `delta_columns_carry_only_session_stable_fields` asserting that any `MUTABLE_DELTA_COLUMNS` type with a production `world.remove::<T>` site is named in an explicit reconciler allowlist.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every other `MUTABLE_DELTA_COLUMNS` type with a production `world.remove::<T>` site
- [ ] **LOCK_ORDER**: If a RwLock scope changes (the reconciler runs inside the load drain's wide lock surface), TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix (removal-direction live-load case)
