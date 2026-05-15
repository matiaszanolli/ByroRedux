//! Regression tests for #976 (NIF-D4-NEW-02) — `BSLightingShaderProperty`
//! with a Starfield `.mat` JSON material reference must populate
//! `material_path` via the shared `material_path_from_name` helper.
//!
//! Pre-fix the walker used an inline suffix check that only accepted
//! `.bgsm` / `.bgem`, so any `.mat` reference arrived with
//! `material_path = None` and the BgsmProvider lookup chain never
//! received the path.

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::shader::{BSLightingShaderProperty, BSShaderTextureSet, ShaderTypeData};
use crate::blocks::tri_shape::NiTriShape;
use crate::blocks::NiObject;
use crate::types::{BlockRef, NiTransform};
use byroredux_core::string::StringPool;
use std::sync::Arc;

fn empty_net_with_name(name: &str) -> NiObjectNETData {
    NiObjectNETData {
        name: Some(Arc::from(name)),
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

/// Minimal `BSLightingShaderProperty` whose `net.name` is set to `name`.
/// All numeric fields are defaults; `texture_set_ref` is NULL so only
/// the `material_path` code-path is exercised.
fn lighting_shader_with_name(name: &str) -> BSLightingShaderProperty {
    BSLightingShaderProperty {
        shader_type: 0,
        net: empty_net_with_name(name),
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

/// Minimal NiTriShape whose `shader_property_ref` points at block `idx`.
fn tri_shape_with_shader_ref(idx: u32) -> NiTriShape {
    NiTriShape {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("TestMesh")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef::NULL,
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef(idx),
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    }
}

/// Shared helper: build a one-block NifScene containing a
/// `BSLightingShaderProperty` with `name`, then extract material info.
fn extract_for_shader_name(name: &str) -> (MaterialInfo, StringPool) {
    let shader = lighting_shader_with_name(name);
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = tri_shape_with_shader_ref(0);
    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);
    (info, pool)
}

/// Core regression: Starfield `.mat` on `BSLightingShaderProperty.name`
/// must land in `material_path`. Pre-#976 the inline `.bgsm`/`.bgem`
/// check silently dropped `.mat` references.
#[test]
fn bslighting_mat_reference_captured() {
    let (info, pool) = extract_for_shader_name("materials\\armor\\dragonscale.mat");
    // StringPool lowercases; the key assertion is that material_path is Some.
    let resolved = info.material_path.and_then(|s| pool.resolve(s));
    assert_eq!(
        resolved,
        Some("materials\\armor\\dragonscale.mat"),
        "BSLightingShaderProperty .mat name must populate material_path"
    );
}

/// Parity: existing `.bgsm` suffix must still work after the refactor.
#[test]
fn bslighting_bgsm_reference_still_captured() {
    let (info, pool) = extract_for_shader_name("materials\\actors\\iron.bgsm");
    let resolved = info.material_path.and_then(|s| pool.resolve(s));
    assert_eq!(
        resolved,
        Some("materials\\actors\\iron.bgsm"),
        "BSLightingShaderProperty .bgsm name must still populate material_path"
    );
}

/// Parity: existing `.bgem` suffix must still work after the refactor.
#[test]
fn bslighting_bgem_reference_still_captured() {
    let (info, pool) = extract_for_shader_name("materials\\effects\\magic.bgem");
    let resolved = info.material_path.and_then(|s| pool.resolve(s));
    assert_eq!(
        resolved,
        Some("materials\\effects\\magic.bgem"),
        "BSLightingShaderProperty .bgem name must still populate material_path"
    );
}

/// Case-insensitive: `.MAT` (capitalised) must be recognised as a material
/// reference. The resolved string is the StringPool-lowercased canonical form.
#[test]
fn bslighting_mat_case_insensitive() {
    let (info, pool) = extract_for_shader_name("materials\\test.MAT");
    let resolved = info.material_path.and_then(|s| pool.resolve(s));
    // StringPool lowercases all paths; the important thing is that material_path
    // is Some (i.e. the path was captured at all), not what the exact case is.
    assert_eq!(
        resolved,
        Some("materials\\test.mat"),
        "is_material_reference must be case-insensitive for .mat (pool returns lowercase)"
    );
}

/// Trailing-whitespace stripping: `is_material_reference` trims trailing
/// whitespace / null bytes (#749), so `"foo.bgsm\0"` must resolve
/// despite the null terminator.
#[test]
fn bslighting_trailing_nul_trimmed() {
    let (info, _pool) = extract_for_shader_name("materials\\foo.bgsm\0");
    assert!(
        info.material_path.is_some(),
        "trailing null byte must not prevent material_path capture"
    );
}

/// A plain NIF name (no material suffix) must leave `material_path = None`.
#[test]
fn bslighting_plain_name_does_not_set_material_path() {
    let (info, _pool) = extract_for_shader_name("SomeEditorName");
    assert!(
        info.material_path.is_none(),
        "plain NIF name must not populate material_path"
    );
}

/// A `BSShaderTextureSet` on a `.mat`-named shader must still populate
/// the texture slots — material_path capture must not gate texture extraction.
#[test]
fn bslighting_mat_name_plus_texture_set_both_captured() {
    let shader = lighting_shader_with_name("materials\\test.mat");
    let tex_set = BSShaderTextureSet {
        textures: vec![
            "textures\\test_d.dds".to_string(),
            "textures\\test_n.dds".to_string(),
        ],
    };

    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(shader),
        Box::new(tex_set),
    ];
    // Patch texture_set_ref to point at block 1.
    let mut scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    // Wire the texture_set_ref on the lighting shader in the scene.
    // NifScene::blocks is Vec<Box<dyn NiObject>>; cast via downcast
    // is not available generically, so rebuild with the right ref.
    let shader2 = {
        let mut s = lighting_shader_with_name("materials\\test.mat");
        s.texture_set_ref = BlockRef(1);
        s
    };
    scene.blocks[0] = Box::new(shader2);

    let shape = tri_shape_with_shader_ref(0);
    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    // StringPool lowercases all paths.
    let mat = info.material_path.and_then(|s| pool.resolve(s));
    assert_eq!(mat, Some("materials\\test.mat"), "material_path must be captured");
    let tex = info.texture_path.and_then(|s| pool.resolve(s));
    assert_eq!(tex, Some("textures\\test_d.dds"), "texture_path must be captured");
    let nrm = info.normal_map.and_then(|s| pool.resolve(s));
    assert_eq!(nrm, Some("textures\\test_n.dds"), "normal_map must be captured");
}
