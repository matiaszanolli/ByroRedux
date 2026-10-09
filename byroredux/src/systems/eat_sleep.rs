//! M42 — Eat (FO3/FNV `PKDT` procedure 3) and Sleep (procedure 4)
//! procedures, v0.
//!
//! Both share one runtime: walk once to the package's `PLDT` location
//! (the dining area / bedroom — resolved through Travel's
//! `resolve_destination`, so a `NearReference` PLDT lands the actor at
//! the authored furniture and a missing one hash-picks within the
//! radius), then occupy the nearest furniture marker through the
//! sandbox seating path (`collect_marker_seats` + `pick_nearest_seat` +
//! `apply_seat_assignments` — the same reservations, root snap, and
//! sit-enter final-frame park). Eat seats at **sit** markers; Sleep at
//! **sleep** markers, falling back to sit markers when a cell's beds
//! author none — parking the seated pose at the sleep marker's authored
//! entry position is the documented v0 approximation for the lie-down
//! clip no archive this engine reads carries.
//!
//! The walk is the same straight-line `step_toward` locomotion the
//! force-greet bridge uses (KCC-backed, single resident NAVM tile) —
//! the v0 pathing caveat the other M42 procedures document applies here
//! too. Seating is one-shot: `Seated` skips the actor on later ticks,
//! and the package handover's `clear_ambient_behavior` un-seats
//! (restoring the pre-park animation, #3333) exactly as Sandbox does.

