//! Actors that move between spaces (PRD cross-cell-actor-travel; #6852, #6853,
//! #6854; design: `docs/engine/cross-cell-travel.md`).
//!
//! A *space* is an interior cell or a whole worldspace. An actor whose Travel
//! destination lies in another space walks to the load door of its own cell
//! that starts the shortest chain of doors toward it ([`SpaceGraph`]), "as he
//! has to go through one or more load doors" (CS *SubSpace*), fades, and passes
//! it (XCT-ROUTE-001/002/008). The pass is a request ([`PendingRelocations`]):
//! the ledger ([`ActorRelocations`]) records the actor's new cell and pose, the
//! actor leaves the cell it was in through the same capture-and-release path a
//! cell unload takes, and [`relocated_in`] hands it to the reference loader for
//! the cell it entered as soon as that cell is loaded. From then on the cell
//! loader never spawns it in its home cell (XCT-LOC-002). An actor out of sight
//! travels as a [`Journey`]: no positions between departure and arrival, an
//! arrival time from its route's length and its walk speed (XCT-OFF-001).

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};

use byroredux_core::ecs::components::{
    EscortBehavior, FollowBehavior, FormIdComponent, GlobalTransform, Transform,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{CellRoot, Component, Resource, SparseSetStorage, World};
use byroredux_core::form_id::FormIdPool;
use byroredux_core::math::coord::{yup_to_zup_pos, zup_to_yup_pos};
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::cell::{CellRef, PlacedRef, SpaceGraph, SpaceKey};
use byroredux_plugin::esm::records::EsmIndex;
use byroredux_scripting::{
    ActorMoveRequest, ActorRelocations, CreatedCell, CreatedPlacement, Journey, PendingActorMoves, PendingRelocations,
    Relocation,
};

use crate::components::DoorTeleport;

/// PlayerRef: the leader FormID a companion's Follow package names.
const PLAYER_REF: u32 = 0x14;

/// The actors the reference loader is spawning at their ledger placement right
/// now: the home-cell skip ([`skipped_at_home`]) lets these through. Present
/// only while [`spawn_relocated`] runs. `NOT_SAVED_BY_DESIGN`.
///
/// [`spawn_relocated`]: super::created::spawn_relocated
#[derive(Debug, Default)]
pub(crate) struct RelocatedSpawning(pub(crate) HashSet<u32>);

impl Resource for RelocatedSpawning {}

/// The routing data of one load order, built on first use: the load-door graph
/// and the interior-persistent actors that need a logical identity while their
/// cell is not loaded. Rebuilt when the load order's index changes.
/// `NOT_SAVED_BY_DESIGN`: derived from the records.
#[derive(Default)]
pub(crate) struct TravelIndex(Mutex<Option<(usize, Arc<TravelData>)>>);

impl Resource for TravelIndex {}

pub(crate) struct TravelData {
    pub(crate) graph: SpaceGraph,
    /// Persistent actor references placed in interior cells, with that cell's
    /// editor ID (XCT-LOC-005's population).
    interior_actors: Vec<(u32, String)>,
}

/// The [`TravelData`] of the loaded load order.
pub(crate) fn travel_data(world: &World) -> Option<Arc<TravelData>> {
    let index = world.try_resource::<super::LoadedCellIndex>().map(|index| Arc::clone(&index.0))?;
    let cache = world.try_resource::<TravelIndex>()?;
    let mut slot = cache.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let key = Arc::as_ptr(&index) as usize;
    if let Some((built_for, data)) = slot.as_ref() {
        if *built_for == key {
            return Some(Arc::clone(data));
        }
    }
    let graph = SpaceGraph::build(&index.cells, &index.navmesh_door_links());
    let interior_actors = index
        .cells
        .cells
        .values()
        .flat_map(|cell| {
            cell.references
                .iter()
                .filter(|placed| placed.is_persistent() && index.is_actor(placed.base_form_id))
                .map(move |placed| (placed.form_id, cell.editor_id.clone()))
        })
        .collect();
    let data = Arc::new(TravelData { graph, interior_actors });
    *slot = Some((key, Arc::clone(&data)));
    Some(data)
}

/// Why each actor that could not route last asked could not (XCT-DEST-003,
/// XCT-DEST-004, no chain of doors), by FormID, for `actor.where`. Cleared for
/// an actor once it routes. `NOT_SAVED_BY_DESIGN`: routes are chosen again
/// after a load.
#[derive(Debug, Default)]
pub(crate) struct TravelReports(pub(crate) BTreeMap<u32, &'static str>);

impl Resource for TravelReports {}

/// An actor walking to the load door that takes it toward its destination's
/// space; passing the door relocates it to `placement` (XCT-ROUTE-001/002).
/// `NOT_SAVED_BY_DESIGN`: the route is chosen again from the package after a
/// load.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CrossSpaceHop {
    pub(crate) door: EntityId,
    pub(crate) placement: CreatedPlacement,
    /// The Travel destination beyond the door.
    pub(crate) target: u32,
}

impl Component for CrossSpaceHop {
    type Storage = SparseSetStorage<Self>;
}

/// XCT-ROUTE-008 — an actor fading out at a load door before it passes, or in
/// after it came through, over the game's `fActorTeleportFadeSeconds`
/// ("actors fade when going through a load door", CS
/// *fActorTeleportFadeSeconds*). Drawn through `AnimatedAlpha`, the sink
/// `SetActorAlpha` drives. `NOT_SAVED_BY_DESIGN`: a load restores actors
/// opaque.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DoorFade {
    pub(crate) fading_out: bool,
    pub(crate) remaining: f32,
    pub(crate) total: f32,
}

impl Component for DoorFade {
    type Storage = SparseSetStorage<Self>;
}

/// The door fade's length: the load order's `fActorTeleportFadeSeconds`, none
/// when the game does not author it.
pub(crate) fn door_fade_seconds(world: &World) -> f32 {
    byroredux_scripting::obscript_state::game_setting(world, "fActorTeleportFadeSeconds")
        .filter(|seconds| seconds.is_finite())
        .unwrap_or(0.0)
        .max(0.0)
}

/// XCT-LOC-002 — whether the home cell's reference list must leave `form`
/// out: the ledger puts it elsewhere (or on the road) and the relocation step
/// is not the one spawning it.
pub(crate) fn skipped_at_home(world: &World, form: u32) -> bool {
    world
        .try_resource::<ActorRelocations>()
        .is_some_and(|ledger| ledger.contains(form))
        && !world
            .try_resource::<RelocatedSpawning>()
            .is_some_and(|spawning| spawning.0.contains(&form))
}

/// Whether two cells lie in the same space: the same interior, or exterior
/// cells of the same worldspace (whatever their grids).
pub(crate) fn same_space(a: &CreatedCell, b: &CreatedCell) -> bool {
    match (a, b) {
        (CreatedCell::Interior { editor_id: a }, CreatedCell::Interior { editor_id: b }) => {
            a.eq_ignore_ascii_case(b)
        }
        (CreatedCell::Exterior { worldspace: a, .. }, CreatedCell::Exterior { worldspace: b, .. }) => {
            a.eq_ignore_ascii_case(b)
        }
        _ => false,
    }
}

