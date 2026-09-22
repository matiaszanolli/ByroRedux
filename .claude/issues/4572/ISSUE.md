# ECS-2026-09-21-D3-01: the new `apply_placed_water_type` subtree walk skips both cycle guards, and three older walks have the same shape

**Labels**: low, ecs, bug

Filed via /audit-publish from docs/audits/AUDIT_ECS_2026-09-21.md.

**Severity**: LOW (defence in depth; no live code path builds a cycle) · **Dimension**: 3 (hierarchy walks: `HierarchyTraversalGuard` rule)
**Location**: `byroredux/src/cell_loader/water.rs` (`apply_placed_water_type`, ~:731-749; new in `ec9ed8524`, 2026-09-14). Siblings: `byroredux/src/ragdoll.rs` (`ragdoll_writeback_system`: the #1981 `mesh_walk_queue` walk ~:629-653 and the #1979 descendant re-derive ~:673-718, both run every frame while a ragdoll is active) and `byroredux/src/anim_convert.rs` (`build_subtree_name_map`, ~:21-54)
**Verified against**: HEAD `f97775ca8`

## Description

The audit-ecs rule reads: "A new hierarchy walk without it, or without a visited set, is an unbounded-loop hazard". #3700's fix bounded the three core walks with `HierarchyTraversalGuard`: `make_transform_propagation_system` in `crates/core/src/ecs/systems.rs`, and the parent climb and post-order walk in `byroredux/src/systems/bounds.rs`. Four walks outside that set still have neither a guard nor a visited set:

1. `apply_placed_water_type`, the Skyrim placed river/stream activator path, called from `cell_loader/references/synth_child.rs` after `spawn_placed_instances`. Its walk is `let mut stack = vec![placement_root]; while let Some(entity) = stack.pop() { … stack.extend(kids.0.iter().copied()); }`.
2. `ragdoll_writeback_system`'s #1981 `LocalBound` BFS (`mesh_walk_queue`) over the actor's subtree.
3. `ragdoll_writeback_system`'s #1979 descendant re-derive BFS (`queue`), seeded from every body bone.
4. `build_subtree_name_map`'s DFS from an animation root.

By contrast, `npc_spawn/loot_appearance.rs::mesh_entities_under`, added two days after the water walk, does keep a visited set.

Save load still does no acyclicity check. `crates/save/src/validate.rs::validate_hierarchy` checks only that `Parent` and `Children` agree, plus dangling ids, so a bidirectionally consistent cycle passes. A corrupt or hand-edited save is still a cycle source for the per-frame ragdoll walks.

## Evidence

```rust
// byroredux/src/cell_loader/water.rs — apply_placed_water_type
let mut stack = vec![placement_root];
while let Some(entity) = stack.pop() {
    if let Some(plane) = planes.get(entity) { /* … */ found.push((entity, *plane, flow)); }
    if let Some(kids) = children.as_ref().and_then(|q| q.get(entity)) {
        stack.extend(kids.0.iter().copied());
    }
}

// byroredux/src/ragdoll.rs — ragdoll_writeback_system, #1979 pass (the #1981 pass has the same shape)
while let Some(entity) = queue.pop_front() {
    // …
    if let Some(children) = cq.get(entity) {
        queue.extend(children.0.iter().copied());
    }
}
```

`grep -rn HierarchyTraversalGuard` finds users only in `crates/core/src/ecs/systems.rs` and `byroredux/src/systems/bounds.rs`.

## Impact

On a cyclic `Children` graph, each of these walks loops forever and its stack or queue grows until OOM, with no diagnostic. The two ragdoll walks run every frame while a ragdoll is active, over hierarchies that may come from a save. A duplicated `Children` edge makes the walk visit the subtree again. In the water case it also pushes duplicate `(entity, WaterPlane)` targets, and each duplicate gets its own texture resolve and insert. The water walk runs over a freshly spawned subtree, so its hazard is theoretical.

## Related

- #3700 (closed) bounded the three core walks, and those fixes are still in place. These four walks were outside its scope, so this is not a regression of it.
- Correctly bounded precedents: the `cell_loader/unload.rs` retained-set walk, the capped console parent climbs (`commands/scene.rs`, `commands/assets.rs`), and `loot_appearance::mesh_entities_under`.

## Suggested Fix

Bound all four walks with `HierarchyTraversalGuard::new(world.next_entity_id() as usize, child_refs)` or a visited set. Follow the `bounds.rs` pattern: when the guard runs out, `log::error!` and bail. Optionally, add an acyclicity check to `validate_hierarchy`, so that a corrupt save is rejected at load time instead of hanging a later frame.

Source: docs/audits/AUDIT_ECS_2026-09-21.md (ECS-2026-09-21-D3-01)

## Completeness Checks
- [ ] **SIBLING**: All four walks are bounded. A grep for other `Children`-following `extend(` loops outside the guarded set finds none left unbounded.
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test builds a `Parent`/`Children` cycle and asserts that each walk terminates with a diagnostic.
