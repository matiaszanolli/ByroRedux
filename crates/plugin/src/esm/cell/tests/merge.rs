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
        script_form_id: 0,
        visible_when_distant: false,
        destruction: None,
    }
}

fn make_interior_cell(form_id: u32, edid: &str) -> CellData {
    CellData {
        form_id,
        editor_id: edid.to_string(),
        is_interior: true,
        ..Default::default()
    }
}

#[test]
fn show_sky_inherits_only_when_override_omits_cell_data() {
    let mut base = make_interior_cell(0x1234, "OpenRoof");
    base.show_sky = Some(true);

    let mut omitted = make_interior_cell(0x1234, "OpenRoof");
    merge_cell_override(&base, &mut omitted);
    assert_eq!(omitted.show_sky, Some(true));

    let mut closed = make_interior_cell(0x1234, "OpenRoof");
    closed.show_sky = Some(false);
    merge_cell_override(&base, &mut closed);
    assert_eq!(closed.show_sky, Some(false));
}

/// #6043 — the XCLC Land Flags come from the same sub-record as the grid,
/// so an override without XCLC inherits both, and one with XCLC keeps its own.
#[test]
fn land_flags_inherit_with_the_grid() {
    let mut base = make_interior_cell(0x1234, "HiddenQuads");
    base.grid = Some((3, -2));
    base.land_flags = 0x0C;

    let mut omitted = make_interior_cell(0x1234, "HiddenQuads");
    merge_cell_override(&base, &mut omitted);
    assert_eq!((omitted.grid, omitted.land_flags), (Some((3, -2)), 0x0C));

    let mut cleared = make_interior_cell(0x1234, "HiddenQuads");
    cleared.grid = Some((3, -2));
    merge_cell_override(&base, &mut cleared);
    assert_eq!(cleared.land_flags, 0, "an authored XCLC owns its flags");
}

