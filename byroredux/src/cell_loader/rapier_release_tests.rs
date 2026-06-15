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
use byroredux_core::math::{Quat, Vec3};
use byroredux_physics::{
    build_ragdoll, physics_sync_system, ContactConfig, PhysicsWorld, Ragdoll, RagdollBodySpec,
    RagdollConstraintSpec, RagdollJointSpec, RagdollSpec, RapierHandles,
};

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

// ── #1531 — Ragdoll bodies/colliders/joints must also be swept ─────────
//
// The PHYSAL ragdoll path attaches a `Ragdoll` component carrying its own
// `Vec<(EntityId, RigidBodyHandle)>` + multibody joints, inserted directly
// into the solver sets — NOT through `RapierHandles`. Cell unload must sweep
// it too, or `World::despawn` drops the row and orphans every ragdoll body +
// collider + joint (the #1520 leak class, re-introduced for the new
// component).

/// World with the physics resource + `Ragdoll` storage registered.
fn world_with_ragdoll_physics() -> World {
    let mut world = World::new();
    world.insert_resource(PhysicsWorld::new());
    world.register::<RapierHandles>();
    world.register::<Ragdoll>();
    world
}

fn ball_body(entity: byroredux_core::ecs::storage::EntityId, x: f32) -> RagdollBodySpec {
    RagdollBodySpec {
        entity,
        translation: Vec3::new(x, 0.0, 0.0),
        rotation: Quat::IDENTITY,
        shape: CollisionShape::Ball { radius: 5.0 },
        mass: 4.0,
        linear_damping: 0.05,
        angular_damping: 0.05,
        friction: 0.5,
        restitution: 0.0,
    }
}

/// Build a 2-body, 1-joint ragdoll into `pw` and attach the resulting
/// `Ragdoll` component to a fresh actor entity. Returns the actor and a
/// clone of the `Ragdoll` (its joint handles drive post-release liveness
/// checks via `MultibodyJointSet::get`, which is panic-safe where `iter()`
/// is not).
fn spawn_ragdoll_actor(
    world: &mut World,
) -> (byroredux_core::ecs::storage::EntityId, Ragdoll) {
    let actor = world.spawn();
    let bone_a = world.spawn();
    let bone_b = world.spawn();
    let spec = RagdollSpec {
        bodies: vec![ball_body(bone_a, 0.0), ball_body(bone_b, 50.0)],
        constraints: vec![RagdollConstraintSpec {
            body_a: 0,
            body_b: 1,
            joint: RagdollJointSpec::Ragdoll {
                twist_a: Vec3::X,
                plane_a: Vec3::Y,
                pivot_a: Vec3::new(25.0, 0.0, 0.0),
                twist_b: Vec3::X,
                plane_b: Vec3::Y,
                pivot_b: Vec3::new(-25.0, 0.0, 0.0),
                cone_max: std::f32::consts::PI,
                twist_min: -std::f32::consts::PI,
                twist_max: std::f32::consts::PI,
            },
        }],
    };
    let ragdoll = {
        let mut pw = world.resource_mut::<PhysicsWorld>();
        build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT)
    };
    world.insert(actor, ragdoll.clone());
    (actor, ragdoll)
}

/// A ragdolling actor's bodies, colliders, AND multibody joints must all
/// be gone after release — the leak #1531 closes.
#[test]
fn release_removes_ragdoll_bodies_colliders_and_joints() {
    let mut world = world_with_ragdoll_physics();
    let (actor, ragdoll) = spawn_ragdoll_actor(&mut world);
    assert_eq!(ragdoll.joints.len(), 1, "one multibody joint built");

    {
        let pw = world.resource::<PhysicsWorld>();
        assert_eq!(pw.body_count(), 2, "ragdoll registered both bodies");
        assert_eq!(pw.colliders.len(), 2, "one collider per ragdoll body");
        assert!(
            ragdoll.joints.iter().all(|&h| pw.multibody_joints.get(h).is_some()),
            "the multibody joint is live before unload",
        );
    }

    release_victim_rapier_bodies(&mut world, &[actor]);

    let pw = world.resource::<PhysicsWorld>();
    assert_eq!(
        pw.body_count(),
        0,
        "cell unload must remove every ragdoll body — no leak across crossings",
    );
    assert_eq!(
        pw.colliders.len(),
        0,
        "removing each ragdoll body cascades its collider",
    );
    assert!(
        ragdoll.joints.iter().all(|&h| pw.multibody_joints.get(h).is_none()),
        "ragdoll multibody joints must not survive in the solver",
    );
}

/// A ragdoll victim and a `RapierHandles` victim in the same unload are
/// both swept — the two component classes coexist (an actor that ragdolls
/// while character bodies remain resident).
#[test]
fn release_sweeps_both_ragdoll_and_rapier_handles() {
    let mut world = world_with_ragdoll_physics();

    let (actor, ragdoll) = spawn_ragdoll_actor(&mut world); // 2 bodies
    let collider = spawn_static_collider(&mut world); // +1 body via sync
    physics_sync_system(&world, 0.0);

    assert_eq!(world.resource::<PhysicsWorld>().body_count(), 3);

    release_victim_rapier_bodies(&mut world, &[actor, collider]);

    let pw = world.resource::<PhysicsWorld>();
    assert_eq!(pw.body_count(), 0, "both the ragdoll and the handle body cleared");
    assert!(
        ragdoll.joints.iter().all(|&h| pw.multibody_joints.get(h).is_none()),
        "the ragdoll's joints are gone too",
    );
}
