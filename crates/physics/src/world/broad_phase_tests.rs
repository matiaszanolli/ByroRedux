//! Regression guards for the broad phase's fixed-on-fixed pair filter.
//!
//! Until the rapier 0.36 upgrade the engine wrapped rapier 0.22's MultiSAP
//! broad phase in `FixedPairFilterBroadPhase`: rapier turned every overlap
//! into a narrow-phase graph edge, and on FO4 Commonwealth 0,0 radius 1
//! (15 941 fixed bodies on the first tick) the narrow phase spent 123 ms of a
//! 176 ms first step inserting fixed-on-fixed pairs that could never produce
//! a contact, 35 ms of a 101 ms step removing them when three cells unloaded,
//! and 1–5 ms of every later step walking them. Rapier 0.35+'s BVH broad
//! phase applies the same `ActiveCollisionTypes` filter before a pair is
//! created and re-inserts a body's colliders when its type changes, so the
//! wrapper is gone — these tests pin that the native behaviour still gives
//! the three guarantees the wrapper was written for.

use super::{PhysicsWorld, PHYSICS_DT};
use byroredux_core::ecs::components::MotionType;
use rapier3d::prelude::*;

fn cuboid(
    world: &mut PhysicsWorld,
    body: RigidBodyBuilder,
    center: [f32; 3],
    half_extents: [f32; 3],
) -> (RigidBodyHandle, ColliderHandle) {
    let body = world
        .bodies
        .insert(body.translation(Vector::new(center[0], center[1], center[2])));
    let collider = world.colliders.insert_with_parent(
        ColliderBuilder::cuboid(half_extents[0], half_extents[1], half_extents[2]).build(),
        body,
        &mut world.bodies,
    );
    (body, collider)
}

/// A fixed crate resting on a fixed floor.
fn crate_on_floor() -> (
    PhysicsWorld,
    ColliderHandle,
    (RigidBodyHandle, ColliderHandle),
) {
    let mut world = PhysicsWorld::new();
    let (_, floor) = cuboid(
        &mut world,
        RigidBodyBuilder::fixed(),
        [0.0, 0.0, 0.0],
        [1000.0, 10.0, 1000.0],
    );
    let crate_ = cuboid(
        &mut world,
        RigidBodyBuilder::fixed(),
        [0.0, 60.0, 0.0],
        [50.0, 50.0, 50.0],
    );
    world.wake();
    world.step(PHYSICS_DT);
    (world, floor, crate_)
}

#[test]
fn two_fixed_bodies_never_reach_the_narrow_phase() {
    let (mut world, floor, (_, crate_)) = crate_on_floor();
    let (_, ball) = cuboid(
        &mut world,
        RigidBodyBuilder::dynamic(),
        [500.0, 15.0, 0.0],
        [10.0, 10.0, 10.0],
    );
    // A spawn does not arm a step (#3969); something else must.
    world.wake();
    world.step(PHYSICS_DT);

    assert!(
        world.narrow_phase.contact_pair(floor, crate_).is_none(),
        "a fixed-on-fixed overlap must be withheld"
    );
    assert!(
        world.narrow_phase.contact_pair(floor, ball).is_some(),
        "a dynamic body's overlap with fixed geometry must still be registered"
    );
}

/// `SetMotionType(Dynamic)` on a static reference: the pair with the floor
/// it rests on was withheld while both were fixed, and the broad phase
/// reports an overlap only once. Without a re-report the crate would sink
/// into the floor, so the pair has to exist on the very first step.
#[test]
fn a_fixed_body_turned_dynamic_stays_on_what_it_rested_on() {
    let (mut world, floor, (crate_body, crate_)) = crate_on_floor();
    assert!(world.narrow_phase.contact_pair(floor, crate_).is_none());

    assert!(world.set_motion_type(crate_body, MotionType::Dynamic, true));
    world.step(PHYSICS_DT);
    assert!(
        world.narrow_phase.contact_pair(floor, crate_).is_some(),
        "the crate's overlap with the floor must be reported on the next step"
    );

    for _ in 0..120 {
        world.step(PHYSICS_DT);
    }
    let height = world.bodies[crate_body].translation().y;
    assert!(
        height > 50.0,
        "the crate must rest on the floor (top at 10), not fall through: y = {height}"
    );
}

/// Unloading static geometry deletes its withheld pairs, which the narrow
/// phase never had.
#[test]
fn removing_a_fixed_body_with_withheld_pairs_is_a_no_op_for_them() {
    let (mut world, floor, (crate_body, _)) = crate_on_floor();
    assert!(world.remove_body(crate_body));
    world.wake();
    world.step(PHYSICS_DT);

    let (_, ball) = cuboid(
        &mut world,
        RigidBodyBuilder::dynamic(),
        [0.0, 15.0, 0.0],
        [10.0, 10.0, 10.0],
    );
    world.wake();
    world.step(PHYSICS_DT);
    assert!(world.narrow_phase.contact_pair(floor, ball).is_some());
}
