//! Actors that move between spaces (PRD cross-cell-actor-travel; #6852, #6853).
//!
//! A *space* is an interior cell or a whole worldspace. An actor whose Travel
//! destination lies in another space walks to a load door in its own cell
//! whose `XTEL` leads into the destination's space, "as he has to go through
//! one or more load doors" (CS *SubSpace*), and passes it (XCT-ROUTE-001/002).
//! The pass is a request ([`PendingRelocations`]): the ledger
//! ([`ActorRelocations`]) records the actor's new cell and pose, the actor
//! leaves the cell it was in through the same capture-and-release path a cell
//! unload takes, and [`relocated_in`] hands it to the reference loader for the
//! cell it entered as soon as that cell is loaded. From then on the cell
//! loader never spawns it in its home cell (XCT-LOC-002).

use std::collections::HashSet;
use std::sync::Arc;

use byroredux_core::ecs::components::{FormIdComponent, GlobalTransform};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{CellRoot, Component, Resource, SparseSetStorage, World};
use byroredux_core::form_id::FormIdPool;
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::cell::{CellRef, PlacedRef};
use byroredux_plugin::esm::records::EsmIndex;
use byroredux_scripting::{ActorRelocations, CreatedCell, CreatedPlacement};

use crate::components::DoorTeleport;

/// The `group_type` of a placement in a cell's Persistent Children group.
const PERSISTENT_CHILDREN: u8 = 8;

/// The actors the reference loader is spawning at their ledger placement right
/// now: the home-cell skip ([`skipped_at_home`]) lets these through. Present
/// only while [`spawn_relocated`] runs. `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Default)]
pub(crate) struct RelocatedSpawning(pub(crate) HashSet<u32>);

impl Resource for RelocatedSpawning {}

/// An actor walking to the load door that takes it toward its destination's
/// space; passing the door relocates it to `placement` (XCT-ROUTE-001/002).
/// `NOT_SAVED_BY_DESIGN`: the route is chosen again from the package after a
/// load.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CrossSpaceHop {
    pub(crate) door: EntityId,
    pub(crate) placement: CreatedPlacement,
}

impl Component for CrossSpaceHop {
    type Storage = SparseSetStorage<Self>;
}

/// XCT-LOC-002 — whether the home cell's reference list must leave `form`
/// out: the ledger puts it elsewhere and the relocation step is not the one
/// spawning it.
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

/// The cell `form` is in now: its ledger placement once it has moved, its
/// placement in the load order before.
pub(crate) fn current_cell(world: &World, index: &EsmIndex, form: u32) -> Option<CreatedCell> {
    if let Some(placement) = world
        .try_resource::<ActorRelocations>()
        .and_then(|ledger| ledger.placement(form).cloned())
    {
        return Some(placement.cell);
    }
    index
        .cells
        .locate_refr(form)
        .map(|(cell, placed)| cell_at(cell, placed.position))
}

/// Where passing `door` puts an actor: the `XTEL` pose, in the cell of the
/// door it leads to.
fn door_destination(index: &EsmIndex, door: &DoorTeleport) -> Option<CreatedPlacement> {
    let (cell, _) = index.cells.locate_refr(door.destination_form_id)?;
    Some(CreatedPlacement {
        cell: cell_at(cell, door.position_zup),
        position_zup: door.position_zup,
        rotation_zup: door.rotation_zup,
    })
}

/// How a Travel actor reaches its destination reference.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Route {
    /// The destination is resident, unknown to this check, or in the actor's
    /// own space: walk to it as before.
    SameSpace,
    /// Walk to this load door, at `position`; passing it relocates the actor.
    Door { door: EntityId, position: Vec3, placement: CreatedPlacement },
    /// No hop leads toward the destination (XCT-DEST-003/004, XCT-ROUTE-003).
    Unroutable(&'static str),
}

/// The load-order FormID of `entity`'s reference.
fn local_form(world: &World, entity: EntityId) -> Option<u32> {
    let id = world.get::<FormIdComponent>(entity)?.0;
    world.try_resource::<FormIdPool>()?.local_of(id)
}

