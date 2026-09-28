//! [`FixedPairFilterBroadPhase`] — Rapier's broad phase without the pairs
//! of two fixed bodies.

use rapier3d::prelude::*;

/// Rapier's default (MultiSAP) broad phase, minus the overlaps no step can
/// act on: two colliders on fixed bodies, neither of whose
/// `ActiveCollisionTypes` admits fixed-on-fixed contact (the default).
///
/// Rapier 0.22 turns every broad-phase overlap into an edge of the narrow
/// phase's contact graph and applies the body-type filter only as it walks
/// the edges on each step. Static placements overlap densely, and on FO4
/// Commonwealth 0,0 radius 1 — 15 941 fixed bodies on the first tick — the
/// narrow phase spent 123 ms of the first step's 176 ms inserting pairs that
/// could never produce a contact, 35 ms of a 101 ms step removing them when
/// three cells unloaded, and 1–5 ms of every later step walking them.
///
/// Withholding a pair is sound only while both bodies stay fixed: the broad
/// phase reports an overlap once, when it begins. So a body that leaves the
/// fixed type has its colliders handed to [`Self::body_left_fixed`]
/// ([`crate::PhysicsWorld::set_motion_type`] does), and the next update
/// reports their overlaps with fixed colliders afresh.
#[derive(Default)]
pub struct FixedPairFilterBroadPhase {
    inner: DefaultBroadPhase,
    /// Colliders whose overlaps with fixed colliders the next update reports:
    /// their body left the fixed type after those pairs were withheld.
    rereport: Vec<ColliderHandle>,
}

impl FixedPairFilterBroadPhase {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue `colliders` — all of a body that just stopped being fixed — so
    /// the next update reports the overlaps withheld while it was.
    pub fn body_left_fixed(&mut self, colliders: &[ColliderHandle]) {
        self.rereport.extend_from_slice(colliders);
    }
}

impl BroadPhase for FixedPairFilterBroadPhase {
    fn update(
        &mut self,
        dt: Real,
        prediction_distance: Real,
        colliders: &mut ColliderSet,
        bodies: &RigidBodySet,
        modified_colliders: &[ColliderHandle],
        removed_colliders: &[ColliderHandle],
        events: &mut Vec<BroadPhasePairEvent>,
    ) {
        self.inner.update(
            dt,
            prediction_distance,
            colliders,
            bodies,
            modified_colliders,
            removed_colliders,
            events,
        );
        let colliders = &*colliders;
        // A `DeletePair` for a withheld pair reaches the narrow phase too;
        // removing an edge it never added is a no-op there.
        events.retain(|event| match event {
            BroadPhasePairEvent::AddPair(pair) => !withheld(pair, colliders, bodies),
            BroadPhasePairEvent::DeletePair(_) => true,
        });

        for handle in self.rereport.drain(..) {
            let Some(collider) = colliders.get(handle) else {
                continue;
            };
            // Back to fixed before this update: nothing to report.
            if is_fixed(collider, bodies) {
                continue;
            }
            // The overlap test the MultiSAP proxies use.
            let aabb = collider.compute_collision_aabb(prediction_distance / 2.0);
            for (other, other_collider) in colliders.iter() {
                // Pairs with a non-fixed collider were never withheld;
                // re-sending one would be allowed, but is wasted work.
                if other == handle
                    || !other_collider.is_enabled()
                    || !is_fixed(other_collider, bodies)
                    || (collider.parent().is_some() && collider.parent() == other_collider.parent())
                {
                    continue;
                }
                if aabb
                    .intersects(&other_collider.compute_collision_aabb(prediction_distance / 2.0))
                {
                    events.push(BroadPhasePairEvent::AddPair(ColliderPair::new(
                        handle, other,
                    )));
                }
            }
        }
    }
}

/// Whether the narrow phase would reject `pair` on every step: both colliders
/// on fixed bodies, and neither admitting fixed-on-fixed contact.
fn withheld(pair: &ColliderPair, colliders: &ColliderSet, bodies: &RigidBodySet) -> bool {
    let (Some(collider1), Some(collider2)) =
        (colliders.get(pair.collider1), colliders.get(pair.collider2))
    else {
        return false;
    };
    let admits_fixed_pairs = |collider: &Collider| {
        collider
            .active_collision_types()
            .test(RigidBodyType::Fixed, RigidBodyType::Fixed)
    };
    is_fixed(collider1, bodies)
        && is_fixed(collider2, bodies)
        && !admits_fixed_pairs(collider1)
        && !admits_fixed_pairs(collider2)
}

/// The narrow phase's reading: a collider without a parent body counts as
/// fixed.
fn is_fixed(collider: &Collider, bodies: &RigidBodySet) -> bool {
    collider
        .parent()
        .and_then(|handle| bodies.get(handle))
        .is_none_or(|body| body.body_type() == RigidBodyType::Fixed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{PhysicsWorld, PHYSICS_DT};
    use byroredux_core::ecs::components::MotionType;

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

    /// `SetMotionType(Dynamic)` on a static reference: the pair with the
    /// floor it rests on was withheld while both were fixed, and the broad
    /// phase will not report an overlap it already has. The crate would sink
    /// into the floor until its box happened to cross into another broad-phase
    /// region, so the pair has to exist on the very first step.
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

    /// Unloading static geometry deletes its withheld pairs, which the
    /// narrow phase never had.
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
}
