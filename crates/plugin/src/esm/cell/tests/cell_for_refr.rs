//! `EsmCellIndex::cell_for_refr_form_id` reverse-lookup tests.
//!
//! M40 Phase 2 Stage 1 — the door-teleport plumbing reads an XTEL's
//! destination FormID and asks the cell index which cell contains a
//! REFR with that FormID. These tests pin the helper's behaviour on
//! four shapes:
//!
//!   * interior cell hit
//!   * exterior cell hit (with worldspace + grid resolution)
//!   * miss when the FormID isn't placed anywhere in the index
//!   * owned variant via `CellRef::to_owned` survives index drop

use super::super::*;

fn placed_ref(form_id: u32) -> PlacedRef {
    PlacedRef {
        form_id,
        base_form_id: 0,
        position: [0.0; 3],
        rotation: [0.0; 3],
        scale: 1.0,
        enable_parent: None,
        teleport: None,
        primitive: None,
        linked_refs: Vec::new(),
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
    }
}

fn empty_cell(editor_id: &str, refr_form_ids: &[u32]) -> CellData {
    CellData {
        form_id: 0xABCD,
        editor_id: editor_id.to_string(),
        display_name: None,
        references: refr_form_ids.iter().copied().map(placed_ref).collect(),
        is_interior: true,
        grid: None,
        lighting: None,
        landscape: None,
        water_height: None,
        image_space_form: None,
        water_type_form: None,
        acoustic_space_form: None,
        music_type_form: None,
        music_type_enum: None,
        climate_override: None,
        location_form: None,
        regions: Vec::new(),
        regional_color_override: None,
        precombined_mesh_hashes: Vec::new(),
        absorbed_refs: std::collections::HashSet::new(),
        lighting_template_form: None,
        ownership: None,
        navmeshes: Vec::new(),
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
