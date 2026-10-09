//! Tests for `pkin_expansion_tests` extracted from ../cell_loader.rs (refactor stage A).
//!
//! Same qualified path preserved (`pkin_expansion_tests::FOO`).

//! Regression tests for #589 (FO4-DIM4-03) — PKIN (Pack-In) REFR
//! expansion. Pre-fix the 872 vanilla Fallout4.esm PKIN records
//! were routed through the MODL-only parser and their CNAM lists
//! were silently dropped.
//!
//! #5231 corrected the model the fixtures below encode: every vanilla
//! CNAM is the pack-in's template CELL, and vanilla carries zero
//! PKIN-based REFRs (the CK bakes pack-ins into ordinary REFRs), so the
//! base-record fan-out shape is the MOD-authored case. The CELL-typed
//! CNAM behaviour is pinned separately at the bottom.
use super::*;
use byroredux_plugin::esm::cell::{CellData, EsmCellIndex};
use byroredux_plugin::esm::records::{PkinRecord, ScolPart, ScolPlacement, ScolRecord};

fn mk_pkin(form_id: u32, editor_id: &str, contents: Vec<u32>) -> PkinRecord {
    PkinRecord {
        form_id,
        editor_id: editor_id.to_string(),
        full_name: String::new(),
        contents,
        version: 0,
        flags: 0,
        // #815 — FLTR (workshop build-mode filter) defaults to
        // empty for the placement-expansion tests; the cell loader
        // doesn't consult `filter` today (pre-render), so the test
        // shape is unchanged.
        filter: Vec::new(),
    }
}

/// A minimal CELL with the given form ID — the CNAM target shape of
/// every vanilla PKIN (#5231). Keyed under any canonical-id string: the
/// expander's CELL scan reads `form_id` off the values, not the key.
fn template_cell(form_id: u32, edid: &str) -> (String, CellData) {
    (
        edid.to_ascii_lowercase(),
        CellData {
            form_id,
            editor_id: edid.to_string(),
            display_name: None,
            references: Vec::new(),
            is_interior: true,
            show_sky: None,
            grid: None,
            lighting: None,
            landscape: None,
            water_height: None,
            water_height_is_explicit: false,
            image_space_form: None,
            water_type_form: None,
            acoustic_space_form: None,
            music_type_form: None,
            music_type_enum: None,
            climate_override: None,
            location_form: None,
            encounter_zone_form: None,
            regions: Vec::new(),
            lighting_template_form: None,
            ownership: None,
            regional_color_override: None,
            precombined_mesh_hashes: Vec::new(),
            absorbed_refs: std::collections::HashSet::new(),
            navmeshes: Vec::new(),
            pathgrids: Vec::new(),
            deleted_refs: Vec::new(),
        },
    )
}

/// Baseline: a base form that isn't a PKIN returns `None` so the
/// caller falls through to the SCOL / default chain.
#[test]
fn expand_non_pkin_returns_none() {
    let index = EsmCellIndex::default();
    let result = expand_pkin_placements(
        0x0010_ABCD,
        Vec3::new(100.0, 50.0, -25.0),
        Quat::IDENTITY,
        2.0,
        &index,
    );
    assert!(result.is_none());
}

/// Vanilla shape: a PKIN with a single CNAM fans out to one
/// synthetic placement at the outer transform. This is how FO4
/// workbench-loot bundles render end-to-end.
#[test]
fn expand_pkin_single_cnam_fans_out_to_one_synth() {
    let mut index = EsmCellIndex::default();
    let pkin_id = 0x0055_0001;
    index.packins.insert(
        pkin_id,
        mk_pkin(pkin_id, "WorkbenchLoot", vec![0x0020_0001]),
    );

    let outer_pos = Vec3::new(500.0, 100.0, 250.0);
    let outer_rot = Quat::IDENTITY;
    let outer_scale = 1.5;
    let synths = expand_pkin_placements(pkin_id, outer_pos, outer_rot, outer_scale, &index)
        .expect("PKIN with a CNAM must fan out");
    assert_eq!(synths.len(), 1);
    assert_eq!(synths[0].0, 0x0020_0001);
    assert_eq!(synths[0].1, outer_pos);
    assert_eq!(synths[0].2, outer_rot);
    // Outer scale propagates verbatim — PKIN has no per-child scale.
    assert_eq!(synths[0].3, outer_scale);
}

