//! Regression tests for SK-D6-02 / #566 — LGTM lighting-template
//! fallback for cells that omit XCLL.
//!
//! Vanilla Skyrim ships interior cells (Solitude inn cluster,
//! Dragonsreach throne room, Markarth cells) that author no XCLL and
//! rely entirely on a `LTMP` reference into a `LGTM` record. Pre-#566
//! the LTMP FormID was unparsed, so the fallback never fired and the
//! cells rendered with the engine default ambient. These tests pin the
//! resolution chain — explicit XCLL > LGTM template > engine default —
//! so the hierarchy stays intact through future cell-loader edits.

use super::*;
use byroredux_plugin::esm::cell::{CellData, CellLighting, EsmCellIndex};
use byroredux_plugin::esm::records::{EsmIndex, LgtmRecord};

fn template_with_amber_ambient(form_id: u32) -> LgtmRecord {
    LgtmRecord {
        form_id,
        editor_id: "DefaultLightingTemplateInteriorMarkarth".to_string(),
        ambient: [0.85, 0.65, 0.40],
        directional: [0.95, 0.80, 0.55],
        fog_color: [0.18, 0.14, 0.10],
        fog_near: 256.0,
        fog_far: 4096.0,
        directional_fade: Some(0.5),
        fog_clip: Some(8192.0),
        fog_power: Some(1.0),
    }
}

fn empty_cell(form_id: u32, edid: &str) -> CellData {
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
    }
}

fn empty_index() -> EsmIndex {
    EsmIndex {
        cells: EsmCellIndex::default(),
        ..EsmIndex::default()
    }
}

/// Explicit XCLL wins over any LGTM template — the cell-authored
/// values must never be overwritten by the fallback chain.
#[test]
fn explicit_xcll_takes_priority_over_lgtm_template() {
    let mut cell = empty_cell(0x0010_0001, "DragonsreachThroneRoom");
    let xcll_ambient = [0.05, 0.12, 0.22];
    cell.lighting = Some(CellLighting {
        ambient: xcll_ambient,
        directional_color: [0.30, 0.30, 0.40],
        directional_rotation: [0.0, 0.0],
        fog_color: [0.10, 0.12, 0.18],
        fog_near: 0.0,
        fog_far: 100.0,
        directional_fade: None,
        fog_clip: None,
        fog_power: None,
        fog_far_color: None,
        fog_max: None,
        light_fade_begin: None,
        light_fade_end: None,
        directional_ambient: None,
        specular_color: None,
        specular_alpha: None,
        fresnel_power: None,
    });
    cell.lighting_template_form = Some(0x0020_0001);

    let mut index = empty_index();
    index
        .lighting_templates
        .insert(0x0020_0001, template_with_amber_ambient(0x0020_0001));

    let resolved = resolve_cell_lighting(&cell, &index)
        .expect("XCLL is Some — fallback must surface it verbatim");
    assert_eq!(
        resolved.ambient, xcll_ambient,
        "Explicit XCLL ambient must override LGTM template"
    );
}

/// XCLL absent + LTMP present → LGTM fields project into the
/// synthesized `CellLighting`. This is the actual SK-D6-02 contract:
/// vanilla Markarth / Solitude / Dragonsreach cells must light up
/// instead of falling to the engine default.
#[test]
fn missing_xcll_with_lgtm_template_synthesizes_cell_lighting() {
    let mut cell = empty_cell(0x0010_0002, "MarkarthInteriorCellA");
    cell.lighting_template_form = Some(0x0020_0001);
    let template = template_with_amber_ambient(0x0020_0001);

    let mut index = empty_index();
    index
        .lighting_templates
        .insert(0x0020_0001, template.clone());

    let resolved = resolve_cell_lighting(&cell, &index)
        .expect("LTMP must produce a synthesized CellLighting when XCLL is absent");
    assert_eq!(resolved.ambient, template.ambient);
    assert_eq!(resolved.directional_color, template.directional);
    assert_eq!(resolved.fog_color, template.fog_color);
    assert_eq!(resolved.fog_near, template.fog_near);
    assert_eq!(resolved.fog_far, template.fog_far);
    assert_eq!(resolved.directional_fade, template.directional_fade);
    assert_eq!(resolved.fog_clip, template.fog_clip);
    assert_eq!(resolved.fog_power, template.fog_power);
    // LGTM doesn't carry directional rotation — the synth defaults to
    // sun-from-+X (matches FO3/FNV pre-rotation behavior).
    assert_eq!(resolved.directional_rotation, [0.0, 0.0]);
    // Skyrim-extended fields (ambient cube, specular, fog far color,
    // light fade) live on 92-byte XCLL only — LGTM has no equivalent
    // and the synthesized lighting leaves them None.
    assert!(resolved.directional_ambient.is_none());
    assert!(resolved.specular_color.is_none());
    assert!(resolved.fog_far_color.is_none());
}

/// XCLL absent + LTMP absent → returns `None` (engine default fallback).
/// The early-return path that ships pre-#566 cells should still hit
/// for cells that legitimately have no lighting authored.
#[test]
fn no_xcll_no_ltmp_returns_none_for_engine_default() {
    let cell = empty_cell(0x0010_0003, "DefaultEngineLitCell");
    let index = empty_index();
    assert!(resolve_cell_lighting(&cell, &index).is_none());
}

/// LTMP present but the referenced LGTM is missing from the index
/// (broken master, unloaded DLC) → also `None`. The fallback must not
/// panic or synthesize garbage; the engine-default path takes over.
#[test]
fn ltmp_pointing_at_missing_lgtm_returns_none() {
    let mut cell = empty_cell(0x0010_0004, "BrokenLtmpCell");
    cell.lighting_template_form = Some(0xDEAD_BEEF);
    let index = empty_index(); // empty `lighting_templates` map.
    assert!(resolve_cell_lighting(&cell, &index).is_none());
}
