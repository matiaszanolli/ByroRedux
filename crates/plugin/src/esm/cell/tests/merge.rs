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
        script_instance: None,
        visible_when_distant: false,
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
        water_height_is_explicit: false,
        image_space_form: None,
        water_type_form: None,
        water_velocity: None,
        acoustic_space_form: None,
        music_type_form: None,
        music_type_enum: None,
        climate_override: None,
        location_form: None,
        regions: Vec::new(),
        lighting_template_form: None,
        ownership: None,
        regional_color_override: None,
        precombined_mesh_hashes: Vec::new(),
        absorbed_refs: std::collections::HashSet::new(),
        navmeshes: Vec::new(),
        deleted_refs: Vec::new(),
    }
}

/// A `PlacedRef` with a given REFR FormID + base FormID (the base encodes
/// which "version" of the ref this is, so an override can be detected).
fn placed(form_id: u32, base_form_id: u32) -> PlacedRef {
    PlacedRef {
        form_id,
        base_form_id,
        position: [0.0; 3],
        rotation: [0.0; 3],
        scale: 1.0,
        enable_parent: None,
        teleport: None,
        primitive: None,
        linked_refs: Vec::new(),
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
    }
}

fn cell_with_refs(edid: &str, refs: Vec<PlacedRef>) -> CellData {
    let mut c = make_interior_cell(0x0001_0000, edid);
    c.references = refs;
    c
}

/// #1546 — a partial DLC override CELL must keep the base game's REFRs and
/// overlay only the ones it re-emits. Pre-fix `extend` replaced the whole
/// `references` vec, so a Dawnguard-style override carrying few/zero REFRs
/// emptied the cell.
#[test]
fn merge_from_interior_override_keeps_base_refrs() {
    let mut master = EsmCellIndex::default();
    master.cells.insert(
        "kagrenzel01".into(),
        cell_with_refs(
            "Kagrenzel01",
            vec![placed(0x10, 0xAA), placed(0x11, 0xBB), placed(0x12, 0xCC)],
        ),
    );

    // DLC override: re-emits REFR 0x11 (changed base) + adds 0x20; says
    // nothing about 0x10 / 0x12.
    let mut child = EsmCellIndex::default();
    child.cells.insert(
        "kagrenzel01".into(),
        cell_with_refs(
            "Kagrenzel01",
            vec![placed(0x11, 0xBEEF), placed(0x20, 0xDD)],
        ),
    );

    master.merge_from(child);

    let cell = master.cells.get("kagrenzel01").unwrap();
    let by_id: std::collections::HashMap<u32, u32> = cell
        .references
        .iter()
        .map(|r| (r.form_id, r.base_form_id))
        .collect();
    assert_eq!(
        cell.references.len(),
        4,
        "3 base REFRs + 1 added, none dropped"
    );
    assert_eq!(
        by_id.get(&0x10),
        Some(&0xAA),
        "untouched base REFR survives"
    );
    assert_eq!(
        by_id.get(&0x12),
        Some(&0xCC),
        "untouched base REFR survives"
    );
    assert_eq!(
        by_id.get(&0x11),
        Some(&0xBEEF),
        "re-emitted REFR takes the override"
    );
    assert_eq!(by_id.get(&0x20), Some(&0xDD), "newly-added REFR appears");
}

/// EX-09/17 item 7 (#2370) — a DLC/mod override CELL that ONLY deletes a
/// base-master REFR (no re-emitted `references` at all) must remove that
/// REFR from the merged cell. Pre-fix, `parse_refr_group` recorded no
/// removal signal for a Deleted-flagged REFR — it just wasn't in `over`'s
/// own `references` — so `merge_placed_references` had nothing telling it
/// to touch the base's copy, and the "deleted" object silently survived
/// the merge, still resident and still rendering.
#[test]
fn cross_plugin_delete_removes_a_base_master_refr() {
    let mut master = EsmCellIndex::default();
    master.cells.insert(
        "kagrenzel01".into(),
        cell_with_refs(
            "Kagrenzel01",
            vec![placed(0x10, 0xAA), placed(0x11, 0xBB), placed(0x12, 0xCC)],
        ),
    );

    // DLC override: deletes REFR 0x11, re-emits nothing else.
    let mut over = make_interior_cell(0x0001_0000, "Kagrenzel01");
    over.deleted_refs = vec![0x11];
    let mut child = EsmCellIndex::default();
    child.cells.insert("kagrenzel01".into(), over);

    master.merge_from(child);

    let cell = master.cells.get("kagrenzel01").unwrap();
    let ids: std::collections::HashSet<u32> = cell.references.iter().map(|r| r.form_id).collect();
    assert_eq!(
        ids,
        [0x10, 0x12].into_iter().collect(),
        "the deleted REFR must be gone; the two untouched base REFRs survive"
    );
}