/// Whether two cells are the same cell (an interior by editor ID, ignoring
/// case; an exterior by worldspace and grid).
pub(crate) fn same_cell(a: &CreatedCell, b: &CreatedCell) -> bool {
    match (a, b) {
        (CreatedCell::Exterior { grid: ga, .. }, CreatedCell::Exterior { grid: gb, .. }) => ga == gb && same_space(a, b),
        _ => same_space(a, b),
    }
}

/// The routing key of the space `cell` is in.
fn space_key(cell: &CreatedCell) -> SpaceKey {
    match cell {
        CreatedCell::Interior { editor_id } => SpaceKey::Interior(editor_id.to_ascii_lowercase()),
        CreatedCell::Exterior { worldspace, .. } => SpaceKey::Worldspace(worldspace.to_ascii_lowercase()),
    }
}

/// The cell holding a point of `cell_ref`'s space: the interior itself, or
/// the exterior grid the Z-up `position` falls in.
fn cell_at(cell_ref: CellRef<'_>, position_zup: [f32; 3]) -> CreatedCell {
    match cell_ref {
        CellRef::Interior { editor_id } => CreatedCell::Interior {
            editor_id: editor_id.to_string(),
        },
        CellRef::Exterior { worldspace, .. } => {
            let units = byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
            CreatedCell::Exterior {
                worldspace: worldspace.to_string(),
                grid: (
                    (position_zup[0] / units).floor() as i32,
                    (position_zup[1] / units).floor() as i32,
                ),
            }
        }
    }
}

/// The placement of a point in the space of reference `anchor` (its cell is
/// found from `anchor`, its grid from the point).
fn placement_in_space_of(index: &EsmIndex, anchor: u32, position_zup: [f32; 3], rotation_zup: [f32; 3]) -> Option<CreatedPlacement> {
    let (cell, _) = index.cells.locate_refr(anchor)?;
    Some(CreatedPlacement { cell: cell_at(cell, position_zup), position_zup, rotation_zup })
}

/// Where `form` stands now: its ledger placement once it has moved, its
/// journey's origin while it travels offscreen, its placement in the load
/// order before.
pub(crate) fn current_placement(world: &World, index: &EsmIndex, form: u32) -> Option<CreatedPlacement> {
    if let Some(ledger) = world.try_resource::<ActorRelocations>() {
        if let Some(journey) = ledger.journey(form) {
            return Some(journey.origin.clone());
        }
        if let Some(placement) = ledger.placement(form) {
            return Some(placement.clone());
        }
    }
    let (cell, placed) = index.cells.locate_refr(form)?;
    Some(CreatedPlacement { cell: cell_at(cell, placed.position), position_zup: placed.position, rotation_zup: placed.rotation })
}

/// The cell `form` is in now (see [`current_placement`]).
pub(crate) fn current_cell(world: &World, index: &EsmIndex, form: u32) -> Option<CreatedCell> {
    current_placement(world, index, form).map(|placement| placement.cell)
}

/// How a Travel actor reaches its destination reference.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Route {
    /// The destination is resident, or unknown to this check: walk to it as
    /// before.
    SameSpace,
    /// XCT-DEST-001 — the destination is in the actor's own space but not
    /// resident: walk to its load-order position.
    Within(Vec3),
    /// Walk to this load door, at `position`; passing it relocates the actor.
    Door { door: EntityId, position: Vec3, placement: CreatedPlacement },
    /// No hop leads toward the destination (XCT-DEST-003/004, no chain of
    /// load doors).
    Unroutable(&'static str),
}

/// The load-order FormID of `entity`'s reference.
pub(crate) fn local_form(world: &World, entity: EntityId) -> Option<u32> {
    let id = world.get::<FormIdComponent>(entity)?.0;
    world.try_resource::<FormIdPool>()?.local_of(id)
}

/// XCT-DEST-004 (Q3) — whether the load order places `form` persistent. A
/// reference no cell holds (created at run time) counts as persistent.
fn persistent(index: &EsmIndex, form: u32) -> bool {
    index.cells.locate_refr(form).is_none_or(|(_, placed)| placed.is_persistent())
}

/// The route through load doors from a placement to the destination reference
/// `target`: `Ok(None)` when it is in the same space, the route otherwise, or
/// why there is none.
fn plan(
    data: &TravelData,
    index: &EsmIndex,
    from: &CreatedPlacement,
    target: u32,
) -> Result<Option<byroredux_plugin::esm::cell::SpaceRoute>, &'static str> {
    let (_, goal) = index.cells.locate_refr(target).ok_or("its destination is not in the load order")?;
    let there = index.cells.space_of_refr(target).ok_or("its destination is not in the load order")?;
    let here = space_key(&from.cell);
    if here == there {
        return Ok(None);
    }
    let graph = &data.graph;
    let from_id = graph.id(&here).ok_or("no load door leaves its space")?;
    let to_id = graph.id(&there).ok_or("no load door leads into its destination's space")?;
    graph
        .route(from_id, from.position_zup, to_id, goal.position)
        .map(Some)
        .ok_or("no chain of load doors leads to its destination's space")
}

/// XCT-ROUTE-001 / XCT-ROUTE-003 — the route from `actor` to its Travel
/// destination `target`: a resident target is walked to directly, one in the
/// actor's own space at its load-order position, one in another space through
/// the first door of the shortest chain of load doors toward it.
pub(crate) fn route_toward(world: &World, actor: EntityId, target: Option<u32>) -> Route {
    let Some(target) = target else {
        return Route::SameSpace;
    };
    if byroredux_scripting::condition::resolve_entity_by_global_form_id(world, target)
        .is_some_and(|entity| !byroredux_scripting::is_logical_stub(world, entity))
    {
        return Route::SameSpace;
    }
    let (Some(index), Some(data)) = (
        world.try_resource::<super::LoadedCellIndex>().map(|index| Arc::clone(&index.0)),
        travel_data(world),
    ) else {
        return Route::SameSpace;
    };
    let (Some(actor_form), Some(from)) = (
        local_form(world, actor),
        world.get::<GlobalTransform>(actor).map(|t| t.translation),
    ) else {
        return Route::SameSpace;
    };
    let Some(mut here) = current_placement(world, &index, actor_form) else {
        return Route::SameSpace;
    };
    here.position_zup = yup_to_zup_pos(from.to_array());
    let route = match plan(&data, &index, &here, target) {
        Ok(None) => {
            return match index.cells.locate_refr(target) {
                Some((_, placed)) => Route::Within(Vec3::from(zup_to_yup_pos(placed.position))),
                None => Route::SameSpace,
            };
        }
        Ok(Some(route)) => route,
        Err(reason) => return Route::Unroutable(reason),
    };
    if !persistent(&index, actor_form) {
        return Route::Unroutable("not persistent");
    }
    let first = &data.graph.doors()[route.doors[0]];
    let cell_root = world.get::<CellRoot>(actor).map(|root| root.0);
    let door = byroredux_scripting::condition::resolve_entity_by_global_form_id(world, first.door)
        .filter(|door| world.has::<DoorTeleport>(*door))
        .filter(|door| cell_root.is_none() || world.get::<CellRoot>(*door).map(|root| root.0) == cell_root);
    let Some(door) = door else {
        return Route::Unroutable("the first load door of its route is not loaded");
    };
    let Some(placement) = placement_in_space_of(&index, first.partner, first.landing, first.landing_rotation) else {
        return Route::Unroutable("its destination is not in the load order");
    };
    Route::Door { door, position: Vec3::from(zup_to_yup_pos(first.approach)), placement }
}