/// XCT-ROUTE-001 — the route from `actor` to its Travel destination
/// `target`: a resident or same-space target is walked to directly; one in
/// another space through the nearest load door of the actor's cell whose
/// `XTEL` leads into that space.
pub(crate) fn route_toward(world: &World, actor: EntityId, target: Option<u32>) -> Route {
    let Some(target) = target else {
        return Route::SameSpace;
    };
    if byroredux_scripting::condition::resolve_entity_by_global_form_id(world, target).is_some() {
        return Route::SameSpace;
    }
    let Some(index) = world
        .try_resource::<super::LoadedCellIndex>()
        .map(|index| Arc::clone(&index.0))
    else {
        return Route::SameSpace;
    };
    let Some(actor_form) = local_form(world, actor) else {
        return Route::SameSpace;
    };
    let Some(here) = current_cell(world, &index, actor_form) else {
        return Route::SameSpace;
    };
    let Some(there) = index
        .cells
        .locate_refr(target)
        .map(|(cell, placed)| cell_at(cell, placed.position))
    else {
        return Route::Unroutable("its destination is not in the load order");
    };
    if same_space(&here, &there) {
        return Route::SameSpace;
    }
    // XCT-DEST-004 (Q3): only persistent actors change spaces.
    let persistent = index
        .cells
        .locate_refr(actor_form)
        .is_none_or(|(_, placed)| placed.group_type == PERSISTENT_CHILDREN || placed.group_type == 0xFF);
    if !persistent {
        return Route::Unroutable("not persistent");
    }
    let (Some(cell_root), Some(from)) = (
        world.get::<CellRoot>(actor).map(|root| root.0),
        world.get::<GlobalTransform>(actor).map(|t| t.translation),
    ) else {
        return Route::SameSpace;
    };
    let doors: Vec<(EntityId, DoorTeleport)> = world
        .query::<DoorTeleport>()
        .map(|doors| doors.iter().map(|(door, teleport)| (door, *teleport)).collect())
        .unwrap_or_default();
    let best = doors
        .into_iter()
        .filter(|(door, _)| world.get::<CellRoot>(*door).is_some_and(|root| root.0 == cell_root))
        .filter_map(|(door, teleport)| {
            let placement = door_destination(&index, &teleport)?;
            if !same_space(&placement.cell, &there) {
                return None;
            }
            // Walk to the floor in front of the door, not into it: the door it
            // leads to carries an `XTEL` back into this cell, the pose the
            // engine places someone coming through it at.
            let position = index
                .cells
                .locate_refr(teleport.destination_form_id)
                .and_then(|(_, partner)| partner.teleport)
                .map(|back| Vec3::from(byroredux_core::math::coord::zup_to_yup_pos(back.position)))
                .or_else(|| world.get::<GlobalTransform>(door).map(|t| t.translation))?;
            Some((door, position, placement))
        })
        .min_by(|a, b| a.1.distance_squared(from).total_cmp(&b.1.distance_squared(from)));
    match best {
        Some((door, position, placement)) => Route::Door { door, position, placement },
        None => Route::Unroutable("no load door of its cell leads into its destination's space"),
    }
}

/// The ledgered actors placed in `cell` that have no live entity, as the
/// placements the reference loader spawns: each actor's own record at its
/// ledger pose. `alive` answers whether an actor's reference already has one.
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
        .filter(|(_, placement)| match (&placement.cell, cell) {
            (CreatedCell::Interior { editor_id: a }, CreatedCell::Interior { editor_id: b }) => {
                a.eq_ignore_ascii_case(b)
            }
            (a, b) => a == b,
        })
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

