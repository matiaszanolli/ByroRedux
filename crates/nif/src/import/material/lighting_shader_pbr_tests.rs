//! Regression tests for #1241 (NIF-DIM4-NEW-01) —
//! `BSLightingShaderProperty` PBR scalars must reach `MaterialInfo`
//! and propagate through the mesh-extractor lift to `ImportedMesh`.
//!
//! Pre-fix the walker copied `emissive_*` / `specular_*` / `glossiness`
//! / `uv_*` / `alpha` out of the parsed BSLSP body but silently dropped:
//!
//! - `refraction_strength` (every BSVER 83+ BSLSP)
//! - `lighting_effect_1` / `lighting_effect_2` (Skyrim subsurface /
//!   backlight scalars, BSVER < FO4, gated on `SLSF2_Soft_Lighting` /
//!   `SLSF2_Back_Lighting`)
//! - `subsurface_rolloff` / `rimlight_power` / `backlight_power`
//!   (FO4 BSVER 130–139)
//! - `grayscale_to_palette_scale` / `fresnel_power` (FO4+ BSVER >= 130)
//!
//! Each parser-side fix (#1175 backlight gate inversion, #115 backlight
//! conditional, #403 wetness gate at BSVER 130) settled the wire
//! capture; the renderer ladder's fallback constants still fired on
//! every BSLightingShaderProperty surface because no field reached
//! `MaterialInfo` or `ImportedMesh`.

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::shader::{BSLightingShaderProperty, ShaderTypeData};
use crate::blocks::tri_shape::NiTriShape;
use crate::blocks::NiObject;
use crate::types::{BlockRef, NiTransform};
use byroredux_core::string::StringPool;
use std::sync::Arc;

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

/// Build a `BSLightingShaderProperty` with the 7 PBR scalar fields set
/// to non-default sentinel values. The walker should propagate every
/// value unchanged into `MaterialInfo` regardless of which BSVER the
/// block was parsed for — gating happens parser-side at
/// `crates/nif/src/blocks/shader.rs:679-695`; once the struct exists
/// the walker just copies fields.
fn bslsp_with_pbr_scalars(
    refraction: f32,
    le1: f32,
    le2: f32,
    sss_rolloff: f32,
    rim: f32,
    back: f32,
    grayscale: f32,
    fresnel: f32,
) -> BSLightingShaderProperty {
    BSLightingShaderProperty {
        shader_type: 0,
        net: empty_net(),
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
        refraction_strength: refraction,
        glossiness: 80.0,
        specular_color: [1.0; 3],
        specular_strength: 1.0,
        lighting_effect_1: le1,
        lighting_effect_2: le2,
        subsurface_rolloff: sss_rolloff,
        rimlight_power: rim,
        backlight_power: back,
        grayscale_to_palette_scale: grayscale,
        fresnel_power: fresnel,
        wetness: None,
        luminance: None,
        do_translucency: false,
        translucency: None,
        texture_arrays: Vec::new(),
        shader_type_data: ShaderTypeData::None,
        starfield_tail: Vec::new(),
    }
}

