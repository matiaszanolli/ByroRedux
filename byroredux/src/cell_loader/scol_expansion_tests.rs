//! Tests for `scol_expansion_tests` extracted from ../cell_loader.rs (refactor stage A).
//!
//! Same qualified path preserved (`scol_expansion_tests::FOO`).

//! Regression tests for #585 — SCOL placement expansion.
//! `expand_scol_placements` is the consumer-side followup to
//! closed #405: when an SCOL REFR's base form has no cached
//! `CM*.NIF` (mod-added SCOL, or a previsibine-bypass loadout
//! drops the combined file), the cell loader synthesises one
//! REFR per child placement with the composed transform.
use super::*;
use byroredux_plugin::esm::cell::{EsmCellIndex, StaticObject};
use byroredux_plugin::esm::records::{ScolPart, ScolPlacement, ScolRecord};

fn mk_stat(form_id: u32, editor_id: &str, model_path: &str) -> StaticObject {
    StaticObject {
        form_id,
        editor_id: editor_id.to_string(),
        model_path: model_path.to_string(),
        record_type: byroredux_plugin::record::RecordType::STAT,
        light_data: None,
        addon_data: None,
        has_script: false,
        visible_when_distant: false,
    }
}

/// Baseline: a non-SCOL base form ID falls through to the single-
/// entry hot path unchanged. The outer transform rides through as
/// the synthetic ref's transform.
#[test]
fn expand_non_scol_returns_single_entry_with_outer_transform() {
    let index = EsmCellIndex::default();
    let outer_pos = Vec3::new(100.0, 50.0, -25.0);
    let outer_rot = Quat::IDENTITY;
    let outer_scale = 2.0;

    let synths = expand_scol_placements(0x0010_ABCD, outer_pos, outer_rot, outer_scale, &index);
    assert_eq!(synths.len(), 1);
    assert_eq!(synths[0].0, 0x0010_ABCD);
    assert_eq!(synths[0].1, outer_pos);
    assert_eq!(synths[0].2, outer_rot);
    assert_eq!(synths[0].3, outer_scale);
}

/// SCOL base form with a cached `CM*.NIF` (non-empty
/// `statics[base].model_path`) does NOT expand — the vanilla
/// 2616/2617 path. The single-entry vec preserves the outer
/// transform so the existing cell_loader branch handles it.
#[test]
fn expand_scol_with_cached_cm_does_not_expand() {
    let mut index = EsmCellIndex::default();
    let scol_id = 0x0024_9DF2;
    index.statics.insert(
        scol_id,
        mk_stat(scol_id, "TestScol", r"SCOL\Fallout4.esm\CM00249DF2.NIF"),
    );
    index.scols.insert(
        scol_id,
        ScolRecord {
            form_id: scol_id,
            editor_id: "TestScol".to_string(),
            model_path: r"SCOL\Fallout4.esm\CM00249DF2.NIF".to_string(),
            parts: vec![ScolPart {
                base_form_id: 0x0010_0001,
                placements: vec![ScolPlacement {
                    pos: [10.0, 0.0, 0.0],
                    rot: [0.0, 0.0, 0.0],
                    scale: 1.0,
                }],
            }],
            filter: Vec::new(),
            full_name: String::new(),
            has_script: false,
        },
    );
    let synths = expand_scol_placements(
        scol_id,
        Vec3::new(500.0, 100.0, 0.0),
        Quat::IDENTITY,
        1.0,
        &index,
    );
    // CM*.NIF is present → hot path: single entry, outer form ID.
    assert_eq!(synths.len(), 1);
    assert_eq!(synths[0].0, scol_id);
}

