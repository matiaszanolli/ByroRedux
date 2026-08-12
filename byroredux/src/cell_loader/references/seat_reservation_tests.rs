//! #2147 / ECS-2507-01 — seat claims must survive a sibling cell's load.
//!
//! Extracted from `references/mod.rs` (#2409 / TD1-006).

use super::prune_seat_reservations;
use crate::components::SeatReservations;
use byroredux_core::ecs::components::{Furniture, Seated};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;

fn world_with_furniture(count: usize) -> (World, Vec<EntityId>) {
    let mut world = World::new();
    world.register::<Furniture>();
    world.register::<Seated>();
    world.insert_resource(SeatReservations::default());
    let ids = (0..count)
        .map(|_| {
            let e = world.spawn();
            world.insert(e, Furniture::default());
            e
        })
        .collect();
    (world, ids)
}

fn seat_actor(world: &mut World, furniture: EntityId) -> EntityId {
    let actor = world.spawn();
    world.insert(actor, Seated { furniture });
    actor
}

/// The bug. Cell A's furniture is seated and still resident; loading cell
/// B must not release A's claim.
///
/// On an exterior grid this ran per `(gx, gy)` — 49 times at
/// `--radius 3` — and again per cell streamed in at a boundary crossing.
/// `Seated` is a one-shot terminal marker, so the actor never re-asserts
/// its claim: the seat stayed physically occupied but was free to claim.
#[test]
fn sibling_cell_load_keeps_claims_on_still_loaded_furniture() {
    let (mut world, furniture) = world_with_furniture(2);
    let (cell_a_chair, cell_b_chair) = (furniture[0], furniture[1]);
    let actor = seat_actor(&mut world, cell_a_chair);

    world
        .resource_mut::<SeatReservations>()
        .0
        .insert((cell_a_chair, 0), actor);

    // Cell B loads. Both cells' furniture is resident.
    prune_seat_reservations(&world);

    let reservations = world.resource::<SeatReservations>();
    assert!(
        reservations.0.contains_key(&(cell_a_chair, 0)),
        "cell A's seat is still occupied and its furniture still loaded — \
     the claim must survive cell B's load (#2147)",
    );
    assert_eq!(reservations.0.len(), 1);
    assert!(!reservations.0.contains_key(&(cell_b_chair, 0)));
}

/// The prune must still do its job: a claim whose furniture was unloaded
/// is dead weight and goes.
#[test]
fn claims_on_despawned_furniture_are_dropped() {
    let (mut world, furniture) = world_with_furniture(2);
    let (kept, unloaded) = (furniture[0], furniture[1]);
    let kept_actor = seat_actor(&mut world, kept);
    let unloaded_actor = seat_actor(&mut world, unloaded);
    {
        let mut r = world.resource_mut::<SeatReservations>();
        r.0.insert((kept, 0), kept_actor);
        r.0.insert((unloaded, 3), unloaded_actor);
    }

    world.despawn(unloaded);
    prune_seat_reservations(&world);

    let reservations = world.resource::<SeatReservations>();
    assert!(reservations.0.contains_key(&(kept, 0)));
    assert!(
        !reservations.0.contains_key(&(unloaded, 3)),
        "a claim on despawned furniture must not accumulate",
    );
}

/// #2392 — an actor can stream out while cross-cell furniture remains.
/// Its claim must be released so a replacement actor can reserve the
/// still-live marker.
#[test]
fn claim_is_released_when_seated_actor_despawns() {
    let (mut world, furniture) = world_with_furniture(1);
    let chair = furniture[0];
    let departed = seat_actor(&mut world, chair);
    world
        .resource_mut::<SeatReservations>()
        .0
        .insert((chair, 0), departed);

    world.despawn(departed);
    prune_seat_reservations(&world);

    assert!(
        !world
            .resource::<SeatReservations>()
            .0
            .contains_key(&(chair, 0)),
        "a live furniture marker must become claimable after its actor despawns",
    );

    let replacement = seat_actor(&mut world, chair);
    assert_eq!(
        world
            .resource_mut::<SeatReservations>()
            .0
            .insert((chair, 0), replacement),
        None,
        "the replacement actor must acquire the released marker",
    );
}

/// Per-marker granularity survives the prune — the resource exists to keep
/// two actors off the same marker of a multi-seat piece, so a shared
/// furniture entity with distinct marker indices must be preserved
/// independently.
#[test]
fn distinct_markers_on_one_furniture_are_preserved_independently() {
    let (mut world, furniture) = world_with_furniture(1);
    let bench = furniture[0];
    let actors = [
        seat_actor(&mut world, bench),
        seat_actor(&mut world, bench),
        seat_actor(&mut world, bench),
    ];
    {
        let mut r = world.resource_mut::<SeatReservations>();
        r.0.insert((bench, 0), actors[0]);
        r.0.insert((bench, 1), actors[1]);
        r.0.insert((bench, 2), actors[2]);
    }

    prune_seat_reservations(&world);

    assert_eq!(world.resource::<SeatReservations>().0.len(), 3);
}

/// No furniture registered at all (loose-NIF demo, test fixtures) must not
/// panic — and, with nothing live, every claim is stale by definition.
#[test]
fn missing_furniture_storage_is_not_a_panic() {
    let mut world = World::new();
    world.insert_resource(SeatReservations::default());
    world.resource_mut::<SeatReservations>().0.insert((7, 0), 8);

    prune_seat_reservations(&world);

    assert!(world.resource::<SeatReservations>().0.is_empty());
}
