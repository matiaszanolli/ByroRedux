//! Tests for `material_path_capture_tests` extracted from ../mesh.rs (refactor stage A).
//!
//! Same qualified path preserved (`material_path_capture_tests::FOO`).

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::shader::{BSEffectShaderProperty, BSLightingShaderProperty, ShaderTypeData};
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3, NiTransform};
use std::sync::Arc;

fn net_with_name(name: &str) -> NiObjectNETData {
    NiObjectNETData {
        name: Some(Arc::from(name)),
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

fn minimal_lighting_shader_named(name: &str) -> BSLightingShaderProperty {
    BSLightingShaderProperty {
        shader_type: 0,
        net: net_with_name(name),
        material_reference: true,
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
        shader_type_data: ShaderTypeData::None,
    }
}

fn minimal_effect_shader_named(name: &str) -> BSEffectShaderProperty {
    BSEffectShaderProperty {
        net: net_with_name(name),
        material_reference: true,
        shader_flags_1: 0,
        shader_flags_2: 0,
        sf1_crcs: Vec::new(),
        sf2_crcs: Vec::new(),
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        source_texture: String::new(),
        texture_clamp_mode: 3,
        lighting_influence: 0,
        env_map_min_lod: 0,
        falloff_start_angle: 1.0,
        falloff_stop_angle: 1.0,
        falloff_start_opacity: 0.0,
        falloff_stop_opacity: 0.0,
        refraction_power: 0.0,
        base_color: [0.0; 4],
        base_color_scale: 1.0,
        soft_falloff_depth: 0.0,
        greyscale_texture: String::new(),
        env_map_texture: String::new(),
        normal_texture: String::new(),
        env_mask_texture: String::new(),
        env_map_scale: 1.0,
        reflectance_texture: String::new(),
        lighting_texture: String::new(),
        emittance_color: [0.0; 3],
        emit_gradient_texture: String::new(),
        luminance: None,
    }
}

/// Build a renderable `BsTriShape` (one triangle, three vertices)
/// bound to a shader block at `shader_idx`. Keeps the shape non-
/// empty so `extract_bs_tri_shape` returns `Some`.
fn renderable_shape(shader_idx: u32) -> BsTriShape {
    BsTriShape {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
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
        kind: BsTriShapeKind::Plain,
        data_size: 0,
    }
}

/// Import the shape and return both the `ImportedMesh` and the engine
/// `StringPool` so tests asserting on resolved texture / material paths
/// can call `pool.resolve` (#609 / D6-NEW-01).
fn import(
    scene: &NifScene,
    shape: &BsTriShape,
) -> (ImportedMesh, byroredux_core::string::StringPool) {
    let mut pool = byroredux_core::string::StringPool::new();
    let mesh = extract_bs_tri_shape(scene, shape, &NiTransform::default(), &mut pool)
        .expect("renderable shape must produce ImportedMesh");
    (mesh, pool)
}

/// Resolve a path handle through the engine `StringPool`. The pool
/// lowercases on intern (Gamebryo `GlobalStringTable` semantic); test
/// assertions therefore compare against the canonical lowercase form.
fn resolved_path<'a>(
    pool: &'a byroredux_core::string::StringPool,
    sym: Option<byroredux_core::string::FixedString>,
) -> Option<&'a str> {
    sym.and_then(|s| pool.resolve(s))
}

#[test]
fn bgsm_on_lighting_shader_still_captured() {
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(minimal_lighting_shader_named(
        "Materials\\Architecture\\WhiterunStone.BGSM",
    )));
    let (mesh, pool) = import(&scene, &renderable_shape(0));
    assert_eq!(
        resolved_path(&pool, mesh.material_path),
        Some("materials\\architecture\\whiterunstone.bgsm")
    );
}

#[test]
fn bgem_on_effect_shader_is_captured() {
    // #434 — pre-fix, only BSLightingShaderProperty was inspected.
    // FO4 laser rifle beam binds a `BSEffectShaderProperty` whose
    // `net.name` ends in `.bgem`.
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(minimal_effect_shader_named(
        "Materials\\Weapons\\LaserRifle\\LaserBeam.BGEM",
    )));
    let (mesh, pool) = import(&scene, &renderable_shape(0));
    assert_eq!(
        resolved_path(&pool, mesh.material_path),
        Some("materials\\weapons\\laserrifle\\laserbeam.bgem")
    );
}

#[test]
fn bgsm_on_effect_shader_also_captured() {
    // Occasionally artists bind a `.bgsm` to a BSEffect shader —
    // the engine treats the suffix as advisory, not gating.
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(minimal_effect_shader_named(
        "Materials\\Statics\\Sign01.BGSM",
    )));
    let (mesh, pool) = import(&scene, &renderable_shape(0));
    assert_eq!(
        resolved_path(&pool, mesh.material_path),
        Some("materials\\statics\\sign01.bgsm")
    );
}

#[test]
fn non_material_name_returns_none() {
    let mut scene = NifScene::default();
    scene
        .blocks
        .push(Box::new(minimal_effect_shader_named("FxGlowEdge01")));
    let (mesh, _pool) = import(&scene, &renderable_shape(0));
    assert!(mesh.material_path.is_none());
}

#[test]
fn lighting_shader_name_takes_priority() {
    // If a BsTriShape's shader_property_ref points at a
    // BSLightingShaderProperty (the canonical case), the shared
    // extractor surfaces its BGSM name. The deterministic dispatch
    // order is preserved by the shared implementation.
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(minimal_lighting_shader_named(
        "Materials\\Primary.BGSM",
    )));
    let (mesh, pool) = import(&scene, &renderable_shape(0));
    assert_eq!(
        resolved_path(&pool, mesh.material_path),
        Some("materials\\primary.bgsm")
    );
}

#[test]
fn material_path_from_name_helper_accepts_both_suffixes() {
    let mut pool = byroredux_core::string::StringPool::new();
    assert_eq!(
        material_path_from_name(Some("x/y/z.bgem"), &mut pool).and_then(|s| pool.resolve(s)),
        Some("x/y/z.bgem")
    );
    // StringPool lowercases on intern, so `.BGSM` round-trips as `.bgsm`.
    assert_eq!(
        material_path_from_name(Some("x/y/z.BGSM"), &mut pool).and_then(|s| pool.resolve(s)),
        Some("x/y/z.bgsm")
    );
    assert_eq!(material_path_from_name(Some("plain_name"), &mut pool), None);
    assert_eq!(material_path_from_name(None, &mut pool), None);
}

#[test]
fn mat_suffix_recognised_case_insensitively() {
    use crate::blocks::shader::is_material_reference;
    assert!(is_material_reference("materials\\test.mat"));
    assert!(is_material_reference("materials\\test.MAT"));
    assert!(is_material_reference("materials\\test.Mat"));
    assert!(!is_material_reference("materials\\test.matx"));
    assert!(!is_material_reference("mat"));
    assert!(!is_material_reference(""));
}

#[test]
fn mat_captured_via_material_path_from_name() {
    let mut pool = byroredux_core::string::StringPool::new();
    assert!(
        material_path_from_name(Some("materials\\foo.mat"), &mut pool).is_some(),
        ".mat suffix should be treated as a material reference"
    );
    assert!(
        material_path_from_name(Some("materials\\foo.MAT"), &mut pool).is_some(),
        ".MAT suffix (uppercase) should be treated as a material reference"
    );
}