/// Mod-added SCOL: `statics[base].model_path` is empty (no MODL
/// shipped) but `scols[base]` carries the ONAM/DATA children. The
/// expander fans the REFR out into one synthetic child per
/// placement with composed transforms.
#[test]
fn expand_scol_without_cached_cm_fans_out_every_placement() {
    let mut index = EsmCellIndex::default();
    let scol_id = 0x0030_0001;
    // Statics entry exists (EDID-only, no MODL) — still counts as
    // "has no valid cached model" for expansion purposes.
    index
        .statics
        .insert(scol_id, mk_stat(scol_id, "ModScol", ""));
    // Two ONAM children, two placements each.
    index.scols.insert(
        scol_id,
        ScolRecord {
            form_id: scol_id,
            editor_id: "ModScol".to_string(),
            model_path: String::new(),
            parts: vec![
                ScolPart {
                    base_form_id: 0x0010_0001,
                    placements: vec![
                        ScolPlacement {
                            pos: [100.0, 0.0, 0.0],
                            rot: [0.0, 0.0, 0.0],
                            scale: 1.0,
                        },
                        ScolPlacement {
                            pos: [0.0, 100.0, 0.0],
                            rot: [0.0, 0.0, 0.0],
                            scale: 2.0,
                        },
                    ],
                },
                ScolPart {
                    base_form_id: 0x0010_0002,
                    placements: vec![ScolPlacement {
                        pos: [0.0, 0.0, 50.0],
                        rot: [0.0, 0.0, 0.0],
                        scale: 1.0,
                    }],
                },
            ],
            filter: Vec::new(),
            full_name: String::new(),
            has_script: false,
        },
    );

    let outer_pos = Vec3::new(1000.0, 2000.0, 3000.0);
    let outer_rot = Quat::IDENTITY;
    let outer_scale = 1.0;
    let synths = expand_scol_placements(scol_id, outer_pos, outer_rot, outer_scale, &index);

    assert_eq!(synths.len(), 3, "2 + 1 placements fan out");
    // First child, first placement: local Y-up pos from [100,0,0]
    // Z-up is [100, 0, -0] = [100, 0, 0], composed with outer.
    assert_eq!(synths[0].0, 0x0010_0001);
    assert_eq!(synths[0].1, Vec3::new(1100.0, 2000.0, 3000.0));
    assert_eq!(synths[0].3, 1.0);
    // First child, second placement: Z-up [0,100,0] → Y-up [0,0,-100].
    assert_eq!(synths[1].0, 0x0010_0001);
    assert_eq!(synths[1].1, Vec3::new(1000.0, 2000.0, 2900.0));
    assert_eq!(synths[1].3, 2.0);
    // Second child: Z-up [0,0,50] → Y-up [0,50,0].
    assert_eq!(synths[2].0, 0x0010_0002);
    assert_eq!(synths[2].1, Vec3::new(1000.0, 2050.0, 3000.0));
}

/// Mod-added SCOL not present in `statics` at all (neither EDID
/// nor MODL survived parse). `scols` has the full record; expand
/// still fans out. Guards against the expander assuming a
/// `statics` entry exists.
#[test]
fn expand_scol_missing_from_statics_still_expands_via_scols_map() {
    let mut index = EsmCellIndex::default();
    let scol_id = 0x0040_0001;
    index.scols.insert(
        scol_id,
        ScolRecord {
            form_id: scol_id,
            editor_id: String::new(),
            model_path: String::new(),
            parts: vec![ScolPart {
                base_form_id: 0x0010_0001,
                placements: vec![ScolPlacement {
                    pos: [0.0, 0.0, 0.0],
                    rot: [0.0, 0.0, 0.0],
                    scale: 1.0,
                }],
            }],
            filter: Vec::new(),
            full_name: String::new(),
            has_script: false,
        },
    );
    let synths = expand_scol_placements(scol_id, Vec3::ZERO, Quat::IDENTITY, 1.0, &index);
    assert_eq!(synths.len(), 1);
    assert_eq!(synths[0].0, 0x0010_0001);
}

// ── #1182 — SCOL-of-SCOL recursion ─────────────────────────────────
//
// Pre-#1182 `expand_scol_placements` was single-level: a SCOL whose
// `parts[i].base_form_id` referenced another SCOL emitted the inner
// SCOL's base form ID as an opaque placement, silently dropping the
// inner SCOL's child tree.