/// A `PlacedRef` with a given REFR FormID + base FormID (the base encodes
/// which "version" of the ref this is, so an override can be detected).
fn placed(form_id: u32, base_form_id: u32) -> PlacedRef {
    PlacedRef {
        form_id,
        base_form_id,
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

/// #4904 — the production load path starts from an empty index and merges
/// every plugin, master included. `landscape_grasses` was missing from the
/// merge, so the LTEX→GRAS map arrived empty on every real load.
#[test]
fn merge_from_carries_landscape_grasses_with_last_writer_winning() {
    let mut merged = EsmCellIndex::default();
    let mut master = EsmCellIndex::default();
    master
        .landscape_grasses
        .insert(0x0000_0A01, vec![0x0000_0B01, 0x0000_0B02]);
    master
        .landscape_grasses
        .insert(0x0000_0A02, vec![0x0000_0B03]);
    merged.merge_from(master);
    assert_eq!(
        merged.landscape_grasses.get(&0x0000_0A01),
        Some(&vec![0x0000_0B01, 0x0000_0B02]),
        "a master's GNAM arrays must survive the merge into the empty index"
    );

    // An override re-authors the whole GNAM array for its LTEX.
    let mut update = EsmCellIndex::default();
    update
        .landscape_grasses
        .insert(0x0000_0A01, vec![0x0000_0B09]);
    merged.merge_from(update);
    assert_eq!(
        merged.landscape_grasses.get(&0x0000_0A01),
        Some(&vec![0x0000_0B09])
    );
    assert_eq!(
        merged.landscape_grasses.get(&0x0000_0A02),
        Some(&vec![0x0000_0B03])
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
    base.absorbed_ref_bakes.push((0x200, 0xDEAD_BEEF));

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
    over.absorbed_ref_bakes.push((0x201, 0xDEAD_BEEF));
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
    assert_eq!(
        merged.absorbed_ref_bakes,
        vec![(0x201, 0xDEAD_BEEF), (0x200, 0xDEAD_BEEF)],
        "override's own bakes first, base's inherited after (#5484 shape)"
    );
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

/// #3362 — a REFR is a globally-unique FormID, so a tombstone authored under
/// a DIFFERENT CELL than the base placement must still remove it. Pre-fix
/// `merge_cell_references` only applied a cell's `deleted_refs` against that
/// same cell's base references, so the `retain` matched nothing and the
/// object stayed resident.
///
/// This is the shape of HearthFires deleting `0010C205` (a cooking pot) under
/// cell `00016778` while `Skyrim.esm` placed it in `00016A06` — the pot still
/// spawned in Proudspire Manor. 5 of 202 vanilla tombstones hit this hole.
#[test]
fn a_tombstone_authored_under_another_cell_still_removes_the_refr() {
    let mut master = EsmCellIndex::default();
    master.cells.insert(
        "proudspiremanor".into(),
        cell_with_refs(
            "SolitudeProudspireManor",
            vec![placed(0x0010_C205, 0x0010_10B3), placed(0x10, 0xAA)],
        ),
    );

    // The DLC tombstones the pot under a completely different cell.
    let mut elsewhere = make_interior_cell(0x0001_6778, "SomeOtherCell");
    elsewhere.deleted_refs = vec![0x0010_C205];
    let mut child = EsmCellIndex::default();
    child.cells.insert("someothercell".into(), elsewhere);

    master.merge_from(child);

    let cell = master.cells.get("proudspiremanor").unwrap();
    let ids: std::collections::HashSet<u32> = cell.references.iter().map(|r| r.form_id).collect();
    assert_eq!(
        ids,
        [0x10].into_iter().collect(),
        "a cross-cell tombstone must remove the placement wherever it lives (#3362)"
    );
}

/// The same, across the exterior / persistent maps rather than interiors —
/// two of the five vanilla survivors cross the persistent(8) -> temporary(9)
/// group boundary.
#[test]
fn a_cross_cell_tombstone_reaches_exterior_and_persistent_cells() {
    let mut master = EsmCellIndex::default();
    let mut ext = cell_with_refs("Tamriel", vec![placed(0x000D_DD36, 0x0002_BCA7)]);
    ext.is_interior = false;
    master
        .exterior_cells
        .entry("tamriel".into())
        .or_default()
        .insert((-39, 2), ext);
    master.worldspace_persistent_cells.insert(
        "tamriel".into(),
        cell_with_refs("TamrielPersistent", vec![placed(0x000C_E60D, 0x0001_40BD)]),
    );

    let mut over = make_interior_cell(0x0000_BC34, "UnrelatedCell");
    over.deleted_refs = vec![0x000D_DD36, 0x000C_E60D];
    let mut child = EsmCellIndex::default();
    child.cells.insert("unrelatedcell".into(), over);

    master.merge_from(child);

    assert!(
        master.exterior_cells["tamriel"][&(-39, 2)]
            .references
            .is_empty(),
        "exterior placement must be removed by a cross-cell tombstone (#3362)"
    );
    assert!(
        master.worldspace_persistent_cells["tamriel"]
            .references
            .is_empty(),
        "persistent placement must be removed by a cross-cell tombstone (#3362)"
    );
}

/// Load order still wins: a plugin that both tombstones a FormID and
/// re-places it keeps its own placement, and a LATER plugin re-placing a
/// FormID an earlier one deleted un-deletes it. The cross-cell sweep must
/// not break either.
#[test]
fn a_replacement_in_the_same_or_a_later_plugin_survives_the_sweep() {
    // Distinct FormIDs: `merge_from` keys interior cells by form_id, so
    // reusing `cell_with_refs`'s fixed id would collapse them.
    let with_id = |form_id: u32, edid: &str, refs: Vec<PlacedRef>| {
        let mut c = make_interior_cell(form_id, edid);
        c.references = refs;
        c
    };

    // Same plugin: deletes 0x11 under cell C, re-places it in cell B.
    let mut master = EsmCellIndex::default();
    master.cells.insert(
        "cella".into(),
        with_id(0xA1, "CellA", vec![placed(0x11, 0xBB)]),
    );

    let mut deleter = make_interior_cell(0xC1, "CellC");
    deleter.deleted_refs = vec![0x11];
    let mut child = EsmCellIndex::default();
    child.cells.insert("cellc".into(), deleter);
    child.cells.insert(
        "cellb".into(),
        with_id(0xB1, "CellB", vec![placed(0x11, 0xDEAD)]),
    );

    master.merge_from(child);

    assert!(
        master.cells["cella"].references.is_empty(),
        "the old placement is removed"
    );
    assert_eq!(
        master.cells["cellb"].references.len(),
        1,
        "the same plugin's own re-placement must survive its own tombstone (#3362)"
    );

    // Later plugin re-places what an earlier one deleted.
    let mut third = EsmCellIndex::default();
    third.cells.insert(
        "cella".into(),
        with_id(0xA1, "CellA", vec![placed(0x11, 0xCC)]),
    );
    master.merge_from(third);
    assert_eq!(
        master.cells["cella"].references.len(),
        1,
        "a later plugin re-placing a deleted FormID un-deletes it"
    );
}

/// #6084 — a REFR is globally unique by FormID. A child plugin that moves
/// one into another cell (the CK files it under the new CELL) must leave ONE
/// copy, at the new position, not the base copy beside it.
#[test]
fn a_refr_moved_into_another_cell_leaves_one_copy() {
    let with_id = |form_id: u32, edid: &str, refs: Vec<PlacedRef>| {
        let mut c = make_interior_cell(form_id, edid);
        c.references = refs;
        c
    };
    let mut master = EsmCellIndex::default();
    master.cells.insert(
        "cella".into(),
        with_id(0xA1, "CellA", vec![placed(0x11, 0xBB), placed(0x12, 0xBB)]),
    );
    master
        .cells
        .insert("cellb".into(), with_id(0xB1, "CellB", vec![placed(0x13, 0xBB)]));

    // The child moves 0x11 from A to B and re-emits nothing for A.
    let mut moved = placed(0x11, 0xBB);
    moved.position = [9.0, 9.0, 9.0];
    let mut child = EsmCellIndex::default();
    child.cells.insert(
        "cellb".into(),
        with_id(0xB1, "CellB", vec![moved]),
    );

    master.merge_from(child);

    let ids = |key: &str| -> Vec<u32> {
        master.cells[key].references.iter().map(|r| r.form_id).collect()
    };
    assert_eq!(ids("cella"), vec![0x12], "the stale base copy is gone from A");
    assert_eq!(ids("cellb"), vec![0x13, 0x11], "B keeps its own and gains the moved REFR");
    assert_eq!(
        master.cells["cellb"]
            .references
            .iter()
            .find(|r| r.form_id == 0x11)
            .map(|r| r.position),
        Some([9.0, 9.0, 9.0]),
        "the surviving copy is the override's"
    );
}

/// The move sweep must not touch a REFR the child re-emits in the cell that
/// already held it (a plain override), and it reaches exterior and
/// persistent cells the same way the tombstone sweep does.
#[test]
fn a_same_cell_override_survives_and_moves_reach_exterior_and_persistent_cells() {
    let mut master = EsmCellIndex::default();
    let mut ext = cell_with_refs("Tamriel", vec![placed(0x21, 0xAA), placed(0x22, 0xAA)]);
    ext.form_id = 0xE1;
    ext.is_interior = false;
    master
        .exterior_cells
        .entry("tamriel".into())
        .or_default()
        .insert((1, 1), ext);
    let mut persistent = cell_with_refs("TamrielPersistent", vec![placed(0x23, 0xAA)]);
    persistent.form_id = 0xE2;
    master
        .worldspace_persistent_cells
        .insert("tamriel".into(), persistent);

    // Child: re-emits 0x21 in the exterior cell (plain override), moves 0x22
    // into the persistent cell, and moves 0x23 out into a new interior.
    let mut child = EsmCellIndex::default();
    let mut ext_over = cell_with_refs("Tamriel", vec![placed(0x21, 0xCC)]);
    ext_over.form_id = 0xE1;
    ext_over.is_interior = false;
    child
        .exterior_cells
        .entry("tamriel".into())
        .or_default()
        .insert((1, 1), ext_over);
    let mut persistent_over = cell_with_refs("TamrielPersistent", vec![placed(0x22, 0xAA)]);
    persistent_over.form_id = 0xE2;
    child
        .worldspace_persistent_cells
        .insert("tamriel".into(), persistent_over);
    let mut interior = make_interior_cell(0xD1, "NewHome");
    interior.references = vec![placed(0x23, 0xAA)];
    child.cells.insert("newhome".into(), interior);

    master.merge_from(child);

    let ids = |cell: &CellData| -> Vec<u32> { cell.references.iter().map(|r| r.form_id).collect() };
    assert_eq!(ids(&master.exterior_cells["tamriel"][&(1, 1)]), vec![0x21]);
    assert_eq!(
        master.exterior_cells["tamriel"][&(1, 1)].references[0].base_form_id,
        0xCC,
        "the same-cell override still wins over its base copy"
    );
    assert_eq!(ids(&master.worldspace_persistent_cells["tamriel"]), vec![0x22]);
    assert_eq!(ids(&master.cells["newhome"]), vec![0x23]);
}

/// #5489 — a form-ID reference to a CELL (a PKIN's `CNAM` template cell)
/// resolves in O(1) wherever the CELL is filed, and `merge_from` invalidates
/// the lookup so a later plugin's cells are found too.
#[test]
fn cell_by_form_id_finds_interior_exterior_and_persistent_cells() {
    let mut index = EsmCellIndex::default();
    index
        .cells
        .insert("interior_a".into(), make_interior_cell(0x0A, "InteriorA"));
    index
        .exterior_cells
        .entry("tamriel".into())
        .or_default()
        .insert((3, -4), make_interior_cell(0x0B, "Tile"));
    index
        .worldspace_persistent_cells
        .insert("tamriel".into(), make_interior_cell(0x0C, "Persistent"));

    assert_eq!(index.cell_by_form_id(0x0A).unwrap().editor_id, "InteriorA");
    assert_eq!(index.cell_by_form_id(0x0B).unwrap().editor_id, "Tile");
    assert_eq!(index.cell_by_form_id(0x0C).unwrap().editor_id, "Persistent");
    assert!(index.cell_by_form_id(0x0D).is_none());

    // The lookup was built above; a merge must rebuild it.
    let mut later = EsmCellIndex::default();
    later
        .cells
        .insert("interior_b".into(), make_interior_cell(0x0D, "InteriorB"));
    index.merge_from(later);
    assert_eq!(index.cell_by_form_id(0x0D).unwrap().editor_id, "InteriorB");
    assert_eq!(index.cell_by_form_id(0x0A).unwrap().editor_id, "InteriorA");
}

fn land_marker(height: f32) -> LandscapeData {
    LandscapeData {
        heights: vec![height],
        normals: None,
        vertex_colors: None,
        quadrants: std::array::from_fn(|_| TerrainQuadrant::default()),
    }
}

fn exterior_cell(form_id: u32, grid: (i32, i32), land: Option<f32>) -> CellData {
    let mut cell = make_interior_cell(form_id, "Tile");
    cell.is_interior = false;
    cell.grid = Some(grid);
    cell.landscape = land.map(land_marker);
    cell
}

fn world(form_id: u32, parent: Option<(u32, u16)>) -> WorldspaceRecord {
    WorldspaceRecord {
        form_id,
        parent_worldspace: parent.map(|(f, _)| f),
        parent_flags: parent.map_or(0, |(_, flags)| flags),
        ..Default::default()
    }
}

fn heights(index: &EsmCellIndex, world: &str, grid: (i32, i32)) -> Option<f32> {
    index.exterior_cells[world][&grid].landscape.as_ref().map(|l| l.heights[0])
}

/// #5505 — a LAND-less cell of a land-inheriting child takes the parent's
/// LAND at the same grid coordinate; a cell's own LAND wins; a coordinate the
/// parent has no LAND at stays empty; a child that does not set the bit is
/// left alone.
#[test]
fn inherit_parent_landscapes_fills_only_land_less_cells_of_inheriting_children() {
    let mut index = EsmCellIndex::default();
    index.worldspaces.insert("tamriel".into(), world(0x3C, None));
    index.worldspaces.insert("bruma".into(), world(0x100, Some((0x3C, 0x01 | 0x02))));
    index.worldspaces.insert("loner".into(), world(0x200, Some((0x3C, 0x02))));
    index.exterior_cells.insert(
        "tamriel".into(),
        HashMap::from([
            ((4, 34), exterior_cell(1, (4, 34), Some(1.0))),
            ((5, 34), exterior_cell(2, (5, 34), Some(2.0))),
        ]),
    );
    index.exterior_cells.insert(
        "bruma".into(),
        HashMap::from([
            ((4, 34), exterior_cell(10, (4, 34), None)),
            ((5, 34), exterior_cell(11, (5, 34), Some(99.0))),
            ((9, 40), exterior_cell(12, (9, 40), None)),
        ]),
    );
    index.exterior_cells.insert(
        "loner".into(),
        HashMap::from([((4, 34), exterior_cell(20, (4, 34), None))]),
    );

    assert_eq!(index.inherit_parent_landscapes(), 1);
    assert_eq!(heights(&index, "bruma", (4, 34)), Some(1.0), "filled from the parent");
    assert_eq!(heights(&index, "bruma", (5, 34)), Some(99.0), "own LAND wins");
    assert_eq!(heights(&index, "bruma", (9, 40)), None, "the parent has no LAND there");
    assert_eq!(heights(&index, "loner", (4, 34)), None, "PNAM bit 0x01 not set");
}

/// #5505 — the chain is followed through intermediate worldspaces that carry
/// no LAND of their own at the coordinate, and a WNAM cycle terminates.
#[test]
fn inherit_parent_landscapes_follows_the_chain_and_survives_a_cycle() {
    let mut index = EsmCellIndex::default();
    index.worldspaces.insert("root".into(), world(0x1, None));
    index.worldspaces.insert("mid".into(), world(0x2, Some((0x1, 0x01))));
    index.worldspaces.insert("leaf".into(), world(0x3, Some((0x2, 0x01))));
    index.exterior_cells.insert("root".into(), HashMap::from([((0, 0), exterior_cell(1, (0, 0), Some(7.0)))]));
    index.exterior_cells.insert("mid".into(), HashMap::from([((0, 0), exterior_cell(2, (0, 0), None))]));
    index.exterior_cells.insert("leaf".into(), HashMap::from([((0, 0), exterior_cell(3, (0, 0), None))]));
    assert_eq!(index.inherit_parent_landscapes(), 2);
    assert_eq!(heights(&index, "leaf", (0, 0)), Some(7.0));

    let mut cyclic = EsmCellIndex::default();
    cyclic.worldspaces.insert("a".into(), world(0xA, Some((0xB, 0x01))));
    cyclic.worldspaces.insert("b".into(), world(0xB, Some((0xA, 0x01))));
    cyclic.exterior_cells.insert("a".into(), HashMap::from([((0, 0), exterior_cell(1, (0, 0), None))]));
    cyclic.exterior_cells.insert("b".into(), HashMap::from([((0, 0), exterior_cell(2, (0, 0), None))]));
    assert_eq!(cyclic.inherit_parent_landscapes(), 0);
}

fn txst(diffuse: &str, normal: &str) -> TextureSet {
    TextureSet {
        diffuse: Some(diffuse.to_string()),
        normal: Some(normal.to_string()),
        ..Default::default()
    }
}

/// #6036 — Honest Hearts' `ScrubBarrenWastes02GrassMix` (LTEX 0x0100CD37)
/// names `FalloutNV.esm`'s TXST 0x000357BD. The per-plugin join could not see
/// the master's TXST, and the merge only extended the already-resolved maps,
/// so the LTEX never resolved and Zion lost ~550 ATXT layers.
#[test]
fn dlc_ltex_resolves_against_a_master_txst() {
    const LTEX: u32 = 0x0100_CD37;
    const TXST: u32 = 0x0003_57BD;
    let mut master = EsmCellIndex::default();
    master
        .texture_sets
        .insert(TXST, txst("landscape\\scrub.dds", "landscape\\scrub_n.dds"));
    let mut dlc = EsmCellIndex::default();
    dlc.landscape_texture_txsts.insert(LTEX, TXST);
    dlc.resolve_landscape_texture_sets();
    assert!(
        !dlc.landscape_textures.contains_key(&LTEX),
        "the DLC alone cannot resolve a master TXST"
    );

    let mut merged = EsmCellIndex::default();
    merged.merge_from(master);
    merged.merge_from(dlc);
    assert_eq!(
        merged.landscape_textures.get(&LTEX).map(String::as_str),
        Some("landscape\\scrub.dds")
    );
    assert_eq!(
        merged.landscape_texture_sets[&LTEX].normal.as_deref(),
        Some("landscape\\scrub_n.dds")
    );
}

/// #6036 — an override that re-points a master LTEX's `TNAM` at another
/// master TXST replaces the master's resolution rather than leaving the stale
/// path in place, and a later TXST override re-resolves the LTEX too.
#[test]
fn ltex_override_and_txst_override_re_resolve_after_merge() {
    const LTEX: u32 = 0x0000_1000;
    let mut master = EsmCellIndex::default();
    master.texture_sets.insert(0xA, txst("old.dds", "old_n.dds"));
    master.texture_sets.insert(0xB, txst("new.dds", "new_n.dds"));
    master.landscape_texture_txsts.insert(LTEX, 0xA);
    master.resolve_landscape_texture_sets();
    assert_eq!(master.landscape_textures[&LTEX], "old.dds");

    let mut repoint = EsmCellIndex::default();
    repoint.landscape_texture_txsts.insert(LTEX, 0xB);
    let mut merged = EsmCellIndex::default();
    merged.merge_from(master);
    merged.merge_from(repoint);
    assert_eq!(merged.landscape_textures[&LTEX], "new.dds");
    assert_eq!(merged.landscape_texture_sets[&LTEX].normal.as_deref(), Some("new_n.dds"));

    let mut txst_override = EsmCellIndex::default();
    txst_override.texture_sets.insert(0xB, txst("newer.dds", "newer_n.dds"));
    merged.merge_from(txst_override);
    assert_eq!(merged.landscape_textures[&LTEX], "newer.dds");
}

/// An LTEX a later plugin re-authors with a direct ICON path (and no TNAM)
/// keeps that path; the earlier plugin's TNAM must not re-resolve over it.
#[test]
fn icon_override_is_not_re_resolved_through_an_earlier_tnam() {
    const LTEX: u32 = 0x0000_2000;
    let mut master = EsmCellIndex::default();
    master.texture_sets.insert(0xA, txst("txst.dds", "txst_n.dds"));
    master.landscape_texture_txsts.insert(LTEX, 0xA);
    master.resolve_landscape_texture_sets();

    let mut icon = EsmCellIndex::default();
    icon.landscape_textures.insert(LTEX, "textures\\landscape\\icon.dds".into());
    let mut merged = EsmCellIndex::default();
    merged.merge_from(master);
    merged.merge_from(icon);
    assert_eq!(merged.landscape_textures[&LTEX], "textures\\landscape\\icon.dds");
    assert!(!merged.landscape_texture_txsts.contains_key(&LTEX));
    assert!(!merged.landscape_texture_sets.contains_key(&LTEX));
}
