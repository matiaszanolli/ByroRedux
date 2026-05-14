//! Tests for `texture_slot_3_4_5_tests` extracted from ../material.rs (refactor stage A).
//!
//! Same qualified path preserved (`texture_slot_3_4_5_tests::FOO`).

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::node::NiNode;
use crate::blocks::properties::NiTexturingProperty;
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

fn fo3_pp_lighting_with_texture_set(tex_set_idx: u32) -> BSShaderPPLightingProperty {
    use crate::blocks::base::BSShaderPropertyData;
    BSShaderPPLightingProperty {
        net: empty_net(),
        shader: BSShaderPropertyData {
            shade_flags: 0,
            shader_type: 7, // Parallax_Occlusion
            shader_flags_1: 0,
            shader_flags_2: 0,
            env_map_scale: 0.5,
        },
        texture_clamp_mode: 0,
        texture_set_ref: BlockRef(tex_set_idx),
        refraction_strength: 0.0,
        refraction_fire_period: 0,
        parallax_max_passes: 4.0,
        parallax_scale: 0.04,
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
        Box::new(fo3_pp_lighting_with_texture_set(2)),
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
            shader_flags_1: 0,
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
fn pp_lighting_with_only_3_slots_leaves_parallax_and_env_none() {
    // Old-style texture set with just base/normal/glow — parallax
    // slots stay None so downstream consumers (FO3-REN-M2) skip
    // the parallax branch cleanly.
    let tex_set = BSShaderTextureSet {
        textures: vec![
            "textures\\wall_d.dds".to_string(),
            "textures\\wall_n.dds".to_string(),
            "textures\\wall_g.dds".to_string(),
        ],
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(fo3_pp_lighting_with_texture_set(1)),
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

// Keep `NiTexturingProperty` imports working — referenced by the
// outer test module via `use super::*`. Otherwise clippy complains.
#[allow(dead_code)]
fn _uses_ni_texturing_property() -> NiTexturingProperty {
    panic!()
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
//   * MultiLayerParallax (11) — slot 4 = Env, slot 7 = inner Layer.
//   * EyeEnvmap (16)          — slot 4 = Env (default arm).
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
    }
}

fn full_8_slot_tex_set(tag: &str) -> BSShaderTextureSet {
    // 8 populated slots so the routing fix can exercise slot 7
    // alongside the legacy slots 0..=5. Slot 6 stays empty —
    // nif.xml doesn't reference it on FaceTint or MultiLayerParallax,
    // so feeding a value would just confuse the assertions.
    BSShaderTextureSet {
        textures: vec![
            format!("{tag}_d.dds"),
            format!("{tag}_n.dds"),
            format!("{tag}_g.dds"),
            format!("{tag}_p.dds"),
            format!("{tag}_4.dds"),
            format!("{tag}_5.dds"),
            String::new(),
            format!("{tag}_7.dds"),
        ],
    }
}

#[test]
fn face_tint_routes_slot_4_to_detail_not_envmap() {
    // FaceTint (4) — slot 4 must land in `detail_map`, NOT
    // `env_map`. Pre-#563 the slot was bound as an env cubemap,
    // visibly corrupting every NPC face once SK-D5-02 lands.
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

    assert_path(&pool, info.detail_map, "face_4.dds");
    assert!(
        info.env_map.is_none(),
        "FaceTint slot 4 must NOT be misbound as an env cubemap (#563)"
    );
    assert!(
        info.env_mask.is_none(),
        "FaceTint has no slot 5 binding either"
    );
    assert_path(&pool, info.tint_map, "face_7.dds");
    assert!(
        info.inner_layer_map.is_none(),
        "FaceTint slot 7 routes to tint_map, not inner_layer_map"
    );
}

#[test]
fn multi_layer_parallax_routes_slot_7_to_inner_layer_alongside_env() {
    // MultiLayerParallax (11) — slot 4 stays the env cube
    // (paired with `multi_layer_envmap_strength`), slot 5 the
    // env mask, and slot 7 must now land in `inner_layer_map`.
    // Pre-#563 the slot was silently dropped, leaving Dragonborn
    // ice walls and modded glass shaders without their inner
    // layer.
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
    assert_path(&pool, info.inner_layer_map, "ice_7.dds");
    assert!(
        info.tint_map.is_none(),
        "MultiLayerParallax slot 7 routes to inner_layer_map, not tint_map"
    );
    assert!(
        info.detail_map.is_none(),
        "MultiLayerParallax has no detail-slot route — slot 4 stays env"
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