#[test]
fn expand_scol_recurses_into_nested_scol() {
    let mut index = EsmCellIndex::default();
    // Outer SCOL — no cached CM, must expand.
    let outer_id = 0x0080_0001;
    // Inner SCOL — same, child of outer.
    let inner_id = 0x0080_0002;
    // Leaf STAT — terminal child of inner.
    let leaf_id = 0x0010_0001;

    index.scols.insert(
        outer_id,
        ScolRecord {
            form_id: outer_id,
            editor_id: "Outer".to_string(),
            model_path: String::new(),
            parts: vec![ScolPart {
                base_form_id: inner_id,
                placements: vec![ScolPlacement {
                    pos: [100.0, 0.0, 0.0],
                    rot: [0.0, 0.0, 0.0],
                    scale: 1.0,
                }],
            }],
            filter: Vec::new(),
            full_name: String::new(),
            has_script: false,
        },
    );
    index.scols.insert(
        inner_id,
        ScolRecord {
            form_id: inner_id,
            editor_id: "Inner".to_string(),
            model_path: String::new(),
            parts: vec![ScolPart {
                base_form_id: leaf_id,
                placements: vec![ScolPlacement {
                    pos: [10.0, 0.0, 0.0],
                    rot: [0.0, 0.0, 0.0],
                    scale: 1.0,
                }],
            }],
            filter: Vec::new(),
            full_name: String::new(),
            has_script: false,
        },
    );

    let synths = expand_scol_placements(outer_id, Vec3::ZERO, Quat::IDENTITY, 1.0, &index);
    assert_eq!(synths.len(), 1, "inner SCOL's leaf must fan out");
    assert_eq!(synths[0].0, leaf_id, "leaf form ID survives the chain");
    // Outer placement Z-up [100,0,0] → Y-up [100,0,0]; inner placement
    // Z-up [10,0,0] → Y-up [10,0,0]. Composed: outer_pos + outer_rot ×
    // (outer_scale × inner_pos) = (0+100, 0, 0) + identity × (1 × (10,0,0))
    // applied through inner-rel-to-outer: final = (100+10, 0, 0).
    assert_eq!(synths[0].1, Vec3::new(110.0, 0.0, 0.0));
}

#[test]
fn expand_scol_recursion_bounded_by_depth_cap() {
    // Cycle: A → B → A. The depth cap (MAX_PKIN_DEPTH = 4) must stop
    // recursion in a finite number of steps and fall through to the
    // leaf-path single-entry emission for the cycle terminal.
    let mut index = EsmCellIndex::default();
    let a = 0x0090_0001;
    let b = 0x0090_0002;
    index.scols.insert(
        a,
        ScolRecord {
            form_id: a,
            editor_id: "A".to_string(),
            model_path: String::new(),
            parts: vec![ScolPart {
                base_form_id: b,
                placements: vec![ScolPlacement {
                    pos: [0.0, 0.0, 0.0],
                    rot: [0.0, 0.0, 0.0],
                    scale: 1.0,
                }],
            }],
            filter: Vec::new(),
            full_name: String::new(),
            has_script: false,
        },
    );
    index.scols.insert(
        b,
        ScolRecord {
            form_id: b,
            editor_id: "B".to_string(),
            model_path: String::new(),
            parts: vec![ScolPart {
                base_form_id: a,
                placements: vec![ScolPlacement {
                    pos: [0.0, 0.0, 0.0],
                    rot: [0.0, 0.0, 0.0],
                    scale: 1.0,
                }],
            }],
            filter: Vec::new(),
            full_name: String::new(),
            has_script: false,
        },
    );

    // Must terminate; the leaf at the depth cap emits a single
    // synthetic placement rather than recursing forever.
    let synths = expand_scol_placements(a, Vec3::ZERO, Quat::IDENTITY, 1.0, &index);
    assert!(
        !synths.is_empty(),
        "depth-capped recursion must still emit a synthetic placement"
    );
    // The synthetic placements are bounded by MAX_PKIN_DEPTH × 1
    // placement per level; at depth 4 we get 1 synthesised leaf.
    assert!(
        synths.len() <= 4,
        "recursion bounded by MAX_PKIN_DEPTH, got {}",
        synths.len(),
    );
}