/// The journey `actor` makes from `from` to its destination reference `target`
/// out of sight, at `speed` world units per second: its arrival is the route's
/// walk time from now in game time (XCT-OFF-001), at the package location
/// (XCT-OFF-002), or through the far side of its last door (XCT-OFF-004).
fn plan_journey(world: &World, from: CreatedPlacement, target: u32, speed: f32) -> Result<Journey, &'static str> {
    let index = world
        .try_resource::<super::LoadedCellIndex>()
        .map(|index| Arc::clone(&index.0))
        .ok_or("no load order")?;
    let data = travel_data(world).ok_or("no load order")?;
    let route = plan(&data, &index, &from, target)?;
    let (cell, goal) = index.cells.locate_refr(target).ok_or("its destination is not in the load order")?;
    let arrival = CreatedPlacement { cell: cell_at(cell, goal.position), position_zup: goal.position, rotation_zup: goal.rotation };
    let (length, last_door) = match &route {
        Some(route) => {
            let last = &data.graph.doors()[*route.doors.last().expect("a route between two spaces has a door")];
            (route.length, placement_in_space_of(&index, last.partner, last.landing, last.landing_rotation))
        }
        None => {
            let d = Vec3::from(from.position_zup).distance(Vec3::from(goal.position));
            (d, None)
        }
    };
    let (now, scale) = world
        .try_resource::<crate::components::GameTimeRes>()
        .map(|time| (time.absolute_hours(), time.time_scale))
        // No clock (a test world): day 0 at the engine's default scale.
        .unwrap_or((0.0, 30.0));
    let walk_seconds = f64::from(length) / f64::from(speed.max(1.0));
    Ok(Journey {
        destination: target,
        origin: from,
        arrival,
        last_door,
        arrives_at_hours: now + walk_seconds * f64::from(scale.max(0.0)) / 3600.0,
    })
}

/// Start `actor`'s journey toward `target` from `from`, or record why it cannot
/// make one.
pub(crate) fn depart(world: &World, actor: u32, from: CreatedPlacement, target: u32, speed: f32) -> Result<(), &'static str> {
    let journey = plan_journey(world, from, target, speed);
    let result = journey.as_ref().map(|_| ()).map_err(|reason| *reason);
    report(world, actor, result.err());
    if let (Ok(journey), Some(mut ledger)) = (journey, world.try_resource_mut::<ActorRelocations>()) {
        log::info!("actor {actor:08X} -> offscreen toward {target:08X}, arrives at {:.2} h", journey.arrives_at_hours);
        ledger.start_journey(actor, journey);
    }
    result
}

/// Record (or clear, `None`) why `actor` cannot route.
pub(crate) fn report(world: &World, actor: u32, unroutable: Option<&'static str>) {
    if let Some(mut reports) = world.try_resource_mut::<TravelReports>() {
        match unroutable {
            Some(reason) => {
                reports.0.insert(actor, reason);
            }
            None => {
                reports.0.remove(&actor);
            }
        }
    }
}

/// XCT-OFF-001 — a **Routing** actor whose cell unloads under it continues its
/// trip offscreen: every released actor with a chosen load door starts a
/// journey from where it stood. Called by the unload path before the actors are
/// released.
pub(crate) fn depart_unwatched(world: &World, victims: &[EntityId]) {
    let routing: Vec<(EntityId, u32)> = world
        .query::<CrossSpaceHop>()
        .map(|hops| victims.iter().filter_map(|&e| hops.get(e).map(|hop| (e, hop.target))).collect())
        .unwrap_or_default();
    if routing.is_empty() {
        return;
    }
    let Some(index) = world.try_resource::<super::LoadedCellIndex>().map(|index| Arc::clone(&index.0)) else {
        return;
    };
    for (entity, target) in routing {
        let Some(form) = local_form(world, entity) else { continue };
        let Some(mut from) = current_placement(world, &index, form) else { continue };
        if let Some(t) = world.get::<GlobalTransform>(entity) {
            from.position_zup = yup_to_zup_pos(t.translation.to_array());
        }
        let speed = crate::components::WalkSpeed::or_default(world.get::<crate::components::WalkSpeed>(entity).as_deref());
        if let Err(reason) = depart(world, form, from, target, speed) {
            log::info!("actor {form:08X} left unwatched but cannot reach {target:08X}: {reason}");
        }
    }
}

/// XCT-FOL-001 (Q2) — the live actors following or escorting `leader`
/// (a Follow or Escort package whose target is it), by FormID.
pub(crate) fn followers_of(world: &World, leader: u32) -> Vec<u32> {
    let mut followers: Vec<EntityId> = Vec::new();
    if let Some(q) = world.query::<FollowBehavior>() {
        followers.extend(q.iter().filter(|(_, f)| f.target_form_id == Some(leader)).map(|(e, _)| e));
    }
    if let Some(q) = world.query::<EscortBehavior>() {
        followers.extend(q.iter().filter(|(_, f)| f.target_form_id == Some(leader)).map(|(e, _)| e));
    }
    let mut forms: Vec<u32> = followers
        .into_iter()
        .filter(|e| world.has::<CellRoot>(*e) && !byroredux_scripting::is_logical_stub(world, *e))
        .filter_map(|e| local_form(world, e))
        .filter(|form| *form != leader)
        .collect();
    forms.sort_unstable();
    forms.dedup();
    forms
}

/// XCT-ROUTE-002 — `entity` passes the load door of `hop`: request its move to
/// the far side, with its followers (XCT-FOL-001).
pub(crate) fn pass_door(world: &World, entity: EntityId, hop: &CrossSpaceHop) {
    let Some(form) = local_form(world, entity) else { return };
    let speed = crate::components::WalkSpeed::or_default(world.get::<crate::components::WalkSpeed>(entity).as_deref());
    let mut moves = vec![Relocation { actor: form, placement: hop.placement.clone(), onward: Some((hop.target, speed)) }];
    moves.extend(
        followers_of(world, form)
            .into_iter()
            .map(|follower| Relocation { actor: follower, placement: hop.placement.clone(), onward: None }),
    );
    if let Some(mut pending) = world.try_resource_mut::<PendingRelocations>() {
        pending.0.extend(moves);
    }
}