/// Multi-CNAM bundle: each content ref becomes a synth. All synths
/// share the outer transform; authoring order is preserved so
/// downstream consumers iterate in the right sequence.
#[test]
fn expand_pkin_multiple_cnam_preserves_order_at_outer_transform() {
    let mut index = EsmCellIndex::default();
    let pkin_id = 0x0055_0002;
    index.packins.insert(
        pkin_id,
        mk_pkin(
            pkin_id,
            "MultiPack",
            vec![0x0020_0001, 0x0020_0002, 0x0020_0003],
        ),
    );

    let outer_pos = Vec3::new(10.0, 20.0, 30.0);
    let outer_rot = Quat::from_rotation_y(0.5);
    let outer_scale = 1.0;
    let synths =
        expand_pkin_placements(pkin_id, outer_pos, outer_rot, outer_scale, &index).unwrap();
    assert_eq!(synths.len(), 3);
    assert_eq!(synths[0].0, 0x0020_0001);
    assert_eq!(synths[1].0, 0x0020_0002);
    assert_eq!(synths[2].0, 0x0020_0003);
    // Every synth shares the outer transform exactly.
    for s in &synths {
        assert_eq!(s.1, outer_pos);
        assert_eq!(s.2, outer_rot);
        assert_eq!(s.3, outer_scale);
    }
}

/// A PKIN with an empty `contents` list (malformed / author-trimmed)
/// returns `None` so the caller falls through rather than spawning
/// zero synthetic children at the outer transform. Prevents the
/// outer REFR from being silently dropped — the single-entry
/// default path still runs its own stat lookup.
#[test]
fn expand_pkin_with_empty_contents_returns_none() {
    let mut index = EsmCellIndex::default();
    let pkin_id = 0x0055_0003;
    index
        .packins
        .insert(pkin_id, mk_pkin(pkin_id, "EmptyBundle", Vec::new()));
    let result = expand_pkin_placements(pkin_id, Vec3::ZERO, Quat::IDENTITY, 1.0, &index);
    assert!(result.is_none());
}

/// #635 / FNV-D3-06 — PKIN-of-PKIN nesting fans out one extra level.
/// Pre-fix the inner PKIN's content was silently dropped when the
/// outer caller looked up the inner PKIN's form ID in `index.statics`
/// and missed (PKINs don't live in the statics table). Verify the
/// outer expansion now flattens to the leaf STAT form IDs.
#[test]
fn expand_pkin_recurses_into_nested_pkin() {
    let mut index = EsmCellIndex::default();
    let outer_pkin_id = 0x0066_0001;
    let inner_pkin_id = 0x0066_0002;
    let leaf_a = 0x0021_0001;
    let leaf_b = 0x0021_0002;
    let leaf_c = 0x0021_0003;
    // Outer PKIN points at a leaf STAT plus a nested PKIN.
    index.packins.insert(
        outer_pkin_id,
        mk_pkin(outer_pkin_id, "OuterBundle", vec![leaf_a, inner_pkin_id]),
    );
    // Inner PKIN expands into two more leaf STATs.
    index.packins.insert(
        inner_pkin_id,
        mk_pkin(inner_pkin_id, "InnerBundle", vec![leaf_b, leaf_c]),
    );

    let outer_pos = Vec3::new(7.0, 8.0, 9.0);
    let outer_rot = Quat::from_rotation_z(0.25);
    let outer_scale = 1.25;
    let synths = expand_pkin_placements(outer_pkin_id, outer_pos, outer_rot, outer_scale, &index)
        .expect("PKIN-of-PKIN must still fan out");
    assert_eq!(synths.len(), 3, "leaf_a + (leaf_b + leaf_c) flattened");
    let leaf_ids: Vec<u32> = synths.iter().map(|s| s.0).collect();
    assert_eq!(leaf_ids, vec![leaf_a, leaf_b, leaf_c]);
    // All leaves inherit the outer REFR's transform — PKIN has no
    // per-child placement data at any nesting level.
    for s in &synths {
        assert_eq!(s.1, outer_pos);
        assert_eq!(s.2, outer_rot);
        assert_eq!(s.3, outer_scale);
    }
}

