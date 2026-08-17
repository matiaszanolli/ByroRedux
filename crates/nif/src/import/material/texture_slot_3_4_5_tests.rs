//! Tests for `texture_slot_3_4_5_tests` extracted from ../material.rs (refactor stage A).
//!
//! Same qualified path preserved (`texture_slot_3_4_5_tests::FOO`).

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::node::NiNode;
use crate::blocks::shader::{
    BSLightingShaderProperty, BSShaderPPLightingProperty, BSShaderTextureSet, ShaderTypeData,
};
use crate::blocks::tri_shape::NiTriShape;
use crate::blocks::NiObject;
use crate::types::{BlockRef, NiTransform};
use byroredux_core::string::{FixedString, StringPool};
use std::sync::Arc;

/// Walker invocation paired with the engine `StringPool` so tests can
/// resolve the [`FixedString`] handles back to `&str` for assertion
/// (#609 / D6-NEW-01). Returns `(MaterialInfo, StringPool)` so the
/// pool stays alive for the resolver lookups.
fn extract_with_pool(
    scene: &NifScene,
    shape: &NiTriShape,
    inherited: &[BlockRef],
) -> (MaterialInfo, StringPool) {
    let mut pool = StringPool::new();
    let info = extract_material_info(scene, shape, inherited, &mut pool);
    (info, pool)
}

#[track_caller]
fn assert_path(pool: &StringPool, sym: Option<FixedString>, expected: &str) {
    let resolved = sym.and_then(|s| pool.resolve(s));
    assert_eq!(
        resolved,
        Some(expected),
        "FixedString resolves to a different path"
    );
}

fn identity_transform() -> NiTransform {
    NiTransform::default()
}

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

fn fo3_pp_lighting_with_texture_set(
    tex_set_idx: u32,
    shader_flags_1: u32,
) -> BSShaderPPLightingProperty {
    use crate::blocks::base::BSShaderPropertyData;
    BSShaderPPLightingProperty {
        net: empty_net(),
        shader: BSShaderPropertyData {
            shade_flags: 0,
            shader_type: 7, // Parallax_Occlusion
            shader_flags_1,
            shader_flags_2: 0,
            env_map_scale: 0.5,
        },
        texture_clamp_mode: 0,
        texture_set_ref: BlockRef(tex_set_idx),
        refraction_strength: 0.0,
        refraction_fire_period: 0,
        parallax_max_passes: 4.0,
        // nif.xml's own on-disk default (range 0.0-10.0) — NOT the
        // engine `heightScale` range. FO3-D1-02 / #2317: the importer
        // converts this via `fo3_parallax_scale_to_height_scale`.
        parallax_scale: 1.0,
        emissive_color: [0.0, 0.0, 0.0, 1.0],
    }
}

fn make_tri_shape_with_props(properties: Vec<BlockRef>) -> NiTriShape {
    NiTriShape {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("TestShape")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: identity_transform(),
            properties,
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef::NULL,
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef::NULL,
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    }
}

#[test]
fn pp_lighting_populates_parallax_env_env_mask_from_slots_3_4_5() {
    // Scene layout:
    //   [0] NiNode (root)  — not used by extract_material_info
    //   [1] BSShaderPPLightingProperty referencing block 2
    //   [2] BSShaderTextureSet with 6 populated slots
    //
    // FO3-D1-02 / #2317: parallax now also requires an authored flag
    // (bit 11 `Parallax_Shader_Index_15` here) — texture-slot-3 presence
    // alone no longer suffices. See `pp_lighting_requires_parallax_flag_*`
    // below for the flag-gating regression coverage.
    let tex_set = BSShaderTextureSet {
        textures: vec![
            "textures\\wall_d.dds".to_string(),
            "textures\\wall_n.dds".to_string(),
            "textures\\wall_g.dds".to_string(),
            "textures\\wall_p.dds".to_string(),
            "textures\\wall_e.dds".to_string(),
            "textures\\wall_em.dds".to_string(),
        ],
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(NiNode {
            av: NiAVObjectData {
                net: empty_net(),
                flags: 0,
                transform: identity_transform(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        }),
        Box::new(fo3_pp_lighting_with_texture_set(
            2,
            crate::shader_flags::fo3nv_f1::PARALLAX,
        )),
        Box::new(tex_set),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(1)]);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);
    assert_path(&pool, info.texture_path, "textures\\wall_d.dds");
    assert_path(&pool, info.normal_map, "textures\\wall_n.dds");
    assert_path(&pool, info.glow_map, "textures\\wall_g.dds");
    assert_path(&pool, info.parallax_map, "textures\\wall_p.dds");
    assert_path(&pool, info.env_map, "textures\\wall_e.dds");
    assert_path(&pool, info.env_mask, "textures\\wall_em.dds");
    // Scalars ride through from BSShaderPPLightingProperty.
    // `parallax_height_scale`: fixture's raw nif.xml-range `parallax_scale`
    // (1.0) converts to the engine's `heightScale` contract via the 0.04
    // factor — 1.0 * 0.04 = 0.04, both pinning the conversion and matching
    // the "unauthored" engine default (FO3-D1-02 / #2317).
    assert_eq!(info.parallax_max_passes, Some(4.0));
    assert_eq!(info.parallax_height_scale, Some(0.04));
}

/// Regression for #773 / FO3-4-PPMAT (FO3-4-01 + FO3-4-02). The
/// FO3/FNV PPLighting walker branch must mirror two scalar fields
/// onto `MaterialInfo`:
///
/// 1. `texture_clamp_mode` (u32 → u8) — pre-fix CLAMP-authored
///    decals / scope reticles silently fell back to default WRAP
///    because no walker site assigned the field.
/// 2. `env_map_scale` (f32) — pre-fix env-cube + mask textures
///    arrived (#452) but the scalar that modulates them was zeroed
///    by `MaterialInfo::default()`, so glass / power armor / brass
///    rendered with zero reflection intensity even with a valid
///    env cube bound.
///
/// The fixture sets both fields to non-default values
/// (`texture_clamp_mode = 1` CLAMP_S_WRAP_T; `env_map_scale = 2.5`)
/// so a future regression that drops either back to the default
/// (`0` / `1.0`) fails the assertion immediately.
#[test]
fn pp_lighting_propagates_texture_clamp_mode_and_env_map_scale() {
    use crate::blocks::base::BSShaderPropertyData;
    let tex_set = BSShaderTextureSet {
        textures: vec![
            "textures\\armor_d.dds".to_string(),
            "textures\\armor_n.dds".to_string(),
            "textures\\armor_g.dds".to_string(),
            "textures\\armor_p.dds".to_string(),
            "textures\\armor_e.dds".to_string(),
            "textures\\armor_em.dds".to_string(),
        ],
    };
    // PPLighting fixture with both NEW assignments exercised:
    //   texture_clamp_mode = 1 (CLAMP_S_WRAP_T per nif.xml enum),
    //   env_map_scale = 2.5 (non-default, must survive the mirror).
    let shader = BSShaderPPLightingProperty {
        net: empty_net(),
        shader: BSShaderPropertyData {
            shade_flags: 0,
            shader_type: 7, // Parallax_Occlusion
            shader_flags_1: crate::shader_flags::fo3nv_f1::ENVIRONMENT_MAPPING,
            shader_flags_2: 0,
            env_map_scale: 2.5,
        },
        texture_clamp_mode: 1,
        texture_set_ref: BlockRef(1),
        refraction_strength: 0.0,
        refraction_fire_period: 0,
        parallax_max_passes: 4.0,
        parallax_scale: 0.04,
        emissive_color: [0.0, 0.0, 0.0, 1.0],
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader), Box::new(tex_set)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);

    // FO3-4-01: texture_clamp_mode flows through.
    assert_eq!(
        info.texture_clamp_mode, 1,
        "PPLighting texture_clamp_mode must mirror to MaterialInfo (#773 / FO3-4-01)"
    );
    // FO3-4-02: env_map_scale flows through.
    assert!(
        (info.env_map_scale - 2.5).abs() < 1e-6,
        "PPLighting env_map_scale must mirror to MaterialInfo \
         (#773 / FO3-4-02), got {}",
        info.env_map_scale
    );
}