/// Outer REFR's scale propagates into both the translation
/// composition and the synthetic scale (synth = outer × local).
#[test]
fn expand_scol_propagates_outer_scale_into_translation_and_scale() {
    let mut index = EsmCellIndex::default();
    let scol_id = 0x0050_0001;
    index.statics.insert(scol_id, mk_stat(scol_id, "S", ""));
    index.scols.insert(
        scol_id,
        ScolRecord {
            form_id: scol_id,
            editor_id: "S".to_string(),
            model_path: String::new(),
            parts: vec![ScolPart {
                base_form_id: 0x0010_0001,
                placements: vec![ScolPlacement {
                    pos: [100.0, 0.0, 0.0],
                    rot: [0.0, 0.0, 0.0],
                    scale: 3.0,
                }],
            }],
            filter: Vec::new(),
            full_name: String::new(),
            has_script: false,
        },
    );
    let outer_scale = 2.0;
    let synths = expand_scol_placements(scol_id, Vec3::ZERO, Quat::IDENTITY, outer_scale, &index);
    assert_eq!(synths.len(), 1);
    // local_pos.x = 100, composed x = outer_scale * 100 = 200.
    assert_eq!(synths[0].1, Vec3::new(200.0, 0.0, 0.0));
    // scale = outer_scale * local_scale = 2 × 3 = 6.
    assert_eq!(synths[0].3, 6.0);
}

/// #1600 — pin parent∘child composition with a NON-identity outer rotation.
/// Every prior test used `Quat::IDENTITY`, which degenerates both
/// `outer_rot * (scale * local_pos)` and `outer_rot * local_rot`, so an
/// order/composition regression stayed green. A 90° yaw about +Y rotates the
/// child position and must propagate to the composed rotation.
#[test]
fn expand_scol_composes_non_identity_outer_rotation() {
    use std::f32::consts::FRAC_PI_2;
    let mut index = EsmCellIndex::default();
    let scol_id = 0x0060_0001;
    index
        .statics
        .insert(scol_id, mk_stat(scol_id, "RotScol", ""));
    index.scols.insert(
        scol_id,
        ScolRecord {
            form_id: scol_id,
            editor_id: "RotScol".to_string(),
            model_path: String::new(),
            parts: vec![ScolPart {
                base_form_id: 0x0010_0001,
                // Z-up [10,0,0] → Y-up [10,0,0]; local rot identity.
                placements: vec![ScolPlacement {
                    pos: [10.0, 0.0, 0.0],
                    rot: [0.0, 0.0, 0.0],
                    scale: 1.0,
                }],
            }],
            filter: Vec::new(),
            full_name: String::new(),
            has_script: false,
        },
    );

    let outer_rot = Quat::from_rotation_y(FRAC_PI_2);
    let outer_pos = Vec3::new(5.0, 0.0, 0.0);
    let synths = expand_scol_placements(scol_id, outer_pos, outer_rot, 1.0, &index);
    assert_eq!(synths.len(), 1);

    // final_pos = outer_rot * (1 * local) + outer_pos.
    let local = Vec3::new(10.0, 0.0, 0.0);
    let expected_pos = outer_rot * local + outer_pos;
    let got_pos = synths[0].1;
    assert!(
        (got_pos - expected_pos).length() < 1e-4,
        "non-identity outer_rot must rotate the child position: got {got_pos:?}, want {expected_pos:?}"
    );
    // A swapped/unrotated order would land at outer_pos + local = (15,0,0).
    assert!(
        (got_pos - Vec3::new(15.0, 0.0, 0.0)).length() > 1e-2,
        "position must reflect the rotation, not the unrotated sum"
    );

    // final_rot = outer_rot * local_rot; local_rot is identity → must equal
    // outer_rot (and not collapse to identity).
    let got_rot = synths[0].2;
    assert!(
        got_rot.dot(outer_rot).abs() > 1.0 - 1e-4,
        "composed rotation must equal outer_rot when local_rot is identity"
    );
    assert!(
        got_rot.dot(Quat::IDENTITY).abs() < 1.0 - 1e-3,
        "composed rotation must not collapse to identity"
    );
}
