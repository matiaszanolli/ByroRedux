//! `EsmCellIndex::cell_for_refr_form_id` reverse-lookup tests.
//!
//! M40 Phase 2 Stage 1 — the door-teleport plumbing reads an XTEL's
//! destination FormID and asks the cell index which cell contains a
//! REFR with that FormID. These tests pin the helper's behaviour on
//! six shapes:
//!
//!   * interior cell hit
//!   * exterior cell hit (with worldspace + grid resolution)
//!   * persistent-worldspace hit (grid derived from authored position)
//!   * negative persistent-worldspace coordinates use floor, not truncation
//!   * miss when the FormID isn't placed anywhere in the index
//!   * owned variant via `CellRef::to_owned` survives index drop

use super::super::*;

fn placed_ref(form_id: u32) -> PlacedRef {
    PlacedRef {
        form_id,
        base_form_id: 0,
        group_type: 0xFF,
        position: [0.0; 3],
        rotation: [0.0; 3],
        scale: 1.0,
        enable_parent: None,
        teleport: None,
        reputation_ref: None,
        primitive: None,
        linked_refs: Vec::new(),
        activate_parents: Vec::new(),
        parent_activate_only: false,
        location_ref_types: Vec::new(),
        rooms: Vec::new(),
        portals: Vec::new(),
        radius_override: None,
        alt_texture_ref: None,
        land_texture_ref: None,
        texture_slot_swaps: Vec::new(),
        emissive_light_ref: None,
        material_swap_ref: None,
        ownership: None,
        script_instance: None,
        lock: None,
        water_velocity: None,
        item_count: None,
        initially_disabled: false,
        starts_dead: false,
        starts_unconscious: false,
        ragdoll_pose: Vec::new(),
        map_marker: None,
        encounter_zone_form: None,
    }
}

fn placed_ref_at(form_id: u32, position: [f32; 3]) -> PlacedRef {
    PlacedRef {
        position,
        ..placed_ref(form_id)
    }
}

fn empty_cell(editor_id: &str, refr_form_ids: &[u32]) -> CellData {
    CellData {
        form_id: 0xABCD,
        editor_id: editor_id.to_string(),
        references: refr_form_ids.iter().copied().map(placed_ref).collect(),
        is_interior: true,
        ..Default::default()
    }
}

fn interior_cell(editor_id: &str, refr_form_ids: &[u32]) -> CellData {
    let mut c = empty_cell(editor_id, refr_form_ids);
    c.is_interior = true;
    c.grid = None;
    c
}

fn exterior_cell(gx: i32, gy: i32, refr_form_ids: &[u32]) -> CellData {
    let mut c = empty_cell("ExtCell", refr_form_ids);
    c.is_interior = false;
    c.grid = Some((gx, gy));
    c
}

#[test]
fn cell_for_refr_form_id_hits_interior_cell() {
    let mut index = EsmCellIndex::default();
    index.cells.insert(
        "gsdocmitchellhouse".to_string(),
        interior_cell("GSDocMitchellHouse", &[0x1001, 0x1002, 0x1003]),
    );

    let found = index.cell_for_refr_form_id(0x1002);
    assert_eq!(
        found,
        Some(CellRef::Interior {
            editor_id: "GSDocMitchellHouse",
        }),
        "FormID 0x1002 must resolve to the interior cell that contains it"
    );
}

#[test]
fn cell_for_refr_form_id_hits_exterior_cell_with_worldspace_and_grid() {
    let mut index = EsmCellIndex::default();
    let mut grids = std::collections::HashMap::new();
    grids.insert((-2, 5), exterior_cell(-2, 5, &[0x2001, 0x2002]));
    index
        .exterior_cells
        .insert("wastelandnv".to_string(), grids);

    let found = index.cell_for_refr_form_id(0x2002);
    assert_eq!(
        found,
        Some(CellRef::Exterior {
            worldspace: "wastelandnv",
            grid: (-2, 5),
        }),
        "FormID 0x2002 must resolve to the exterior cell at (-2, 5) in wastelandnv"
    );
}

