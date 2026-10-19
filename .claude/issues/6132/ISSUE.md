# PHYS-SD-01: The #4818 pickup-tombstone sweep despawns parented collision entities without detaching them, so a respawned FO4/Starfield picked-up item leaves a dangling `Children` entry and every later save is refused

Labels: high,physics,save-load,game:fo4,game:starfield,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6132

**Severity**: HIGH (every later save in the affected session is refused) · **Dimension**: Teardown (Dim 2 contract with the streaming restore path) · **Games**: FO4, Starfield (packed-Havok proxy); any game for a pickup that also carries a behaviour-graph node collider

## Location
`byroredux/src/cell_loader/reference_state.rs` (the #4818 pickup-tombstone sweep in `restore`, and `restore_resident`); parented producers at `byroredux/src/cell_loader/spawn.rs` (`spawn_packed_collision_proxy`: `Parent(placement_root)` + `add_child`; graph-node collision: `Parent(node_entity)` + `add_child`); the missing step lives privately at `byroredux/src/cell_loader/unload.rs` (`detach_victims_from_surviving_parents`)

## Trigger Conditions
FO4 or Starfield. A Clutter- or Actor-layer item whose NIF authors only an opaque packed-Havok body (`needs_packed_collision_fallback()`) is picked up; its cell then streams out (radius eviction or a door) and back in, or a save containing the pickup is loaded. Any game, for a pickup that also carries a behaviour-graph node collider.

## Description
On a respawn whose parked row says `picked_up`, `restore` removes the placement's collision entities with `collision_entities_of` (all entities tagged `PhysicsSourceForm` with the placement's FormId), then `release_victim_rapier_bodies` followed by a raw `world.despawn_batch(colliders)`. The comment at the sweep ("standalone entities, not descendants") and its unit test both assume those entities are never descendants. That holds for the bhk ghosts and the F3 trimesh ghosts, but not for the packed-Havok proxy (`Parent(placement_root)` plus `add_child`) nor for graph-node colliders (`Parent(node_entity)` plus `add_child`). `World::despawn_batch` removes component rows only and never edits another entity's `Children`; that is exactly why `release_entities_timed` calls `detach_victims_from_surviving_parents` before its own `despawn_batch` (#5310). The tombstone sweep skips that step, and the placement root survives (it stays as the hidden `PickedUp` placement), so its `Children` keeps the dead id.

## Evidence
`validate_hierarchy` (`crates/save/src/validate.rs`) reports `Children lists {child}, but its Parent != {parent}` for a child whose `Parent` row is gone. `SaveCommand::execute` aborts on any issue: `save ABORTED: N referential-integrity issue(s) - refusing to write a poisoned save` (`save_io/commands.rs`). The post-load diagnostic only logs (`live_load.rs`), so the first symptom is the refused save, not the load. Verified at publish time: the sweep calls `release_victim_rapier_bodies` then `despawn_batch(colliders)` with no detach; `spawn_packed_collision_proxy` inserts `Parent(placement_root)` and calls `add_child`. Static reading; no engine launch.

## Impact
After the pickup is restored, every quicksave / manual save is refused until the placement's cell is evicted again (a second eviction despawns the placement and its stale `Children` together). Because `restore_resident` re-runs the same sweep after a save load (#4695), loading a save that contains such a pickup poisons the session the same way. The duplicated `Children` entry also lingers in propagation and the child-reference budget walk, as #5310 documents. Item loot is still correct; the damage is the save gate.

## Related
#4818 (the sweep), #5310 (the same dangling-`Children` leak on the gear path), #5490, SAVE-D4-2026-10-09-01 (validate_hierarchy arm).

## Suggested Fix
Call `detach_victims_from_surviving_parents(world, &colliders)` (widen it to `pub(super)`) before the `despawn_batch` in the sweep, and extend the `save_reload_window_tombstone_is_applied_by_restore_resident` fixture with one parented collider plus a `validate_world` assertion (the current fixture's collider is unparented, so it cannot see this).

Source: docs/audits/AUDIT_PHYSICS_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