/// A single override CELL can both delete one base REFR and add/change
/// others in the same pass — the two operations must not interfere.
#[test]
fn cross_plugin_delete_and_override_combine() {
    let mut master = EsmCellIndex::default();
    master.cells.insert(
        "kagrenzel01".into(),
        cell_with_refs(
            "Kagrenzel01",
            vec![placed(0x10, 0xAA), placed(0x11, 0xBB), placed(0x12, 0xCC)],
        ),
    );

    // DLC override: deletes 0x11, changes 0x12's base, adds 0x20.
    let mut over = cell_with_refs(
        "Kagrenzel01",
        vec![placed(0x12, 0xBEEF), placed(0x20, 0xDD)],
    );
    over.deleted_refs = vec![0x11];
    let mut child = EsmCellIndex::default();
    child.cells.insert("kagrenzel01".into(), over);

    master.merge_from(child);

    let cell = master.cells.get("kagrenzel01").unwrap();
    let by_id: std::collections::HashMap<u32, u32> = cell
        .references
        .iter()
        .map(|r| (r.form_id, r.base_form_id))
        .collect();
    assert_eq!(
        cell.references.len(),
        3,
        "0x11 deleted, 0x10/0x12/0x20 remain"
    );
    assert_eq!(
        by_id.get(&0x10),
        Some(&0xAA),
        "untouched base REFR survives"
    );
    assert!(!by_id.contains_key(&0x11), "deleted REFR must not survive");
    assert_eq!(
        by_id.get(&0x12),
        Some(&0xBEEF),
        "re-emitted REFR takes the override"
    );
    assert_eq!(by_id.get(&0x20), Some(&0xDD), "newly-added REFR appears");
}

/// A plugin loaded after the one that deleted a REFR can legitimately
/// re-place a REFR at the same FormID — that's a deliberate un-delete, not
/// a bug, and load-order last-write-wins must still apply: the third
/// plugin's copy wins, same as any other override.
#[test]
fn cross_plugin_delete_then_later_plugin_readds_same_form_id() {
    let mut master = EsmCellIndex::default();
    master.cells.insert(
        "cell".into(),
        cell_with_refs("Cell", vec![placed(0x10, 0xAA)]),
    );

    let mut deleting = make_interior_cell(0x0001_0000, "Cell");
    deleting.deleted_refs = vec![0x10];
    let mut dlc = EsmCellIndex::default();
    dlc.cells.insert("cell".into(), deleting);
    master.merge_from(dlc);
    assert!(
        master.cells["cell"].references.is_empty(),
        "the DLC's deletion must remove the base REFR"
    );

    let mut mod_over = EsmCellIndex::default();
    mod_over.cells.insert(
        "cell".into(),
        cell_with_refs("Cell", vec![placed(0x10, 0xFEED)]),
    );
    master.merge_from(mod_over);

    assert_eq!(
        master.cells["cell"]
            .references
            .iter()
            .map(|r| (r.form_id, r.base_form_id))
            .collect::<Vec<_>>(),
        vec![(0x10, 0xFEED)],
        "a later plugin re-placing the same FormID must win, not be blocked by the earlier deletion"
    );
}