#[test]
fn pp_lighting_without_environment_mapping_flag_ignores_default_scale() {
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(fo3_pp_lighting_with_texture_set(1, 0))];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);

    assert_eq!(
        info.env_map_scale, 0.0,
        "FO3/FNV's default env_map_scale is unauthored unless an environment-mapping flag is set"
    );
}

#[test]
fn pp_lighting_with_only_3_slots_leaves_parallax_and_env_none() {
    // Old-style texture set with just base/normal/glow — parallax
    // slots stay None so downstream consumers (FO3-REN-M2) skip
    // the parallax branch cleanly. Flag authored (PARALLAX) but no
    // slot-3 texture exists: still None either way (FO3-D1-02 / #2317).
    let tex_set = BSShaderTextureSet {
        textures: vec![
            "textures\\wall_d.dds".to_string(),
            "textures\\wall_n.dds".to_string(),
            "textures\\wall_g.dds".to_string(),
        ],
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(fo3_pp_lighting_with_texture_set(
            1,
            crate::shader_flags::fo3nv_f1::PARALLAX,
        )),
        Box::new(tex_set),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);
    assert!(info.parallax_map.is_none());
    assert!(info.env_map.is_none());
    assert!(info.env_mask.is_none());
}

/// FO3-D1-02 / #2317 — a bound slot-3 texture alone must NOT enable POM.
/// Pre-fix, `apply_pp_lighting_property` bound `parallax_map` (and its
/// scalar pair) purely from `BSShaderTextureSet` slot-3 presence, with no
/// check on the authored `Parallax_Shader_Index_15`/`Parallax_Occulsion`
/// flag bits — every FO3/FNV `BSShaderTextureSet` has a slot 3 whether or
/// not the material actually authors parallax.
#[test]
fn pp_lighting_without_parallax_flag_leaves_parallax_map_none_despite_bound_texture() {
    let tex_set = BSShaderTextureSet {
        textures: vec![
            "textures\\wall_d.dds".to_string(),
            "textures\\wall_n.dds".to_string(),
            "textures\\wall_g.dds".to_string(),
            "textures\\wall_p.dds".to_string(),
        ],
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(fo3_pp_lighting_with_texture_set(1, 0)),
        Box::new(tex_set),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);
    assert!(
        info.parallax_map.is_none(),
        "an unauthored (no PARALLAX/PARALLAX_OCCLUSION flag) texture-slot-3 \
         path must not enable POM, even though the texture is bound"
    );
    assert!(info.parallax_max_passes.is_none());
    assert!(info.parallax_height_scale.is_none());
}

/// FO3-D1-02 / #2317 — either authored flag bit (`Parallax_Shader_Index_15`
/// bit 11, `Parallax_Occulsion` bit 28) independently enables POM, and the
/// raw nif.xml-range `parallax_scale` converts to the engine's
/// `heightScale` contract (0.02–0.08 typical) rather than passing through
/// unconverted. FO3's bsver<=24 fallback value (1.0, `blocks/shader.rs`)
/// would otherwise be a ~25× overshoot on that contract.
#[test]
fn pp_lighting_either_parallax_flag_enables_pom_with_converted_height_scale() {
    for flag in [
        crate::shader_flags::fo3nv_f1::PARALLAX,
        crate::shader_flags::fo3nv_f1::PARALLAX_OCCLUSION,
    ] {
        let tex_set = BSShaderTextureSet {
            textures: vec![
                "textures\\wall_d.dds".to_string(),
                "textures\\wall_n.dds".to_string(),
                "textures\\wall_g.dds".to_string(),
                "textures\\wall_p.dds".to_string(),
            ],
        };
        let blocks: Vec<Box<dyn NiObject>> = vec![
            Box::new(fo3_pp_lighting_with_texture_set(1, flag)),
            Box::new(tex_set),
        ];
        let scene = NifScene {
            blocks,
            ..NifScene::default()
        };
        let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
        let (info, pool) = extract_with_pool(&scene, &shape, &[]);
        assert_path(&pool, info.parallax_map, "textures\\wall_p.dds");
        let height_scale = info
            .parallax_height_scale
            .expect("authored POM must set a height scale");
        assert!(
            (0.02..=0.08).contains(&height_scale),
            "flag {flag:#x}: height scale {height_scale} outside the shader's \
             documented 0.02-0.08 contract range (material_sampling.glsl)"
        );
        assert!(
            (height_scale - 0.04).abs() < 1e-6,
            "flag {flag:#x}: fixture's raw parallax_scale=1.0 (nif.xml \
             default) must convert to exactly the engine's 0.04 default, \
             got {height_scale}"
        );
    }
}

#[test]
fn bs_lighting_shader_populates_parallax_env_slots() {
    // Skyrim+ path: same 6-slot texture set should flow through.
    let tex_set = BSShaderTextureSet {
        textures: vec![
            "d.dds".to_string(),
            "n.dds".to_string(),
            "g.dds".to_string(),
            "p.dds".to_string(),
            "e.dds".to_string(),
            "em.dds".to_string(),
        ],
    };
    let shader = BSLightingShaderProperty {
        shader_type: 7, // ParallaxOcc
        net: empty_net(),
        material_reference: false,
        shader_flags_1: 0,
        shader_flags_2: 0,
        sf1_crcs: Vec::new(),
        sf2_crcs: Vec::new(),
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        texture_set_ref: BlockRef(1),
        emissive_color: [0.0; 3],
        emissive_multiple: 1.0,
        root_material_path: None,
        texture_clamp_mode: 0,
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
        grayscale_to_palette_scale: 0.0,
        fresnel_power: 0.0,
        wetness: None,
        luminance: None,
        do_translucency: false,
        translucency: None,
        texture_arrays: Vec::new(),
        shader_type_data: ShaderTypeData::None,
        starfield_tail: Vec::new(),
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader), Box::new(tex_set)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut shape = make_tri_shape_with_props(Vec::new());
    shape.shader_property_ref = BlockRef(0);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);
    assert_path(&pool, info.parallax_map, "p.dds");
    assert_path(&pool, info.env_map, "e.dds");
    assert_path(&pool, info.env_mask, "em.dds");
}

