//! Tests for `bs_tri_shape_shader_flag_tests` extracted from ../mesh.rs (refactor stage A).
//!
//! Same qualified path preserved (`bs_tri_shape_shader_flag_tests::FOO`).

//! Regression tests for issues #128 (two_sided via
//! BSEffectShaderProperty), #346 (effect-shader material capture +
//! decal flag mirroring), and #129 (shared material extractor so
//! these can't drift from the NiTriShape path). All tests drive
//! [`extract_bs_tri_shape`] end-to-end so the coverage tracks the
//! observable `ImportedMesh` output rather than deleted helper
//! implementation details.
use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::shader::BSEffectShaderProperty;
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3};

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

/// Build a renderable `BsTriShape` (one triangle, three vertices)
/// bound to a shader block at `shader_idx`. The geometry is
/// minimal but non-empty so `extract_bs_tri_shape` returns `Some`.
fn renderable_shape(shader_idx: u32) -> BsTriShape {
    BsTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: crate::types::NiTransform::default(),
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

/// Minimal `BSEffectShaderProperty` with only the bit under test set.
fn effect_shader(flags2: u32) -> BSEffectShaderProperty {
    BSEffectShaderProperty {
        net: empty_net(),
        material_reference: false,
        shader_flags_1: 0,
        shader_flags_2: flags2,
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

fn import(scene: &NifScene, shape: &BsTriShape) -> ImportedMesh {
    let (mesh, _) = import_with_pool(scene, shape);
    mesh
}

/// Variant returning both the mesh and the engine `StringPool` so
/// tests asserting on resolved texture paths can call `pool.resolve`
/// (#609 / D6-NEW-01).
fn import_with_pool(
    scene: &NifScene,
    shape: &BsTriShape,
) -> (ImportedMesh, byroredux_core::string::StringPool) {
    let mut pool = byroredux_core::string::StringPool::new();
    let mesh = extract_bs_tri_shape(scene, shape, &crate::types::NiTransform::default(), &mut pool)
        .expect("renderable shape must produce ImportedMesh");
    (mesh, pool)
}

/// #128 — Double_Sided bit on BSEffectShaderProperty.shader_flags_2
/// routes through the shared extractor onto `ImportedMesh.two_sided`.
#[test]
fn two_sided_via_bs_effect_shader_property() {
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(effect_shader(0x10)));
    assert!(import(&scene, &renderable_shape(0)).two_sided);
}

#[test]
fn not_two_sided_via_bs_effect_shader_without_flag() {
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(effect_shader(0x00)));
    assert!(!import(&scene, &renderable_shape(0)).two_sided);
}

#[test]
fn null_shader_ref_yields_single_sided() {
    let scene = NifScene::default();
    let mut shape = renderable_shape(0);
    shape.shader_property_ref = BlockRef::NULL;
    assert!(!import(&scene, &shape).two_sided);
}

#[test]
fn shader_ref_pointing_at_unrelated_block_yields_single_sided() {
    // A `shader_property_ref` that points at a non-shader block
    // (e.g. a NiNode — file corruption or a ref-resolution bug)
    // must not spuriously flip two_sided.
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: crate::types::NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    }));
    assert!(!import(&scene, &renderable_shape(0)).two_sided);
}

/// #618 — BsTriShape vertex_colors carry RGBA per nif.xml
/// `ByteColor4`; the importer must not collapse them to RGB. The
/// alpha lane is the per-vertex modulation that hair tip cards,
/// eyelash strips, and BSEffectShader meshes rely on.
#[test]
fn vertex_alpha_preserved_through_bs_tri_shape_path() {
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(effect_shader(0x00)));

    let mut shape = renderable_shape(0);
    shape.vertex_colors = vec![
        [1.0, 1.0, 1.0, 0.25],
        [1.0, 1.0, 1.0, 0.50],
        [1.0, 1.0, 1.0, 1.00],
    ];

    let mesh = import(&scene, &shape);
    let alphas: Vec<f32> = mesh.colors.iter().map(|c| c[3]).collect();
    assert_eq!(
        alphas,
        vec![0.25, 0.50, 1.00],
        "alpha lane must survive BsTriShape extraction (#618)"
    );
}

