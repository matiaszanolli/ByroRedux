//! The enable edge of a scripted `Enable()` on a resident reference (#5065).
//!
//! A placement whose reference is disabled when its cell loads keeps a bare
//! root marked [`PlacementContentWithheld`] — identity, scripts and payloads,
//! no mesh, collider or light (`spawn::spawn_placed_instances`, #3278). Once
//! the enable ledger says the reference is enabled — its own `Enable()`, or
//! its enable parent's (New Vegas's shooting-tutorial bottles come in when
//! Sunny's line enables `VCG02BottleMarkerREF`) — its content is spawned here
//! through the ordinary reference loader, *into that same root*: the loader's
//! `spawn_placement_root` takes the root from [`RealizingPlacementRoots`]
//! instead of spawning one. So the reference keeps the entity its scripts,
//! their variables and anything holding it already know, and its scripts get
//! the `OnLoad` the original runs when a reference's 3D loads.
//!
//! The disable edge (a live reference going away) is still deferred to the
//! next cell load; see the `spawn.rs` gate.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use byroredux_core::ecs::components::FormIdComponent;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{CellRoot, Resource, World};
use byroredux_core::form_id::{FormId, FormIdPool};
use byroredux_plugin::esm::cell::PlacedRef;
use byroredux_renderer::VulkanContext;

use crate::asset_provider::{MaterialProvider, TextureProvider};
use crate::components::PlacementContentWithheld;

/// Withheld roots the reference loader fills instead of spawning new ones,
/// by placement form id. Present only while [`realize_enabled_placements`]
/// runs the loader. Session state, `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Default)]
pub(crate) struct RealizingPlacementRoots(pub(crate) HashMap<FormId, EntityId>);

impl Resource for RealizingPlacementRoots {}

/// The root waiting to be filled for placement `fid`, taken so it is filled
/// once. `None` outside a realization, and for every other placement.
pub(crate) fn take_realizing_root(world: &World, fid: FormId) -> Option<EntityId> {
    world
        .try_resource_mut::<RealizingPlacementRoots>()
        .and_then(|mut roots| roots.0.remove(&fid))
}

/// A withheld root the ledger now enables, with what the loader needs to fill it.
#[derive(Debug, Clone)]
pub(crate) struct EnabledPlacement {
    pub(crate) root: EntityId,
    pub(crate) fid: FormId,
    pub(crate) placed: PlacedRef,
}

/// The enabled withheld placements, by the cell they belong to.
#[derive(Debug, Default)]
pub(crate) struct EnabledPlacements {
    pub(crate) by_cell: Vec<(EntityId, Vec<EnabledPlacement>)>,
    /// A root the ledger enables is not stamped into a cell yet (its cell's
    /// load is still running): ask again next frame.
    pub(crate) pending: bool,
}

/// Every withheld placement root whose reference is now enabled, through the
/// same enable-parent resolution its spawn gate used. A root whose placement
/// the load order no longer holds is left alone.
pub(crate) fn enabled_withheld_placements(world: &World) -> EnabledPlacements {
    let withheld: Vec<EntityId> = match world.query::<PlacementContentWithheld>() {
        Some(marks) => marks.iter().map(|(entity, _)| entity).collect(),
        None => return EnabledPlacements::default(),
    };
    if withheld.is_empty() {
        return EnabledPlacements::default();
    }
    let Some(index) = world
        .try_resource::<super::LoadedCellIndex>()
        .map(|index| Arc::clone(&index.0))
    else {
        return EnabledPlacements::default();
    };
    let mut found = EnabledPlacements::default();
    let mut by_cell: HashMap<EntityId, Vec<EnabledPlacement>> = HashMap::new();
    for root in withheld {
        // Non-primary SCOL/PKIN children carry no identity of their own.
        let Some(fid) = world.get::<FormIdComponent>(root).map(|id| id.0) else {
            continue;
        };
        if super::spawn::placement_is_disabled(world, Some(fid)) {
            continue;
        }
        let Some(local) = world.try_resource::<FormIdPool>().and_then(|pool| pool.local_of(fid)) else {
            continue;
        };
        let Some((_, placed)) = index.cells.locate_refr(local) else {
            log::debug!("enabled reference {local:08X} has no placement in the load order; left withheld");
            continue;
        };
        let Some(cell_root) = world.get::<CellRoot>(root).map(|cell| cell.0) else {
            found.pending = true;
            continue;
        };
        by_cell.entry(cell_root).or_default().push(EnabledPlacement { root, fid, placed: placed.clone() });
    }
    found.by_cell = by_cell.into_iter().collect();
    found
}