// Keep the MaterialInfo default honest: new fields land as None.
#[test]
fn default_material_info_has_none_for_parallax_env_slots() {
    let info = MaterialInfo::default();
    assert!(info.parallax_map.is_none());
    assert!(info.env_map.is_none());
    assert!(info.env_mask.is_none());
}

/// Regression: #435 / NIF-D4-N06 — when a NiTriShape's property
/// list is `[NiMaterialProperty, NiTexturingProperty]` (the common
/// Oblivion / FO3 / FNV order), the base-slot UV transform on the
/// `NiTexturingProperty` must still reach `MaterialInfo`. Pre-fix
/// the gate at the texture-slot UV-transform copy site was
/// `!info.has_material_data`, which `NiMaterialProperty` had
/// already set to `true` — silently dropping authored UV scrolls
/// on tapestries / signs / banner cloth.
#[test]
fn ni_texturing_uv_transform_survives_preceding_ni_material_property() {
    use crate::blocks::properties::{
        NiMaterialProperty, NiTexturingProperty, TexDesc, TexTransform,
    };
    use crate::types::NiColor;

    let mat = NiMaterialProperty {
        net: empty_net(),
        ambient: NiColor::default(),
        diffuse: NiColor {
            r: 0.5,
            g: 0.6,
            b: 0.7,
        },
        specular: NiColor::default(),
        emissive: NiColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        },
        shininess: 50.0,
        alpha: 1.0,
        emissive_mult: 1.0,
    };
    let tex = NiTexturingProperty {
        net: empty_net(),
        flags: 0,
        texture_count: 1,
        base_texture: Some(TexDesc {
            source_ref: BlockRef::NULL,
            flags: 0,
            transform: Some(TexTransform {
                translation: [0.5, 0.0],
                scale: [2.0, 1.0],
                rotation: 0.0,
                transform_method: 0,
                center: [0.0, 0.0],
            }),
        }),
        dark_texture: None,
        detail_texture: None,
        gloss_texture: None,
        glow_texture: None,
        bump_texture: None,
        normal_texture: None,
        parallax_texture: None,
        parallax_offset: 0.0,
        decal_textures: Vec::new(),
    };
    // Property order intentionally mirrors how Oblivion / FO3 / FNV
    // ship NiTriShape properties: NiMaterialProperty FIRST.
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(mat), Box::new(tex)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0), BlockRef(1)]);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);
    assert_eq!(
        info.uv_offset,
        [0.5, 0.0],
        "NiTexturingProperty base-slot uv_offset must survive a preceding NiMaterialProperty"
    );
    assert_eq!(
        info.uv_scale,
        [2.0, 1.0],
        "NiTexturingProperty base-slot uv_scale must survive a preceding NiMaterialProperty"
    );
    assert!(
        info.has_uv_transform,
        "has_uv_transform must be set after a UV transform copy"
    );
    // Sanity: the NiMaterialProperty values still flowed through.
    assert!(info.has_material_data);
    assert!((info.diffuse_color[0] - 0.5).abs() < 1e-6);
}

/// Regression: #221 — `NiMaterialProperty.ambient` must reach
/// `MaterialInfo.ambient_color`. Pre-fix the field was discarded
/// at the same site that captured `mat.diffuse` — visible as
/// authored-ambient meshes (lit-from-within glass, occluded
/// alcoves) reacting incorrectly to cell ambient lighting.
#[test]
fn ni_material_property_ambient_color_reaches_material_info() {
    use crate::blocks::properties::NiMaterialProperty;
    use crate::types::NiColor;

    let mat = NiMaterialProperty {
        net: empty_net(),
        ambient: NiColor {
            r: 0.25,
            g: 0.5,
            b: 0.75,
        },
        diffuse: NiColor::default(),
        specular: NiColor::default(),
        emissive: NiColor::default(),
        shininess: 50.0,
        alpha: 1.0,
        emissive_mult: 1.0,
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(mat)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);
    assert!((info.ambient_color[0] - 0.25).abs() < 1e-6);
    assert!((info.ambient_color[1] - 0.5).abs() < 1e-6);
    assert!((info.ambient_color[2] - 0.75).abs() < 1e-6);
}

/// Regression: #435 — a Skyrim+ `BSLightingShaderProperty`'s
/// uv_offset / uv_scale must also stamp `has_uv_transform`, so a
/// later `NiTexturingProperty` (rare but possible on mixed-property
/// meshes) cannot silently overwrite the shader-supplied transform.
#[test]
fn bs_lighting_shader_uv_transform_blocks_later_ni_texturing_property() {
    use crate::blocks::properties::{NiTexturingProperty, TexDesc, TexTransform};

    let shader = BSLightingShaderProperty {
        shader_type: 0,
        net: empty_net(),
        material_reference: false,
        shader_flags_1: 0,
        shader_flags_2: 0,
        sf1_crcs: Vec::new(),
        sf2_crcs: Vec::new(),
        uv_offset: [0.25, 0.75],
        uv_scale: [4.0, 4.0],
        texture_set_ref: BlockRef::NULL,
        emissive_color: [0.0; 3],
        emissive_multiple: 1.0,
        root_material_path: None,
        texture_clamp_mode: 0,
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
        starfield_tail: Vec::new(),
    };
    let tex = NiTexturingProperty {
        net: empty_net(),
        flags: 0,
        texture_count: 1,
        base_texture: Some(TexDesc {
            source_ref: BlockRef::NULL,
            flags: 0,
            transform: Some(TexTransform {
                translation: [0.99, 0.99],
                scale: [9.0, 9.0],
                rotation: 0.0,
                transform_method: 0,
                center: [0.0, 0.0],
            }),
        }),
        dark_texture: None,
        detail_texture: None,
        gloss_texture: None,
        glow_texture: None,
        bump_texture: None,
        normal_texture: None,
        parallax_texture: None,
        parallax_offset: 0.0,
        decal_textures: Vec::new(),
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader), Box::new(tex)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    // Skyrim+ binds BSLightingShaderProperty via `shader_property_ref`,
    // not through the legacy properties array — replicating the same
    // wiring extract_material_info uses.
    let mut shape = make_tri_shape_with_props(vec![BlockRef(1)]);
    shape.shader_property_ref = BlockRef(0);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);
    // Shader transform wins — the later NiTexturingProperty must
    // not stomp it.
    assert_eq!(info.uv_offset, [0.25, 0.75]);
    assert_eq!(info.uv_scale, [4.0, 4.0]);
    assert!(info.has_uv_transform);
}

// ── #706 / FX-1 regression guards ──────────────────────────────
//
// BSEffectShaderProperty meshes must arrive at the renderer with
// `material_kind = 101` so `triangle.frag` short-circuits lit
// shading and writes pure additive emissive. Pre-fix every effect
// surface (fire, magic, glow rings, force fields) ran the full
// PBR + RT-GI pipeline and got modulated by every nearby light.