/// SIBLING: the exterior per-grid path applies cross-plugin deletes the
/// same way the interior path does.
#[test]
fn cross_plugin_delete_removes_a_base_master_exterior_refr() {
    let mut master = EsmCellIndex::default();
    master
        .exterior_cells
        .entry("tamriel".into())
        .or_default()
        .insert(
            (5, 7),
            cell_with_refs("Ext", vec![placed(0x30, 0x01), placed(0x31, 0x02)]),
        );

    let mut over = make_interior_cell(0x0001_C000, "Ext");
    over.deleted_refs = vec![0x30];
    let mut child = EsmCellIndex::default();
    child
        .exterior_cells
        .entry("tamriel".into())
        .or_default()
        .insert((5, 7), over);

    master.merge_from(child);

    let cell = &master.exterior_cells["tamriel"][&(5, 7)];
    assert_eq!(
        cell.references
            .iter()
            .map(|r| r.form_id)
            .collect::<Vec<_>>(),
        vec![0x31],
        "deleted exterior REFR 0x30 is gone; untouched 0x31 survives"
    );
}

#[test]
fn merge_from_partial_override_inherits_cell_water_velocity() {
    let mut master = EsmCellIndex::default();
    let mut base = make_interior_cell(0x100, "CurrentPool");
    base.water_velocity = Some([3.0, 4.0, 0.0]);
    master.cells.insert("currentpool".into(), base);

    let mut child = EsmCellIndex::default();
    child
        .cells
        .insert("currentpool".into(), make_interior_cell(0x100, "CurrentPool"));
    master.merge_from(child);

    assert_eq!(
        master.cells["currentpool"].water_velocity,
        Some([3.0, 4.0, 0.0])
    );
}

/// A zero-REFR override (the `kagrenzel01` 1017→0 case) keeps ALL base REFRs
/// — the override only changes CELL-level fields.
#[test]
fn merge_from_zero_refr_override_keeps_whole_base() {
    let mut master = EsmCellIndex::default();
    master.cells.insert(
        "cell".into(),
        cell_with_refs("Cell", vec![placed(0x10, 0xAA), placed(0x11, 0xBB)]),
    );
    let mut child = EsmCellIndex::default();
    child
        .cells
        .insert("cell".into(), cell_with_refs("Cell", Vec::new()));

    master.merge_from(child);
    assert_eq!(
        master.cells.get("cell").unwrap().references.len(),
        2,
        "an override with no REFRs must not empty the cell",
    );
}

