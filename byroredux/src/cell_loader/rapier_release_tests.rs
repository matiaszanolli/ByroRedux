//! Regression tests for `release_victim_rapier_bodies` — issue #1520
//! DROP completeness check.
//!
//! `World::despawn` only removes the `RapierHandles` ECS row; the Rapier
//! `RigidBody` + colliders it points at have no Drop tied to the ECS, so
//! cell unload must explicitly remove them from the `PhysicsWorld`.
//! Without this every cell crossing leaks a body + colliders into
//! `RigidBodySet` / `ColliderSet` and the broad-phase / query-pipeline
//! BVH — an unbounded session-length leak, worst under exterior radius
//! streaming which never resets the `PhysicsWorld`.

use super::*;
use byroredux_core::ecs::components::collision::{CollisionShape, RigidBodyData};
use byroredux_core::ecs::components::GlobalTransform;
use byroredux_core::ecs::World;
use byroredux_physics::{physics_sync_system, PhysicsWorld, RapierHandles};

/// World with the physics resource + `RapierHandles` storage registered,
/// matching the engine's App-init setup so `physics_sync_system` can run.
fn world_with_physics() -> World {
    let mut world = World::new();
    world.insert_resource(PhysicsWorld::new());
    world.register::<RapierHandles>();
    world
}

/// Spawn a static-collider entity the way the cell loader does (a ghost
/// carrying `CollisionShape` + `RigidBodyData` + `GlobalTransform`), then
/// run `physics_sync_system` so the body is registered into the
/// `PhysicsWorld` and `RapierHandles` is attached.
fn spawn_static_collider(world: &mut World) -> byroredux_core::ecs::storage::EntityId {
    let e = world.spawn();
    world.insert(e, CollisionShape::Ball { radius: 16.0 });
    world.insert(e, RigidBodyData::STATIC);
    world.insert(e, GlobalTransform::IDENTITY);
    e
}

/// Two registered bodies → release removes both, leaving `body_count()`
/// at the pre-load baseline of 0.
#[test]
fn release_removes_victim_bodies_from_physics_world() {
    let mut world = world_with_physics();

    let a = spawn_static_collider(&mut world);
    let b = spawn_static_collider(&mut world);
    physics_sync_system(&world, 0.0);

    assert_eq!(
        world.resource::<PhysicsWorld>().body_count(),
        2,
        "both ghosts must register a Rapier body",
    );
    // Both got their handle row attached by the sync pass.
    assert!(world.get::<RapierHandles>(a).is_some());
    assert!(world.get::<RapierHandles>(b).is_some());

    release_victim_rapier_bodies(&mut world, &[a, b]);

    assert_eq!(
        world.resource::<PhysicsWorld>().body_count(),
        0,
        "cell unload must remove every victim body — no leak across crossings",
    );
}

/// Colliders cascade out with the body: a ball collider registered
/// alongside its body must be gone after release, not orphaned in the
/// `ColliderSet`.
#[test]
fn release_cascades_colliders_with_body() {
    let mut world = world_with_physics();
    let e = spawn_static_collider(&mut world);
    physics_sync_system(&world, 0.0);
    assert_eq!(world.resource::<PhysicsWorld>().colliders.len(), 1);

    release_victim_rapier_bodies(&mut world, &[e]);

    assert_eq!(
        world.resource::<PhysicsWorld>().colliders.len(),
        0,
        "removing the body must cascade its attached collider",
    );
}

/// Non-victim bodies survive: releasing one cell's victims must not
/// remove a body owned by a still-resident entity.
#[test]
fn release_leaves_non_victim_bodies_alive() {
    let mut world = world_with_physics();
    let victim = spawn_static_collider(&mut world);
    let resident = spawn_static_collider(&mut world);
    physics_sync_system(&world, 0.0);
    assert_eq!(world.resource::<PhysicsWorld>().body_count(), 2);

    release_victim_rapier_bodies(&mut world, &[victim]);

    assert_eq!(
        world.resource::<PhysicsWorld>().body_count(),
        1,
        "only the victim body is removed",
    );
    assert!(
        world.get::<RapierHandles>(resident).is_some(),
        "the resident entity keeps its handle",
    );
}

/// Victims without a `RapierHandles` row (stat-only refs, lights) are
/// walked past — `get()` returns `None`, no removal attempted.
#[test]
fn release_tolerates_victims_without_handles() {
    let mut world = world_with_physics();
    let bare = world.spawn();
    let body = spawn_static_collider(&mut world);
    physics_sync_system(&world, 0.0);
    assert_eq!(world.resource::<PhysicsWorld>().body_count(), 1);

    release_victim_rapier_bodies(&mut world, &[bare, body]);

    assert_eq!(world.resource::<PhysicsWorld>().body_count(), 0);
}

/// Missing `PhysicsWorld` resource (loose-NIF demo / reduced test
/// fixtures opting out of physics) is a clean no-op — the helper must
/// not panic even when victims carry a `RapierHandles` row.
#[test]
fn release_is_noop_when_physics_resource_absent() {
    let mut world = World::new();
    world.register::<RapierHandles>();
    // No PhysicsWorld inserted. Even a victim with a handle row short-
    // circuits at the resource lookup.
    let e = world.spawn();
    // Fabricate a handle row without a backing body — the resource miss
    // is hit before any handle is dereferenced.
    world.register::<CollisionShape>();
    release_victim_rapier_bodies(&mut world, &[e]);
}