/// XCT-FOL-001 — the player passes a load door to `placement` (Z-up `XTEL` pose
/// in the destination cell): its followers come through with it.
pub(crate) fn player_passes_door(world: &World, destination_ref: u32, position_zup: [f32; 3], rotation_zup: [f32; 3]) {
    let followers = followers_of(world, PLAYER_REF);
    if followers.is_empty() {
        return;
    }
    let Some(index) = world.try_resource::<super::LoadedCellIndex>().map(|index| Arc::clone(&index.0)) else {
        return;
    };
    let Some(placement) = placement_in_space_of(&index, destination_ref, position_zup, rotation_zup) else {
        return;
    };
    if let Some(mut pending) = world.try_resource_mut::<PendingRelocations>() {
        for follower in followers {
            log::info!("follower {follower:08X} passes the door with the player");
            pending.0.push(Relocation { actor: follower, placement: placement.clone(), onward: None });
        }
    }
}

/// Whether `cell` is one of the `loaded` cells.
pub(crate) fn is_loaded(cell: &CreatedCell, loaded: &[CreatedCell]) -> bool {
    loaded.iter().any(|other| same_cell(cell, other))
}

/// Carry out the hop bookkeeping of one relocation: a hop that lands short of
/// its destination's space in a cell that is not loaded continues as a journey
/// (XCT-ROUTE-006); any other move ends a journey. Returns whether the actor
/// went on the road (it is then not placed by the ledger yet).
pub(crate) fn continue_hop(world: &World, relocation: &Relocation, loaded: &[CreatedCell]) -> bool {
    let Some((target, speed)) = relocation.onward else {
        return false;
    };
    let Some(index) = world.try_resource::<super::LoadedCellIndex>().map(|index| Arc::clone(&index.0)) else {
        return false;
    };
    let arrived = index.cells.space_of_refr(target).is_some_and(|there| there == space_key(&relocation.placement.cell));
    if arrived || is_loaded(&relocation.placement.cell, loaded) {
        report(world, relocation.actor, None);
        return false;
    }
    depart(world, relocation.actor, relocation.placement.clone(), target, speed).is_ok()
}

/// XCT-OFF-001..004 — the journeys whose arrival time has come, as moves: to
/// the far side of the last door when the player is in that cell, to the
/// package location otherwise.
pub(crate) fn due_arrivals(world: &World, loaded: &[CreatedCell]) -> Vec<Relocation> {
    let Some(now) = world.try_resource::<crate::components::GameTimeRes>().map(|time| time.absolute_hours()) else {
        return Vec::new();
    };
    let Some(ledger) = world.try_resource::<ActorRelocations>() else {
        return Vec::new();
    };
    ledger
        .journeys()
        .filter(|(_, journey)| journey.arrives_at_hours <= now)
        .map(|(actor, journey)| {
            let placement = match &journey.last_door {
                Some(door) if is_loaded(&door.cell, loaded) => door.clone(),
                _ => journey.arrival.clone(),
            };
            Relocation { actor, placement, onward: None }
        })
        .collect()
}

/// XCT-LOC-004 / XCT-LOC-006 — resolve the scripted moves raised this frame
/// into relocations: the target's current placement (its body, its ledger
/// entry, or the load order) plus the offset, or the running package's
/// location reference.
pub(crate) fn resolve_actor_moves(world: &World) -> Vec<Relocation> {
    let requests: Vec<ActorMoveRequest> = world
        .try_resource_mut::<PendingActorMoves>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    if requests.is_empty() {
        return Vec::new();
    }
    let Some(index) = world.try_resource::<super::LoadedCellIndex>().map(|index| Arc::clone(&index.0)) else {
        return Vec::new();
    };
    let mut moves = Vec::new();
    for request in requests {
        let (actor, target, offset) = match request {
            ActorMoveRequest::ToReference { actor, target, offset } => (actor, target, offset),
            ActorMoveRequest::ToPackageLocation { actor } => {
                let Some(target) = running_package(world, actor)
                    .and_then(|package| index.packages.get(&package))
                    .and_then(|package| package.location_reference())
                else {
                    log::info!("MoveToPackageLocation {actor:08X}: its package names no location reference");
                    continue;
                };
                (actor, target, [0.0; 3])
            }
        };
        let is_actor = index.cells.locate_refr(actor).is_none_or(|(_, placed)| index.is_actor(placed.base_form_id));
        if !is_actor {
            log::info!("MoveTo {actor:08X} -> {target:08X} declined: only actors move between cells");
            continue;
        }
        let Some(mut placement) = placement_of_target(world, &index, target) else {
            log::info!("MoveTo {actor:08X} -> {target:08X} declined: the target is in no cell");
            continue;
        };
        for (axis, delta) in offset.iter().enumerate() {
            placement.position_zup[axis] += delta;
        }
        if let Some(cell) = index.cells.locate_refr(target).map(|(cell, _)| cell) {
            if let CellRef::Exterior { .. } = cell {
                placement.cell = cell_at(cell, placement.position_zup);
            }
        }
        log::info!("MoveTo {actor:08X} -> {target:08X} in {:?}", placement.cell);
        moves.push(Relocation { actor, placement, onward: None });
    }
    moves
}

/// Where reference `target` stands now: its body's pose, else its ledger or
/// load-order placement.
fn placement_of_target(world: &World, index: &EsmIndex, target: u32) -> Option<CreatedPlacement> {
    let mut placement = current_placement(world, index, target)?;
    if let Some(body) = live_body(world, target) {
        if let Some(t) = world.get::<GlobalTransform>(body) {
            placement.position_zup = yup_to_zup_pos(t.translation.to_array());
            let (x, y, z) = byroredux_core::math::coord::quat_yup_to_euler_zup(t.rotation);
            placement.rotation_zup = [x, y, z];
            if let CreatedCell::Exterior { worldspace, .. } = &placement.cell {
                let units = byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
                placement.cell = CreatedCell::Exterior {
                    worldspace: worldspace.clone(),
                    grid: ((placement.position_zup[0] / units).floor() as i32, (placement.position_zup[1] / units).floor() as i32),
                };
            }
        }
    }
    Some(placement)
}

/// The package `actor` runs, when it has a body.
fn running_package(world: &World, actor: u32) -> Option<u32> {
    let body = live_body(world, actor)?;
    world
        .get::<crate::components::AmbientPackageRuntime>(body)
        .and_then(|runtime| runtime.active_package_form_id)
        .or_else(|| {
            world
                .try_resource::<byroredux_scripting::ActivePackages>()
                .and_then(|active| active.0.get(&body).copied())
        })
}