/// #635 / FNV-D3-06 — depth cap at MAX_PKIN_DEPTH (4) prevents
/// runaway recursion. Construct a self-referential PKIN (its own
/// CNAM points back at itself); the expander must terminate and at
/// most emit MAX_PKIN_DEPTH synth entries (one per level explored)
/// rather than looping until the stack overflows.
#[test]
fn expand_pkin_self_referential_terminates_at_depth_cap() {
    let mut index = EsmCellIndex::default();
    let cycle_pkin_id = 0x0066_0010;
    // PKIN whose only content is itself — pathological author error.
    index.packins.insert(
        cycle_pkin_id,
        mk_pkin(cycle_pkin_id, "Cycle", vec![cycle_pkin_id]),
    );

    let synths = expand_pkin_placements(cycle_pkin_id, Vec3::ZERO, Quat::IDENTITY, 1.0, &index)
        .expect("self-reference must still return Some(_) for the depth-cap leaf");
    // At the cap, the expander stops recursing and emits the last
    // form ID as a leaf. Verify recursion terminated cleanly with a
    // single trailing leaf entry (the caller's `stat_miss` accounting
    // handles the bogus form ID downstream).
    assert!(
        !synths.is_empty(),
        "depth-cap fallback must still produce a leaf so the bogus \
         form ID is logged via stat_miss rather than silently dropped"
    );
    assert!(
        synths.len() <= 8,
        "MAX_PKIN_DEPTH (4) caps blow-up; observed {} synths",
        synths.len()
    );
    // Every synth points at the cycle PKIN's own ID (the only content
    // entry at every level).
    for s in &synths {
        assert_eq!(s.0, cycle_pkin_id);
    }
}

/// #635 / FNV-D3-06 — children that are NOT PKINs pass through
/// unchanged. The recursive helper must not mistakenly probe
/// non-PKIN form IDs against `index.packins`.
#[test]
fn expand_pkin_non_pkin_children_pass_through_unchanged() {
    let mut index = EsmCellIndex::default();
    let pkin_id = 0x0066_0020;
    let stat_a = 0x0030_0001;
    let stat_b = 0x0030_0002;
    index.packins.insert(
        pkin_id,
        mk_pkin(pkin_id, "FlatBundle", vec![stat_a, stat_b]),
    );
    // Note: no nested PKINs registered.

    let synths = expand_pkin_placements(pkin_id, Vec3::ZERO, Quat::IDENTITY, 1.0, &index)
        .expect("flat PKIN still expands");
    assert_eq!(synths.len(), 2);
    assert_eq!(synths[0].0, stat_a);
    assert_eq!(synths[1].0, stat_b);
}

// ── #1180 — PKIN → SCOL recursion ──────────────────────────────────
//
// Pre-#1180 a PKIN whose `contents` list referenced a SCOL emitted
// the SCOL's base form ID as an opaque placement, silently dropping
// the SCOL's child tree. The PKIN expander now consults the SCOL
// expander for SCOL-typed children up to the shared depth cap.

#[test]
fn expand_pkin_recurses_into_scol_child() {
    let mut index = EsmCellIndex::default();
    let pkin_id = 0x0067_0001;
    let scol_id = 0x0067_0002;
    let leaf_id = 0x0010_0001;
    index
        .packins
        .insert(pkin_id, mk_pkin(pkin_id, "PkinWithScol", vec![scol_id]));
    // SCOL has no cached CM model_path → must expand.
    index.scols.insert(
        scol_id,
        ScolRecord {
            form_id: scol_id,
            editor_id: "ScolChild".to_string(),
            model_path: String::new(),
            parts: vec![ScolPart {
                base_form_id: leaf_id,
                placements: vec![ScolPlacement {
                    pos: [50.0, 0.0, 0.0],
                    rot: [0.0, 0.0, 0.0],
                    scale: 1.0,
                }],
            }],
            filter: Vec::new(),
            full_name: String::new(),
            has_script: false,
        },
    );

    let outer_pos = Vec3::new(1000.0, 0.0, 0.0);
    let synths = expand_pkin_placements(pkin_id, outer_pos, Quat::IDENTITY, 1.0, &index)
        .expect("PKIN with SCOL child must expand");
    assert_eq!(synths.len(), 1, "SCOL child's leaf placement must fan out");
    assert_eq!(synths[0].0, leaf_id);
    // Outer (1000,0,0) + SCOL placement Y-up (50,0,0) = (1050,0,0).
    assert_eq!(synths[0].1, Vec3::new(1050.0, 0.0, 0.0));
}

