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

        // Phase 1: find root entities (have Transform but no Parent).
        {
            let Some(tq) = world.query::<Transform>() else {
                return;
            };
            let parent_q = world.query::<Parent>();

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

        // Update root GlobalTransforms.
        {
            let tq = world.query::<Transform>().unwrap();
            let mut gq = match world.query_mut::<GlobalTransform>() {
                Some(q) => q,
                None => return,
            };
            for &entity in &roots {
                if let Some(t) = tq.get(entity) {
                    if let Some(g) = gq.get_mut(entity) {
                        g.translation = t.translation;
                        g.rotation = t.rotation;
                        g.scale = t.scale;
                    }
                }
            }
        }

        // Phase 2: propagate to children using BFS.
        let children_q = world.query::<Children>();
        let Some(ref cq) = children_q else { return };

        for &root in &roots {
            if let Some(children) = cq.get(root) {
                queue.extend_from_slice(&children.0);
            }
        }

        while let Some(entity) = queue.pop() {
            let parent_q = world.query::<Parent>().unwrap();
            let Some(parent) = parent_q.get(entity) else {
                continue;
            };
            let parent_id = parent.0;
            drop(parent_q);

            let gq_read = world.query::<GlobalTransform>().unwrap();
            let Some(parent_global) = gq_read.get(parent_id) else {
                continue;
            };
            let parent_global = *parent_global;
            drop(gq_read);

            let tq = world.query::<Transform>().unwrap();
            let local = tq.get(entity).copied().unwrap_or(Transform::IDENTITY);
            drop(tq);

            let composed = GlobalTransform::compose(
                &parent_global,
                local.translation,
                local.rotation,
                local.scale,
            );

            let mut gq_write = world.query_mut::<GlobalTransform>().unwrap();
            if let Some(g) = gq_write.get_mut(entity) {
                *g = composed;
            }
            drop(gq_write);

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
}
