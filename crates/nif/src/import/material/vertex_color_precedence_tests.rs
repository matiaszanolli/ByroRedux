//! Regression tests for #1208 — `NiVertexColorProperty` arriving on the
//! inherited NiNode property chain must NOT silently overwrite the
//! `BSLightingShaderProperty`-driven Skyrim+ default.
//!
//! Pre-fix the consumer at `walker.rs`'s NVCP arm unconditionally wrote
//! `info.vertex_color_mode`, so a Skyrim+ mesh that authors both
//! `BSLightingShaderProperty` (with the implicit `AmbientDiffuse` intent)
//! AND a legacy NVCP from the property chain landed on whatever the
//! NVCP encoded. Modded Skyrim content fielding both could land on the
//! wrong mode (Emissive payload routed through AmbientDiffuse, vice versa).
//!
//! The fix gates the NVCP write on `!info.has_material_data`, mirroring
//! the precedence pattern used by every other secondary-source consumer
//! in the inherited-property loop.

use super::*;
use crate::blocks::base::NiObjectNETData;
use crate::blocks::properties::NiVertexColorProperty;
use crate::blocks::shader::{BSLightingShaderProperty, ShaderTypeData};
use crate::blocks::NiObject;
use crate::types::BlockRef;
use byroredux_core::string::StringPool;
use std::sync::Arc;

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

fn named_net(name: &str) -> NiObjectNETData {
    NiObjectNETData {
        name: Some(Arc::from(name)),
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

/// Minimal `BSLightingShaderProperty` with a real name (so the Skyrim+
/// shader branch sets `has_material_data = true`).
fn lighting_shader() -> BSLightingShaderProperty {
    BSLightingShaderProperty {
        shader_type: 0,
        net: named_net("materials\\armor\\iron.bgsm"),
        material_reference: false,
        shader_flags_1: 0,
        shader_flags_2: 0,
        sf1_crcs: Vec::new(),
        sf2_crcs: Vec::new(),
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        texture_set_ref: BlockRef::NULL,
        emissive_color: [0.0; 3],
        emissive_multiple: 1.0,
        root_material_path: None,
        texture_clamp_mode: 3,
        alpha: 1.0,
        refraction_strength: 0.0,
        glossiness: 80.0,
        specular_color: [1.0; 3],
        specular_strength: 1.0,
        lighting_effect_1: 0.3,
        lighting_effect_2: 2.0,
        subsurface_rolloff: 0.3,
        rimlight_power: 2.0,
        backlight_power: 0.0,
        grayscale_to_palette_scale: 1.0,
        fresnel_power: 5.0,
        wetness: None,
        luminance: None,
        do_translucency: false,
        translucency: None,
        texture_arrays: Vec::new(),
        shader_type_data: ShaderTypeData::None,
    }
}

/// Build an NVCP with the (vertex_mode, lighting_mode) pair encoded
/// directly into the legacy fields (pre-FO3 path; the walker doesn't
/// re-decode `flags`).
fn vcol_property(vertex_mode: u32, lighting_mode: u32) -> NiVertexColorProperty {
    NiVertexColorProperty {
        net: empty_net(),
        flags: 0,
        vertex_mode,
        lighting_mode,
    }
}

/// The post-#1208 precedence: BSLightingShaderProperty +
/// inherited NiVertexColorProperty(SRC_IGNORE) → the NVCP arm must
/// be skipped, so `vertex_color_mode` stays at the Skyrim+ default
/// (`AmbientDiffuse`).
#[test]
fn bsl_inhibits_inherited_nvcp_ignore() {
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(lighting_shader()),
        Box::new(vcol_property(0, 1)), // SRC_IGNORE + LIGHTING_E_A_D
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut pool = StringPool::new();
    let info = walker::extract_material_info_from_refs(
        &scene,
        BlockRef(0),         // shader_property_ref → BSL
        BlockRef::NULL,      // no alpha
        &[],                 // direct_properties (empty for BsTriShape-like)
        &[BlockRef(1)],      // inherited_props carries NVCP
        &mut pool,
    );
    assert!(
        info.has_material_data,
        "BSL must have set has_material_data = true",
    );
    assert_eq!(
        info.vertex_color_mode,
        VertexColorMode::AmbientDiffuse,
        "BSL Skyrim+ default must win over inherited NVCP(SRC_IGNORE)",
    );
}

/// Same with the Emissive routing — BSL still wins.
#[test]
fn bsl_inhibits_inherited_nvcp_emissive() {
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(lighting_shader()),
        Box::new(vcol_property(1, 1)), // SRC_EMISSIVE + LIGHTING_E_A_D
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut pool = StringPool::new();
    let info = walker::extract_material_info_from_refs(
        &scene,
        BlockRef(0),
        BlockRef::NULL,
        &[],
        &[BlockRef(1)],
        &mut pool,
    );
    assert_eq!(
        info.vertex_color_mode,
        VertexColorMode::AmbientDiffuse,
        "BSL Skyrim+ default must win over inherited NVCP(SRC_EMISSIVE)",
    );
}

/// Non-regression: with NO BSL (legacy FO3/FNV/Oblivion path), the
/// inherited NVCP is still honored — the gate only blocks when the
/// shader path already populated material data.
#[test]
fn legacy_path_still_honors_inherited_nvcp() {
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(vcol_property(0, 1))];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut pool = StringPool::new();
    let info = walker::extract_material_info_from_refs(
        &scene,
        BlockRef::NULL,      // no shader property
        BlockRef::NULL,
        &[],
        &[BlockRef(0)],      // inherited NVCP
        &mut pool,
    );
    assert!(
        !info.has_material_data,
        "no shader property → has_material_data must stay false",
    );
    assert_eq!(
        info.vertex_color_mode,
        VertexColorMode::Ignore,
        "inherited NVCP SRC_IGNORE must reach info when no BSL is bound",
    );
}

/// Non-regression: legacy + Emissive routing also still honored.
#[test]
fn legacy_path_still_honors_inherited_nvcp_emissive() {
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(vcol_property(1, 1))];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut pool = StringPool::new();
    let info = walker::extract_material_info_from_refs(
        &scene,
        BlockRef::NULL,
        BlockRef::NULL,
        &[],
        &[BlockRef(0)],
        &mut pool,
    );
    assert_eq!(info.vertex_color_mode, VertexColorMode::Emissive);
}