fn empty_effect_shader_with_base_color(rgba: [f32; 4]) -> BSEffectShaderProperty {
    BSEffectShaderProperty {
        net: empty_net(),
        material_reference: false,
        shader_flags_1: 0,
        shader_flags_2: 0,
        sf1_crcs: Vec::new(),
        sf2_crcs: Vec::new(),
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        source_texture: "fx/glow.dds".to_string(),
        texture_clamp_mode: 3,
        lighting_influence: 0,
        env_map_min_lod: 0,
        falloff_start_angle: 1.0,
        falloff_stop_angle: 0.0,
        falloff_start_opacity: 1.0,
        falloff_stop_opacity: 0.0,
        refraction_power: 0.0,
        base_color: rgba,
        base_color_scale: 1.0,
        soft_falloff_depth: 1.0,
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
        starfield_tail: Vec::new(),
    }
}

#[test]
fn bs_effect_shader_property_sets_material_kind_to_101() {
    // Synthesised scene: a NiTriShape whose properties list
    // points at a single BSEffectShaderProperty. The pre-fix
    // import path captured `effect_shader: Some(_)` but left
    // `material_kind = 0` (Default Lit), causing the renderer
    // to drop the surface into the lit pipeline.
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(empty_effect_shader_with_base_color([
        1.0, 0.5, 0.1, 1.0,
    ]))];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    // BSEffectShaderProperty binds via the dedicated Skyrim+
    // shader_property_ref (same slot as BSLightingShaderProperty).
    let mut shape = make_tri_shape_with_props(Vec::new());
    shape.shader_property_ref = BlockRef(0);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);

    assert_eq!(
        info.material_kind, 101,
        "BSEffectShaderProperty must route through MATERIAL_KIND_EFFECT_SHADER \
             (101) so the fragment shader short-circuits lit shading"
    );
    assert!(
        info.effect_shader.is_some(),
        "rich effect-shader payload also captured (#345)"
    );
    // Existing import-side data plumbing still runs (regression
    // guard — the material_kind override must not stomp emissive
    // routing, alpha_blend, or texture path):
    assert_path(&pool, info.texture_path, "fx/glow.dds");
    assert!(
        info.alpha_blend,
        "BSEffectShaderProperty implies alpha-blend"
    );
    assert_eq!(info.emissive_color, [1.0, 0.5, 0.1]);
    // 2026-05-27 — effect-shader surfaces (light shafts, dust planes,
    // glow rings) are non-occluding and must not write depth, else the
    // FO4 god-ray cone's stacked additive BSTriShapes hard-edge against
    // each other. No NiZBufferProperty in this scene, so the
    // effect-shader default is the only thing setting z_write.
    assert!(
        !info.z_write,
        "BSEffectShaderProperty must default z_write=false (transparent-pass glow)"
    );
}

// ── BSShaderNoLightingProperty → MATERIAL_KIND_NO_LIGHTING (102) ─────
//
// FO3/FNV fullbright/unlit surfaces (terminal screens, computer text,
// neon/sign faces, HUD/scope overlays, blood decals) must arrive at the
// renderer with material_kind = 102 so triangle.frag emits the texture
// directly with no scene lighting / GI / camera-distance term. Pre-fix
// these went through the full lit path (material_kind = 0) and
// self-illumination dimmed with distance as GI faded at the rtLOD tier.
fn fo3_no_lighting(file_name: &str) -> crate::blocks::shader::BSShaderNoLightingProperty {
    use crate::blocks::base::BSShaderPropertyData;
    crate::blocks::shader::BSShaderNoLightingProperty {
        net: empty_net(),
        shader: BSShaderPropertyData {
            shade_flags: 0,
            shader_type: 0,
            shader_flags_1: 0,
            shader_flags_2: 0,
            env_map_scale: 1.0,
        },
        texture_clamp_mode: 3,
        file_name: file_name.to_string(),
        falloff_start_angle: 1.0,
        falloff_stop_angle: 1.0,
        falloff_start_opacity: 1.0,
        falloff_stop_opacity: 1.0,
    }
}

#[test]
fn nolighting_sets_material_kind_to_102() {
    // Scene: [0] BSShaderNoLightingProperty (FO3/FNV terminal screen).
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(fo3_no_lighting(
        "textures\\terminals\\terminalscreen01.dds",
    ))];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    // NoLighting binds via the FO3/FNV property LIST (not the Skyrim+
    // shader_property_ref slot).
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);

    assert_eq!(
        info.material_kind, 102,
        "BSShaderNoLightingProperty must route through MATERIAL_KIND_NO_LIGHTING \
         (102) so the fragment shader emits fullbright/unlit"
    );
    // Texture path still captured from the NoLighting file_name.
    assert_path(
        &pool,
        info.texture_path,
        "textures\\terminals\\terminalscreen01.dds",
    );
}

#[test]
fn nolighting_does_not_demote_an_existing_effect_kind() {
    // Guard the `material_kind == 0` gate: if a mesh somehow bound BOTH
    // an effect shader (101) and a NoLighting block, the NoLighting tag
    // must NOT stomp the engine-synthesized effect kind.
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(empty_effect_shader_with_base_color([1.0, 0.5, 0.1, 1.0])),
        Box::new(fo3_no_lighting("textures\\fx\\glow.dds")),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    // Effect via shader_property_ref (block 0), NoLighting via the
    // property list (block 1).
    let mut shape = make_tri_shape_with_props(vec![BlockRef(1)]);
    shape.shader_property_ref = BlockRef(0);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);
    assert_eq!(
        info.material_kind, 101,
        "effect-shader kind must survive a co-bound NoLighting block"
    );
}

fn skin_tint_lighting_shader() -> BSLightingShaderProperty {
    BSLightingShaderProperty {
        shader_type: 5, // SkinTint
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
        texture_clamp_mode: 0,
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
        grayscale_to_palette_scale: 0.0,
        fresnel_power: 0.0,
        wetness: None,
        luminance: None,
        do_translucency: false,
        translucency: None,
        texture_arrays: Vec::new(),
        shader_type_data: ShaderTypeData::None,
        starfield_tail: Vec::new(),
    }
}