/// Fill `placements` — withheld roots of the loaded cell `cell_root` whose
/// references are now enabled — with their content, through the ordinary
/// reference loader, and stamp what it spawned into the cell so it unloads
/// with it.
pub(crate) fn realize_enabled_placements(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
    cell_root: EntityId,
    placements: &[EnabledPlacement],
) {
    let Some(index) = world
        .try_resource::<super::LoadedCellIndex>()
        .map(|index| Arc::clone(&index.0))
    else {
        return;
    };
    let load_order = world
        .try_resource::<super::load_order::ActiveLoadOrder>()
        .map(|order| Arc::clone(&order.0))
        .unwrap_or_default();
    world.insert_resource(RealizingPlacementRoots(
        placements.iter().map(|placement| (placement.fid, placement.root)).collect(),
    ));
    let refs: Vec<PlacedRef> = placements.iter().map(|placement| placement.placed.clone()).collect();
    let first = world.next_entity_id();
    super::references::load_references(
        &refs,
        &index.cells,
        &index,
        &index.races,
        index.game,
        world,
        ctx,
        tex_provider,
        mat_provider,
        "enabled references",
        &load_order,
        &HashSet::new(),
    );
    let last = world.next_entity_id();
    super::load::stamp_cell_root_range(world, cell_root, first, last);
    let unfilled = world
        .remove_resource::<RealizingPlacementRoots>()
        .map(|roots| roots.0)
        .unwrap_or_default();
    for placement in placements {
        if unfilled.contains_key(&placement.fid) {
            log::warn!(
                "enabled reference {:08X}: the reference loader did not reach its placement; it stays withheld",
                placement.placed.form_id
            );
        } else {
            log::info!("Enabled reference {:08X}: its content spawned (#5065)", placement.placed.form_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};
    use byroredux_plugin::esm::cell::{CellData, EnableParent};

    const MARKER: u32 = 0x0016_8A92;
    const BOTTLE: u32 = 0x0010_A202;
    const LONER: u32 = 0x0010_A300;

    /// A loaded cell holding a disabled marker, a bottle that is its enable
    /// child and an independently disabled reference, each spawned as a
    /// withheld root stamped into the cell.
    fn world_with_withheld_roots() -> (World, EntityId, EntityId, EntityId) {
        let mut world = World::new();
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        let mut bottle = PlacedRef::at(BOTTLE, 0x10_A1F6, [1.0, 2.0, 3.0], [0.0; 3]);
        bottle.enable_parent = Some(EnableParent { form_id: MARKER, inverted: false });
        let mut marker = PlacedRef::at(MARKER, 0x3B, [0.0; 3], [0.0; 3]);
        marker.initially_disabled = true;
        let mut loner = PlacedRef::at(LONER, 0x10_A1F6, [0.0; 3], [0.0; 3]);
        loner.initially_disabled = true;
        index.cells.cells.insert(
            "goodsprings".into(),
            CellData {
                form_id: 0x10,
                editor_id: "Goodsprings".into(),
                references: vec![marker, bottle, loner],
                ..Default::default()
            },
        );
        world.insert_resource(byroredux_scripting::AuthoredDisabledReferences {
            initially_disabled: [MARKER, LONER].into_iter().collect(),
            enable_parents: [(BOTTLE, EnableParent { form_id: MARKER, inverted: false })].into_iter().collect(),
        });
        world.insert_resource(super::super::LoadedCellIndex(Arc::new(index)));
        world.insert_resource(byroredux_scripting::ReferenceEnableState::default());
        world.insert_resource(FormIdPool::new());
        let cell_root = world.spawn();
        let withheld_root = |world: &mut World, local: u32| {
            let fid = world.resource_mut::<FormIdPool>().intern(FormIdPair {
                plugin: PluginId::from_filename("FalloutNV.esm"),
                local: LocalFormId(local),
            });
            let root = world.spawn();
            world.insert(root, FormIdComponent(fid));
            world.insert(root, PlacementContentWithheld);
            world.insert(root, CellRoot(cell_root));
            root
        };
        let bottle_root = withheld_root(&mut world, BOTTLE);
        let loner_root = withheld_root(&mut world, LONER);
        (world, cell_root, bottle_root, loner_root)
    }

    /// Nothing is enabled: nothing to fill. Enabling the marker enables its
    /// child — the bottle's root is found in its cell with its own placement —
    /// and leaves an unrelated disabled reference withheld.
    #[test]
    fn an_enable_parent_brings_its_withheld_child_in() {
        let (world, cell_root, bottle_root, _loner_root) = world_with_withheld_roots();
        let found = enabled_withheld_placements(&world);
        assert!(found.by_cell.is_empty() && !found.pending, "every root is still disabled");

        world.resource_mut::<byroredux_scripting::ReferenceEnableState>().set_enabled(MARKER, true);
        let found = enabled_withheld_placements(&world);
        assert!(!found.pending);
        assert_eq!(found.by_cell.len(), 1);
        let (cell, placements) = &found.by_cell[0];
        assert_eq!(*cell, cell_root);
        assert_eq!(placements.len(), 1, "the loner stays disabled");
        assert_eq!(placements[0].root, bottle_root);
        assert_eq!(placements[0].placed.form_id, BOTTLE);
        assert_eq!(placements[0].placed.position, [1.0, 2.0, 3.0]);
    }

    /// A root whose cell load has not stamped it yet is asked about again.
    #[test]
    fn a_root_not_yet_in_its_cell_is_pending() {
        let (mut world, _cell_root, bottle_root, _loner_root) = world_with_withheld_roots();
        world.remove::<CellRoot>(bottle_root);
        world.resource_mut::<byroredux_scripting::ReferenceEnableState>().set_enabled(MARKER, true);
        let found = enabled_withheld_placements(&world);
        assert!(found.pending);
        assert!(found.by_cell.is_empty());
    }

    /// The loader takes the waiting root for its placement once, and no other.
    #[test]
    fn the_waiting_root_is_taken_once_for_its_own_placement() {
        let (mut world, _cell_root, bottle_root, loner_root) = world_with_withheld_roots();
        let fid = |world: &World, entity| world.get::<FormIdComponent>(entity).unwrap().0;
        let (bottle_fid, loner_fid) = (fid(&world, bottle_root), fid(&world, loner_root));
        assert_eq!(take_realizing_root(&world, bottle_fid), None, "outside a realization");
        world.insert_resource(RealizingPlacementRoots(HashMap::from([(bottle_fid, bottle_root)])));
        assert_eq!(take_realizing_root(&world, loner_fid), None);
        assert_eq!(take_realizing_root(&world, bottle_fid), Some(bottle_root));
        assert_eq!(take_realizing_root(&world, bottle_fid), None, "filled once");
    }
}