#[test]
fn expand_pkin_with_cached_scol_child_does_not_recurse() {
    // If the SCOL child has a cached CM model_path, the SCOL expander
    // returns the single base-form-id entry rather than fanning out.
    // The PKIN expander's non-empty check sees a 1-element vec and
    // takes it — same result as the pre-#1180 leaf path. Pin this
    // non-regression so the cached-CM hot path doesn't accidentally
    // start fanning out vanilla content.
    let mut index = EsmCellIndex::default();
    let pkin_id = 0x0068_0001;
    let scol_id = 0x0068_0002;
    let leaf_id = 0x0010_0001;
    index.packins.insert(
        pkin_id,
        mk_pkin(pkin_id, "PkinWithCachedScol", vec![scol_id]),
    );
    // SCOL with a cached CM*.NIF — vanilla 2616/2617 path. Expand
    // should bail with single-entry.
    index.statics.insert(
        scol_id,
        byroredux_plugin::esm::cell::StaticObject {
            form_id: scol_id,
            editor_id: "CachedScol".to_string(),
            model_path: r"SCOL\Fallout4.esm\CM00680002.NIF".to_string(),
            record_type: byroredux_plugin::record::RecordType::STAT,
            light_data: None,
            addon_data: None,
            has_script: false,
            script_instance: None,
            // #3941 — PKIN / SCOL are Skyrim+/FO4-era types; `SCRI` is the
            // Oblivion / FO3 / FNV ObScript attachment and never appears.
            script_form_id: 0,
            visible_when_distant: false,
        },
    );
    index.scols.insert(
        scol_id,
        ScolRecord {
            form_id: scol_id,
            editor_id: "CachedScol".to_string(),
            model_path: r"SCOL\Fallout4.esm\CM00680002.NIF".to_string(),
            parts: vec![ScolPart {
                base_form_id: leaf_id,
                placements: vec![ScolPlacement {
                    pos: [50.0, 0.0, 0.0],
                    rot: [0.0, 0.0, 0.0],
                    scale: 1.0,
                }],
            }],
            filter: Vec::new(),
            full_name: String::new(),
            has_script: false,
        },
    );

    let outer_pos = Vec3::new(1000.0, 0.0, 0.0);
    let synths = expand_pkin_placements(pkin_id, outer_pos, Quat::IDENTITY, 1.0, &index)
        .expect("PKIN with cached SCOL still expands");
    assert_eq!(synths.len(), 1);
    // Cached path emits SCOL's own form id at outer position — the
    // downstream cell_loader resolves model_path normally.
    assert_eq!(synths[0].0, scol_id);
    assert_eq!(synths[0].1, outer_pos);
}

// ── #5231 — CELL-typed CNAMs (every vanilla PKIN) ──────────────────

/// A CNAM that resolves to a CELL is the pack-in's template cell, not a
/// placeable base: it must be an explicit miss (warn + skip) rather than
/// a synthetic placement that silently fails every base lookup. An
/// all-CELL contents list — every vanilla PKIN — returns `None` so the
/// outer REFR reaches the default single-entry path's miss accounting.
#[test]
fn expand_pkin_cell_typed_cnam_is_an_explicit_miss() {
    let mut index = EsmCellIndex::default();
    let pkin_id = 0x0069_0001;
    let template_cell_id = 0x000A_0BCD;
    index.packins.insert(
        pkin_id,
        mk_pkin(pkin_id, "WorkshopPackIn", vec![template_cell_id]),
    );
    let (key, cell) = template_cell(template_cell_id, "WorkshopPackInTemplate");
    index.cells.insert(key, cell);

    let result = expand_pkin_placements(pkin_id, Vec3::ZERO, Quat::IDENTITY, 1.0, &index);
    assert!(
        result.is_none(),
        "an all-CELL contents list must return None — the outer REFR runs the \
         default path whose miss accounting names it (#5231)"
    );
}

/// Mixed contents (the mod-authored shape alongside a template CELL):
/// the CELL child is skipped with the explicit miss; the base-record
/// children still fan out at the outer transform.
#[test]
fn expand_pkin_skips_the_cell_child_but_fans_out_base_children() {
    let mut index = EsmCellIndex::default();
    let pkin_id = 0x0069_0002;
    let template_cell_id = 0x000A_0BCE;
    let stat_id = 0x0010_7777;
    index.packins.insert(
        pkin_id,
        mk_pkin(pkin_id, "MixedPackIn", vec![template_cell_id, stat_id]),
    );
    let (key, cell) = template_cell(template_cell_id, "MixedPackInTemplate");
    index.cells.insert(key, cell);

    let outer_pos = Vec3::new(12.0, 34.0, 56.0);
    let synths = expand_pkin_placements(pkin_id, outer_pos, Quat::IDENTITY, 1.0, &index)
        .expect("the base-record child must still fan out");
    assert_eq!(synths.len(), 1, "only the non-CELL child is emitted");
    assert_eq!(synths[0].0, stat_id);
    assert_eq!(synths[0].1, outer_pos);
}