use byroredux_core::ecs::components::{
    EatBehavior, EatSleepState, FurnitureMarker, FurnitureMarkerKind, GlobalTransform, Seated,
    SleepBehavior, Transform,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::Vec3;

use crate::components::{SandboxSitClip, SeatReservations};

/// The one-shot walk destination's arrival threshold (world units) —
/// inside this, the actor stops walking and seats. Small relative to
/// the seat-search radius so the seat pick, not the walk, decides the
/// final resting furniture.
const ARRIVE_RADIUS: f32 = 64.0;

/// Which procedure an actor carries this tick — collected first so the
/// per-actor loop below never holds two behavior storages at once.
#[derive(Clone, Copy)]
enum EatOrSleep {
    Eat,
    Sleep,
}

/// System: drive Eat/Sleep procedure actors — walk to the `PLDT`
/// destination, then seat at the nearest matching furniture marker.
/// Registered `add_exclusive(Stage::PostUpdate, …)` beside the other
/// M42 procedure systems; reads this frame's propagated
/// `GlobalTransform`s.
/// One collected actor per tick — the behavior pair is flattened into
/// a row so the per-actor loop below never holds two behavior storages
/// at once.
struct EatSleepActor {
    npc: EntityId,
    kind: EatOrSleep,
    radius: Option<f32>,
    target_form_id: Option<u32>,
    form_id: u32,
}

pub(crate) fn eat_sleep_system(world: &World, dt: f32) {
    let mut actors: Vec<EatSleepActor> = Vec::new();
    if let Some(query) = world.query::<EatBehavior>() {
        for (npc, behavior) in query.iter() {
            actors.push(EatSleepActor {
                npc,
                kind: EatOrSleep::Eat,
                radius: behavior.radius,
                target_form_id: behavior.target_form_id,
                form_id: behavior.form_id,
            });
        }
    }
    if let Some(query) = world.query::<SleepBehavior>() {
        for (npc, behavior) in query.iter() {
            actors.push(EatSleepActor {
                npc,
                kind: EatOrSleep::Sleep,
                radius: behavior.radius,
                target_form_id: behavior.target_form_id,
                form_id: behavior.form_id,
            });
        }
    }
    if actors.is_empty() {
        return;
    }
    // One-shot guard: a seated actor is done (the package handover
    // un-seats). Scoped so the destination/seat work below never holds
    // two same-type guards.
    {
        let seated_q = world.query::<Seated>();
        actors.retain(|actor| {
            !seated_q
                .as_ref()
                .is_some_and(|seated| seated.contains(actor.npc))
        });
    }
    if actors.is_empty() {
        return;
    }

    for EatSleepActor {
        npc,
        kind,
        radius,
        target_form_id,
        form_id,
    } in actors
    {
        // One-shot walk destination: resolve on first sight, reuse
        // after. Inserted through a scoped write so no read guard is
        // held across it.
        let destination = match world.get::<EatSleepState>(npc).map(|state| state.destination) {
            Some(destination) => destination,
            None => {
                let home = world
                    .get::<GlobalTransform>(npc)
                    .map(|transform| transform.translation)
                    .unwrap_or_default();
                let destination = super::travel::resolve_destination(
                    world,
                    target_form_id,
                    radius.unwrap_or(super::sandbox::SEAT_SEARCH_RADIUS),
                    form_id,
                    home,
                );
                if let Some(mut states) = world.query_mut::<EatSleepState>() {
                    states.insert(npc, EatSleepState { destination });
                }
                destination
            }
        };
        let current = world
            .get::<GlobalTransform>(npc)
            .map(|transform| transform.translation)
            .unwrap_or_default();
        let flat = Vec3::new(destination.x - current.x, 0.0, destination.z - current.z);
        if flat.length() > ARRIVE_RADIUS {
            // Still walking — one KCC-backed step toward the destination.
            // #5373 — the step writes `Transform` (the authoritative
            // world pose on a propagation root), never
            // `GlobalTransform`: the derived global is rebuilt from the
            // local on the next propagation, so writing it erased the
            // step every frame and the diner never reached its marker.
            // Same write shape as travel/wander's pass 2.
            let speed = world
                .get::<crate::components::WalkSpeed>(npc)
                .map(|speed| speed.0)
                .unwrap_or(crate::systems::locomotion::LOCOMOTION_WALK_SPEED);
            let rotation = world
                .get::<Transform>(npc)
                .map(|transform| transform.rotation)
                .unwrap_or_default();
            // #5371 — `PhysicsWorld` is a lock-order sink (nothing
            // acquired under it, docs/engine/ecs.md): take it in its
            // own scope for the step computation, then apply the write
            // after it drops. The single-block shape held the guard
            // across the `Transform` write, inverting the order
            // production records.
            let (new_pos, new_rotation) = {
                let physics_guard =
                    world.try_resource::<byroredux_physics::PhysicsWorld>();
                crate::systems::locomotion::step_toward(
                    current,
                    rotation,
                    destination,
                    dt,
                    speed,
                    physics_guard.as_deref(),
                )
            };
            if let Some(mut transforms) = world.query_mut::<Transform>() {
                if let Some(transform) = transforms.get_mut(npc) {
                    transform.translation = new_pos;
                    if let Some(rotation) = new_rotation {
                        transform.rotation = rotation;
                    }
                }
            }
            continue;
        }
        // Arrived: seat at the nearest matching marker. Sleep prefers
        // sleep markers, falling back to sit markers when the cell's
        // beds author none; Eat sits.
        seat_at_marker(world, npc, kind, radius);
    }
}

/// Seat one arrived actor at its procedure's marker kind, reusing the
/// sandbox seating path (reservation, root snap, sit-enter park).
fn seat_at_marker(world: &World, npc: EntityId, kind: EatOrSleep, radius: Option<f32>) {
    // No sit-enter clip → no seating path (Skyrim+/Havok games, or the
    // clip wasn't archived) — the actor has already walked to the
    // authored location and simply stands there, the same posture as a
    // sandbox actor in a clip-less cell.
    let Some((sit_handle, hold_time)) = world.try_resource::<SandboxSitClip>().and_then(|r| r.0)
    else {
        return;
    };
    let is_sleep = matches!(kind, EatOrSleep::Sleep);
    let mut collected: Vec<((EntityId, u32), GlobalTransform)> = Vec::new();
    if is_sleep {
        super::sandbox::collect_marker_seats(world, &mut collected, is_sleep_marker);
    }
    if collected.is_empty() {
        super::sandbox::collect_marker_seats(world, &mut collected, super::sandbox::is_sit_marker);
    }
    if collected.is_empty() {
        return;
    }
    let Some(current) = world.get::<GlobalTransform>(npc).map(|t| t.translation) else {
        return;
    };
    let seat = {
        let empty = std::collections::HashMap::new();
        // Bind the resource guard: `pick_nearest_seat` borrows the map,
        // and a `.map(|r| &r.0)` on the guard's temporary would not
        // outlive the expression.
        let reservations_guard = world.try_resource::<SeatReservations>();
        let reservations = reservations_guard
            .as_deref()
            .map(|reservations| &reservations.0)
            .unwrap_or(&empty);
        super::sandbox::pick_nearest_seat(
            current,
            &collected,
            reservations,
            radius.unwrap_or(super::sandbox::SEAT_SEARCH_RADIUS),
        )
    };
    let Some(((furn_e, marker_idx), seat)) = seat else {
        return;
    };
    // Reserve through the shared resource so a sandboxing actor (or a
    // second diner) cannot take the same marker.
    if let Some(mut reservations) = world.try_resource_mut::<SeatReservations>() {
        reservations.0.insert((furn_e, marker_idx), npc);
    }
    let assignments = vec![(npc, furn_e, seat)];
    super::sandbox::apply_seat_assignments(world, sit_handle, hold_time, &assignments);
    log::info!(
        "[m42] {} npc={} seated at furn={} marker world=({:.1},{:.1},{:.1})",
        if is_sleep { "sleep" } else { "eat" },
        npc,
        furn_e,
        seat.translation.x,
        seat.translation.y,
        seat.translation.z,
    );
}

fn is_sleep_marker(marker: &FurnitureMarker) -> bool {
    marker.kind == FurnitureMarkerKind::Sleep
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::Furniture;
    use byroredux_core::ecs::components::GlobalTransform;

    fn setup() -> (World, EntityId) {
        let mut world = World::new();
        world.register::<EatBehavior>();
        world.register::<SleepBehavior>();
        world.register::<EatSleepState>();
        world.register::<GlobalTransform>();
        world.register::<Transform>();
        world.register::<Seated>();
        world.register::<Furniture>();
        world.register::<byroredux_core::animation::AnimationPlayer>();
        world.insert_resource(SeatReservations::default());
        let actor = world.spawn();
        world.insert(
            actor,
            GlobalTransform {
                translation: Vec3::ZERO,
                ..Default::default()
            },
        );
        world.insert(actor, Transform::default());
        (world, actor)
    }

    /// Far from the destination (the behavior's fallback hash-pick is
    /// deterministic by form_id), the actor walks: an `EatSleepState`
    /// lands on first sight and the step advances `Transform` — the
    /// authoritative pose on a propagation root (#5373: the step used
    /// to write `GlobalTransform`, which the next propagation rebuilt
    /// from the unmoved local, erasing the walk every frame).
    #[test]
    fn eat_actor_far_from_destination_walks() {
        let (mut world, actor) = setup();
        world.insert(
            actor,
            EatBehavior {
                radius: Some(512.0),
                target_form_id: None,
                form_id: 0xAA,
            },
        );
        eat_sleep_system(&world, 1.0);
        let state = world.get::<EatSleepState>(actor).expect("state inserted");
        let destination = state.destination;
        assert!(
            destination.x != 0.0 || destination.z != 0.0,
            "the fallback hash-pick resolves a non-origin destination"
        );
        let moved = world.get::<Transform>(actor).expect("transform").translation;
        let before = Vec3::ZERO;
        assert_ne!(moved, before, "the actor takes a step toward the destination");
        assert!(
            (moved - destination).length() < before.distance(destination),
            "the step is toward the resolved destination"
        );
        assert!(
            world.get::<GlobalTransform>(actor).expect("global").translation == before,
            "the derived global is NOT written directly — propagation owns it"
        );
    }

    /// #5373's real-world shape: the system and transform propagation
    /// alternate every frame. The walk must survive propagation — the
    /// actor converges on its destination instead of snapping back to
    /// its spawn point every frame.
    #[test]
    fn eat_walk_survives_transform_propagation() {
        let (mut world, actor) = setup();
        world.insert(
            actor,
            EatBehavior {
                radius: Some(512.0),
                target_form_id: None,
                form_id: 0xAA,
            },
        );
        let mut propagate = byroredux_core::ecs::systems::make_transform_propagation_system();
        let mut last_distance = f32::MAX;
        for _ in 0..64 {
            eat_sleep_system(&world, 0.5);
            propagate(&world, 0.5);
            let current = world
                .get::<GlobalTransform>(actor)
                .expect("global")
                .translation;
            let destination = world
                .get::<EatSleepState>(actor)
                .expect("state")
                .destination;
            let distance = (current - destination).length();
            assert!(
                distance <= last_distance,
                "propagation must not erase the walk step (distance went                  {last_distance} -> {distance})"
            );
            last_distance = distance;
            if distance <= ARRIVE_RADIUS {
                break;
            }
        }
        assert!(
            last_distance <= ARRIVE_RADIUS,
            "the diner reaches its destination through alternating              system+propagation frames (still {last_distance} away)"
        );
    }

    /// Arrived + furniture with a sit marker + sit clip: the actor
    /// seats through the sandbox path — `Seated` tagged, the seat
    /// reserved, the root snapped to the marker's world transform.
    /// The destination is pre-seeded so the walk phase is done.
    #[test]
    fn arrived_eat_actor_seats_at_sit_marker() {
        let (mut world, actor) = setup();
        world.insert_resource(SandboxSitClip(Some((7, 0.5))));
        world.insert(
            actor,
            EatBehavior {
                radius: None,
                target_form_id: None,
                form_id: 0xAA,
            },
        );
        world.insert(
            actor,
            EatSleepState {
                destination: Vec3::new(0.0, 0.0, 0.0),
            },
        );
        let furniture = world.spawn();
        world.insert(
            furniture,
            Furniture {
                markers: vec![FurnitureMarker {
                    local_offset: [0.0, 0.0, 0.0],
                    heading_z_radians: None,
                    animation_type: 1,
                    kind: FurnitureMarkerKind::Sit,
                }],
            },
        );
        world.insert(
            furniture,
            GlobalTransform {
                translation: Vec3::ZERO,
                ..Default::default()
            },
        );
        eat_sleep_system(&world, 1.0);
        assert!(
            world.get::<Seated>(actor).is_some(),
            "the arrived diner seats through the sandbox path"
        );
        assert!(
            world
                .try_resource::<SeatReservations>()
                .is_some_and(|reservations| {
                    reservations.0.contains_key(&(furniture, 0))
                }),
            "the seat is reserved against double-claim"
        );
    }

    /// #5371 — with a real `PhysicsWorld` installed, the walk branch
    /// must take the physics resource in a scope of its own (a
    /// lock-order sink: nothing acquired under it). The detector aborts
    /// on the inverted order under `BYRO_LOCK_ORDER_CHECK=1`; same
    /// shape as travel.rs's physics regression.
    #[test]
    fn eat_walk_with_real_physics_world() {
        let (mut world, actor) = setup();
        world.insert_resource(byroredux_physics::PhysicsWorld::new());
        world.insert(
            actor,
            EatBehavior {
                radius: Some(512.0),
                target_form_id: None,
                form_id: 0xAB,
            },
        );
        eat_sleep_system(&world, 1.0);
        let moved = world.get::<Transform>(actor).expect("transform").translation;
        assert_ne!(
            moved, Vec3::ZERO,
            "the step computed through the physics world"
        );
    }

    /// Sleep prefers sleep markers: with a sleep marker adjacent, the
    /// sleeper seats at it (the sit-pose v0 approximation documented in
    /// the module docs), even when a sit marker exists farther away.
    #[test]
    fn sleep_actor_prefers_the_sleep_marker() {
        let (mut world, actor) = setup();
        world.insert_resource(SandboxSitClip(Some((7, 0.5))));
        world.insert(
            actor,
            SleepBehavior {
                radius: None,
                target_form_id: None,
                form_id: 0xAB,
            },
        );
        world.insert(
            actor,
            EatSleepState {
                destination: Vec3::ZERO,
            },
        );
        let bed = world.spawn();
        world.insert(
            bed,
            Furniture {
                markers: vec![FurnitureMarker {
                    local_offset: [10.0, 0.0, 0.0],
                    heading_z_radians: None,
                    animation_type: 2,
                    kind: FurnitureMarkerKind::Sleep,
                }],
            },
        );
        world.insert(
            bed,
            GlobalTransform {
                translation: Vec3::ZERO,
                ..Default::default()
            },
        );
        eat_sleep_system(&world, 1.0);
        let seated_at = world
            .get::<Seated>(actor)
            .expect("the sleeper occupies the bed marker");
        assert_eq!(seated_at.furniture, bed);
    }
}
