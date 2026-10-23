//! #6076 — the persistent CELL beyond the entry window streams with its tiles.

use super::*;
use byroredux_plugin::esm::cell::{CellData, EsmCellIndex, PlacedRef};
use byroredux_plugin::esm::records::NpcRecord;

const PERSISTENT_CELL: u32 = 0x0010_1000;
const DOOR: u32 = 0x0010_1001;
const NEAR_DOOR: u32 = 0x0010_1002;
const ACTOR: u32 = 0x0010_1003;
const ACTOR_BASE: u32 = 0x0010_1100;

fn placed(form_id: u32, base_form_id: u32, tile: (i32, i32)) -> PlacedRef {
    PlacedRef {
        form_id,
        position: [
            tile.0 as f32 * EXTERIOR_CELL_UNITS + 100.0,
            tile.1 as f32 * EXTERIOR_CELL_UNITS + 100.0,
            0.0,
        ],
        ..crate::cell_loader::refr_texture_overlay_tests::empty_placed_ref(base_form_id)
    }
}

fn wctx() -> ExteriorWorldContext {
    let mut cells = EsmCellIndex::default();
    cells.worldspace_persistent_cells.insert(
        "wastelandnv".into(),
        CellData {
            form_id: PERSISTENT_CELL,
            references: vec![
                placed(NEAR_DOOR, 0x0010_1200, (0, 0)),
                placed(DOOR, 0x0010_1200, (20, -3)),
                placed(ACTOR, ACTOR_BASE, (20, -3)),
            ],
            ..Default::default()
        },
    );
    let mut index = byroredux_plugin::esm::records::EsmIndex {
        cells,
        ..Default::default()
    };
    index.npcs.insert(
        ACTOR_BASE,
        NpcRecord {
            form_id: ACTOR_BASE,
            ..Default::default()
        },
    );
    ExteriorWorldContext {
        record_index: std::sync::Arc::new(index),
        load_order: std::sync::Arc::new(Default::default()),
        plugin_path: String::new(),
        plugin_paths: Vec::new(),
        worldspace_key: "wastelandnv".into(),
        climate: None,
        default_weather: None,
        default_water_height: None,
        default_water_type_form: None,
    }
}

fn window(world: &mut World, cell_form_id: u32) {
    world.insert_resource(PersistentCellWindow {
        cell_form_id,
        center: (0, 0),
        radius: 3,
    });
}

fn ids(refs: Vec<&PlacedRef>) -> Vec<u32> {
    refs.iter().map(|placed| placed.form_id).collect()
}

#[test]
fn a_tile_beyond_the_entry_window_owns_the_persistent_refs_standing_in_it() {
    let wctx = wctx();
    let mut world = World::new();
    assert!(
        persistent_refs_for_tile(&wctx, &world, 20, -3).is_empty(),
        "no partition yet: the persistent root's job owns the whole set"
    );
    window(&mut world, PERSISTENT_CELL);
    assert_eq!(ids(persistent_refs_for_tile(&wctx, &world, 20, -3)), vec![DOOR, ACTOR]);
    assert!(
        persistent_refs_for_tile(&wctx, &world, 0, 0).is_empty(),
        "inside the window the root spawned them"
    );
    assert!(persistent_refs_for_tile(&wctx, &world, 21, -3).is_empty());
    window(&mut world, 0x0BAD_CE11);
    assert!(
        persistent_refs_for_tile(&wctx, &world, 20, -3).is_empty(),
        "a window over another CELL says nothing about this one"
    );
}

#[test]
fn a_persistent_actor_is_promoted_from_its_stub_and_demoted_back() {
    let wctx = wctx();
    let mut world = World::new();
    world.insert_resource(byroredux_core::form_id::FormIdPool::new());
    world.insert_resource(crate::cell_loader::reference_state::PersistentReferenceStates::default());
    window(&mut world, PERSISTENT_CELL);
    let root = world.spawn();
    let carriers = |world: &World| {
        world
            .query::<byroredux_scripting::SceneAliasCandidate>()
            .map(|query| {
                query
                    .iter()
                    .filter(|(_, candidate)| candidate.reference_form_id == ACTOR)
                    .count()
            })
            .unwrap_or(0)
    };

    ensure_persistent_actor_stubs(&mut world, &wctx, &[(20, -3)], Some(root));
    assert_eq!(carriers(&world), 1, "the out-of-range actor stands as a stub");
    ensure_persistent_actor_stubs(&mut world, &wctx, &[(20, -3)], Some(root));
    assert_eq!(carriers(&world), 1, "idempotent");

    let refs: Vec<PlacedRef> = persistent_refs_for_tile(&wctx, &world, 20, -3)
        .into_iter()
        .cloned()
        .collect();
    promote_persistent_actor_stubs(&mut world, &wctx, &refs);
    assert_eq!(carriers(&world), 0, "the tile's real actor takes over its identity");

    ensure_persistent_actor_stubs(&mut world, &wctx, &[(20, -3)], Some(root));
    assert_eq!(carriers(&world), 1, "the tile left: back to a stub");
    ensure_persistent_actor_stubs(&mut world, &wctx, &[(20, -3)], None);
    assert_eq!(carriers(&world), 1, "no persistent root, no owner: nothing spawned");
}