/// SIBLING: the exterior per-grid path merges REFRs the same way.
#[test]
fn merge_from_exterior_override_keeps_base_refrs() {
    let mut master = EsmCellIndex::default();
    master
        .exterior_cells
        .entry("tamriel".into())
        .or_default()
        .insert(
            (5, 7),
            cell_with_refs("Ext", vec![placed(0x30, 0x01), placed(0x31, 0x02)]),
        );

    let mut child = EsmCellIndex::default();
    child
        .exterior_cells
        .entry("tamriel".into())
        .or_default()
        .insert((5, 7), cell_with_refs("Ext", vec![placed(0x31, 0xFEED)]));

    master.merge_from(child);

    let cell = &master.exterior_cells["tamriel"][&(5, 7)];
    assert_eq!(
        cell.references.len(),
        2,
        "base exterior REFR 0x30 survives the override"
    );
    let by_id: std::collections::HashMap<u32, u32> = cell
        .references
        .iter()
        .map(|r| (r.form_id, r.base_form_id))
        .collect();
    assert_eq!(by_id.get(&0x30), Some(&0x01));
    assert_eq!(
        by_id.get(&0x31),
        Some(&0xFEED),
        "re-emitted exterior REFR overrides"
    );
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

#[test]
fn merge_from_worldspace_persistent_cell_merges_refs_by_form_id() {
    let mut master = EsmCellIndex::default();
    master.worldspace_persistent_cells.insert(
        "tamriel".into(),
        cell_with_refs(
            "Wilderness",
            vec![placed(0x100, 0xaaa), placed(0x101, 0xbbb)],
        ),
    );

    let mut child = EsmCellIndex::default();
    child.worldspace_persistent_cells.insert(
        "tamriel".into(),
        cell_with_refs(
            "Wilderness",
            vec![placed(0x101, 0xbeef), placed(0x102, 0xccc)],
        ),
    );

    master.merge_from(child);

    let cell = master.worldspace_persistent_cells.get("tamriel").unwrap();
    let refs: std::collections::HashMap<_, _> = cell
        .references
        .iter()
        .map(|placed| (placed.form_id, placed.base_form_id))
        .collect();
    assert_eq!(refs.len(), 3);
    assert_eq!(refs.get(&0x100), Some(&0xaaa));
    assert_eq!(refs.get(&0x101), Some(&0xbeef));
    assert_eq!(refs.get(&0x102), Some(&0xccc));
}

/// #2910 — a partial exterior CELL override inherits every child payload it
/// omits. Dawnguard omits LAND on 218 Skyrim tiles; replacing the whole cell
/// produced literal holes in the world. NAVM and FO4 precombine metadata use
/// the same partial-record contract.
#[test]
fn merge_from_partial_exterior_override_preserves_authored_payloads() {
    let mut base = make_interior_cell(0x0001_C000, "Tamriel05");
    base.is_interior = false;
    base.grid = Some((5, 7));
    base.display_name = Some("Base tile".into());
    base.landscape = Some(LandscapeData {
        heights: vec![42.0],
        normals: None,
        vertex_colors: None,
        quadrants: std::array::from_fn(|_| TerrainQuadrant::default()),
    });
    base.navmeshes = vec![crate::esm::records::NavmRecord {
        form_id: 0x100,
        version: 1,
        ..Default::default()
    }];
    base.precombined_mesh_hashes = vec![0xDEAD_BEEF];
    base.absorbed_refs.insert(0x200);

    let mut master = EsmCellIndex::default();
    master
        .exterior_cells
        .entry("tamriel".into())
        .or_default()
        .insert((5, 7), base);

    let mut over = make_interior_cell(0x0001_C000, "Tamriel05");
    over.is_interior = false;
    over.grid = Some((5, 7));
    over.navmeshes.push(crate::esm::records::NavmRecord {
        form_id: 0x101,
        version: 2,
        ..Default::default()
    });
    over.absorbed_refs.insert(0x201);
    let mut child = EsmCellIndex::default();
    child
        .exterior_cells
        .entry("tamriel".into())
        .or_default()
        .insert((5, 7), over);

    master.merge_from(child);
    let merged = &master.exterior_cells["tamriel"][&(5, 7)];
    assert_eq!(merged.display_name.as_deref(), Some("Base tile"));
    assert_eq!(
        merged
            .landscape
            .as_ref()
            .map(|land| land.heights.as_slice()),
        Some([42.0].as_slice()),
        "an override without LAND must inherit the base terrain",
    );
    assert_eq!(
        merged
            .navmeshes
            .iter()
            .map(|navm| navm.form_id)
            .collect::<Vec<_>>(),
        vec![0x100, 0x101],
    );
    assert_eq!(merged.precombined_mesh_hashes, vec![0xDEAD_BEEF]);
    assert_eq!(merged.absorbed_refs, [0x200, 0x201].into_iter().collect());
}

/// #2911 — CELL identity is the FormID, while EDID is an optional alias. An
/// empty-EDID DLC override must merge into the named base cell rather than
/// creating a second synthetic cell or dropping the override.
#[test]
fn merge_from_empty_edid_override_matches_interior_by_form_id() {
    let form_id = 0x0001_AAAA;
    let mut master = EsmCellIndex::default();
    master.cells.insert(
        "namedcell".into(),
        cell_with_refs("NamedCell", vec![placed(0x10, 0xAA)]),
    );
    master.cells.get_mut("namedcell").unwrap().form_id = form_id;

    let mut child = EsmCellIndex::default();
    let synthetic_id = canonical_interior_cell_id(form_id, "");
    let mut over = cell_with_refs(&synthetic_id, vec![placed(0x11, 0xBB)]);
    over.form_id = form_id;
    child.cells.insert(synthetic_id.to_ascii_lowercase(), over);

    master.merge_from(child);
    assert_eq!(master.cells.len(), 1);
    let merged = master.cells.get("namedcell").expect("base alias retained");
    assert_eq!(merged.editor_id, "NamedCell");
    assert_eq!(
        merged
            .references
            .iter()
            .map(|reference| reference.form_id)
            .collect::<Vec<_>>(),
        vec![0x10, 0x11],
    );
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
