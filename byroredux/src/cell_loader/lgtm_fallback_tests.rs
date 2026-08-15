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
        directional_rotation: [15.0f32.to_radians(), (-30.0f32).to_radians()],
        directional_fade: Some(0.5),
        fog_clip: Some(8192.0),
        fog_power: Some(1.0),
        fog_far_color: Some([0.12, 0.10, 0.08]),
        fog_max: Some(0.75),
        light_fade_begin: Some(1024.0),
        light_fade_end: Some(4096.0),
        directional_ambient: Some([
            [0.10, 0.11, 0.12],
            [0.20, 0.21, 0.22],
            [0.30, 0.31, 0.32],
            [0.40, 0.41, 0.42],
            [0.50, 0.51, 0.52],
            [0.60, 0.61, 0.62],
        ]),
        specular_color: Some([0.8, 0.7, 0.6]),
        specular_alpha: Some(0.5),
        fresnel_power: Some(2.0),
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
        water_height_is_explicit: false,
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
        precombined_mesh_hashes: Vec::new(),
        absorbed_refs: std::collections::HashSet::new(),
        navmeshes: Vec::new(),
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
        directional_azimuth: 0.0,
        directional_elevation: 0.0,
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
        inheritance_flags: Some(0),
        starfield: None,
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

/// WinterholdCollegeArchMageQuarters authors an almost-empty XCLL with
/// mask 0x079f and relies on FarmLightingTemplate for every selected
/// group. Rotation and directional fade are deliberately *not* inherited.
#[test]
fn xcll_inheritance_mask_merges_only_selected_lgtm_groups() {
    let mut cell = empty_cell(0x000C_AB92, "WinterholdCollegeArchMageQuarters");
    let local_rotation = [0.7, -0.4];
    let local_directional_fade = Some(9.0);
    cell.lighting = Some(CellLighting {
        ambient: [0.01; 3],
        directional_color: [0.02; 3],
        directional_azimuth: local_rotation[0],
        directional_elevation: local_rotation[1],
        fog_color: [0.03; 3],
        fog_near: 3.0,
        fog_far: 4.0,
        directional_fade: local_directional_fade,
        fog_clip: Some(5.0),
        fog_power: Some(6.0),
        fog_far_color: Some([0.07; 3]),
        fog_max: Some(0.08),
        light_fade_begin: Some(9.0),
        light_fade_end: Some(10.0),
        directional_ambient: Some([[0.11; 3]; 6]),
        specular_color: Some([0.12; 3]),
        specular_alpha: Some(0.13),
        fresnel_power: Some(0.14),
        inheritance_flags: Some(0x079f),
        starfield: None,
    });
    cell.lighting_template_form = Some(0x000A_1196);
    let template = template_with_amber_ambient(0x000A_1196);
    let mut index = empty_index();
    index
        .lighting_templates
        .insert(0x000A_1196, template.clone());

    let resolved = resolve_cell_lighting(&cell, &index).expect("resolved XCLL + LTMP");
    assert_eq!(resolved.ambient, template.ambient);
    assert_eq!(resolved.directional_ambient, template.directional_ambient);
    assert_eq!(resolved.specular_color, template.specular_color);
    assert_eq!(resolved.fresnel_power, template.fresnel_power);
    assert_eq!(resolved.directional_color, template.directional);
    assert_eq!(resolved.fog_color, template.fog_color);
    assert_eq!(resolved.fog_far_color, template.fog_far_color);
    assert_eq!(resolved.fog_near, template.fog_near);
    assert_eq!(resolved.fog_far, template.fog_far);
    assert_eq!(resolved.fog_clip, template.fog_clip);
    assert_eq!(resolved.fog_power, template.fog_power);
    assert_eq!(resolved.fog_max, template.fog_max);
    assert_eq!(resolved.light_fade_begin, template.light_fade_begin);
    assert_eq!(resolved.light_fade_end, template.light_fade_end);
    assert_eq!(resolved.directional_azimuth, local_rotation[0]);
    assert_eq!(resolved.directional_elevation, local_rotation[1]);
    assert_eq!(resolved.directional_fade, local_directional_fade);
}

#[test]
fn xcll_full_inheritance_includes_rotation_and_directional_fade() {
    let mut cell = empty_cell(0x0010_0005, "FullTemplateInheritance");
    cell.lighting = Some(CellLighting {
        ambient: [0.0; 3],
        directional_color: [0.0; 3],
        directional_azimuth: 0.0,
        directional_elevation: 0.0,
        fog_color: [0.0; 3],
        fog_near: 0.0,
        fog_far: 0.0,
        directional_fade: Some(0.0),
        fog_clip: Some(0.0),
        fog_power: Some(0.0),
        fog_far_color: None,
        fog_max: None,
        light_fade_begin: None,
        light_fade_end: None,
        directional_ambient: None,
        specular_color: None,
        specular_alpha: None,
        fresnel_power: None,
        inheritance_flags: Some(0x07ff),
        starfield: None,
    });
    cell.lighting_template_form = Some(0x0020_0001);
    let template = template_with_amber_ambient(0x0020_0001);
    let mut index = empty_index();
    index
        .lighting_templates
        .insert(0x0020_0001, template.clone());

    let resolved = resolve_cell_lighting(&cell, &index).expect("resolved full inheritance");
    assert_eq!(
        resolved.directional_azimuth,
        template.directional_rotation[0]
    );
    assert_eq!(
        resolved.directional_elevation,
        template.directional_rotation[1]
    );
    assert_eq!(resolved.directional_fade, template.directional_fade);
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
    assert_eq!(
        resolved.directional_azimuth,
        template.directional_rotation[0]
    );
    assert_eq!(
        resolved.directional_elevation,
        template.directional_rotation[1]
    );
    assert_eq!(resolved.directional_ambient, template.directional_ambient);
    assert_eq!(resolved.specular_color, template.specular_color);
    assert_eq!(resolved.fog_far_color, template.fog_far_color);
    assert_eq!(resolved.fog_max, template.fog_max);
    assert_eq!(resolved.light_fade_begin, template.light_fade_begin);
    assert_eq!(resolved.light_fade_end, template.light_fade_end);
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