fn tri_shape_with_shader_ref(idx: u32) -> NiTriShape {
    NiTriShape {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("PbrScalarsTestMesh")),
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

fn extract_with_shader(shader: BSLightingShaderProperty) -> MaterialInfo {
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = crate::scene::NifScene {
        blocks,
        ..crate::scene::NifScene::default()
    };
    let shape = tri_shape_with_shader_ref(0);
    let mut pool = StringPool::new();
    extract_material_info(&scene, &shape, &[], &mut pool)
}

/// Skyrim BSVER band (< FO4): `lighting_effect_1` / `lighting_effect_2`
/// drive subsurface + backlight scalars. Refraction is also present on
/// every BSVER 83+ BSLSP.
#[test]
fn skyrim_subsurface_and_backlight_scalars_land_in_material_info() {
    let shader = bslsp_with_pbr_scalars(
        0.65, // refraction_strength
        0.25, // lighting_effect_1 — subsurface
        0.40, // lighting_effect_2 — backlight
        0.0, 0.0, 0.0, 1.0, 5.0, // FO4-band defaults
    );
    let info = extract_with_shader(shader);
    assert_eq!(info.refraction_strength, 0.65);
    assert_eq!(info.lighting_effect_1, 0.25);
    assert_eq!(info.lighting_effect_2, 0.40);
}

/// FO4 BSVER 130–139: `subsurface_rolloff` / `rimlight_power` /
/// `backlight_power` carry the per-material SSS/rim/back exponents.
#[test]
fn fo4_subsurface_rolloff_rim_and_back_scalars_land_in_material_info() {
    let shader = bslsp_with_pbr_scalars(
        0.10, // refraction
        0.0, 0.0,  // Skyrim-band defaults
        0.35, // subsurface_rolloff
        2.50, // rimlight_power
        1.75, // backlight_power
        1.0, 5.0, // FO4+ defaults
    );
    let info = extract_with_shader(shader);
    assert_eq!(info.subsurface_rolloff, 0.35);
    assert_eq!(info.rimlight_power, 2.50);
    assert_eq!(info.backlight_power, 1.75);
}

/// FO4+ BSVER >= 130 (FO4 / FO76 / Starfield): `grayscale_to_palette_scale`
/// and `fresnel_power` ride alongside the FO4-only rolloff/rim/back trio.
#[test]
fn fo4_plus_grayscale_and_fresnel_scalars_land_in_material_info() {
    let shader = bslsp_with_pbr_scalars(
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.75, // grayscale_to_palette_scale
        3.5,  // fresnel_power
    );
    let info = extract_with_shader(shader);
    assert_eq!(info.grayscale_to_palette_scale, 0.75);
    assert_eq!(info.fresnel_power, 3.5);
}

/// `MaterialInfo::default()` must mirror the BSLSP parser stub at
/// `crates/nif/src/blocks/shader.rs:739-749` so the no-author fallback
/// is the same as the FO76+ stopcond fallback. If either side drifts
/// the renderer would see different rim/SSS/fresnel defaults depending
/// on whether the BGSM stopcond fired or the block was parsed normally.
#[test]
fn material_info_default_matches_bslsp_parser_stub_defaults() {
    let info = MaterialInfo::default();
    // The stub defaults: 0.0 for everything except grayscale (1.0 =
    // no modulation) and fresnel (5.0 = standard Schlick exponent).
    assert_eq!(info.refraction_strength, 0.0);
    assert_eq!(info.lighting_effect_1, 0.0);
    assert_eq!(info.lighting_effect_2, 0.0);
    assert_eq!(info.subsurface_rolloff, 0.0);
    assert_eq!(info.rimlight_power, 0.0);
    assert_eq!(info.backlight_power, 0.0);
    assert_eq!(info.grayscale_to_palette_scale, 1.0);
    assert_eq!(info.fresnel_power, 5.0);
}

/// SIBLING check: the 7 fields must reach `ImportedMesh` through all
/// three mesh extractors. The NiTriShape path is exercised end-to-end
/// here via the full `import_nif_scene` entry. The BsTriShape and
/// BSGeometry paths are exercised through `cargo test` of their own
/// sibling test files (no synthetic NIF helper exists for those today;
/// the literal field-copy in the constructor was added in lockstep
/// with the NiTriShape change, so a per-extractor unit test on the
/// same data would just re-test the compiler).
///
/// What this test actually pins: when a BSLightingShaderProperty backs
/// a NiTriShape, every scalar lands on the produced `ImportedMesh`. If
/// a future refactor accidentally drops one of the field copies in the
/// NiTriShape constructor (the most common BSLSP path on Skyrim+ FO4
/// imports today), this test fails immediately.
#[test]
fn pbr_scalars_propagate_to_imported_mesh_through_ni_tri_shape() {
    use crate::blocks::node::NiNode;
    use crate::blocks::tri_shape::NiTriShapeData;
    use crate::types::NiPoint3;

    let shader = bslsp_with_pbr_scalars(0.42, 0.11, 0.22, 0.33, 0.44, 0.55, 0.66, 7.7);

    // Minimal NiTriShape with a 1-triangle NiTriShapeData so the
    // extractor actually produces an ImportedMesh (zero-triangle
    // shapes are skipped).
    let v = |x, y, z| NiPoint3 { x, y, z };
    let n_up = v(0.0, 0.0, 1.0);
    let data = NiTriShapeData {
        vertices: vec![v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0)],
        normals: vec![n_up; 3],
        center: NiPoint3::default(),
        radius: 1.0,
        vertex_colors: Vec::new(),
        uv_sets: vec![vec![[0.0; 2]; 3]],
        triangles: vec![[0, 1, 2]],
    };

    // Scene layout: [0] NiNode root → child BlockRef(1) NiTriShape →
    // shader BlockRef(2) BSLightingShaderProperty + data BlockRef(3)
    // NiTriShapeData.
    let root = NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("Root")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: vec![BlockRef(1)],
        effects: Vec::new(),
    };
    let mut shape = tri_shape_with_shader_ref(2);
    shape.data_ref = BlockRef(3);

    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(root),
        Box::new(shape),
        Box::new(shader),
        Box::new(data),
    ];
    let scene = crate::scene::NifScene {
        blocks,
        root_index: Some(0),
        ..crate::scene::NifScene::default()
    };

    let mut pool = StringPool::new();
    let imported = crate::import::import_nif_scene(&scene, &mut pool);
    assert_eq!(
        imported.meshes.len(),
        1,
        "single triangle must produce one ImportedMesh"
    );
    let mesh = &imported.meshes[0];

    assert_eq!(mesh.refraction_strength, 0.42);
    assert_eq!(mesh.lighting_effect_1, 0.11);
    assert_eq!(mesh.lighting_effect_2, 0.22);
    assert_eq!(mesh.subsurface_rolloff, 0.33);
    assert_eq!(mesh.rimlight_power, 0.44);
    assert_eq!(mesh.backlight_power, 0.55);
    assert_eq!(mesh.grayscale_to_palette_scale, 0.66);
    assert_eq!(mesh.fresnel_power, 7.7);
}
