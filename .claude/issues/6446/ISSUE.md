# ECS-2026-10-27-04: GearImportLoader.failed_forms keeps despawned wearers for the session

Labels: low, ecs, inventory, bug
Source: docs/audits/AUDIT_ECS_2026-10-27.md

**Source**: `docs/audits/AUDIT_ECS_2026-10-27.md` finding `ECS-2026-10-27-04` (Dimension 4, resources and world-level state) · Severity LOW · Status NEW (ec88863a3, #5721 follow-up).

**Location**: `byroredux/src/npc_spawn/loot_appearance.rs:1045` (`failed_forms` field), `:1058-1085` (`hold_out` / `failed_set`, the only pruning).

## Description
`GearImportLoader.failed_forms` is an `EntityId`-keyed map (`HashMap<EntityId, Vec<u32>>`). A wearer's failed-import forms are pruned against its worn slots only inside `failed_set`, and that runs only when the same wearer hits another failed import (or is reconciled). An NPC that streams out with a still-worn failed piece keeps its `Vec<u32>` entry until process exit, because entity ids are never recycled. Nothing on the release path (`step_releases`) or the cell unload path touches the map.

## Evidence
```rust
failed_forms: std::collections::HashMap<EntityId, Vec<u32>>,
...
fn failed_set(&mut self, world: &World, wearer: EntityId) -> Vec<u32> {
    match equipped_forms(world, wearer) {
        Some(worn) => { ... entry.retain(|form| worn.contains(form)); ... }
        None => { self.failed_forms.remove(&wearer); Vec::new() }   // only when called for this wearer
```
Callers of `hold_out` / `failed_set` are at the import-failure and reconcile sites only (`loot_appearance.rs:1179,1200,1243,1321`).

## Impact
One entry per NPC spawned with a gear mesh that every archive is missing. On a modded load order with missing archives this can be most NPCs over a long streaming session. The memory cost is small. The finding is the pattern the ECS audit names: a per-entity fact held in a global `EntityId` map that despawn cannot reclaim, same shape as closed #6292.

## Related
#6292 (closed), #5721.

## Suggested Fix
Retain the map against a live storage (e.g. `EquipmentSlots` / `GlobalTransform`) once per `step`, or when that storage's `structural_generation` moves. Or drop a wearer's entry when its import queue releases it.

## Completeness Checks
- [ ] **SIBLING**: Other `EntityId`-keyed maps in `npc_spawn/` and `loot_appearance.rs` checked for despawn reclamation
- [ ] **LOCK_ORDER**: The retain pass takes only one storage guard at a time
- [ ] **TESTS**: A test spawns a wearer with a failed form, despawns it, steps the loader, and asserts the entry is gone

