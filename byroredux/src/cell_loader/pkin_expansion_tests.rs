//! Tests for `pkin_expansion_tests` extracted from ../cell_loader.rs (refactor stage A).
//!
//! Same qualified path preserved (`pkin_expansion_tests::FOO`).

//! Regression tests for #589 (FO4-DIM4-03) — PKIN (Pack-In) REFR
//! expansion. Pre-fix the 872 vanilla Fallout4.esm PKIN records
//! were routed through the MODL-only parser and their CNAM lists
//! were silently dropped.
//!
//! #5231 corrected the model the fixtures below encode: every vanilla
//! CNAM is the pack-in's template CELL, and vanilla FO4 carries zero
//! PKIN-based REFRs (the CK bakes pack-ins into ordinary REFRs), so the
//! base-record fan-out shape is the MOD-authored case there. Starfield
//! does not bake: its PKIN REFRs instance the template cell (#5489) —
//! the CELL-typed CNAM behaviour is pinned at the bottom.
use super::*;
use byroredux_plugin::esm::cell::{CellData, EsmCellIndex, PlacedRef};
use super::refr_texture_overlay_tests::empty_placed_ref;
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
        filter: String::new(),
    }
}

/// A minimal CELL with the given form ID — the CNAM target shape of
/// every vanilla PKIN (#5231). Keyed under any canonical-id string: the
/// expander's CELL lookup reads `form_id` off the values, not the key.
fn template_cell(form_id: u32, edid: &str) -> (String, CellData) {
    (
        edid.to_ascii_lowercase(),
        CellData {
            form_id,
            editor_id: edid.to_string(),
            is_interior: true,
            ..Default::default()
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
            filter: String::new(),
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
            destruction: None,
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
            filter: String::new(),
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
/// placeable base: it must never become a synthetic placement that
/// silently fails every base lookup. A template with nothing placeable
/// (no references) leaves the contents list empty, so `None` sends the
/// outer REFR to the default single-entry path's miss accounting.
#[test]
fn expand_pkin_empty_template_cell_places_nothing() {
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
/// an empty template contributes nothing; the base-record children still
/// fan out at the outer transform.
#[test]
fn expand_pkin_empty_template_leaves_the_base_children_fanning_out() {
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

// ── #5489 — Starfield: a PKIN REFR instances its template cell ─────

/// One template-cell child: a REFR of `base` at Z-up `position`, Euler
/// `rotation` (radians) and `scale`.
fn template_child(base: u32, position: [f32; 3], rotation: [f32; 3], scale: f32) -> PlacedRef {
    let mut placed = empty_placed_ref(base);
    placed.position = position;
    placed.rotation = rotation;
    placed.scale = scale;
    placed
}

/// The reported bug: Starfield keeps the PKIN as the REFR's base and the game
/// instances the template cell's references under the REFR's transform. The
/// expander skipped every CELL-typed CNAM, so the REFR spawned nothing and
/// Cydonia lost 12,218 template placements. Each child must land at the outer
/// transform composed with its own, and the outer PKIN's own entry stays
/// FIRST so the REFR keeps its identity on the primary synthetic child.
#[test]
fn expand_pkin_instances_its_template_cell_under_the_outer_transform() {
    let mut index = EsmCellIndex::default();
    let pkin_id = 0x0070_0001;
    let template_id = 0x0070_00A0;
    let (stat_a, stat_b) = (0x0071_0001, 0x0071_0002);
    index
        .packins
        .insert(pkin_id, mk_pkin(pkin_id, "SCOL_StructKit_Truss03", vec![template_id]));
    let (key, mut cell) = template_cell(template_id, "SCOL_StructKit_Truss03_Template");
    cell.references = vec![
        template_child(stat_a, [10.0, 20.0, 30.0], [0.0; 3], 1.0),
        template_child(stat_b, [-4.0, 0.0, 8.0], [0.0; 3], 2.0),
    ];
    index.cells.insert(key, cell);

    let outer_pos = Vec3::new(100.0, 5.0, -50.0);
    let outer_rot = Quat::from_rotation_y(0.5);
    let outer_scale = 1.5;
    let synths = expand_pkin_placements(pkin_id, outer_pos, outer_rot, outer_scale, &index)
        .expect("a PKIN REFR with a populated template cell must expand");

    assert_eq!(synths.len(), 3, "the outer entry + the two template children");
    assert_eq!(
        synths[0],
        (pkin_id, outer_pos, outer_rot, outer_scale, Vec::new()),
        "the outer PKIN stays the primary synthetic child"
    );
    // Z-up (x, y, z) -> Y-up (x, z, -y), then composed under the outer
    // transform: outer_pos + outer_rot * (outer_scale * local).
    let compose = |local: Vec3, local_scale: f32| {
        (
            outer_pos + outer_rot * (outer_scale * local),
            outer_scale * local_scale,
        )
    };
    let (pos_a, scale_a) = compose(Vec3::new(10.0, 30.0, -20.0), 1.0);
    let (pos_b, scale_b) = compose(Vec3::new(-4.0, 8.0, 0.0), 2.0);
    assert_eq!(synths[1].0, stat_a);
    assert!(synths[1].1.abs_diff_eq(pos_a, 1e-4), "{:?} vs {pos_a:?}", synths[1].1);
    assert!(synths[1].2.abs_diff_eq(outer_rot, 1e-5), "no local rotation");
    assert!((synths[1].3 - scale_a).abs() < 1e-6);
    assert_eq!(synths[2].0, stat_b);
    assert!(synths[2].1.abs_diff_eq(pos_b, 1e-4), "{:?} vs {pos_b:?}", synths[2].1);
    assert!((synths[2].3 - scale_b).abs() < 1e-6);
}

/// The child's own rotation composes UNDER the outer rotation
/// (`outer_rot * local_rot`), through the same Euler convention every REFR uses.
#[test]
fn expand_pkin_template_child_rotation_composes_under_the_outer_rotation() {
    let mut index = EsmCellIndex::default();
    let (pkin_id, template_id, stat) = (0x0070_0002, 0x0070_00A1, 0x0071_0003);
    index
        .packins
        .insert(pkin_id, mk_pkin(pkin_id, "Rotated", vec![template_id]));
    let (key, mut cell) = template_cell(template_id, "RotatedTemplate");
    cell.references = vec![template_child(stat, [0.0; 3], [0.0, 0.0, 1.0], 1.0)];
    index.cells.insert(key, cell);

    let outer_rot = Quat::from_rotation_y(0.75);
    let synths = expand_pkin_placements(pkin_id, Vec3::ZERO, outer_rot, 1.0, &index).unwrap();
    let local = euler_zup_to_quat_yup_refr(0.0, 0.0, 1.0);
    assert!(synths[1].2.abs_diff_eq(outer_rot * local, 1e-5));
    assert!(
        !synths[1].2.abs_diff_eq(outer_rot, 1e-3),
        "fixture: the local rotation must be non-trivial"
    );
}

/// Template children that are initially disabled are not placed; an empty
/// outcome (only disabled children) falls back to the default path.
#[test]
fn expand_pkin_template_skips_initially_disabled_children() {
    // The CELL lookup is built on first use, so every fixture goes in first.
    let mut index = EsmCellIndex::default();
    let (pkin_id, template_id) = (0x0070_0003, 0x0070_00A2);
    let (live, hidden) = (0x0071_0004, 0x0071_0005);
    index
        .packins
        .insert(pkin_id, mk_pkin(pkin_id, "Gated", vec![template_id]));
    let mut disabled = template_child(hidden, [1.0, 1.0, 1.0], [0.0; 3], 1.0);
    disabled.initially_disabled = true;
    let (key, mut cell) = template_cell(template_id, "GatedTemplate");
    cell.references = vec![template_child(live, [0.0; 3], [0.0; 3], 1.0), disabled.clone()];
    index.cells.insert(key, cell);
    // A pack-in whose template holds only a disabled child.
    let only_hidden = 0x0070_0004;
    let template_b = 0x0070_00A3;
    index
        .packins
        .insert(only_hidden, mk_pkin(only_hidden, "AllHidden", vec![template_b]));
    let (key, mut cell) = template_cell(template_b, "AllHiddenTemplate");
    cell.references = vec![disabled];
    index.cells.insert(key, cell);

    let synths = expand_pkin_placements(pkin_id, Vec3::ZERO, Quat::IDENTITY, 1.0, &index).unwrap();
    let bases: Vec<u32> = synths.iter().map(|s| s.0).collect();
    assert_eq!(bases, vec![pkin_id, live]);
    assert!(
        expand_pkin_placements(only_hidden, Vec3::ZERO, Quat::IDENTITY, 1.0, &index).is_none(),
        "nothing placeable falls back to the default path"
    );
}

/// Starfield template cells nest pack-ins (40 of Cydonia's 12,218 children are
/// PKINs). A nested PKIN expands under the composed child transform and adds
/// no second "outer" entry of its own; a self-referential template terminates.
#[test]
fn expand_pkin_template_nests_pack_ins_and_survives_a_cycle() {
    let mut index = EsmCellIndex::default();
    let (outer, inner) = (0x0070_0005, 0x0070_0006);
    let (outer_cell, inner_cell) = (0x0070_00A4, 0x0070_00A5);
    let leaf = 0x0071_0006;
    index
        .packins
        .insert(outer, mk_pkin(outer, "Outer", vec![outer_cell]));
    index
        .packins
        .insert(inner, mk_pkin(inner, "Inner", vec![inner_cell]));
    let (key, mut cell) = template_cell(outer_cell, "OuterTemplate");
    cell.references = vec![template_child(inner, [10.0, 0.0, 0.0], [0.0; 3], 1.0)];
    index.cells.insert(key, cell);
    let (key, mut cell) = template_cell(inner_cell, "InnerTemplate");
    cell.references = vec![template_child(leaf, [0.0, 5.0, 0.0], [0.0; 3], 1.0)];
    index.cells.insert(key, cell);

    // The cycle fixture goes in before the first expansion (the CELL lookup is
    // built on first use).
    let cyc = 0x0070_0007;
    let cyc_cell = 0x0070_00A6;
    index.packins.insert(cyc, mk_pkin(cyc, "Cycle", vec![cyc_cell]));
    let (key, mut cell) = template_cell(cyc_cell, "CycleTemplate");
    cell.references = vec![template_child(cyc, [1.0, 0.0, 0.0], [0.0; 3], 1.0)];
    index.cells.insert(key, cell);

    let synths = expand_pkin_placements(outer, Vec3::ZERO, Quat::IDENTITY, 1.0, &index).unwrap();
    let bases: Vec<u32> = synths.iter().map(|s| s.0).collect();
    assert_eq!(bases, vec![outer, leaf], "outer self-entry, then the nested leaf");
    // outer child at (10, 0, 0) Y-up; leaf at Z-up (0,5,0) -> Y-up (0,0,-5).
    assert!(synths[1].1.abs_diff_eq(Vec3::new(10.0, 0.0, -5.0), 1e-4), "{:?}", synths[1].1);

    // A template whose only child is its own PKIN: bounded by the depth cap.
    let synths = expand_pkin_placements(cyc, Vec3::ZERO, Quat::IDENTITY, 1.0, &index)
        .expect("a self-referential template must still terminate with a result");
    assert!(synths.len() <= 2 * 4, "{}", synths.len());
}

/// #6260 — a template child's own `XMSP` / `XLMS` ride with its placement
/// (they used to be dropped with everything but base + transform). A nested
/// pack-in's leaves carry their own swaps first, then the nesting child's; the
/// outer REFR's identity entry carries none (its overlay is the outer one).
#[test]
fn expand_pkin_template_children_carry_their_own_material_swaps() {
    use super::refr::MaterialSwapRef::{Layered, Table};
    let mut index = EsmCellIndex::default();
    let (outer, inner) = (0x0070_0010, 0x0070_0011);
    let (outer_cell, inner_cell) = (0x0070_00B0, 0x0070_00B1);
    let (plain, swapped, leaf) = (0x0071_0010, 0x0071_0011, 0x0071_0012);
    index.packins.insert(outer, mk_pkin(outer, "Outer", vec![outer_cell]));
    index.packins.insert(inner, mk_pkin(inner, "Inner", vec![inner_cell]));
    let mut swapped_child = template_child(swapped, [0.0; 3], [0.0; 3], 1.0);
    swapped_child.material_swap_ref = Some(0x0024_0001);
    swapped_child.layered_material_swap_refs = vec![0x10, 0x11];
    let mut nested_child = template_child(inner, [10.0, 0.0, 0.0], [0.0; 3], 1.0);
    nested_child.layered_material_swap_refs = vec![0x20];
    let (key, mut cell) = template_cell(outer_cell, "OuterTemplate");
    cell.references = vec![
        template_child(plain, [0.0; 3], [0.0; 3], 1.0),
        swapped_child,
        nested_child,
    ];
    index.cells.insert(key, cell);
    let mut leaf_child = template_child(leaf, [0.0; 3], [0.0; 3], 1.0);
    leaf_child.layered_material_swap_refs = vec![0x30];
    let (key, mut cell) = template_cell(inner_cell, "InnerTemplate");
    cell.references = vec![leaf_child];
    index.cells.insert(key, cell);

    let synths = expand_pkin_placements(outer, Vec3::ZERO, Quat::IDENTITY, 1.0, &index).unwrap();
    let carried: Vec<(u32, Vec<_>)> = synths.iter().map(|s| (s.0, s.4.clone())).collect();
    assert_eq!(
        carried,
        vec![
            (outer, vec![]),
            (plain, vec![]),
            (swapped, vec![Table(0x0024_0001), Layered(0x10), Layered(0x11)]),
            (leaf, vec![Layered(0x30), Layered(0x20)]),
        ]
    );
}

/// #5489 real-data gate: on vanilla `Starfield.esm`, `citycydoniamainlevel`
/// places 370 pack-ins whose template cells hold 12,218 child REFRs (audit
/// `SF-2026-10-09-D4-01`, GRUP-walk census). Every one of those placements must
/// now expand to its template's children (nested pack-ins expanding further,
/// so the total can only exceed the one-level census).
/// Run with `-- --ignored`.
#[test]
#[ignore = "needs Starfield game data on disk"]
fn starfield_cydonia_pack_ins_instance_their_template_cells() {
    let data = std::env::var("BYROREDUX_STARFIELD_DATA")
        .unwrap_or_else(|_| "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data".into());
    let path = std::path::Path::new(&data).join("Starfield.esm");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("[Starfield PKIN] skipping: {path:?} unavailable");
        return;
    };
    let index = byroredux_plugin::esm::records::parse_esm(&bytes).expect("parse Starfield.esm");
    let cell = index
        .cells
        .cells
        .get("citycydoniamainlevel")
        .expect("the reference cell parses");
    let (mut pack_in_refs, mut expanded_children, mut unexpanded) = (0usize, 0usize, 0usize);
    let mut children_with_swaps = 0usize;
    for placed in &cell.references {
        if !index.cells.packins.contains_key(&placed.base_form_id) {
            continue;
        }
        pack_in_refs += 1;
        match expand_pkin_placements(
            placed.base_form_id,
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0,
            &index.cells,
        ) {
            // Minus the outer pack-in's own identity entry.
            Some(synths) => {
                expanded_children += synths.len() - 1;
                children_with_swaps += synths.iter().filter(|s| !s.4.is_empty()).count();
            }
            None => unexpanded += 1,
        }
    }
    eprintln!(
        "[Starfield PKIN] Cydonia: {pack_in_refs} pack-in REFRs, {expanded_children} instanced \
         template children ({children_with_swaps} carrying their own material swaps), \
         {unexpanded} left to the default path"
    );
    assert_eq!(pack_in_refs, 370, "audit census (SF-2026-10-09-D4-01)");
    assert!(
        expanded_children >= 12_218,
        "the template cells hold 12,218 children one level deep: {expanded_children}"
    );
    // #6260 — audit census (SF-2026-10-21-D4-02): 2,107 of the leaf children
    // author an XLMS. Nested levels pass theirs down, so the count can exceed it.
    assert!(
        children_with_swaps >= 2_107,
        "template children's own XLMS must ride with them: {children_with_swaps}"
    );
}