/// Default fallback when the BsTriShape carries no vertex_colors
/// must be opaque white (1.0 alpha) — guards against a regression
/// that would mark every fallback vertex transparent.
#[test]
fn vertex_color_fallback_is_opaque_white() {
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(effect_shader(0x00)));

    let shape = renderable_shape(0); // vertex_colors stays empty
    let mesh = import(&scene, &shape);

    for c in &mesh.colors {
        assert_eq!(*c, [1.0, 1.0, 1.0, 1.0]);
    }
}

/// #346 — BSEffectShaderProperty material fields reach the mesh
/// (emissive / UV / alpha / env-map / FO4+ normal / greyscale
/// palette). Pre-#129 this logic was inline in extract_bs_tri_shape;
/// post-refactor the shared extractor delivers the same fields.
fn effect_shader_with_payload() -> BSEffectShaderProperty {
    let mut s = effect_shader(0);
    s.uv_offset = [0.25, 0.5];
    s.uv_scale = [2.0, 4.0];
    s.base_color = [0.7, 0.8, 0.9, 0.5];
    s.base_color_scale = 3.5;
    s.env_map_scale = 0.75;
    s.normal_texture = "fx/glow_n.dds".to_string();
    s.greyscale_texture = "fx/fire_palette.dds".to_string();
    s
}

#[test]
fn extract_bs_tri_shape_pulls_effect_shader_emissive_uv_alpha_normal() {
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(effect_shader_with_payload()));
    let (mesh, pool) = import_with_pool(&scene, &renderable_shape(0));
    assert_eq!(mesh.emissive_color, [0.7, 0.8, 0.9]);
    assert!((mesh.emissive_mult - 3.5).abs() < 1e-6);
    assert_eq!(mesh.uv_offset, [0.25, 0.5]);
    assert_eq!(mesh.uv_scale, [2.0, 4.0]);
    assert!((mesh.mat_alpha - 0.5).abs() < 1e-6);
    assert!((mesh.env_map_scale - 0.75).abs() < 1e-6);
    assert_eq!(
        mesh.normal_map.and_then(|s| pool.resolve(s)),
        Some("fx/glow_n.dds")
    );
    let fx = mesh.effect_shader.expect("effect_shader should populate");
    assert_eq!(fx.greyscale_texture.as_deref(), Some("fx/fire_palette.dds"));
    assert!((fx.env_map_scale - 0.75).abs() < 1e-6);
}

/// #414 / FO4-D3-M1 — `shader_flags_2 & 0x0020_0000` on a modern
/// `BSEffectShaderProperty` is **not** a decal flag. It's
/// `Cloud_LOD` on Skyrim and `Anisotropic_Lighting` on FO4.
///
/// Pre-#414 the shared decal helper treated this bit as the
/// FO3/FNV `Alpha_Decal` regardless of property game-era — an
/// earlier #346 fix specifically asserted `is_decal = true` here
/// based on the same misreading. Post-#414 the modern decal helper
/// ignores flags2 bit 21; blood-splat decals on Skyrim+/FO4 must
/// set the SLSF1 `Decal` / `Dynamic_Decal` bits (26/27) instead,
/// which every vanilla decal mesh in Skyrim+ does.
#[test]
fn effect_shader_flag2_bit_21_is_not_decal_on_modern_properties() {
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(effect_shader(0x0020_0000)));
    assert!(
        !import(&scene, &renderable_shape(0)).is_decal,
        "flags2 bit 21 is Cloud_LOD (Skyrim) / Anisotropic_Lighting (FO4), not a decal bit"
    );
}

/// #346 — DECAL_SINGLE_PASS on shader_flags_1 works on either
/// shader variant. Shared extractor mirrors the check.
#[test]
fn decal_via_effect_shader_decal_single_pass() {
    let mut scene = NifScene::default();
    let mut shader = effect_shader(0);
    shader.shader_flags_1 = 0x0400_0000;
    scene.blocks.push(Box::new(shader));
    assert!(import(&scene, &renderable_shape(0)).is_decal);
}
