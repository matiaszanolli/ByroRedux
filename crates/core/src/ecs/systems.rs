//! Built-in ECS systems.
//!
//! Systems live in the core crate so every downstream crate (binary,
//! editor, server, tests) gets the same scene-graph semantics for free.
//! Each public factory returns a `impl FnMut(&World, f32) + Send + Sync`
//! closure that captures reusable scratch buffers — the scheduler
//! wires them into a stage via `Scheduler::add_to`.
//!
//! The first resident is `make_transform_propagation_system`, moved
//! out of `byroredux/src/systems.rs` in #81. More engine systems
//! (bounds propagation, billboard, animation ticks) may follow as the
//! binary crate consolidates.

use std::collections::VecDeque;

use crate::ecs::components::{Children, GlobalTransform, Parent, Transform};
use crate::ecs::storage::EntityId;
use crate::ecs::world::World;

/// Transform propagation system — the ECS equivalent of Gamebryo's
/// `NiNode::UpdateDownwardPass`.
///
/// Each frame:
///
/// 1. Every entity that has a `Transform` but no `Parent` is a root;
///    its `GlobalTransform` is copied straight from its local
///    `Transform`.
/// 2. Starting from each root, a breadth-first walk through `Children`
///    composes each child's `GlobalTransform` as
///    `parent_global ∘ child_local`.
///
/// Must run after the animation system and before rendering. When a
/// system pipeline overwrites `GlobalTransform` *after* propagation
/// (the billboard system, the world-bound folder) it should be
/// scheduled as an `add_exclusive` step inside the same stage so the
/// read/write doesn't race against this walk.
///
/// Returns a closure that owns two reusable `Vec` scratch buffers
/// (`roots` and `queue`), cleared and reused every frame instead of
/// reallocating.
pub fn make_transform_propagation_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut roots: Vec<EntityId> = Vec::new();
    let mut queue: VecDeque<EntityId> = VecDeque::new();
    // (Transform::len(), Parent::len(), World::next_entity_id()) — keys
    // the cached `roots` set. Any spawn / despawn / Parent insert-or-
    // remove changes one of these three values, so equality means the
    // root set hasn't moved since last frame. `next_entity_id` covers
    // the despawn-then-spawn-in-same-frame edge case where the two
    // `len()`s happen to net out unchanged. See #825.
    let mut last_roots_key: Option<(usize, usize, EntityId)> = None;
    // Full change-detection state: the roots key plus the `Parent` /
    // `Children` structural generations. When this is unchanged AND no
    // `Transform` was mutated this frame, every `GlobalTransform` is
    // already correct and the whole propagation is skipped — the
    // change-detection fast path (the render-distance root fix). The
    // generations catch hierarchy edits (reparent / attach) that move no
    // Transform and leave entity counts unchanged.
    let mut last_state: Option<((usize, usize, EntityId), u64, u64)> = None;
    // Persistent scratch for the dirty-entity list (#1371). Reusing the
    // same Vec across frames keeps the backing allocation alive so the
    // next `mark_dirty` call does not re-grow from zero capacity.
    let mut transform_dirty: Vec<EntityId> = Vec::new();

    move |world: &World, _dt: f32| {
        queue.clear();

        // Acquire all ECS queries once per frame and hold them across
        // both phases and the BFS walk. The prior implementation called
        // `world.query*` four times *per child* inside the BFS loop,
        // costing ~4 RwLock acquisitions + drops per node on top of the
        // actual transform composition work. Holding the handles for
        // the whole function collapses that to 4 acquisitions total.
        // See #238.
        //
        // `Transform` is taken as a WRITE query (vs read pre-change-
        // detection) so we can drain its per-entity dirty set — the
        // local transforms are only read, never mutated, by this system.
        let Some(mut tq) = world.query_mut::<Transform>() else {
            return;
        };
        let parent_q = world.query::<Parent>();
        let children_q = world.query::<Children>();
        let Some(mut gq) = world.query_mut::<GlobalTransform>() else {
            return;
        };

        // Change-detection drain: which entities' local Transform was
        // mutated since last frame. Draining every frame keeps the dirty
        // Vec bounded (it would otherwise grow unbounded). See
        // `Component::TRACK_CHANGES` + `PackedStorage::drain_dirty_into`.
        // Using `drain_dirty_into` rather than `take_dirty` preserves the
        // storage's capacity across frames (#1371).
        tq.storage_mut().drain_dirty_into(&mut transform_dirty);

        // Hierarchy structural generations — bumped on any Parent/Children
        // insert/remove (incl. reparent overwrites), 0 when nothing's
        // changed. Together with the roots key this detects every hierarchy
        // edit a pure Transform-dirty check would miss.
        let parent_gen = parent_q
            .as_ref()
            .map(|q| q.storage().structural_generation())
            .unwrap_or(0);
        let children_gen = children_q
            .as_ref()
            .map(|q| q.storage().structural_generation())
            .unwrap_or(0);

        let roots_key = (
            tq.len(),
            parent_q.as_ref().map(|q| q.len()).unwrap_or(0),
            world.next_entity_id(),
        );
        let state = (roots_key, parent_gen, children_gen);

        // FAST PATH: nothing moved and the hierarchy is identical to last
        // frame → all GlobalTransforms are already correct, skip the walk.
        // This is the change-detection win: a static cell (camera parked or
        // moving — the camera's own Transform mutation marks only itself
        // dirty) skips the full O(entities) BFS entirely.
        if transform_dirty.is_empty() && last_state == Some(state) {
            return;
        }
        // `structural_changed` = the hierarchy itself moved (spawn / despawn
        // / reparent / cell load), detected by any change to the roots key
        // OR the Parent/Children structural generations. When false, only
        // some local Transforms moved and the cheap incremental path applies.
        // `topology_changed` (roots-key only) further gates the root rescan.
        let structural_changed = last_state != Some(state);
        let topology_changed = last_roots_key != Some(roots_key);
        last_state = Some(state);
        last_roots_key = Some(roots_key);

        // Phase 1: find root entities (have Transform but no Parent).
        // Steady-state interior cells touch ~6 k Transforms with ~30
        // roots; rescanning every frame burned ~250 µs / frame on
        // Megaton (#825). The generation key matches the `NameIndex`
        // pattern used in `animation_system`.
        // Seed the BFS queue. Two modes:
        //
        // * STRUCTURAL change (spawn / despawn / reparent / cell load):
        //   rebuild the root set if counts moved, set every root's global
        //   from its local, and seed the walk with every root's children —
        //   the original full O(entities) propagation.
        //
        // * INCREMENTAL (only some locals moved, hierarchy identical): set
        //   each moved entity's own global, seed only ITS children. The
        //   static cell with a moving camera lands here — the camera's
        //   Transform is the only dirty entry, so the walk touches ~1
        //   subtree instead of all 110 k entities. This is the win.
        queue.clear();
        if structural_changed {
            if topology_changed {
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
            }
            // Phase 1b: every root's global = its local.
            for &entity in &roots {
                if let (Some(t), Some(g)) = (tq.get(entity), gq.get_mut(entity)) {
                    g.translation = t.translation;
                    g.rotation = t.rotation;
                    g.scale = t.scale;
                }
            }
            if let Some(ref cq) = children_q {
                for &root in &roots {
                    if let Some(children) = cq.get(root) {
                        queue.extend(children.0.iter().copied());
                    }
                }
            }
        } else {
            // Dedup the dirty list (an entity may be marked multiple times
            // in one frame) so each subtree is seeded once.
            transform_dirty.sort_unstable();
            transform_dirty.dedup();
            for &e in &transform_dirty {
                let local = tq.get(e).copied().unwrap_or(Transform::IDENTITY);
                // Recompose e's own global: parent's current global ∘ local
                // (the parent did not move, so its global is correct), or the
                // local itself when e is a root. If e and an ancestor are both
                // dirty, the ancestor's seeded subtree re-fixes e — order
                // doesn't matter, only that every moved subtree is walked.
                let e_global = match parent_q.as_ref().and_then(|pq| pq.get(e)) {
                    Some(parent) => match gq.get_mut(parent.0).map(|g| *g) {
                        Some(pg) => GlobalTransform::compose(
                            &pg,
                            local.translation,
                            local.rotation,
                            local.scale,
                        ),
                        None => continue,
                    },
                    None => GlobalTransform::new(local.translation, local.rotation, local.scale),
                };
                if let Some(g) = gq.get_mut(e) {
                    *g = e_global;
                }
                if let Some(ref cq) = children_q {
                    if let Some(children) = cq.get(e) {
                        queue.extend(children.0.iter().copied());
                    }
                }
            }
        }

        // Shared BFS drain. Requires both Parent (to look up each child's
        // parent) and Children (to enqueue grandchildren); a flat scene with
        // neither already reached its final state in the seeding above.
        let (Some(ref pq), Some(ref cq)) = (parent_q.as_ref(), children_q.as_ref()) else {
            return;
        };

        while let Some(entity) = queue.pop_front() {
            let Some(parent) = pq.get(entity) else {
                continue;
            };
            let parent_id = parent.0;

            // Read the parent's GlobalTransform through the held write
            // query. `get_mut` returns `&mut GlobalTransform`, and the
            // deref copies it out, ending the borrow before the child
            // write below begins. Transform is `Copy`, so there's no
            // aliasing.
            let Some(parent_global) = gq.get_mut(parent_id).map(|g| *g) else {
                continue;
            };

            let local = tq.get(entity).copied().unwrap_or(Transform::IDENTITY);

            let composed = GlobalTransform::compose(
                &parent_global,
                local.translation,
                local.rotation,
                local.scale,
            );

            if let Some(g) = gq.get_mut(entity) {
                *g = composed;
            }

            if let Some(children) = cq.get(entity) {
                queue.extend(children.0.iter().copied());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Regression tests for the core transform propagation system
    //! extracted from the binary crate in #81.

    use super::*;
    use crate::ecs::components::{GlobalTransform, Parent, Transform};
    use crate::ecs::world::World;
    use crate::math::{Quat, Vec3};

    fn spawn_with_transform(
        world: &mut World,
        translation: Vec3,
        rotation: Quat,
        scale: f32,
    ) -> EntityId {
        let e = world.spawn();
        world.insert(e, Transform::new(translation, rotation, scale));
        world.insert(e, GlobalTransform::IDENTITY);
        e
    }

    #[test]
    fn root_global_matches_local_transform() {
        let mut world = World::new();
        let e = spawn_with_transform(
            &mut world,
            Vec3::new(1.0, 2.0, 3.0),
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            2.0,
        );

        let mut sys = make_transform_propagation_system();
        sys(&world, 0.016);

        let gq = world.query::<GlobalTransform>().unwrap();
        let g = gq.get(e).unwrap();
        assert!((g.translation - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5);
        assert!((g.scale - 2.0).abs() < 1e-5);
    }

    #[test]
    fn child_inherits_parent_translation_and_scale() {
        let mut world = World::new();
        let parent =
            spawn_with_transform(&mut world, Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 2.0);
        let child = spawn_with_transform(&mut world, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        world.insert(child, Parent(parent));
        world.insert(parent, Children(vec![child]));

        let mut sys = make_transform_propagation_system();
        sys(&world, 0.016);

        let gq = world.query::<GlobalTransform>().unwrap();
        let cg = gq.get(child).unwrap();
        // Child local offset (1, 0, 0) scaled by parent scale 2 and
        // added to parent world position (10, 0, 0) → (12, 0, 0).
        assert!((cg.translation - Vec3::new(12.0, 0.0, 0.0)).length() < 1e-5);
        assert!((cg.scale - 2.0).abs() < 1e-5);
    }

    #[test]
    fn grandchild_composes_through_two_parents() {
        let mut world = World::new();
        let a = spawn_with_transform(&mut world, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        let b = spawn_with_transform(&mut world, Vec3::new(2.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        let c = spawn_with_transform(&mut world, Vec3::new(3.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        world.insert(b, Parent(a));
        world.insert(c, Parent(b));
        world.insert(a, Children(vec![b]));
        world.insert(b, Children(vec![c]));

        let mut sys = make_transform_propagation_system();
        sys(&world, 0.016);

        let gq = world.query::<GlobalTransform>().unwrap();
        let ga = gq.get(a).unwrap();
        let gb = gq.get(b).unwrap();
        let gc = gq.get(c).unwrap();
        assert!((ga.translation.x - 1.0).abs() < 1e-5);
        assert!((gb.translation.x - 3.0).abs() < 1e-5);
        assert!((gc.translation.x - 6.0).abs() < 1e-5);
    }

    #[test]
    fn child_rotation_is_composed_with_parent() {
        use std::f32::consts::FRAC_PI_2;
        let mut world = World::new();
        // Parent rotated 90° around Y — its local +X now points at world +Z
        // (in a right-handed Y-up coordinate system with CCW-positive Y rotation,
        // `rot_y(π/2) * +X → -Z`). We check the child's translation ends up
        // consistent with the parent rotation regardless of sign convention.
        let parent = spawn_with_transform(
            &mut world,
            Vec3::ZERO,
            Quat::from_rotation_y(FRAC_PI_2),
            1.0,
        );
        let child = spawn_with_transform(&mut world, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        world.insert(child, Parent(parent));
        world.insert(parent, Children(vec![child]));

        let mut sys = make_transform_propagation_system();
        sys(&world, 0.016);

        let gq = world.query::<GlobalTransform>().unwrap();
        let cg = gq.get(child).unwrap();
        // Child's world translation must lie on the Z axis (x and y ≈ 0).
        assert!(
            cg.translation.x.abs() < 1e-5,
            "x should be 0, got {}",
            cg.translation.x
        );
        assert!(cg.translation.y.abs() < 1e-5);
        assert!((cg.translation.z.abs() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn parentless_orphan_keeps_identity_for_missing_global() {
        // Entity with Transform but no GlobalTransform inserted — the
        // system must not panic; it simply skips the missing storage.
        let mut world = World::new();
        let e = world.spawn();
        world.insert(
            e,
            Transform::new(Vec3::new(9.0, 9.0, 9.0), Quat::IDENTITY, 1.0),
        );
        // Deliberately no GlobalTransform.
        let mut sys = make_transform_propagation_system();
        sys(&world, 0.016);
        // No crash, and no global got invented for `e`.
        let gq = world.query::<GlobalTransform>();
        if let Some(q) = gq {
            assert!(q.get(e).is_none());
        }
    }

    /// Regression test for #238: a 10-level-deep chain composed in a
    /// single propagation call must produce a linear translation
    /// accumulation. The old implementation would acquire four ECS
    /// locks per child (~40 acquisitions for this chain); the new
    /// implementation holds all four queries across the BFS. Functional
    /// correctness must be identical.
    #[test]
    fn ten_level_chain_composes_correctly_with_held_locks() {
        let mut world = World::new();
        let mut prev: Option<EntityId> = None;
        let mut ids: Vec<EntityId> = Vec::new();
        for _ in 0..10 {
            let e = spawn_with_transform(&mut world, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
            if let Some(parent) = prev {
                world.insert(e, Parent(parent));
                world.insert(parent, Children(vec![e]));
            }
            prev = Some(e);
            ids.push(e);
        }

        let mut sys = make_transform_propagation_system();
        sys(&world, 0.016);

        let gq = world.query::<GlobalTransform>().unwrap();
        for (i, &id) in ids.iter().enumerate() {
            let g = gq.get(id).unwrap();
            let expected_x = (i + 1) as f32;
            assert!(
                (g.translation.x - expected_x).abs() < 1e-4,
                "level {i}: expected x={expected_x}, got {}",
                g.translation.x,
            );
            assert!(g.translation.y.abs() < 1e-4);
            assert!(g.translation.z.abs() < 1e-4);
        }
    }

    /// M41.0 Phase 1b.x — replicate the NPC spawn topology that's
    /// rendering with broken body skinning interactively. Spawn order
    /// mirrors `byroredux::npc_spawn::spawn_npc_entity`:
    ///   1. placement_root with T + GT BOTH explicitly set to the
    ///      cell-world ref position.
    ///   2. skel_root spawned (no Parent), then `Parent(placement_root)`
    ///      inserted afterwards.
    ///   3. Several bones spawned as a deep chain *before* the skel_root
    ///      → placement_root edge is set, so add_child runs after.
    /// If propagation works under this exact ordering, the runtime bug
    /// isn't in the topology — it's in the GPU palette upload or the
    /// dispatch order against the scheduler.
    #[test]
    fn npc_spawn_topology_propagates_to_deep_bone_chain() {
        let mut world = World::new();
        // Step 1: placement_root with EXPLICIT GT (mirrors
        // npc_spawn.rs:319-323 — both Transform AND GlobalTransform
        // get inserted at spawn time so the renderer can read a valid
        // pose on frame 0 before propagation runs).
        let placement_root = world.spawn();
        let ref_pos = Vec3::new(2288.76, 7360.0, -2244.41);
        world.insert(placement_root, Transform::new(ref_pos, Quat::IDENTITY, 1.0));
        world.insert(
            placement_root,
            GlobalTransform::new(ref_pos, Quat::IDENTITY, 1.0),
        );

        // Step 2: skel_root spawned via "import" (no Parent yet), all
        // bones spawned next as a deep chain inside the skel.nif. Mirror
        // load_nif_bytes_with_skeleton's Phase 1+2 ordering: spawn all
        // node entities first, then walk the parent_node array to set
        // Parent + add_child.
        let skel_root = spawn_with_transform(&mut world, Vec3::ZERO, Quat::IDENTITY, 1.0);
        let bones: Vec<EntityId> = (0..30)
            .map(|_| {
                spawn_with_transform(&mut world, Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, 1.0)
            })
            .collect();
        // Build the bone chain: bones[0] under skel_root, bones[i] under
        // bones[i-1].
        world.insert(bones[0], Parent(skel_root));
        world.insert(skel_root, Children(vec![bones[0]]));
        for i in 1..bones.len() {
            world.insert(bones[i], Parent(bones[i - 1]));
            world.insert(bones[i - 1], Children(vec![bones[i]]));
        }

        // Step 3: NOW set Parent(skel_root) = placement_root and
        // add_child(placement_root, skel_root). Mirrors npc_spawn.rs:
        // 366-367. This is the "external skeleton parent edge" — set
        // AFTER the NIF spawn assembled the skel internals.
        world.insert(skel_root, Parent(placement_root));
        world.insert(placement_root, Children(vec![skel_root]));

        // Run propagation.
        let mut sys = make_transform_propagation_system();
        sys(&world, 0.016);

        // Verify: placement_root keeps its world ref. skel_root composes
        // ref + identity = ref. bones[0] = ref + (0,1,0) = ref+y.
        // bones[i] should accumulate i+1 of (0,1,0) on top of ref.
        let gq = world.query::<GlobalTransform>().unwrap();
        let gp = gq.get(placement_root).unwrap();
        assert!(
            (gp.translation - ref_pos).length() < 1e-3,
            "placement_root GT must equal ref_pos"
        );
        let gs = gq.get(skel_root).unwrap();
        assert!(
            (gs.translation - ref_pos).length() < 1e-3,
            "skel_root GT must compose to ref_pos (got {:?})",
            gs.translation
        );
        for (i, &b) in bones.iter().enumerate() {
            let gb = gq.get(b).unwrap();
            let expected = ref_pos + Vec3::new(0.0, (i + 1) as f32, 0.0);
            assert!(
                (gb.translation - expected).length() < 1e-3,
                "bone[{i}] expected {:?}, got {:?}",
                expected,
                gb.translation
            );
        }
    }

    /// Two sibling subtrees under a common root must BOTH receive the
    /// root's world translation. This pins the fan-out case — the BFS
    /// enqueues both children, and both pops must re-read the same
    /// `parent_global` through the held write query.
    #[test]
    fn fan_out_siblings_both_compose_from_same_root() {
        let mut world = World::new();
        let root =
            spawn_with_transform(&mut world, Vec3::new(100.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        let left = spawn_with_transform(&mut world, Vec3::new(-5.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        let right = spawn_with_transform(&mut world, Vec3::new(5.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        world.insert(left, Parent(root));
        world.insert(right, Parent(root));
        world.insert(root, Children(vec![left, right]));

        let mut sys = make_transform_propagation_system();
        sys(&world, 0.016);

        let gq = world.query::<GlobalTransform>().unwrap();
        let gl = gq.get(left).unwrap();
        let gr = gq.get(right).unwrap();
        assert!((gl.translation.x - 95.0).abs() < 1e-4);
        assert!((gr.translation.x - 105.0).abs() < 1e-4);
    }

    /// Regression test for #825: the cached root set must invalidate
    /// when a new top-level entity (Transform-only) is spawned, when an
    /// entity gains a Parent (becomes non-root), and when a Parent is
    /// removed (becomes a root). All three transitions move the
    /// `(Transform::len, Parent::len, next_entity_id)` key.
    #[test]
    fn root_cache_invalidates_on_topology_change() {
        let mut world = World::new();
        let r1 = spawn_with_transform(&mut world, Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 1.0);

        let mut sys = make_transform_propagation_system();
        sys(&world, 0.016);
        assert_eq!(
            world
                .query::<GlobalTransform>()
                .unwrap()
                .get(r1)
                .unwrap()
                .translation
                .x,
            10.0
        );

        // 1) New root appears — cache must rescan and produce its GT.
        let r2 = spawn_with_transform(&mut world, Vec3::new(20.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        sys(&world, 0.016);
        let gq = world.query::<GlobalTransform>().unwrap();
        assert_eq!(gq.get(r2).unwrap().translation.x, 20.0);
        drop(gq);

        // 2) New child of r1 — gains a Parent (Parent::len changes), so
        //    the rescan must NOT classify it as a root, and BFS must
        //    still compose its GT through r1.
        let child = spawn_with_transform(&mut world, Vec3::new(3.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        world.insert(child, Parent(r1));
        world.insert(r1, Children(vec![child]));
        sys(&world, 0.016);
        let gq = world.query::<GlobalTransform>().unwrap();
        // r1 (10) + child local (3) = 13, composed via the BFS pass.
        assert!((gq.get(child).unwrap().translation.x - 13.0).abs() < 1e-4);
        drop(gq);

        // 3) Remove the Parent — `child` should be promoted to root and
        //    its GT should now equal its local Transform alone (3.0, not
        //    13.0). Parent::len drops, invalidating the cache.
        world.remove::<Parent>(child);
        sys(&world, 0.016);
        let gq = world.query::<GlobalTransform>().unwrap();
        assert!(
            (gq.get(child).unwrap().translation.x - 3.0).abs() < 1e-4,
            "child should be a root after Parent removed (got x={})",
            gq.get(child).unwrap().translation.x
        );
    }

    /// Steady-state cache hit: with no topology change between frames,
    /// the propagated values must remain correct (i.e. Phase 1b/2 still
    /// run on the cached root set).
    #[test]
    fn root_cache_steady_state_still_runs_propagation() {
        let mut world = World::new();
        let root = spawn_with_transform(&mut world, Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        let child = spawn_with_transform(&mut world, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        world.insert(child, Parent(root));
        world.insert(root, Children(vec![child]));

        let mut sys = make_transform_propagation_system();
        sys(&world, 0.016);

        // Mutate the root's local transform without touching topology —
        // cache stays valid, but Phase 1b/2 must still re-compose.
        {
            let mut tq = world.query_mut::<Transform>().unwrap();
            tq.get_mut(root).unwrap().translation.x = 50.0;
        }
        sys(&world, 0.016);
        let gq = world.query::<GlobalTransform>().unwrap();
        assert!((gq.get(root).unwrap().translation.x - 50.0).abs() < 1e-4);
        assert!((gq.get(child).unwrap().translation.x - 51.0).abs() < 1e-4);
    }

    /// Change-detection fast path: a frame with no `Transform` mutation and
    /// no hierarchy change must SKIP the propagation walk entirely. Proven
    /// by corrupting a `GlobalTransform` directly and showing the skip
    /// leaves the corruption in place — then a real local mutation
    /// re-propagates and fixes it.
    #[test]
    fn change_detection_skips_when_nothing_moved() {
        let mut world = World::new();
        let parent =
            spawn_with_transform(&mut world, Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        let child = spawn_with_transform(&mut world, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        world.insert(child, Parent(parent));
        world.insert(parent, Children(vec![child]));

        let mut sys = make_transform_propagation_system();
        sys(&world, 0.016); // frame 1 — full propagation
        assert!(
            (world
                .query::<GlobalTransform>()
                .unwrap()
                .get(child)
                .unwrap()
                .translation
                .x
                - 11.0)
                .abs()
                < 1e-4
        );

        // Corrupt the child's GlobalTransform directly. GlobalTransform is
        // not change-tracked, so this does not arm the dirty set.
        {
            let mut gq = world.query_mut::<GlobalTransform>().unwrap();
            gq.get_mut(child).unwrap().translation.x = -999.0;
        }
        // No Transform mutated, hierarchy unchanged → MUST skip, leaving the
        // corruption untouched (a re-run would overwrite it back to 11).
        sys(&world, 0.016);
        assert_eq!(
            world
                .query::<GlobalTransform>()
                .unwrap()
                .get(child)
                .unwrap()
                .translation
                .x,
            -999.0,
            "propagation should have skipped (nothing changed)"
        );

        // Move the parent → marks Transform dirty → re-propagation fixes the
        // child (and overwrites the corruption).
        {
            let mut tq = world.query_mut::<Transform>().unwrap();
            tq.get_mut(parent).unwrap().translation.x = 20.0;
        }
        sys(&world, 0.016);
        assert!(
            (world
                .query::<GlobalTransform>()
                .unwrap()
                .get(child)
                .unwrap()
                .translation
                .x
                - 21.0)
                .abs()
                < 1e-4,
            "moving the parent must re-propagate the child to 21"
        );
    }

    /// A reparent that overwrites an existing `Parent` (no Transform moved,
    /// entity count unchanged) must NOT be skipped — the `Parent`/`Children`
    /// structural generation guards against the dirty-set blind spot.
    #[test]
    fn change_detection_reacts_to_reparent_overwrite() {
        let mut world = World::new();
        let parent =
            spawn_with_transform(&mut world, Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        let child = spawn_with_transform(&mut world, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        world.insert(child, Parent(parent));
        world.insert(parent, Children(vec![child]));

        let mut sys = make_transform_propagation_system();
        sys(&world, 0.016);

        // Corrupt the child global, then re-insert Parent (an overwrite with
        // the same value — entity count + Transforms all unchanged, but the
        // Parent storage's structural generation bumps).
        {
            let mut gq = world.query_mut::<GlobalTransform>().unwrap();
            gq.get_mut(child).unwrap().translation.x = -999.0;
        }
        world.insert(child, Parent(parent)); // structural insert → gen bump
        sys(&world, 0.016);
        assert!(
            (world
                .query::<GlobalTransform>()
                .unwrap()
                .get(child)
                .unwrap()
                .translation
                .x
                - 11.0)
                .abs()
                < 1e-4,
            "a Parent overwrite must defeat the skip and re-propagate the child"
        );
    }
}