/// The ledgered actors placed in `cell` that have no live entity and are not
/// on the road, as the placements the reference loader spawns: each actor's
/// own record at its ledger pose. `alive` answers whether an actor's reference
/// already has one.
pub(crate) fn relocated_in(
    world: &World,
    cell: &CreatedCell,
    alive: impl Fn(u32) -> bool,
) -> Vec<PlacedRef> {
    let Some(index) = world
        .try_resource::<super::LoadedCellIndex>()
        .map(|index| Arc::clone(&index.0))
    else {
        return Vec::new();
    };
    let Some(ledger) = world.try_resource::<ActorRelocations>() else {
        return Vec::new();
    };
    ledger
        .iter()
        .filter(|(actor, _)| ledger.journey(*actor).is_none())
        .filter(|(_, placement)| same_cell(&placement.cell, cell))
        .filter(|(actor, _)| !alive(*actor))
        .filter_map(|(actor, placement)| {
            let (_, placed) = index.cells.locate_refr(actor)?;
            let mut placed = placed.clone();
            placed.position = placement.position_zup;
            placed.rotation = placement.rotation_zup;
            Some(placed)
        })
        .collect()
}

/// The live, spawned entity of `form`'s reference (not a logical stub): one
/// carrying its identity and a cell root. Every entity with the identity is
/// asked, so a stub that has not been retired yet cannot hide a body.
pub(crate) fn live_body(world: &World, form: u32) -> Option<EntityId> {
    if form == PLAYER_REF {
        return byroredux_scripting::condition::resolve_entity_by_global_form_id(world, form);
    }
    let pool = world.try_resource::<FormIdPool>()?;
    let ids = world.query::<FormIdComponent>()?;
    let found = ids.iter().find(|(entity, id)| {
        pool.local_of(id.0) == Some(form)
            && world.has::<CellRoot>(*entity)
            && !byroredux_scripting::is_logical_stub(world, *entity)
    });
    found.map(|(entity, _)| entity)
}

/// Every FormID with a live body ([`live_body`]), in one pass.
fn live_forms(world: &World) -> HashSet<u32> {
    let (Some(pool), Some(ids)) = (world.try_resource::<FormIdPool>(), world.query::<FormIdComponent>()) else {
        return HashSet::new();
    };
    ids.iter()
        .filter(|(entity, _)| world.has::<CellRoot>(*entity) && !byroredux_scripting::is_logical_stub(world, *entity))
        .filter_map(|(_, id)| pool.local_of(id.0))
        .collect()
}

/// Whether `form`'s reference has a live, spawned entity ([`live_body`]).
pub(crate) fn has_live_entity(world: &World, form: u32) -> bool {
    live_body(world, form).is_some()
}

/// XCT-LOC-005 — the logical identities this module stands in for actors whose
/// cell is not loaded, by FormID: stubs with no cell root (no unload releases
/// them), stamped `RemoteSceneActorStub`.
fn identity_stubs(world: &World) -> BTreeMap<u32, EntityId> {
    let Some(stubs) = world.query::<byroredux_scripting::RemoteSceneActorStub>() else {
        return BTreeMap::new();
    };
    let candidates = world.query::<byroredux_scripting::SceneAliasCandidate>();
    stubs
        .iter()
        .filter(|(entity, _)| !world.has::<CellRoot>(*entity))
        .filter_map(|(entity, _)| {
            let form = candidates.as_ref()?.get(entity)?.reference_form_id;
            Some((form, entity))
        })
        .collect()
}

/// XCT-LOC-005 — promote: the actors in `forms` are about to get bodies; park
/// their stubs' state and despawn the stubs, so the body takes the identity
/// over instead of running beside it.
pub(crate) fn promote(world: &mut World, forms: &HashSet<u32>) {
    if forms.is_empty() {
        return;
    }
    let stubs: Vec<EntityId> = identity_stubs(world)
        .into_iter()
        .filter(|(form, _)| forms.contains(form))
        .map(|(_, entity)| entity)
        .collect();
    if stubs.is_empty() {
        return;
    }
    super::reference_state::capture(world, &stubs);
    world.despawn_batch(stubs);
    byroredux_scripting::mark_scene_actor_bindings_dirty(world);
}

/// XCT-LOC-005 — promote the stubs of every actor the interior `editor_id` is
/// about to load: its own persistent actors not moved away, and the actors the
/// ledger puts in it. Called when a transition into that interior begins.
pub(crate) fn promote_for_interior(world: &mut World, editor_id: &str) {
    let Some(data) = travel_data(world) else { return };
    let ledger = world.try_resource::<ActorRelocations>().map(|ledger| ledger.clone()).unwrap_or_default();
    let mut forms: HashSet<u32> = data
        .interior_actors
        .iter()
        .filter(|(form, cell)| cell.eq_ignore_ascii_case(editor_id) && !ledger.contains(*form))
        .map(|(form, _)| *form)
        .collect();
    forms.extend(ledger.iter().filter(|(actor, placement)| {
        ledger.journey(*actor).is_none()
            && matches!(&placement.cell, CreatedCell::Interior { editor_id: cell } if cell.eq_ignore_ascii_case(editor_id))
    }).map(|(actor, _)| actor));
    promote(world, &forms);
}

/// XCT-LOC-005 — give every persistent actor that has no entity a logical
/// identity at its current pose: the interior-persistent actors whose cell is
/// not loaded, and every ledgered or journeying actor whose current cell is
/// not loaded. A stub whose actor has since gained a body is retired, and one
/// whose actor has moved is moved with it. Worldspace-persistent actors keep
/// the worldspace loader's own stubs (#6076).
pub(crate) fn reconcile_logical_identities(world: &mut World, loaded: &[CreatedCell]) {
    let (Some(data), Some(index)) = (
        travel_data(world),
        world.try_resource::<super::LoadedCellIndex>().map(|index| Arc::clone(&index.0)),
    ) else {
        return;
    };
    let Some(load_order) = world.try_resource::<super::load_order::ActiveLoadOrder>().map(|order| Arc::clone(&order.0)) else {
        return;
    };
    let ledger = world.try_resource::<ActorRelocations>().map(|ledger| ledger.clone()).unwrap_or_default();
    let stubs = identity_stubs(world);
    let carried: HashSet<u32> = world
        .query::<byroredux_scripting::SceneAliasCandidate>()
        .map(|q| q.iter().map(|(_, candidate)| candidate.reference_form_id).collect())
        .unwrap_or_default();

    let live = live_forms(world);
    // Retire stubs beside a body.
    let superseded: Vec<EntityId> = stubs.iter().filter(|(form, _)| live.contains(*form)).map(|(_, e)| *e).collect();
    if !superseded.is_empty() {
        world.despawn_batch(superseded);
    }

    let mut wanted: Vec<(u32, CreatedPlacement)> = Vec::new();
    for (form, cell) in &data.interior_actors {
        if ledger.contains(*form) {
            continue;
        }
        let here = CreatedCell::Interior { editor_id: cell.clone() };
        if is_loaded(&here, loaded) {
            continue;
        }
        if let Some(placement) = current_placement(world, &index, *form) {
            wanted.push((*form, placement));
        }
    }
    let moved: Vec<u32> = ledger.iter().map(|(actor, _)| actor).chain(ledger.journeys().map(|(actor, _)| actor)).collect();
    for form in moved {
        if let Some(placement) = current_placement(world, &index, form) {
            if ledger.journey(form).is_some() || !is_loaded(&placement.cell, loaded) {
                wanted.push((form, placement));
            }
        }
    }

    let mut spawned = 0usize;
    for (form, placement) in wanted {
        if live.contains(&form) {
            continue;
        }
        let pose = (
            Vec3::from(zup_to_yup_pos(placement.position_zup)),
            crate::zup_yup::rotation_zup_to_yup_quat(placement.rotation_zup),
        );
        if let Some(&stub) = stubs.get(&form) {
            // Moved while a stub: the identity moves with it.
            if let Some(transform) = world.get_mut::<Transform>(stub) {
                transform.translation = pose.0;
                transform.rotation = pose.1;
            }
            if let Some(global) = world.get_mut::<GlobalTransform>(stub) {
                global.translation = pose.0;
                global.rotation = pose.1;
            }
            continue;
        }
        if carried.contains(&form) {
            continue; // the worldspace loader's or a scene's stub stands for it
        }
        let Some((_, placed)) = index.cells.locate_refr(form) else { continue };
        let placed = placed.clone();
        let (entity, _) = super::references::spawn_logical_quest_reference_with_scripts(
            world, &placed, &load_order, &index, pose.0, pose.1, placed.scale,
        );
        world.insert(entity, byroredux_scripting::RemoteSceneActorStub);
        spawned += 1;
    }
    if spawned > 0 {
        byroredux_scripting::mark_scene_actor_bindings_dirty(world);
        log::info!("XCT-LOC-005: {spawned} persistent actor(s) given a logical identity");
    }
}

