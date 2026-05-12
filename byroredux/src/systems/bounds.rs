//! World-bound propagation — bottom-up fold of `LocalBound` + child bounds.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Children, GlobalTransform, LocalBound, Parent, World, WorldBound};

/// Compute each entity's world-space `WorldBound`.
///
/// Two passes:
///
/// 1. **Leaf bounds** — for every entity with a `LocalBound` (set at import
///    time from NIF `NiBound`), compose it with `GlobalTransform` to
///    produce a world-space sphere. The center is rotated and translated
///    by the entity's world transform; the radius is scaled uniformly
///    by the world scale.
///
/// 2. **Parent bounds** — for every entity that has `Children` but no
///    `LocalBound` (i.e. pure scene-graph nodes), fold the children's
///    `WorldBound`s into a single enclosing sphere via
///    [`WorldBound::merge`]. Runs bottom-up through post-order traversal
///    so each parent sees its descendants' final bounds. Walks the
///    hierarchy reusing the same `queue` vec so we don't allocate per
///    frame; the initial root set is also reused across frames.
///
/// Runs after `transform_propagation_system` and `billboard_system`
/// (scheduled as an exclusive PostUpdate step) so both leaf transforms
/// and billboard overrides are final. See issue #217.
pub(crate) fn make_world_bound_propagation_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut roots: Vec<EntityId> = Vec::new();
    let mut post_order: Vec<EntityId> = Vec::new();
    let mut stack: Vec<(EntityId, bool)> = Vec::new();
    // (GlobalTransform::len(), Parent::len(), World::next_entity_id()) —
    // generation key for the cached `roots` set. Mirrors the
    // `transform_propagation_system` pattern from #825 — same root
    // discovery anti-pattern, different storage. See #826.
    let mut last_seen_roots: Option<(usize, usize, EntityId)> = None;

    move |world: &World, _dt: f32| {
        // Acquire Children and LocalBound once — used by both passes.
        // Previously re-acquired for pass 2 (#250).
        let local_q = world.query::<LocalBound>();
        let children_q = world.query::<Children>();

        // ── Pass 1: leaf bounds from LocalBound + GlobalTransform ──────
        {
            let Some(ref lb_q) = local_q else {
                return;
            };
            let Some(g_q) = world.query::<GlobalTransform>() else {
                return;
            };
            let Some(mut wb_q) = world.query_mut::<WorldBound>() else {
                return;
            };
            for (entity, local) in lb_q.iter() {
                let Some(global) = g_q.get(entity) else {
                    continue;
                };
                let world_center =
                    global.translation + global.rotation * (local.center * global.scale);
                let world_radius = local.radius * global.scale;
                if let Some(wb) = wb_q.get_mut(entity) {
                    *wb = WorldBound::new(world_center, world_radius);
                }
            }
        }

        // ── Pass 2: parent bounds as unions of children ────────────────
        //
        // Walk the hierarchy from each root entity (one without a Parent)
        // and record a post-order list. Then iterate that list in order —
        // by the time we process a parent, every child has already had its
        // bound assigned. Entities that already have a LocalBound are
        // leaves in this sense (their bound comes from pass 1) and are
        // skipped here.
        post_order.clear();
        stack.clear();

        {
            let Some(tq) = world.query::<GlobalTransform>() else {
                return;
            };
            let parent_q = world.query::<Parent>();
            // Cache the root set across frames; re-scan only when the
            // (GlobalTransform::len, Parent::len, next_entity_id) key
            // moves. Steady-state interior cells touch ~6 k
            // GlobalTransforms with ~30 roots — pre-#826 this rescanned
            // every frame at ~250 µs, on top of the same waste in
            // transform_propagation_system (#825). Same generation
            // pattern as `NameIndex` / `transform_propagation`.
            let key = (
                tq.len(),
                parent_q.as_ref().map(|q| q.len()).unwrap_or(0),
                world.next_entity_id(),
            );
            if last_seen_roots != Some(key) {
                roots.clear();
                for (entity, _) in tq.iter() {
                    let is_root = parent_q
                        .as_ref()
                        .map(|pq| pq.get(entity).is_none())
                        .unwrap_or(true);
                    if is_root {
                        roots.push(entity);
                    }
                }
                last_seen_roots = Some(key);
            }
        }

        for &root in &roots {
            stack.push((root, false));
            while let Some((entity, visited)) = stack.pop() {
                if visited {
                    post_order.push(entity);
                    continue;
                }
                stack.push((entity, true));
                if let Some(ref cq) = children_q {
                    if let Some(children) = cq.get(entity) {
                        for &child in &children.0 {
                            stack.push((child, false));
                        }
                    }
                }
            }
        }

        // Fold children into parents. Must be post-order — children first.
        let Some(mut wb_q) = world.query_mut::<WorldBound>() else {
            return;
        };

        for &entity in &post_order {
            // Leaves (entities with a LocalBound) already have their bound
            // from pass 1. We still need parents above them to fold them in,
            // so skip only the write step here.
            if local_q
                .as_ref()
                .map(|q| q.get(entity).is_some())
                .unwrap_or(false)
            {
                continue;
            }

            // Collect child bounds.
            let Some(ref cq) = children_q else {
                continue;
            };
            let Some(children) = cq.get(entity) else {
                continue;
            };
            let mut merged = WorldBound::ZERO;
            for &child in &children.0 {
                // Read the child's bound via the mutable query — the
                // storage allows a copy-out even though we hold `wb_q`
                // mutably, because we're not aliasing across iterations.
                if let Some(child_bound) = wb_q.get_mut(child).map(|b| *b) {
                    merged = merged.merge(&child_bound);
                }
            }
            if let Some(wb) = wb_q.get_mut(entity) {
                *wb = merged;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Regression tests for `make_world_bound_propagation_system` — issue #217.
    //! These cover leaf derivation, parent merging, and the scale path.

    use super::*;
    use byroredux_core::ecs::World;
    use byroredux_core::ecs::{Children, GlobalTransform, LocalBound, Parent, WorldBound};
    use byroredux_core::math::{Quat, Vec3};

    /// Spawn an entity with a LocalBound + GlobalTransform + empty WorldBound.
    fn spawn_leaf(
        world: &mut World,
        translation: Vec3,
        scale: f32,
        local_center: Vec3,
        local_radius: f32,
    ) -> byroredux_core::ecs::storage::EntityId {
        let e = world.spawn();
        world.insert(e, GlobalTransform::new(translation, Quat::IDENTITY, scale));
        world.insert(e, LocalBound::new(local_center, local_radius));
        world.insert(e, WorldBound::ZERO);
        e
    }

    #[test]
    fn leaf_bound_composes_local_with_global_transform() {
        let mut world = World::new();
        let e = spawn_leaf(&mut world, Vec3::new(10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 2.0);

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();
        let wb = wb_q.get(e).unwrap();
        assert!((wb.center - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-5);
        assert!((wb.radius - 2.0).abs() < 1e-5);
    }

    #[test]
    fn leaf_bound_scale_multiplies_radius() {
        let mut world = World::new();
        let e = spawn_leaf(&mut world, Vec3::ZERO, 3.0, Vec3::ZERO, 1.0);

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();
        let wb = wb_q.get(e).unwrap();
        assert!((wb.radius - 3.0).abs() < 1e-5);
    }

    #[test]
    fn leaf_bound_nonzero_local_center_is_offset() {
        let mut world = World::new();
        // Mesh sits at world origin, scale 2, but its local sphere is
        // centered at (1, 0, 0) local. World center should be (2, 0, 0).
        let e = spawn_leaf(&mut world, Vec3::ZERO, 2.0, Vec3::new(1.0, 0.0, 0.0), 0.5);

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();
        let wb = wb_q.get(e).unwrap();
        assert!((wb.center - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-5);
        assert!((wb.radius - 1.0).abs() < 1e-5);
    }

    #[test]
    fn parent_bound_unions_child_bounds() {
        // Parent at origin (no LocalBound) with two leaf children at ±10
        // along x, each with local radius 1. The parent WorldBound should
        // be the smallest sphere enclosing both — center at origin, r=11.
        let mut world = World::new();
        let parent = world.spawn();
        world.insert(parent, GlobalTransform::IDENTITY);
        world.insert(parent, WorldBound::ZERO);

        let left = spawn_leaf(&mut world, Vec3::new(-10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);
        let right = spawn_leaf(&mut world, Vec3::new(10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);

        // Wire the hierarchy: both leaves are children of `parent`.
        world.insert(left, Parent(parent));
        world.insert(right, Parent(parent));
        world.insert(parent, Children(vec![left, right]));

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();

        let left_wb = *wb_q.get(left).unwrap();
        assert!((left_wb.center - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-5);
        let right_wb = *wb_q.get(right).unwrap();
        assert!((right_wb.center - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-5);

        let parent_wb = *wb_q.get(parent).unwrap();
        assert!(
            (parent_wb.center - Vec3::ZERO).length() < 1e-5,
            "parent center should be midpoint, got {:?}",
            parent_wb.center,
        );
        assert!(
            (parent_wb.radius - 11.0).abs() < 1e-5,
            "parent radius should enclose both leaves, got {}",
            parent_wb.radius,
        );
        // Contains-check both leaves' centers.
        assert!(parent_wb.contains_point(left_wb.center));
        assert!(parent_wb.contains_point(right_wb.center));
    }

    #[test]
    fn pure_parent_with_no_children_keeps_zero_bound() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, GlobalTransform::IDENTITY);
        world.insert(e, WorldBound::ZERO);

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();
        let wb = wb_q.get(e).unwrap();
        assert_eq!(wb.radius, 0.0);
    }

    /// Regression test for #826: the cached root set must invalidate
    /// when the scene-graph topology changes between frames. Mirrors
    /// the sibling test for `transform_propagation_system` (#825). All
    /// three transitions (new root spawned, child-gains-Parent,
    /// child-loses-Parent) move the
    /// `(GlobalTransform::len, Parent::len, next_entity_id)` key, so
    /// each must trigger a rescan and a corrected post-order walk.
    #[test]
    fn root_cache_invalidates_on_topology_change() {
        let mut world = World::new();
        // Initial: one root with one child leaf at (-10, 0, 0) r=1.
        let parent = world.spawn();
        world.insert(parent, GlobalTransform::IDENTITY);
        world.insert(parent, WorldBound::ZERO);

        let leaf = spawn_leaf(&mut world, Vec3::new(-10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);
        world.insert(leaf, Parent(parent));
        world.insert(parent, Children(vec![leaf]));

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);
        let parent_wb_initial = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!((parent_wb_initial.center - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-5);

        // 1) Spawn a NEW top-level root (unrelated). Cache key
        //    (GlobalTransform::len) bumps; rescan must include it
        //    even though `parent` already had an entry.
        let new_root = spawn_leaf(&mut world, Vec3::new(50.0, 0.0, 0.0), 1.0, Vec3::ZERO, 2.0);
        sys(&world, 0.016);
        let new_root_wb = *world.query::<WorldBound>().unwrap().get(new_root).unwrap();
        assert!(
            (new_root_wb.center - Vec3::new(50.0, 0.0, 0.0)).length() < 1e-5,
            "new root must be discovered after cache invalidation, got {:?}",
            new_root_wb.center
        );

        // 2) Add a SECOND child to `parent` — Parent::len bumps. The
        //    parent's WorldBound must re-fold to enclose both leaves.
        let leaf2 = spawn_leaf(&mut world, Vec3::new(10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);
        world.insert(leaf2, Parent(parent));
        world.insert(parent, Children(vec![leaf, leaf2]));
        sys(&world, 0.016);
        let parent_wb = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!(
            (parent_wb.center - Vec3::ZERO).length() < 1e-5,
            "parent center should be midpoint after second child added, got {:?}",
            parent_wb.center
        );
        assert!(
            (parent_wb.radius - 11.0).abs() < 1e-5,
            "parent radius should enclose both leaves (r=11), got {}",
            parent_wb.radius
        );

        // 3) Promote `leaf2` to root by removing its Parent. Parent::len
        //    drops; rescan must include it. After the walk, `parent`'s
        //    WorldBound should fall back to enclosing only `leaf`.
        world.remove::<Parent>(leaf2);
        world.insert(parent, Children(vec![leaf]));
        sys(&world, 0.016);
        let parent_wb_after = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!(
            (parent_wb_after.center - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-5,
            "parent should re-fold to single child after promote, got {:?}",
            parent_wb_after.center
        );
    }

    /// Steady-state cache hit: with no topology change, the cached
    /// root set must still drive a correct post-order walk so leaf
    /// transform changes propagate up to parent bounds (the
    /// counterpart to `root_cache_steady_state_still_runs_propagation`
    /// in #825 — confirms cache hits don't stall pass 2).
    #[test]
    fn root_cache_steady_state_still_refolds_parent_bounds() {
        let mut world = World::new();
        let parent = world.spawn();
        world.insert(parent, GlobalTransform::IDENTITY);
        world.insert(parent, WorldBound::ZERO);

        let leaf = spawn_leaf(&mut world, Vec3::new(-10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);
        world.insert(leaf, Parent(parent));
        world.insert(parent, Children(vec![leaf]));

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);
        let initial = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!((initial.center - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-5);

        // Move the leaf without any topology change — the cache key
        // stays valid, but pass 1 (leaf bound) and pass 2 (parent
        // fold) must still re-execute against the cached root.
        {
            let mut gq = world.query_mut::<GlobalTransform>().unwrap();
            let g = gq.get_mut(leaf).unwrap();
            g.translation = Vec3::new(20.0, 0.0, 0.0);
        }
        sys(&world, 0.016);
        let after = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!(
            (after.center - Vec3::new(20.0, 0.0, 0.0)).length() < 1e-5,
            "parent bound must re-fold against cached root after leaf moved, got {:?}",
            after.center
        );
    }
}