#[test]
fn nispecular_disabled_clears_color_for_glass_ior_path() {
    // #696 / O4-04 — when NiSpecularProperty has bit 0 clear
    // (specular disabled), pre-fix only `specular_strength` was
    // zeroed. The IOR glass branch in triangle.frag:1004 does
    // `specStrength = max(specStrength, 3.0)`, silently re-
    // enabling spec on glass-classified meshes. The downstream
    // BRDF gates on `specStrength * specColor` — clearing the
    // color too kills the contribution on every path, including
    // the IOR glass re-promotion.
    //
    // Synthesise a scene where a NiTriShape's properties list
    // carries: a NiMaterialProperty (gives a non-trivial spec
    // color via `info.specular_color = ...`), then a disabled
    // NiSpecularProperty. Pre-fix: specular_color stayed at the
    // material's authored value. Post-fix: zeroed alongside
    // specular_strength.
    use crate::blocks::properties::{NiFlagProperty, NiMaterialProperty};
    use crate::types::NiColor;

    let mat_prop = NiMaterialProperty {
        net: empty_net(),
        ambient: NiColor {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        },
        diffuse: NiColor {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        },
        specular: NiColor {
            r: 0.8,
            g: 0.8,
            b: 0.8,
        },
        emissive: NiColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        },
        shininess: 80.0,
        alpha: 1.0,
        emissive_mult: 1.0,
    };
    let spec_prop = NiFlagProperty::for_test(0, "NiSpecularProperty");

    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(mat_prop), Box::new(spec_prop)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0), BlockRef(1)]);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);

    assert!(!info.specular_enabled);
    assert_eq!(info.specular_strength, 0.0);
    assert_eq!(
        info.specular_color,
        [0.0, 0.0, 0.0],
        "specular_color must zero out alongside strength so the IOR \
             glass branch's max(specStrength, 3.0) re-promotion can't \
             revive a disabled spec via the (strength * color) gate"
    );
}

#[test]
fn nispecular_enabled_preserves_color() {
    // Negative guard: a NiSpecularProperty with bit 0 set
    // (default behavior) must NOT zero specular_color. Without
    // this guard, a future "always zero specular_color" refactor
    // would silently kill spec on every working material.
    use crate::blocks::properties::{NiFlagProperty, NiMaterialProperty};
    use crate::types::NiColor;

    let mat_prop = NiMaterialProperty {
        net: empty_net(),
        ambient: NiColor {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        },
        diffuse: NiColor {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        },
        specular: NiColor {
            r: 0.8,
            g: 0.8,
            b: 0.8,
        },
        emissive: NiColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        },
        shininess: 80.0,
        alpha: 1.0,
        emissive_mult: 1.0,
    };
    let spec_prop = NiFlagProperty::for_test(1, "NiSpecularProperty");

    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(mat_prop), Box::new(spec_prop)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0), BlockRef(1)]);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);

    assert!(info.specular_enabled);
    assert_eq!(info.specular_color, [0.8, 0.8, 0.8]);
    assert!(info.specular_strength > 0.0);
}

#[test]
fn bs_lighting_shader_property_keeps_low_range_material_kind() {
    // Negative guard: a normal Skyrim+ BSLightingShaderProperty
    // mesh (SkinTint = 5) must NOT be promoted to 101. Only
    // BSEffectShaderProperty triggers the engine-synthesized
    // material_kind. Without this guard, a future refactor that
    // conflates the two property types would silently demote
    // normal lit meshes to the emit-only path.
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(skin_tint_lighting_shader())];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut shape = make_tri_shape_with_props(Vec::new());
    shape.shader_property_ref = BlockRef(0);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);

    assert_eq!(
        info.material_kind, 5,
        "BSLightingShaderProperty must stay in the 0..=19 range — \
             only BSEffectShaderProperty promotes to 101"
    );
    assert!(
        info.effect_shader.is_none(),
        "no effect-shader payload on a lit material"
    );
}

// ── #563 / SK-D3-02 regression guards ──────────────────────────
//
// Per nif.xml `BSLightingShaderType`:
//   * FaceTint (4)            — slot 4 = Detail, slot 7 = Tint.
//   * MultiLayerParallax (11) — slot 4 = Env, slot 6 = inner Layer.
//   * EyeEnvmap (16)          — slot 4 = Env (default arm).
//
// #2693 — the MultiLayerParallax row said "slot 7 = inner Layer" until
// shipped data settled it: nif.xml's enum prose ("Layer(TS7)") and its
// `BSShaderTextureSet` field table (slot 6 = "Subsurface for Multilayer
// Parallax", slot 7 = "Back Lighting Map") contradict each other, and the
// field table is the one that matches every vanilla type-11 shape.
//
// Pre-#563 the importer treated slot 4 as env on every variant,
// positively misbinding FaceTint detail textures as cubemaps and
// silently dropping slot 7 across the board.

fn lighting_shader_with_type_and_texset(
    shader_type: u32,
    tex_set_idx: u32,
) -> BSLightingShaderProperty {
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
        texture_set_ref: BlockRef(tex_set_idx),
        emissive_color: [0.0; 3],
        emissive_multiple: 1.0,
        root_material_path: None,
        texture_clamp_mode: 0,
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
        grayscale_to_palette_scale: 0.0,
        fresnel_power: 0.0,
        wetness: None,
        luminance: None,
        do_translucency: false,
        translucency: None,
        texture_arrays: Vec::new(),
        shader_type_data: ShaderTypeData::None,
        starfield_tail: Vec::new(),
    }
}

fn full_8_slot_tex_set(tag: &str) -> BSShaderTextureSet {
    // All 8 slots populated, so a test can tell "routed to the wrong slot"
    // apart from "routed nowhere".
    //
    // #2693 — slot 6 used to be left empty here, on the same nif.xml enum-prose
    // premise that put the MultiLayerParallax inner layer on slot 7. An empty
    // slot 6 cannot distinguish those two failures, so the fixture quietly
    // guaranteed the misbind would pass. Vanilla type-11 content populates slot
    // 6 on 607/607 shapes; the fixture now matches.
    BSShaderTextureSet {
        textures: vec![
            format!("{tag}_d.dds"),
            format!("{tag}_n.dds"),
            format!("{tag}_g.dds"),
            format!("{tag}_p.dds"),
            format!("{tag}_4.dds"),
            format!("{tag}_5.dds"),
            format!("{tag}_6.dds"),
            format!("{tag}_7.dds"),
        ],
    }
}

#[test]
fn face_tint_routes_authored_slots_and_leaves_the_empty_ones_alone() {
    // FaceTint (4). #563 wired this arm from nif.xml's enum prose
    // ("Enables Detail(TS4), Tint(TS7)") and this test pinned it. #2694
    // measured vanilla: across `Skyrim - Meshes0.bsa`'s 3158 FaceTint
    // properties, slots 4/5/7 never appear at all, while slot 2 (`_sk`
    // skin-tint mask, 3158), slot 3 (`MaleHeadDetail_*`, 3149) and slot 6
    // (baked FaceGen tint, 3150) are the authored ones. So the arm was
    // inert and the populated slots each landed wrong:
    //   * slot 2 → `glow_map` (the `skin_tint_slot` gate missed type 4)
    //   * slot 3 → `parallax_map`, which makes triangle.frag ray-march POM
    //     over a face complexion map
    //   * slot 6 → dropped
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(lighting_shader_with_type_and_texset(4, 1)),
        Box::new(full_8_slot_tex_set("face")),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut shape = make_tri_shape_with_props(Vec::new());
    shape.shader_property_ref = BlockRef(0);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);

    // Slot 2 is the skin-tint mask, the same role SkinTint (5) gives it.
    assert_path(&pool, info.tint_map, "face_g.dds");
    assert!(
        info.glow_map.is_none(),
        "FaceTint slot 2 is an `_sk` skin-tint mask, not a glow map — binding \
         it as emissive is one authored non-black `emissive_color` away from \
         glowing faces (#2694)"
    );

    // Slot 3 is the face detail map, and must NOT reach the POM path.
    assert_path(&pool, info.detail_map, "face_p.dds");
    assert!(
        info.parallax_map.is_none(),
        "FaceTint slot 3 is a detail map; feeding it to `parallax_map` makes \
         triangle.frag ray-march POM over a face complexion map, since its POM \
         branch gates only on `parallaxMapIndex != 0u` (#2694)"
    );

    // Slots 4/5/7 are unauthored on real content; nothing may bind them.
    assert!(
        info.env_map.is_none(),
        "FaceTint slot 4 must NOT be misbound as an env cubemap (#563)"
    );
    assert!(
        info.env_mask.is_none(),
        "FaceTint has no slot 5 binding either"
    );
    assert!(
        info.inner_layer_map.is_none(),
        "FaceTint binds no inner layer"
    );
}

