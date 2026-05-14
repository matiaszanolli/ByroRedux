//! Plugin-merge tests across statics, cells, and worldspaces.
//!
//! Disjoint extension, last-plugin-wins on overlap, exterior cell
//! per-worldspace merge, worldspace last-write-wins.

use super::super::*;

fn make_static(form_id: u32, model: &str) -> StaticObject {
    StaticObject {
        form_id,
        editor_id: String::new(),
        model_path: model.to_string(),
        record_type: crate::record::RecordType::STAT,
        light_data: None,
        addon_data: None,
        has_script: false,
    }
}

fn make_interior_cell(form_id: u32, edid: &str) -> CellData {
    CellData {
        form_id,
        editor_id: edid.to_string(),
        display_name: None,
        references: Vec::new(),
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
        lighting_template_form: None,
        ownership: None,
        regional_color_override: None,
    }
}


#[test]
fn merge_from_extends_disjoint_statics_and_cells() {
    // Master plugin contributes one cell + one static; child
    // plugin contributes a disjoint pair. Merge → both visible.
    let mut master = EsmCellIndex::default();
    master
        .statics
        .insert(0x0001_0001, make_static(0x0001_0001, "master/wall.nif"));
    master.cells.insert(
        "homecell".into(),
        make_interior_cell(0x0001_AAAA, "HomeCell"),
    );

    let mut child = EsmCellIndex::default();
    child
        .statics
        .insert(0x0002_0001, make_static(0x0002_0001, "child/door.nif"));
    child
        .cells
        .insert("newcell".into(), make_interior_cell(0x0002_BBBB, "NewCell"));

    master.merge_from(child);

    assert_eq!(master.statics.len(), 2);
    assert_eq!(master.cells.len(), 2);
    assert_eq!(
        master.statics.get(&0x0001_0001).unwrap().model_path,
        "master/wall.nif"
    );
    assert_eq!(
        master.statics.get(&0x0002_0001).unwrap().model_path,
        "child/door.nif"
    );
}

#[test]
fn merge_from_later_plugin_wins_on_overlapping_statics() {
    // Both master and child define the same FormID. Bethesda
    // load-order semantics: child overrides master.
    let mut master = EsmCellIndex::default();
    master
        .statics
        .insert(0x0001_0001, make_static(0x0001_0001, "master/v1.nif"));

    let mut child = EsmCellIndex::default();
    child
        .statics
        .insert(0x0001_0001, make_static(0x0001_0001, "child/v2.nif"));

    master.merge_from(child);

    assert_eq!(master.statics.len(), 1);
    assert_eq!(
        master.statics.get(&0x0001_0001).unwrap().model_path,
        "child/v2.nif",
        "child plugin must override master's STAT (last-write-wins)"
    );
}

#[test]
fn merge_from_exterior_cells_merge_per_worldspace() {
    // Master defines Tamriel grid (0,0); child defines Tamriel
    // grid (1,0) AND a new "soulcairn" worldspace. Both Tamriel
    // entries must coexist; soulcairn lands as a new key.
    let mut master = EsmCellIndex::default();
    master
        .exterior_cells
        .entry("tamriel".into())
        .or_default()
        .insert((0, 0), make_interior_cell(0x0001_C000, "Tamriel00"));

    let mut child = EsmCellIndex::default();
    child
        .exterior_cells
        .entry("tamriel".into())
        .or_default()
        .insert((1, 0), make_interior_cell(0x0002_C001, "Tamriel10"));
    child
        .exterior_cells
        .entry("soulcairn".into())
        .or_default()
        .insert((0, 0), make_interior_cell(0x0002_D000, "SoulCairn00"));

    master.merge_from(child);

    assert_eq!(master.exterior_cells.len(), 2);
    let tam = master.exterior_cells.get("tamriel").unwrap();
    assert_eq!(
        tam.len(),
        2,
        "DLC adding tamriel grid (1,0) must NOT stomp master's grid (0,0)"
    );
    assert!(tam.contains_key(&(0, 0)));
    assert!(tam.contains_key(&(1, 0)));
    assert!(master.exterior_cells.contains_key("soulcairn"));
}

// ── WRLD record decoding (#965 / OBL-D3-NEW-01) ─────────────────────

#[test]
fn merge_from_worldspaces_last_write_wins() {
    let mut master = EsmCellIndex::default();
    master.worldspaces.insert(
        "tamriel".into(),
        WorldspaceRecord {
            form_id: 0x0000_003C,
            editor_id: "Tamriel".into(),
            usable_min: (-122_880.0, -204_800.0),
            usable_max: (143_360.0, 225_280.0),
            ..Default::default()
        },
    );

    let mut child = EsmCellIndex::default();
    child.worldspaces.insert(
        "tamriel".into(),
        WorldspaceRecord {
            form_id: 0x0000_003C,
            editor_id: "Tamriel".into(),
            // DLC widens the usable bounds.
            usable_min: (-163_840.0, -245_760.0),
            usable_max: (184_320.0, 266_240.0),
            parent_flags: 0x004C,
            ..Default::default()
        },
    );
    child.worldspaces.insert(
        "soulcairn".into(),
        WorldspaceRecord {
            form_id: 0x0200_5500,
            editor_id: "SoulCairn".into(),
            parent_worldspace: Some(0x0000_003C),
            ..Default::default()
        },
    );

    master.merge_from(child);

    let tam = master.worldspaces.get("tamriel").unwrap();
    assert_eq!(
        tam.usable_min,
        (-163_840.0, -245_760.0),
        "DLC override must replace master's bounds"
    );
    assert_eq!(tam.parent_flags, 0x004C);
    let sc = master.worldspaces.get("soulcairn").unwrap();
    assert_eq!(sc.parent_worldspace, Some(0x0000_003C));
    assert_eq!(master.worldspaces.len(), 2);
}
