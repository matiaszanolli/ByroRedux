//! Whether a reference is in an interior or an exterior cell, and which worldspace (M47.3).
//!
//! GECK *IsInInterior*: "returns 1 if the calling reference is in an interior cell". GECK
//! *GetInWorldspace*: "returns 1 if the actor is currently in one of the exterior cells of the
//! specified worldspace. Always returns 0 if the actor is in an interior cell."
//!
//! A loaded exterior cell's root carries [`ExteriorCell`] (the cell loader stamps it with the
//! worldspace's form id); an interior root has none. A cell-owned reference follows its
//! `CellRoot`. The player is not cell-owned, so the engine publishes [`PlayerCellKind`] as the
//! player moves, the way it publishes `PlayerLocation`.

use byroredux_core::ecs::components::{CellFormId, CellRoot};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::{Resource, World};

/// Stamped on an exterior cell's root: the form id of the worldspace the cell belongs to.
/// `NOT_SAVED_BY_DESIGN`: re-stamped whenever the cell loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExteriorCell(pub u32);

impl Component for ExteriorCell {
    type Storage = SparseSetStorage<Self>;
}

/// The kind of cell a reference is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Interior,
    Exterior { worldspace: u32 },
}

/// The kind of cell the player occupies; `None` while no cell is loaded (loose-NIF mode, a
/// transition in flight). Not saved: derived from the loaded cell.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PlayerCellKind(pub Option<CellKind>);

impl Resource for PlayerCellKind {}

/// The kind of cell `entity` is in.
pub fn cell_kind_of(world: &World, entity: EntityId) -> Option<CellKind> {
    if crate::papyrus_demo::PapyrusPlayerEntity::get(world) == Some(entity) {
        return world.try_resource::<PlayerCellKind>()?.0;
    }
    let root = world.get::<CellRoot>(entity).map_or(entity, |root| root.0);
    // A root with no authored cell identity is not a cell.
    world.get::<CellFormId>(root)?;
    Some(match world.get::<ExteriorCell>(root) {
        Some(exterior) => CellKind::Exterior { worldspace: exterior.0 },
        None => CellKind::Interior,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_follows_its_cell_roots_kind() {
        let mut world = World::new();
        world.register::<ExteriorCell>();
        let interior = world.spawn();
        world.insert(interior, CellFormId(1));
        let exterior = world.spawn();
        world.insert(exterior, CellFormId(2));
        world.insert(exterior, ExteriorCell(0x0A));
        let indoors = world.spawn();
        world.insert(indoors, CellRoot(interior));
        let outdoors = world.spawn();
        world.insert(outdoors, CellRoot(exterior));
        assert_eq!(cell_kind_of(&world, indoors), Some(CellKind::Interior));
        assert_eq!(cell_kind_of(&world, outdoors), Some(CellKind::Exterior { worldspace: 0x0A }));
        // No cell identity, no answer.
        let loose = world.spawn();
        assert_eq!(cell_kind_of(&world, loose), None);
    }
}