#[test]
fn multi_layer_parallax_routes_slot_6_to_inner_layer_alongside_env() {
    // MultiLayerParallax (11) — slot 4 stays the env cube
    // (paired with `multi_layer_envmap_strength`), slot 5 the
    // env mask, and slot **6** lands in `inner_layer_map`.
    //
    // #563 wired this to slot 7 from nif.xml's enum prose, and this test
    // pinned that reading — which is why the misbind survived. #2693
    // measured shipped content: across `Skyrim - Meshes0.bsa`'s 607
    // type-11 properties, slot 6 is populated on 607/607 with inner-layer
    // art (`RiftenWindowInner01`, `IceCaveWall02`) while slot 7 carries
    // tint maps (`IceCaveSubsurfacetint01`) on 370. Slot 7 is the back-
    // lighting map per nif.xml's field table and is deliberately parked:
    // no `MaterialTextureSet` role and no shader consumer exists for it.
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(lighting_shader_with_type_and_texset(11, 1)),
        Box::new(full_8_slot_tex_set("ice")),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut shape = make_tri_shape_with_props(Vec::new());
    shape.shader_property_ref = BlockRef(0);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);

    assert_path(&pool, info.env_map, "ice_4.dds");
    assert_path(&pool, info.env_mask, "ice_5.dds");
    assert_path(&pool, info.inner_layer_map, "ice_6.dds");
    assert!(
        info.tint_map.is_none(),
        "MultiLayerParallax has no tint route — slot 7 is the back-lighting \
         map, which has no canonical role yet"
    );
    assert!(
        info.detail_map.is_none(),
        "MultiLayerParallax has no detail-slot route — slot 4 stays env"
    );
    // Slot 7 must reach NO role. Pinning the park explicitly, because the
    // failure this fixes was a slot quietly routed to the wrong role rather
    // than to none — `ice_7.dds` appearing anywhere is the regression.
    for (role, handle) in [
        ("specular_map", info.specular_map),
        ("glow_map", info.glow_map),
        ("parallax_map", info.parallax_map),
        ("dark_map", info.dark_map),
    ] {
        if let Some(h) = handle {
            let resolved = pool.resolve(h).unwrap_or_default();
            assert!(
                !resolved.ends_with("ice_7.dds"),
                "slot 7 (back lighting) leaked into `{role}` — it has no \
                 canonical role and must stay parked (#2693)"
            );
        }
    }
}

/// #2694 sibling — the tint family (FaceTint 4 / SkinTint 5 / HairTint 6)
/// must all take slot 2 as the skin-tint mask, not as glow.
///
/// The `5 | 6 =>` arm already treats 5 and 6 as one family, but the slot-2
/// gate keyed on `shader_type == 5` alone, so 4 and 6 fell through to the
/// glow branch. Measured: slot 2 is `_sk`-suffixed on 3158/3158 FaceTint,
/// 913/1618 SkinTint and 16/10815 HairTint properties in
/// `Skyrim - Meshes0.bsa` — never a glow map on any of them.
#[test]
fn tint_family_routes_slot_2_to_tint_not_glow() {
    for shader_type in [4u32, 5, 6] {
        let blocks: Vec<Box<dyn NiObject>> = vec![
            Box::new(lighting_shader_with_type_and_texset(shader_type, 1)),
            Box::new(full_8_slot_tex_set("head")),
        ];
        let scene = NifScene {
            blocks,
            ..NifScene::default()
        };
        let mut shape = make_tri_shape_with_props(Vec::new());
        shape.shader_property_ref = BlockRef(0);
        let (info, pool) = extract_with_pool(&scene, &shape, &[]);

        assert_path(&pool, info.tint_map, "head_g.dds");
        assert!(
            info.glow_map.is_none(),
            "shader_type {shader_type} is a tint shader — slot 2 is its `_sk` \
             skin-tint mask and must not bind as emissive (#2694)"
        );
    }
}

/// Negative guard for the above: a non-tint shader with Skyrim's explicit
/// `SLSF2_Glow_Map` bit keeps slot 2 as glow.
#[test]
fn non_tint_shader_keeps_slot_2_as_glow() {
    let mut shader = lighting_shader_with_type_and_texset(0, 1);
    shader.shader_flags_2 |= crate::shader_flags::skyrim_slsf2::GLOW_MAP;
    let blocks: Vec<Box<dyn NiObject>> =
        vec![Box::new(shader), Box::new(full_8_slot_tex_set("wall"))];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut shape = make_tri_shape_with_props(Vec::new());
    shape.shader_property_ref = BlockRef(0);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);

    assert_path(&pool, info.glow_map, "wall_g.dds");
    assert!(
        info.tint_map.is_none(),
        "Default (0) is not a tint shader — slot 2 stays the glow map"
    );
}

/// #3068 — slot 2 is multiplexed. Without `SLSF2_Glow_Map`, Skyrim uses it
/// for soft/rim-lighting masks, which have no canonical texture role yet.
#[test]
fn skyrim_non_tint_slot_2_without_glow_flag_is_not_emissive() {
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(lighting_shader_with_type_and_texset(0, 1)),
        Box::new(full_8_slot_tex_set("soft_mask")),
    ];
    let scene = NifScene {
        blocks,
        bsver: crate::version::bsver::SKYRIM_SE,
        ..NifScene::default()
    };
    let mut shape = make_tri_shape_with_props(Vec::new());
    shape.shader_property_ref = BlockRef(0);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);

    assert!(
        info.glow_map.is_none(),
        "Glow_Map-clear Skyrim slot 2 must not become emissive (#3068)"
    );
}

#[test]
fn fo4_slot_3_is_greyscale_lut_and_slot_7_is_specular_without_msn() {
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(lighting_shader_with_type_and_texset(0, 1)),
        Box::new(full_8_slot_tex_set("fo4")),
    ];
    let scene = NifScene {
        blocks,
        bsver: crate::version::bsver::FALLOUT4,
        ..NifScene::default()
    };
    let mut shape = make_tri_shape_with_props(Vec::new());
    shape.shader_property_ref = BlockRef(0);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);

    assert_path(&pool, info.greyscale_lut_map, "fo4_p.dds");
    assert!(
        info.parallax_map.is_none(),
        "FO4 palette gradient must not enter the POM height lane (#2997)"
    );
    assert_path(&pool, info.specular_map, "fo4_7.dds");
    assert!(
        !info.model_space_normals,
        "fixture must prove FO4 specular no longer depends on MSN (#2998)"
    );
}