/// XCT-ROUTE-008 — run the door fades: fade an actor's meshes out at its door,
/// then pass it; fade a new arrival in. Registered beside `actor_alpha_system`.
pub(crate) fn door_fade_system(world: &World, dt: f32) {
    use byroredux_core::ecs::components::AnimatedAlpha;
    let fading: Vec<(EntityId, DoorFade)> = match world.query::<DoorFade>() {
        Some(q) if !q.is_empty() => q.iter().map(|(e, f)| (e, *f)).collect(),
        _ => return,
    };
    let mut finished = Vec::new();
    for (entity, mut fade) in fading {
        // A fade-out whose hop was dropped (the package changed at the door)
        // is called off: the actor stays, opaque.
        if fade.fading_out && !world.has::<CrossSpaceHop>(entity) {
            fade = DoorFade { fading_out: false, remaining: 0.0, total: fade.total };
        }
        fade.remaining = (fade.remaining - dt.max(0.0)).max(0.0);
        let progress = if fade.total > 0.0 { fade.remaining / fade.total } else { 0.0 };
        let alpha = if fade.fading_out { progress } else { 1.0 - progress };
        let meshes = crate::npc_spawn::loot_appearance::mesh_entities_under(world, entity);
        if let Some(mut alphas) = world.query_mut::<AnimatedAlpha>() {
            for &mesh in &meshes {
                if fade.remaining > 0.0 || (fade.fading_out && world.has::<CrossSpaceHop>(entity)) {
                    alphas.insert(mesh, AnimatedAlpha(alpha));
                } else {
                    alphas.remove(mesh);
                }
            }
        }
        if fade.remaining > 0.0 {
            if let Some(mut q) = world.query_mut::<DoorFade>() {
                q.insert(entity, fade);
            }
        } else {
            finished.push((entity, fade.fading_out));
        }
    }
    for (entity, faded_out) in finished {
        if let Some(mut q) = world.query_mut::<DoorFade>() {
            q.remove(entity);
        }
        if faded_out {
            if let Some(hop) = world.get::<CrossSpaceHop>(entity).map(|hop| hop.clone()) {
                pass_door(world, entity, &hop);
            }
        }
    }
}

/// Whether a Travel actor stands at its door mid-fade (`travel_system` leaves
/// it be).
pub(crate) fn is_fading_at_door(world: &World, entity: EntityId) -> bool {
    world.get::<DoorFade>(entity).is_some_and(|fade| fade.fading_out)
}

/// The actor that reached its door starts its fade, or passes at once when the
/// game authors no fade.
pub(crate) fn reach_door(world: &World, entity: EntityId) {
    let seconds = door_fade_seconds(world);
    if seconds <= 0.0 {
        if let Some(hop) = world.get::<CrossSpaceHop>(entity).map(|hop| hop.clone()) {
            pass_door(world, entity, &hop);
        }
        return;
    }
    if let Some(mut q) = world.query_mut::<DoorFade>() {
        q.insert(entity, DoorFade { fading_out: true, remaining: seconds, total: seconds });
    }
}

