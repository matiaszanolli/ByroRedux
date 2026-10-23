//! #3376 — identity is necessary but not sufficient for preserving a
//! persistent-CELL root across a worldspace crossing: the root's own
//! spawn must also be finished.
//!
//! The job that finishes it (`WorldStreamingState::persistent_apply`)
//! does not survive the imminent `drain_streaming_state`, which
//! cancels it rather than resuming it (#3377). Preserving an
//! unfinished root then makes
//! `stream_initial_radius`'s `persistent_root.is_none() &&
//! persistent_apply.is_none()` guard false, so no replacement job is
//! created and the CELL's unspawned tail — doors, quest refs, unique
//! actors — is absent for the rest of the session, silently.
use super::*;
use byroredux_plugin::esm::cell::{CellData, EsmCellIndex, WorldspaceRecord};

fn wctx_with_same_persistent_cell() -> ExteriorWorldContext {
    let mut cells = EsmCellIndex::default();
    cells.worldspaces.insert(
        "parentworld".into(),
        WorldspaceRecord {
            form_id: 0x10,
            editor_id: "ParentWorld".into(),
            parent_worldspace: None,
            ..WorldspaceRecord::default()
        },
    );
    cells.worldspaces.insert(
        "childworld".into(),
        WorldspaceRecord {
            form_id: 0x20,
            editor_id: "ChildWorld".into(),
            parent_worldspace: Some(0x10),
            ..WorldspaceRecord::default()
        },
    );
    cells.worldspace_persistent_cells.insert(
        "parentworld".into(),
        CellData {
            form_id: 0x999,
            ..Default::default()
        },
    );

    let index = byroredux_plugin::esm::records::EsmIndex {
        cells,
        ..Default::default()
    };
    ExteriorWorldContext {
        record_index: std::sync::Arc::new(index),
        load_order: std::sync::Arc::new(Default::default()),
        plugin_path: String::new(),
        plugin_paths: Vec::new(),
        worldspace_key: "childworld".into(),
        climate: None,
        default_weather: None,
        default_water_height: None,
        default_water_type_form: None,
    }
}

/// A finished root whose identity is unchanged is preserved — the
/// #2369 item-C2 behaviour this must not regress.
#[test]
fn a_finished_root_with_unchanged_identity_is_preserved() {
    let mut world = World::new();
    let root = world.spawn();
    world.insert(root, CellFormId(0x999));
    let wctx = wctx_with_same_persistent_cell();

    assert_eq!(
        persistent_root_survives_crossing(&world, Some(root), &wctx, false),
        Some(root),
        "an unchanged, finished persistent root must still survive the crossing"
    );
}

/// The fix: the same root, same identity, but its apply job is still
/// in flight — must fall back to the always-rebuild path.
#[test]
fn an_unfinished_root_is_not_preserved_even_when_identity_matches() {
    let mut world = World::new();
    let root = world.spawn();
    world.insert(root, CellFormId(0x999));
    let wctx = wctx_with_same_persistent_cell();

    assert_eq!(
        persistent_root_survives_crossing(&world, Some(root), &wctx, true),
        None,
        "a persistent root whose PersistentCellApplyJob is still in flight must \
         NOT be preserved — the job does not survive the drain, so the CELL's \
         unspawned refs would never be spawned again (#3376)"
    );
}

/// No root at all is still `None`, in-flight or not.
#[test]
fn no_root_is_never_preserved() {
    let world = World::new();
    let wctx = wctx_with_same_persistent_cell();
    assert_eq!(
        persistent_root_survives_crossing(&world, None, &wctx, false),
        None
    );
    assert_eq!(
        persistent_root_survives_crossing(&world, None, &wctx, true),
        None
    );
}