/// Whether `form`'s reference has a live, spawned entity (not a logical
/// stub): one carrying its identity and a cell root.
pub(crate) fn has_live_entity(world: &World, form: u32) -> bool {
    byroredux_scripting::condition::resolve_entity_by_global_form_id(world, form).is_some_and(|entity| {
        world.has::<CellRoot>(entity)
            && !world.has::<byroredux_scripting::obscript_objects::UnloadedReferenceStub>(entity)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::cell::CellData;

    fn interior(editor_id: &str) -> CreatedCell {
        CreatedCell::Interior { editor_id: editor_id.into() }
    }

    #[test]
    fn a_space_is_an_interior_or_a_whole_worldspace() {
        let a = CreatedCell::Exterior { worldspace: "WastelandNV".into(), grid: (-18, 0) };
        let b = CreatedCell::Exterior { worldspace: "wastelandnv".into(), grid: (-17, 1) };
        assert!(same_space(&a, &b), "another grid of the same worldspace is the same space");
        assert!(same_space(&interior("GSProspectorSaloonInterior"), &interior("gsprospectorsalooninterior")));
        assert!(!same_space(&interior("GSProspectorSaloonInterior"), &a));
        assert!(!same_space(&interior("A"), &interior("B")));
    }

    /// The home cell's list leaves a ledgered actor out unless the relocation
    /// step is the one spawning it.
    #[test]
    fn a_ledgered_actor_is_skipped_at_home_except_by_the_relocation_step() {
        let mut world = World::new();
        assert!(!skipped_at_home(&world, 0x104E85), "no ledger");
        let mut ledger = ActorRelocations::default();
        ledger.relocate(
            0x104E85,
            CreatedPlacement { cell: interior("Outside"), position_zup: [0.0; 3], rotation_zup: [0.0; 3] },
        );
        world.insert_resource(ledger);
        assert!(skipped_at_home(&world, 0x104E85));
        assert!(!skipped_at_home(&world, 0x104E86), "only the moved actor");
        world.insert_resource(RelocatedSpawning(HashSet::from([0x104E85])));
        assert!(!skipped_at_home(&world, 0x104E85));
    }

    /// A ledgered actor placed in a cell, with no live entity, is handed to the
    /// loader as its own record at the ledger pose; one alive, or placed in
    /// another cell, is not.
    #[test]
    fn the_relocated_actors_of_a_cell_come_back_at_their_ledger_pose() {
        let mut world = World::new();
        let mut index = EsmIndex::default();
        let mut sunny = PlacedRef::at(0x104E85, 0x104E84, [10.0, 20.0, 30.0], [0.0; 3]);
        sunny.group_type = PERSISTENT_CHILDREN;
        index.cells.cells.insert(
            "gsprospectorsalooninterior".into(),
            CellData {
                form_id: 0x106185,
                editor_id: "GSProspectorSaloonInterior".into(),
                references: vec![sunny],
                ..Default::default()
            },
        );
        world.insert_resource(super::super::LoadedCellIndex(Arc::new(index)));
        let outside = CreatedCell::Exterior { worldspace: "WastelandNV".into(), grid: (-17, 0) };
        let mut ledger = ActorRelocations::default();
        ledger.relocate(
            0x104E85,
            CreatedPlacement { cell: outside.clone(), position_zup: [-68000.0, 100.0, 8000.0], rotation_zup: [0.0, 0.0, 1.5] },
        );
        world.insert_resource(ledger);

        let refs = relocated_in(&world, &outside, |_| false);
        assert_eq!(refs.len(), 1);
        assert_eq!((refs[0].form_id, refs[0].base_form_id), (0x104E85, 0x104E84));
        assert_eq!(refs[0].position, [-68000.0, 100.0, 8000.0]);
        assert_eq!(refs[0].rotation, [0.0, 0.0, 1.5]);
        assert!(relocated_in(&world, &outside, |_| true).is_empty(), "already alive");
        assert!(relocated_in(&world, &interior("GSProspectorSaloonInterior"), |_| false).is_empty(), "not its cell any more");
        let other_grid = CreatedCell::Exterior { worldspace: "WastelandNV".into(), grid: (-18, 0) };
        assert!(relocated_in(&world, &other_grid, |_| false).is_empty(), "another grid of the space");
    }
}
