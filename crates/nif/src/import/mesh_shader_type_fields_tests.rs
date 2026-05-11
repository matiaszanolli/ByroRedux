//! Tests for `shader_type_fields_tests` extracted from ../mesh.rs (refactor stage A).
//!
//! Same qualified path preserved (`shader_type_fields_tests::FOO`).

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::shader::{BSLightingShaderProperty, ShaderTypeData};
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3, NiTransform};

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

fn lighting_shader_with(shader_type: u32, data: ShaderTypeData) -> BSLightingShaderProperty {
    BSLightingShaderProperty {
        shader_type,
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
        texture_clamp_mode: 3,
        alpha: 1.0,
        refraction_strength: 0.0,
        glossiness: 80.0,
        specular_color: [1.0; 3],
        specular_strength: 1.0,
        lighting_effect_1: 0.0,
        lighting_effect_2: 0.0,
        subsurface_rolloff: 0.0,
        rimlight_power: 0.0,
        backlight_power: 0.0,
        grayscale_to_palette_scale: 1.0,
        fresnel_power: 5.0,
        wetness: None,
        luminance: None,
        do_translucency: false,
        translucency: None,
        texture_arrays: Vec::new(),
        shader_type_data: data,
    }
}

/// Minimal renderable `BsTriShape` (one triangle, three vertices) bound
/// to a shader block at index `shader_idx` on the scene.
fn renderable_shape(shader_idx: u32) -> BsTriShape {
    BsTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        center: NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        radius: 0.0,
        skin_ref: BlockRef::NULL,
        shader_property_ref: BlockRef(shader_idx),
        alpha_property_ref: BlockRef::NULL,
        vertex_desc: 0,
        num_triangles: 1,
        num_vertices: 3,
        vertices: vec![
            NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            NiPoint3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            NiPoint3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
        ],
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        normals: Vec::new(),
        vertex_colors: Vec::new(),
        triangles: vec![[0, 1, 2]],
        bone_weights: Vec::new(),
        bone_indices: Vec::new(),
        tangents: Vec::new(),
        kind: BsTriShapeKind::Plain,
        data_size: 0,
    }
}

#[test]
fn bs_tri_shape_captures_skin_tint_color() {
    // Skyrim SE NPC head — shader_type = 5 (SkinTint). Pre-#430 the
    // match arm dropped the color silently.
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(lighting_shader_with(
        5,
        ShaderTypeData::SkinTint {
            skin_tint_color: [0.87, 0.65, 0.54],
        },
    )));
    let shape = renderable_shape(0);
    let imported = extract_bs_tri_shape(
        &scene,
        &shape,
        &NiTransform::default(),
        &mut byroredux_core::string::StringPool::new(),
    )
    .expect("synthetic shape should import");
    assert_eq!(imported.material_kind, 5);
    assert_eq!(
        imported.shader_type_fields.skin_tint_color,
        Some([0.87, 0.65, 0.54])
    );
    assert_eq!(imported.shader_type_fields.skin_tint_alpha, None);
}

#[test]
fn bs_tri_shape_captures_hair_tint_color() {
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(lighting_shader_with(
        6,
        ShaderTypeData::HairTint {
            hair_tint_color: [0.3, 0.15, 0.05],
        },
    )));
    let shape = renderable_shape(0);
    let imported = extract_bs_tri_shape(
        &scene,
        &shape,
        &NiTransform::default(),
        &mut byroredux_core::string::StringPool::new(),
    )
    .unwrap();
    assert_eq!(imported.material_kind, 6);
    assert_eq!(
        imported.shader_type_fields.hair_tint_color,
        Some([0.3, 0.15, 0.05])
    );
}

#[test]
fn bs_tri_shape_captures_eye_envmap_centers() {
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(lighting_shader_with(
        16,
        ShaderTypeData::EyeEnvmap {
            eye_cubemap_scale: 1.5,
            left_eye_reflection_center: [0.1, 0.2, 0.3],
            right_eye_reflection_center: [0.4, 0.5, 0.6],
        },
    )));
    let shape = renderable_shape(0);
    let imported = extract_bs_tri_shape(
        &scene,
        &shape,
        &NiTransform::default(),
        &mut byroredux_core::string::StringPool::new(),
    )
    .unwrap();
    assert_eq!(imported.shader_type_fields.eye_cubemap_scale, Some(1.5));
    assert_eq!(
        imported.shader_type_fields.eye_left_reflection_center,
        Some([0.1, 0.2, 0.3])
    );
    assert_eq!(
        imported.shader_type_fields.eye_right_reflection_center,
        Some([0.4, 0.5, 0.6])
    );
}

#[test]
fn bs_tri_shape_fo76_skin_tint_splits_rgba() {
    // FO76 BSShaderType155::SkinTint — the 4-wide variant. Must split
    // into rgb + alpha exactly the way MaterialInfo's copy does.
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(lighting_shader_with(
        4,
        ShaderTypeData::Fo76SkinTint {
            skin_tint_color: [0.9, 0.7, 0.55, 0.25],
        },
    )));
    let shape = renderable_shape(0);
    let imported = extract_bs_tri_shape(
        &scene,
        &shape,
        &NiTransform::default(),
        &mut byroredux_core::string::StringPool::new(),
    )
    .unwrap();
    assert_eq!(
        imported.shader_type_fields.skin_tint_color,
        Some([0.9, 0.7, 0.55])
    );
    assert_eq!(imported.shader_type_fields.skin_tint_alpha, Some(0.25));
}

#[test]
fn bs_tri_shape_environment_map_routes_scale_not_fields() {
    // EnvironmentMap lives on `env_map_scale`, not `shader_type_fields`.
    // The default / no-variant-match ShaderTypeFields should stay clean.
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(lighting_shader_with(
        1,
        ShaderTypeData::EnvironmentMap { env_map_scale: 2.5 },
    )));
    let shape = renderable_shape(0);
    let imported = extract_bs_tri_shape(
        &scene,
        &shape,
        &NiTransform::default(),
        &mut byroredux_core::string::StringPool::new(),
    )
    .unwrap();
    assert_eq!(imported.env_map_scale, 2.5);
    assert_eq!(
        imported.shader_type_fields,
        super::super::material::ShaderTypeFields::default()
    );
}

#[test]
fn bs_tri_shape_without_shader_has_default_fields() {
    let scene = NifScene::default();
    let mut shape = renderable_shape(0);
    shape.shader_property_ref = BlockRef::NULL;
    let imported = extract_bs_tri_shape(
        &scene,
        &shape,
        &NiTransform::default(),
        &mut byroredux_core::string::StringPool::new(),
    )
    .unwrap();
    assert_eq!(imported.material_kind, 0);
    assert_eq!(
        imported.shader_type_fields,
        super::super::material::ShaderTypeFields::default()
    );
}
