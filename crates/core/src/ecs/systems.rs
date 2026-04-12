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
    let mut queue: Vec<EntityId> = Vec::new();

    move |world: &World, _dt: f32| {
        roots.clear();
        queue.clear();

        // Acquire all ECS queries once per frame and hold them across
        // both phases and the BFS walk. The prior implementation called
        // `world.query*` four times *per child* inside the BFS loop,
        // costing ~4 RwLock acquisitions + drops per node on top of the
        // actual transform composition work. Holding the handles for
        // the whole function collapses that to 4 acquisitions total.
        // See #238.
        //
        // Lock-order note: the ECS schedules parallel systems with
        // TypeId-sorted lock acquisition to prevent deadlocks. Acquire
        // order here doesn't matter for single-system correctness, but
        // we keep the read queries first and the write query last so
        // downstream readers that want the same bundle can mirror this
        // pattern.
        let Some(tq) = world.query::<Transform>() else {
            return;
        };
        let parent_q = world.query::<Parent>();
        let children_q = world.query::<Children>();
        let Some(mut gq) = world.query_mut::<GlobalTransform>() else {
            return;
        };

        // Phase 1: find root entities (have Transform but no Parent).
        for (entity, _) in tq.iter() {
            let is_root = parent_q
                .as_ref()
                .map(|pq| pq.get(entity).is_none())
                .unwrap_or(true);
            if is_root {
                roots.push(entity);
            }
        }

        // Phase 1b: update root GlobalTransforms via the held write query.
        for &entity in &roots {
            if let Some(t) = tq.get(entity) {
                if let Some(g) = gq.get_mut(entity) {
                    g.translation = t.translation;
                    g.rotation = t.rotation;
                    g.scale = t.scale;
                }
            }
        }

        // Phase 2: propagate to children using BFS. Requires both
        // Parent (to look up each child's parent) and Children (to
        // enqueue grandchildren). If either query is absent the scene
        // graph is flat and phase 1 already produced the final state.
        let Some(ref pq) = parent_q else {
            return;
        };
        let Some(ref cq) = children_q else {
            return;
        };

        for &root in &roots {
            if let Some(children) = cq.get(root) {
                queue.extend_from_slice(&children.0);
            }
        }

        while let Some(entity) = queue.pop() {
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
                queue.extend_from_slice(&children.0);
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
        let parent = spawn_with_transform(&mut world, Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 2.0);
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
        let child =
            spawn_with_transform(&mut world, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        world.insert(child, Parent(parent));
        world.insert(parent, Children(vec![child]));

        let mut sys = make_transform_propagation_system();
        sys(&world, 0.016);

        let gq = world.query::<GlobalTransform>().unwrap();
        let cg = gq.get(child).unwrap();
        // Child's world translation must lie on the Z axis (x and y ≈ 0).
        assert!(cg.translation.x.abs() < 1e-5, "x should be 0, got {}", cg.translation.x);
        assert!(cg.translation.y.abs() < 1e-5);
        assert!((cg.translation.z.abs() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn parentless_orphan_keeps_identity_for_missing_global() {
        // Entity with Transform but no GlobalTransform inserted — the
        // system must not panic; it simply skips the missing storage.
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Transform::new(Vec3::new(9.0, 9.0, 9.0), Quat::IDENTITY, 1.0));
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
            let e = spawn_with_transform(
                &mut world,
                Vec3::new(1.0, 0.0, 0.0),
                Quat::IDENTITY,
                1.0,
            );
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

    /// Two sibling subtrees under a common root must BOTH receive the
    /// root's world translation. This pins the fan-out case — the BFS
    /// enqueues both children, and both pops must re-read the same
    /// `parent_global` through the held write query.
    #[test]
    fn fan_out_siblings_both_compose_from_same_root() {
        let mut world = World::new();
        let root =
            spawn_with_transform(&mut world, Vec3::new(100.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        let left =
            spawn_with_transform(&mut world, Vec3::new(-5.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        let right =
            spawn_with_transform(&mut world, Vec3::new(5.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
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
}