#[test]
fn cell_for_refr_form_id_maps_worldspace_persistent_ref_to_authored_grid() {
    let mut index = EsmCellIndex::default();
    let mut persistent = exterior_cell(0, 0, &[]);
    persistent.grid = None;
    persistent.references.push(placed_ref_at(
        0x1A6F4,
        [
            2.25 * EXTERIOR_CELL_UNITS,
            4.75 * EXTERIOR_CELL_UNITS,
            123.0,
        ],
    ));
    index
        .worldspace_persistent_cells
        .insert("tamriel".to_string(), persistent);

    assert_eq!(
        index.cell_for_refr_form_id(0x1A6F4),
        Some(CellRef::Exterior {
            worldspace: "tamriel",
            grid: (2, 4),
        }),
        "A persistent XTEL destination must resolve to the exterior tile that contains it"
    );
}

#[test]
fn cell_for_refr_form_id_floors_negative_persistent_ref_coordinates() {
    let mut index = EsmCellIndex::default();
    let mut persistent = exterior_cell(0, 0, &[]);
    persistent.grid = None;
    persistent.references.push(placed_ref_at(
        0x2003,
        [
            -0.25 * EXTERIOR_CELL_UNITS,
            -3.01 * EXTERIOR_CELL_UNITS,
            0.0,
        ],
    ));
    index
        .worldspace_persistent_cells
        .insert("wastelandnv".to_string(), persistent);

    assert_eq!(
        index.cell_for_refr_form_id(0x2003),
        Some(CellRef::Exterior {
            worldspace: "wastelandnv",
            grid: (-1, -4),
        }),
        "Negative world coordinates must use floor so the destination tile is not shifted toward zero"
    );
}

#[test]
fn cell_for_refr_form_id_misses_when_form_id_is_unknown() {
    let mut index = EsmCellIndex::default();
    index
        .cells
        .insert("stub".to_string(), interior_cell("Stub", &[0x1001]));
    let mut grids = std::collections::HashMap::new();
    grids.insert((0, 0), exterior_cell(0, 0, &[0x2001]));
    index.exterior_cells.insert("worldspace".to_string(), grids);

    let found = index.cell_for_refr_form_id(0xDEADBEEF);
    assert_eq!(
        found, None,
        "Unknown FormID must return None — typical case of an XTEL pointing \
         into an unloaded master plugin"
    );
}

#[test]
fn cell_for_refr_form_id_returns_owned_variant_via_to_owned() {
    let mut index = EsmCellIndex::default();
    index.cells.insert(
        "interior".to_string(),
        interior_cell("MyInterior", &[0xFF00]),
    );

    let owned = index
        .cell_for_refr_form_id(0xFF00)
        .map(|c| c.to_owned())
        .expect("FormID must resolve");
    assert_eq!(
        owned,
        OwnedCellRef::Interior {
            editor_id: "MyInterior".to_string(),
        },
        "to_owned must materialise an owned-string variant that outlives the index borrow"
    );
}

/// #6125 — `locate_refr` answers from a reverse index built on first use:
/// it returns the placement itself (not just the cell), a miss is a miss, and
/// a `merge_from` rebuilds it instead of answering from the old maps.
#[test]
fn locate_refr_uses_a_reverse_index_that_merge_from_rebuilds() {
    let mut index = EsmCellIndex::default();
    index.cells.insert("a".to_string(), interior_cell("A", &[0x1001, 0x1002]));
    let mut grids = std::collections::HashMap::new();
    grids.insert((3, -4), exterior_cell(3, -4, &[0x2001, 0x2002]));
    index.exterior_cells.insert("ws".to_string(), grids);

    let (cell, reference) = index.locate_refr(0x2002).expect("hit");
    assert_eq!(cell, CellRef::Exterior { worldspace: "ws", grid: (3, -4) });
    assert_eq!(reference.form_id, 0x2002, "the placement itself, not a neighbour");
    assert_eq!(index.locate_refr(0x1001).unwrap().1.form_id, 0x1001);
    assert!(index.locate_refr(0x9999).is_none());

    // A merged-in cell is found: the index built above is dropped by merge_from.
    let mut other = EsmCellIndex::default();
    other.cells.insert("b".to_string(), interior_cell("B", &[0x3001]));
    index.merge_from(other);
    assert_eq!(
        index.locate_refr(0x3001).map(|(cell, _)| cell),
        Some(CellRef::Interior { editor_id: "B" })
    );
    assert!(index.locate_refr(0x1002).is_some(), "the old cells are still there");
}