#[test]
fn fo76_slot_6_reaches_specular_and_hair_kind_is_canonical() {
    let mut shader = lighting_shader_with_type_and_texset(5, 1);
    shader.shader_type_data = ShaderTypeData::HairTint {
        hair_tint_color: [0.3, 0.15, 0.05],
    };
    let blocks: Vec<Box<dyn NiObject>> =
        vec![Box::new(shader), Box::new(full_8_slot_tex_set("fo76"))];
    let scene = NifScene {
        blocks,
        bsver: crate::version::bsver::FO76,
        ..NifScene::default()
    };
    let mut shape = make_tri_shape_with_props(Vec::new());
    shape.shader_property_ref = BlockRef(0);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);

    assert_path(&pool, info.specular_map, "fo76_6.dds");
    assert_eq!(
        info.material_kind,
        slot_role::bs_lighting::HAIR_TINT,
        "FO76 raw type 5 must reach canonical HairTint kind 6 (#2579)"
    );
}

#[test]
fn eye_envmap_keeps_default_slot_4_envmap_routing() {
    // EyeEnvmap (16) — the one variant that legitimately carries
    // the env cube at slot 4. Falls through the default arm of
    // the new shader_type match. Negative guard against a future
    // refactor that drops EyeEnvmap into its own arm and forgets
    // to route slot 4.
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(lighting_shader_with_type_and_texset(16, 1)),
        Box::new(full_8_slot_tex_set("eye")),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut shape = make_tri_shape_with_props(Vec::new());
    shape.shader_property_ref = BlockRef(0);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);

    assert_path(&pool, info.env_map, "eye_4.dds");
    assert_path(&pool, info.env_mask, "eye_5.dds");
    assert!(
        info.tint_map.is_none(),
        "EyeEnvmap doesn't reference slot 7"
    );
    assert!(
        info.inner_layer_map.is_none(),
        "EyeEnvmap doesn't reference slot 7"
    );
    assert!(
        info.detail_map.is_none(),
        "EyeEnvmap doesn't reference the detail slot"
    );
}

/// Regression for #1350 / FO4-D3-04: SkinTint (5) and HairTint (6)
/// enable a tint COLOUR per nif.xml `BSLightingShaderType`, not a
/// texture set slot — they declare no TS slot 4/5. Pre-#1350 these
/// fell into the default arm, which routes slot 4 → `env_map` and
/// slot 5 → `env_mask`. A modded / mis-exported SkinTint NIF with a
/// non-empty slot 4 would therefore spuriously bind an env cubemap.
/// The explicit `5 | 6 =>` arm must skip slots 4/5 entirely.
#[test]
fn skin_tint_and_hair_tint_do_not_bind_slots_4_5_as_envmap() {
    for shader_type in [5u32, 6u32] {
        let blocks: Vec<Box<dyn NiObject>> = vec![
            Box::new(lighting_shader_with_type_and_texset(shader_type, 1)),
            // Slot 4/5 deliberately NON-empty so the pre-fix default-arm
            // misroute would bind them; the fix must skip them.
            Box::new(full_8_slot_tex_set("skin")),
        ];
        let scene = NifScene {
            blocks,
            ..NifScene::default()
        };
        let mut shape = make_tri_shape_with_props(Vec::new());
        shape.shader_property_ref = BlockRef(0);
        let (info, _pool) = extract_with_pool(&scene, &shape, &[]);

        assert!(
            info.env_map.is_none(),
            "shader_type {shader_type} (SkinTint/HairTint) must NOT bind slot 4 as env_map (#1350)"
        );
        assert!(
            info.env_mask.is_none(),
            "shader_type {shader_type} (SkinTint/HairTint) must NOT bind slot 5 as env_mask (#1350)"
        );
        // Base/normal slots (0/1) still flow — the fix only skips 4/5.
        assert!(
            info.texture_path.is_some(),
            "base texture (slot 0) must still bind for SkinTint/HairTint"
        );
    }
}

#[test]
fn skin_tint_routes_slot_2_to_tint_not_glow() {
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(lighting_shader_with_type_and_texset(5, 1)),
        Box::new(full_8_slot_tex_set("skin")),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut shape = make_tri_shape_with_props(Vec::new());
    shape.shader_property_ref = BlockRef(0);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);

    assert_path(&pool, info.tint_map, "skin_g.dds");
    assert!(
        info.glow_map.is_none(),
        "SkinTint slot 2 must not make skin emissive"
    );
}

#[test]
fn fo76_skin_tint_routes_slot_2_to_tint_not_glow() {
    let mut shader = lighting_shader_with_type_and_texset(4, 1);
    shader.shader_type_data = ShaderTypeData::Fo76SkinTint {
        skin_tint_color: [1.0; 4],
    };
    let blocks: Vec<Box<dyn NiObject>> =
        vec![Box::new(shader), Box::new(full_8_slot_tex_set("fo76_skin"))];
    let scene = NifScene {
        blocks,
        bsver: crate::version::bsver::FO76,
        ..NifScene::default()
    };
    let mut shape = make_tri_shape_with_props(Vec::new());
    shape.shader_property_ref = BlockRef(0);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);

    assert_path(&pool, info.tint_map, "fo76_skin_g.dds");
    assert!(info.glow_map.is_none());
}

#[test]
fn model_space_normals_route_slot_7_to_alternate_specular() {
    let mut shader = lighting_shader_with_type_and_texset(0, 1);
    shader.shader_flags_1 = crate::shader_flags::skyrim_slsf1::MODEL_SPACE_NORMALS;
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(shader),
        Box::new(full_8_slot_tex_set("modelspace")),
    ];
    let scene = NifScene {
        blocks,
        bsver: crate::version::bsver::SKYRIM_SE,
        ..NifScene::default()
    };
    let mut shape = make_tri_shape_with_props(Vec::new());
    shape.shader_property_ref = BlockRef(0);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);

    assert!(info.model_space_normals);
    assert_path(&pool, info.specular_map, "modelspace_7.dds");
    assert!(info.gloss_map.is_none());
}