/// Fade in the actors the relocation step just spawned through a door.
pub(crate) fn fade_in(world: &World, forms: &[u32]) {
    let seconds = door_fade_seconds(world);
    if seconds <= 0.0 {
        return;
    }
    for &form in forms {
        if let (Some(body), Some(mut q)) = (live_body(world, form), world.query_mut::<DoorFade>()) {
            q.insert(body, DoorFade { fading_out: false, remaining: seconds, total: seconds });
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::cell::{CellData, TeleportDest};

    /// The `group_type` of a placement in a cell's Persistent Children group.
    const PERSISTENT_CHILDREN: u8 = 8;
    const SUNNY: u32 = 0x104E85;
    const MARKER: u32 = 0x10A201;
    const STORE_MARKER: u32 = 0x20A001;

    fn interior(editor_id: &str) -> CreatedCell {
        CreatedCell::Interior { editor_id: editor_id.into() }
    }

    fn door(form_id: u32, at: [f32; 3], to: u32, landing: [f32; 3]) -> PlacedRef {
        let mut door = PlacedRef::at(form_id, 0x31, at, [0.0; 3]);
        door.group_type = 0;
        door.teleport = Some(TeleportDest { destination: to, position: landing, rotation: [0.0, 0.0, 1.0] });
        door
    }

    /// A saloon (Sunny, persistent) with a door to the wasteland, the
    /// wasteland (Sunny's marker, a door back, a door to a store) and a store
    /// with a marker: the store is two doors from the saloon.
    fn town() -> EsmIndex {
        let mut index = EsmIndex::default();
        let mut sunny = PlacedRef::at(SUNNY, 0x104E84, [10.0, 20.0, 30.0], [0.0; 3]);
        sunny.group_type = PERSISTENT_CHILDREN;
        index.cells.cells.insert(
            "gsprospectorsalooninterior".into(),
            CellData {
                form_id: 0x106185,
                editor_id: "GSProspectorSaloonInterior".into(),
                references: vec![sunny, door(0x10, [0.0; 3], 0x20, [1000.0, 0.0, 0.0])],
                ..Default::default()
            },
        );
        index.cells.cells.insert(
            "store".into(),
            CellData {
                form_id: 0x106186,
                editor_id: "Store".into(),
                references: vec![
                    door(0x30, [0.0; 3], 0x22, [3000.0, 0.0, 0.0]),
                    PlacedRef::at(STORE_MARKER, 0x32, [0.0, 400.0, 0.0], [0.0; 3]),
                ],
                ..Default::default()
            },
        );
        let mut grids = std::collections::HashMap::new();
        grids.insert(
            (0, 0),
            CellData {
                references: vec![
                    door(0x20, [1000.0, 0.0, 0.0], 0x10, [50.0, 0.0, 0.0]),
                    door(0x22, [3000.0, 0.0, 0.0], 0x30, [0.0, 100.0, 0.0]),
                    PlacedRef::at(MARKER, 0x32, [1500.0, 0.0, 0.0], [0.0, 0.0, 0.5]),
                ],
                ..Default::default()
            },
        );
        index.cells.exterior_cells.insert("wastelandnv".into(), grids);
        index
    }

    fn world_with(index: EsmIndex) -> World {
        let mut world = World::new();
        world.insert_resource(super::super::LoadedCellIndex(Arc::new(index)));
        world.insert_resource(TravelIndex::default());
        world.insert_resource(TravelReports::default());
        world.insert_resource(ActorRelocations::default());
        world.insert_resource(PendingRelocations::default());
        world.insert_resource(PendingActorMoves::default());
        world
    }

    fn saloon_placement() -> CreatedPlacement {
        CreatedPlacement { cell: interior("GSProspectorSaloonInterior"), position_zup: [10.0, 20.0, 30.0], rotation_zup: [0.0; 3] }
    }

    #[test]
    fn a_space_is_an_interior_or_a_whole_worldspace() {
        let a = CreatedCell::Exterior { worldspace: "WastelandNV".into(), grid: (-18, 0) };
        let b = CreatedCell::Exterior { worldspace: "wastelandnv".into(), grid: (-17, 1) };
        assert!(same_space(&a, &b), "another grid of the same worldspace is the same space");
        assert!(!same_cell(&a, &b), "but not the same cell");
        assert!(same_space(&interior("GSProspectorSaloonInterior"), &interior("gsprospectorsalooninterior")));
        assert!(!same_space(&interior("GSProspectorSaloonInterior"), &a));
        assert!(!same_space(&interior("A"), &interior("B")));
    }

    /// The home cell's list leaves a ledgered actor out unless the relocation
    /// step is the one spawning it.
    #[test]
    fn a_ledgered_actor_is_skipped_at_home_except_by_the_relocation_step() {
        let mut world = World::new();
        assert!(!skipped_at_home(&world, SUNNY), "no ledger");
        let mut ledger = ActorRelocations::default();
        ledger.relocate(
            SUNNY,
            CreatedPlacement { cell: interior("Outside"), position_zup: [0.0; 3], rotation_zup: [0.0; 3] },
        );
        world.insert_resource(ledger);
        assert!(skipped_at_home(&world, SUNNY));
        assert!(!skipped_at_home(&world, SUNNY + 1), "only the moved actor");
        world.insert_resource(RelocatedSpawning(HashSet::from([SUNNY])));
        assert!(!skipped_at_home(&world, SUNNY));
    }

    /// A ledgered actor placed in a cell, with no live entity, is handed to the
    /// loader as its own record at the ledger pose; one alive, placed in
    /// another cell, or on the road, is not.
    #[test]
    fn the_relocated_actors_of_a_cell_come_back_at_their_ledger_pose() {
        let world = world_with(town());
        let outside = CreatedCell::Exterior { worldspace: "WastelandNV".into(), grid: (-17, 0) };
        let placement = CreatedPlacement { cell: outside.clone(), position_zup: [-68000.0, 100.0, 8000.0], rotation_zup: [0.0, 0.0, 1.5] };
        world.resource_mut::<ActorRelocations>().relocate(SUNNY, placement.clone());

        let refs = relocated_in(&world, &outside, |_| false);
        assert_eq!(refs.len(), 1);
        assert_eq!((refs[0].form_id, refs[0].base_form_id), (SUNNY, 0x104E84));
        assert_eq!(refs[0].position, [-68000.0, 100.0, 8000.0]);
        assert_eq!(refs[0].rotation, [0.0, 0.0, 1.5]);
        assert!(relocated_in(&world, &outside, |_| true).is_empty(), "already alive");
        assert!(relocated_in(&world, &interior("GSProspectorSaloonInterior"), |_| false).is_empty(), "not its cell any more");
        let other_grid = CreatedCell::Exterior { worldspace: "WastelandNV".into(), grid: (-18, 0) };
        assert!(relocated_in(&world, &other_grid, |_| false).is_empty(), "another grid of the space");
        world.resource_mut::<ActorRelocations>().start_journey(
            SUNNY,
            Journey { destination: MARKER, origin: placement.clone(), arrival: placement, last_door: None, arrives_at_hours: 9.0 },
        );
        assert!(relocated_in(&world, &outside, |_| false).is_empty(), "XCT-OFF-003: on the road, spawned nowhere");
    }

    /// XCT-OFF-001 — a journey's arrival is its route's walk time from now,
    /// in game time; it ends at the package location, and records the far side
    /// of its last door.
    #[test]
    fn a_journey_arrives_after_its_routes_walk_time_in_game_time() {
        let mut world = world_with(town());
        world.insert_resource(crate::components::GameTimeRes::new(10.0, 30.0));
        depart(&world, SUNNY, saloon_placement(), MARKER, 100.0).unwrap();
        let ledger = world.resource::<ActorRelocations>();
        let journey = ledger.journey(SUNNY).expect("on the road");
        // (10,20,30) -> the door's approach, its partner's XTEL (50,0,0); land
        // at (1000,0,0) -> the marker (1500,0,0).
        let length = Vec3::new(40.0, -20.0, -30.0).length() + 500.0;
        let hours = 10.0 + f64::from(length) / 100.0 * 30.0 / 3600.0;
        assert!((journey.arrives_at_hours - hours).abs() < 1e-6, "{} vs {hours}", journey.arrives_at_hours);
        assert_eq!(journey.arrival.position_zup, [1500.0, 0.0, 0.0], "XCT-OFF-002: the package location");
        assert_eq!(journey.arrival.rotation_zup, [0.0, 0.0, 0.5]);
        assert_eq!(journey.last_door.as_ref().unwrap().position_zup, [1000.0, 0.0, 0.0]);
        assert!(matches!(journey.arrival.cell, CreatedCell::Exterior { grid: (0, 0), .. }));
    }

    /// XCT-ROUTE-003 — a destination two doors away is routed through both.
    #[test]
    fn a_two_hop_destination_is_routable() {
        let world = world_with(town());
        depart(&world, SUNNY, saloon_placement(), STORE_MARKER, 100.0).unwrap();
        let journey = world.resource::<ActorRelocations>().journey(SUNNY).cloned().unwrap();
        assert_eq!(journey.arrival.cell, interior("Store"));
        assert_eq!(journey.last_door.unwrap().position_zup, [0.0, 100.0, 0.0], "through the store's door");
    }

    /// XCT-DEST-003 — a destination no cell holds is Unroutable where it stands.
    #[test]
    fn a_destination_in_no_cell_is_unroutable() {
        let world = world_with(town());
        assert_eq!(depart(&world, SUNNY, saloon_placement(), 0xDEAD, 100.0), Err("its destination is not in the load order"));
        assert_eq!(world.resource::<TravelReports>().0.get(&SUNNY), Some(&"its destination is not in the load order"));
        assert!(world.resource::<ActorRelocations>().journey(SUNNY).is_none());
    }

    /// XCT-OFF-002 / XCT-OFF-004 — a due journey ends at the far side of its
    /// last door when the player is in that cell, at the package location
    /// otherwise; one not yet due does not end.
    #[test]
    fn a_due_journey_arrives_at_its_door_when_watched_and_its_place_when_not() {
        let mut world = world_with(town());
        world.insert_resource(crate::components::GameTimeRes::new(10.0, 30.0));
        depart(&world, SUNNY, saloon_placement(), MARKER, 100.0).unwrap();
        assert!(due_arrivals(&world, &[]).is_empty(), "XCT-OFF-003: not yet");
        world.resource_mut::<crate::components::GameTimeRes>().advance_hours(1.0);
        let unwatched = due_arrivals(&world, &[]);
        assert_eq!(unwatched.len(), 1);
        assert_eq!(unwatched[0].placement.position_zup, [1500.0, 0.0, 0.0]);
        let town_cell = CreatedCell::Exterior { worldspace: "wastelandnv".into(), grid: (0, 0) };
        let watched = due_arrivals(&world, &[town_cell]);
        assert_eq!(watched[0].placement.position_zup, [1000.0, 0.0, 0.0], "through the far side of the door");
    }

    /// XCT-ROUTE-006 / XCT-ROUTE-009 — a hop into the destination's space
    /// arrives; one short of it into a cell that is not loaded goes on as a
    /// journey; one into a loaded cell spawns there.
    #[test]
    fn a_hop_short_of_its_destination_continues_offscreen_unless_its_cell_is_loaded() {
        let world = world_with(town());
        let landing = CreatedPlacement {
            cell: CreatedCell::Exterior { worldspace: "wastelandnv".into(), grid: (0, 0) },
            position_zup: [1000.0, 0.0, 0.0],
            rotation_zup: [0.0; 3],
        };
        let hop = |target| Relocation { actor: SUNNY, placement: landing.clone(), onward: Some((target, 100.0)) };
        assert!(!continue_hop(&world, &hop(MARKER), &[]), "the marker's space: arrived");
        assert!(!continue_hop(&world, &hop(STORE_MARKER), &[landing.cell.clone()]), "a loaded cell: spawn and route on");
        assert!(continue_hop(&world, &hop(STORE_MARKER), &[]), "unwatched, short of the store: on the road");
        let journey = world.resource::<ActorRelocations>().journey(SUNNY).cloned().unwrap();
        assert_eq!(journey.origin, landing);
        assert_eq!(journey.arrival.cell, interior("Store"));
    }

    /// XCT-LOC-004 — a scripted move to a reference whose cell is not loaded
    /// relocates the actor to the reference's load-order pose plus the offset,
    /// in the reference's cell; a non-actor is declined.
    #[test]
    fn a_scripted_move_to_an_unloaded_reference_relocates_through_the_ledger() {
        let mut index = town();
        index.npcs.insert(0x104E84, Default::default());
        let world = world_with(index);
        world.resource_mut::<PendingActorMoves>().0.extend([
            ActorMoveRequest::ToReference { actor: SUNNY, target: MARKER, offset: [0.0, 0.0, 64.0] },
            ActorMoveRequest::ToReference { actor: 0x10, target: MARKER, offset: [0.0; 3] },
        ]);
        let moves = resolve_actor_moves(&world);
        assert_eq!(moves.len(), 1, "the door is no actor");
        assert_eq!(moves[0].actor, SUNNY);
        assert_eq!(moves[0].placement.position_zup, [1500.0, 0.0, 64.0]);
        assert_eq!(moves[0].placement.rotation_zup, [0.0, 0.0, 0.5], "the target's rotation");
        assert!(matches!(&moves[0].placement.cell, CreatedCell::Exterior { worldspace, grid: (0, 0) } if worldspace == "wastelandnv"));
        assert!(world.resource::<PendingActorMoves>().0.is_empty(), "drained");
    }

    /// XCT-FOL-001 (Q2) — an actor passing a door takes the live actors whose
    /// Follow or Escort package targets it through with it, to the same far
    /// side; a follower of someone else, or a stub, stays. With no authored
    /// fade it passes the moment it reaches the door.
    #[test]
    fn followers_pass_a_load_door_with_their_leader() {
        use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};
        let mut world = world_with(town());
        world.register::<CrossSpaceHop>();
        world.register::<DoorFade>();
        let mut pool = FormIdPool::new();
        let mut actor = |world: &mut World, local: u32| {
            let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("FalloutNV.esm"), local: LocalFormId(local) });
            let entity = world.spawn();
            world.insert(entity, FormIdComponent(id));
            world.insert(entity, CellRoot(1));
            entity
        };
        let (leader, follower, escort, stranger, stub) =
            (actor(&mut world, SUNNY), actor(&mut world, 0x501), actor(&mut world, 0x502), actor(&mut world, 0x503), actor(&mut world, 0x504));
        world.insert_resource(pool);
        world.insert(follower, FollowBehavior { target_form_id: Some(SUNNY), follow_distance: None });
        world.insert(escort, EscortBehavior { target_form_id: Some(SUNNY), destination_form_id: None, destination_radius: None, collect_distance: None, form_id: 0x77 });
        world.insert(stranger, FollowBehavior { target_form_id: Some(0x14), follow_distance: None });
        world.insert(stub, FollowBehavior { target_form_id: Some(SUNNY), follow_distance: None });
        world.insert(stub, byroredux_scripting::RemoteSceneActorStub);
        assert_eq!(followers_of(&world, SUNNY), vec![0x501, 0x502]);

        let far_side = CreatedPlacement {
            cell: CreatedCell::Exterior { worldspace: "wastelandnv".into(), grid: (0, 0) },
            position_zup: [1000.0, 0.0, 0.0],
            rotation_zup: [0.0; 3],
        };
        world.insert(leader, CrossSpaceHop { door: 9, placement: far_side.clone(), target: MARKER });
        reach_door(&world, leader);
        let moves = std::mem::take(&mut world.resource_mut::<PendingRelocations>().0);
        let actors: Vec<(u32, bool)> = moves.iter().map(|m| (m.actor, m.onward.is_some())).collect();
        assert_eq!(actors, vec![(SUNNY, true), (0x501, false), (0x502, false)]);
        assert!(moves.iter().all(|m| m.placement == far_side));
    }
}