/// Regression for #2742 / REN-D6-2026-08-12-01: SkinTint (5) and
/// HairTint (6) are diverted into their own `5 | 6 =>` arm (#1350, to
/// guard slots 4/5 from a spurious env-cube bind) which reads no slot
/// at or above 3 at all — including slot 7. That silently dropped the
/// same model-space-normals alternate-specular rule the `_ =>` arm
/// applies (pinned above by
/// `model_space_normals_route_slot_7_to_alternate_specular`), for
/// 100% of Skyrim SE body/hands/beast-skin materials (measured
/// 390/390 + 4/4 on real BSA data — every slot-7-bearing SkinTint
/// property is model-space-normal).
#[test]
fn skin_tint_and_hair_tint_route_slot_7_to_alternate_specular_under_msn() {
    for shader_type in [5u32, 6u32] {
        let mut shader = lighting_shader_with_type_and_texset(shader_type, 1);
        shader.shader_flags_1 = crate::shader_flags::skyrim_slsf1::MODEL_SPACE_NORMALS;
        let blocks: Vec<Box<dyn NiObject>> = vec![
            Box::new(shader),
            Box::new(full_8_slot_tex_set("skin_modelspace")),
        ];
        let scene = NifScene {
            blocks,
            bsver: crate::version::bsver::SKYRIM_SE,
            ..NifScene::default()
        };
        let mut shape = make_tri_shape_with_props(Vec::new());
        shape.shader_property_ref = BlockRef(0);
        let (info, pool) = extract_with_pool(&scene, &shape, &[]);

        assert!(
            info.model_space_normals,
            "shader_type {shader_type} must report model_space_normals from SLSF1"
        );
        assert_path(&pool, info.specular_map, "skin_modelspace_7.dds");
        // The #1350 guard (env_map/env_mask skipped for slots 4/5) must
        // still hold — this fix only adds slot 7, it doesn't reopen 4/5.
        assert!(
            info.env_map.is_none(),
            "shader_type {shader_type} must still NOT bind slot 4 as env_map (#1350)"
        );
        assert!(
            info.env_mask.is_none(),
            "shader_type {shader_type} must still NOT bind slot 5 as env_mask (#1350)"
        );
    }
}

/// Regression for #725 / NIF-D4-06: when the legacy
/// `NiTexturingProperty.parallax_texture` slot is bound WITHOUT a
/// co-bound `BSShaderPPLightingProperty` (rare on FO3/FNV with an
/// Oblivion-style property chain), the producer must default
/// `parallax_max_passes` / `parallax_height_scale` to the engine's
/// expected values (4.0 passes / 0.04 scale — same constants the
/// `GpuMaterial::default()` uses at
/// `renderer/src/vulkan/material.rs:216-217` and the consumer-side
/// fallback at `cell_loader.rs:2463`). Pre-fix the scalars stayed
/// `None`, requiring every consumer to repeat the `unwrap_or` —
/// the producer-side default keeps the `Option` semantics honest:
/// "Some = import committed to a value, None = no parallax
/// authoring at all".
#[test]
fn ni_texturing_property_parallax_slot_defaults_scalars_when_no_pp_lighting() {
    use crate::blocks::properties::{NiTexturingProperty, TexDesc};
    use crate::blocks::texture::NiSourceTexture;

    // Block layout:
    //   [0] NiSourceTexture for parallax_texture
    //   [1] NiTexturingProperty with parallax_texture = block 0
    // No BSShaderPPLightingProperty in the chain — the only
    // parallax-authoring source is the NiTexturingProperty slot.
    let parallax_src = NiSourceTexture {
        net: empty_net(),
        use_external: true,
        filename: Some(Arc::from("textures\\stone_p.dds")),
        pixel_data_ref: BlockRef::NULL,
        pixel_layout: 0,
        use_mipmaps: 0,
        alpha_format: 0,
        is_static: true,
    };
    let tex = NiTexturingProperty {
        net: empty_net(),
        flags: 0,
        texture_count: 8,
        base_texture: None,
        dark_texture: None,
        detail_texture: None,
        gloss_texture: None,
        glow_texture: None,
        bump_texture: None,
        normal_texture: None,
        parallax_texture: Some(TexDesc {
            source_ref: BlockRef(0),
            flags: 0,
            transform: None,
        }),
        parallax_offset: 0.0,
        decal_textures: Vec::new(),
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(parallax_src), Box::new(tex)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(1)]);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);

    assert_path(&pool, info.parallax_map, "textures\\stone_p.dds");
    assert_eq!(
        info.parallax_max_passes,
        Some(4.0),
        "NiTexturingProperty parallax slot must default parallax_max_passes to the engine value (4.0) \
         when no BSShaderPPLightingProperty is co-bound — pre-#725 stayed None and relied on \
         consumer-side `unwrap_or` fallbacks",
    );
    assert_eq!(
        info.parallax_height_scale,
        Some(0.04),
        "NiTexturingProperty parallax slot must default parallax_height_scale to the engine value \
         (0.04) when no BSShaderPPLightingProperty is co-bound",
    );
}

#[test]
fn ni_texturing_property_decals_reach_semantic_overlay_slots() {
    use crate::blocks::properties::{NiTexturingProperty, TexDesc};
    use crate::blocks::texture::NiSourceTexture;

    let source = |name: &'static str| NiSourceTexture {
        net: empty_net(),
        use_external: true,
        filename: Some(Arc::from(name)),
        pixel_data_ref: BlockRef::NULL,
        pixel_layout: 0,
        use_mipmaps: 0,
        alpha_format: 0,
        is_static: true,
    };
    let tex = NiTexturingProperty {
        net: empty_net(),
        flags: 0,
        texture_count: 2,
        base_texture: None,
        dark_texture: None,
        detail_texture: None,
        gloss_texture: None,
        glow_texture: None,
        bump_texture: None,
        normal_texture: None,
        parallax_texture: None,
        parallax_offset: 0.0,
        decal_textures: vec![
            TexDesc {
                source_ref: BlockRef(0),
                flags: 0,
                transform: None,
            },
            TexDesc {
                source_ref: BlockRef(1),
                flags: 0,
                transform: None,
            },
        ],
    };
    let scene = NifScene {
        blocks: vec![
            Box::new(source("textures\\decals\\blood.dds")),
            Box::new(source("textures\\decals\\sign.dds")),
            Box::new(tex),
        ],
        ..Default::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(2)]);
    let (info, pool) = extract_with_pool(&scene, &shape, &[]);

    assert_path(&pool, info.decal_maps[0], "textures\\decals\\blood.dds");
    assert_path(&pool, info.decal_maps[1], "textures\\decals\\sign.dds");
    assert!(info.decal_maps[2].is_none());
    assert!(info.decal_maps[3].is_none());
}

/// Sibling: an absent parallax slot must NOT trigger the default
/// — `info.parallax_max_passes` / `parallax_height_scale` stay
/// `None` when no parallax authoring was found anywhere in the
/// property chain. Pins the `Option` semantics: defaults fire only
/// when the slot is actually bound.
#[test]
fn ni_texturing_property_without_parallax_slot_leaves_scalars_none() {
    use crate::blocks::properties::NiTexturingProperty;

    let tex = NiTexturingProperty {
        net: empty_net(),
        flags: 0,
        texture_count: 0,
        base_texture: None,
        dark_texture: None,
        detail_texture: None,
        gloss_texture: None,
        glow_texture: None,
        bump_texture: None,
        normal_texture: None,
        parallax_texture: None,
        parallax_offset: 0.0,
        decal_textures: Vec::new(),
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(tex)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);

    assert!(info.parallax_map.is_none());
    assert!(
        info.parallax_max_passes.is_none(),
        "absent parallax slot must NOT trigger the engine default — stays None",
    );
    assert!(info.parallax_height_scale.is_none());
}
