//! Regression tests for #2328 (FO3-D1-06) — inherited-property
//! precedence inversion. `apply_legacy_property_chain`'s documented
//! intent is "shape properties first so they take priority" (#208),
//! but `texture_clamp_mode`/`env_map_scale` used a bare `=` in every
//! FO3/FNV shader branch — an inherited parent-NiNode property
//! silently overwrote the shape's own authored value, the opposite of
//! the stated rule.

use super::*;
use crate::blocks::base::{BSShaderPropertyData, NiAVObjectData, NiObjectNETData};
use crate::blocks::shader::BSShaderPPLightingProperty;
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

fn pp_lighting_with_clamp_and_env(
    texture_clamp_mode: u32,
    env_map_scale: f32,
) -> BSShaderPPLightingProperty {
    pp_lighting_with(texture_clamp_mode, env_map_scale, 0.0)
}

fn pp_lighting_with(
    texture_clamp_mode: u32,
    env_map_scale: f32,
    refraction_strength: f32,
) -> BSShaderPPLightingProperty {
    BSShaderPPLightingProperty {
        net: empty_net(),
        shader: BSShaderPropertyData {
            shade_flags: 0,
            shader_type: 1,
            // ENVIRONMENT_MAPPING so `legacy_env_map_scale` doesn't
            // zero the authored scale out from under the precedence
            // check itself.
            shader_flags_1: crate::shader_flags::fo3nv_f1::ENVIRONMENT_MAPPING,
            shader_flags_2: 0,
            env_map_scale,
        },
        texture_clamp_mode,
        texture_set_ref: BlockRef::NULL,
        refraction_strength,
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
            transform: NiTransform::default(),
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
fn shape_own_texture_clamp_mode_and_env_map_scale_survive_inherited_property() {
    // Scene layout:
    //   [0] BSShaderPPLightingProperty — the SHAPE's own direct property.
    //       texture_clamp_mode = 1 (CLAMP_S_WRAP_T), env_map_scale = 2.0.
    //   [1] BSShaderPPLightingProperty — an INHERITED parent-NiNode property.
    //       texture_clamp_mode = 2 (WRAP_S_CLAMP_T), env_map_scale = 5.0.
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(pp_lighting_with_clamp_and_env(1, 2.0)),
        Box::new(pp_lighting_with_clamp_and_env(2, 5.0)),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let inherited = [BlockRef(1)];

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &inherited, &mut pool);

    assert_eq!(
        info.texture_clamp_mode, 1,
        "the shape's own direct texture_clamp_mode must win over an \
         inherited parent NiNode property (#208 precedence)"
    );
    assert_eq!(
        info.env_map_scale, 2.0,
        "the shape's own direct env_map_scale must win over an \
         inherited parent NiNode property (#208 precedence)"
    );
}

#[test]
fn inherited_property_still_fills_gap_when_shape_has_none() {
    // No direct properties at all — the inherited parent property is
    // the ONLY source, so it must still apply (the `_consumed` gate
    // must not suppress the fallback case, only the override case).
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(pp_lighting_with_clamp_and_env(2, 5.0))];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![]);
    let inherited = [BlockRef(0)];

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &inherited, &mut pool);

    assert_eq!(info.texture_clamp_mode, 2);
    assert_eq!(info.env_map_scale, 5.0);
}

/// #3514 (FO3-2026-08-27-D1-01) — `refraction_strength` was the one write
/// in `apply_pp_lighting_property` that #2328 left as a bare `=` inside
/// the direct-then-inherited walk, two statements below the two it
/// converted. Same scene shape as the sibling test above.
#[test]
fn shape_own_refraction_strength_survives_inherited_property() {
    let blocks: Vec<Box<dyn NiObject>> = vec![
        // [0] the shape's own direct property.
        Box::new(pp_lighting_with(1, 2.0, 0.25)),
        // [1] an inherited parent-NiNode property.
        Box::new(pp_lighting_with(2, 5.0, 0.75)),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let inherited = [BlockRef(1)];

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &inherited, &mut pool);

    assert_eq!(
        info.refraction_strength, 0.25,
        "the shape's own direct refraction_strength must win over an \
         inherited parent NiNode property (#208 precedence)"
    );
}

/// …and the gate must not suppress the fallback case: with no direct
/// property, the inherited one is the only source and must still apply.
#[test]
fn inherited_refraction_strength_still_fills_the_gap() {
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(pp_lighting_with(2, 5.0, 0.75))];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![]);
    let inherited = [BlockRef(0)];

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &inherited, &mut pool);

    assert_eq!(info.refraction_strength, 0.75);
}

/// #3517 (OBL-2026-08-27-02) — the `NiTexturingProperty` clamp writer used
/// to gate on the *value* (`info.texture_clamp_mode == 3`) rather than on
/// the `_consumed` latch its four `BSShader*` siblings read and set. Both
/// directions of the resulting precedence inversion are pinned here.
///
/// Direction 1: a shape-level `NiTexturingProperty` must latch, so an
/// inherited `BSShaderPPLightingProperty` cannot overwrite it.
#[test]
fn shape_texturing_property_clamp_survives_inherited_bsshader() {
    use crate::blocks::properties::{NiTexturingProperty, TexDesc};

    let texturing = NiTexturingProperty {
        net: empty_net(),
        flags: 0,
        apply_mode: 2,
        texture_count: 1,
        base_texture: Some(TexDesc {
            source_ref: BlockRef::NULL,
            flags: 0,
            // CLAMP_S_WRAP_T — an authored non-default the inherited
            // property must not clobber.
            clamp_mode: 1,
            transform: None,
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
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(texturing),
        Box::new(pp_lighting_with_clamp_and_env(2, 5.0)),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let inherited = [BlockRef(1)];

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &inherited, &mut pool);

    assert_eq!(
        info.texture_clamp_mode, 1,
        "the shape's own NiTexturingProperty clamp mode must win over an \
         inherited BSShaderPPLightingProperty (#208 / #3517)"
    );
}

/// Direction 2: a shape-level `BSShader*` authoring the *default* clamp
/// mode 3 must still latch, so an inherited `NiTexturingProperty` cannot
/// overwrite it. The old `== 3` value gate could not see the difference
/// between "authored WRAP" and "nobody wrote anything".
#[test]
fn shape_bsshader_clamp_of_three_survives_inherited_texturing_property() {
    use crate::blocks::properties::{NiTexturingProperty, TexDesc};

    let texturing = NiTexturingProperty {
        net: empty_net(),
        flags: 0,
        apply_mode: 2,
        texture_count: 1,
        base_texture: Some(TexDesc {
            source_ref: BlockRef::NULL,
            flags: 0,
            clamp_mode: 0, // CLAMP_S_CLAMP_T
            transform: None,
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
    let blocks: Vec<Box<dyn NiObject>> = vec![
        // [0] the shape's own BSShader property, authoring WRAP/WRAP.
        Box::new(pp_lighting_with_clamp_and_env(3, 2.0)),
        // [1] an inherited NiTexturingProperty authoring CLAMP/CLAMP.
        Box::new(texturing),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let inherited = [BlockRef(1)];

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &inherited, &mut pool);

    assert_eq!(
        info.texture_clamp_mode, 3,
        "an authored default (3 = WRAP_S_WRAP_T) is still authored — the \
         old value-shape gate read it as 'nobody wrote anything' (#3517)"
    );
}

/// The fallback direction for the same writer: with no shape-level
/// property, an inherited `NiTexturingProperty` is the only source and
/// must apply.
#[test]
fn inherited_texturing_property_clamp_still_fills_the_gap() {
    use crate::blocks::properties::{NiTexturingProperty, TexDesc};

    let texturing = NiTexturingProperty {
        net: empty_net(),
        flags: 0,
        apply_mode: 2,
        texture_count: 1,
        base_texture: Some(TexDesc {
            source_ref: BlockRef::NULL,
            flags: 0,
            clamp_mode: 2, // WRAP_S_CLAMP_T
            transform: None,
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
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(texturing)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![]);
    let inherited = [BlockRef(0)];

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &inherited, &mut pool);

    assert_eq!(info.texture_clamp_mode, 2);
}

fn source_texture(path: &str) -> crate::blocks::texture::NiSourceTexture {
    crate::blocks::texture::NiSourceTexture {
        net: empty_net(),
        use_external: true,
        filename: Some(Arc::from(path)),
        pixel_data_ref: BlockRef::NULL,
        pixel_layout: 0,
        use_mipmaps: 0,
        alpha_format: 0,
        is_static: false,
    }
}

/// `NiTexturingProperty` binding blocks `[0]` (base) and `[1]` (normal).
fn texturing_with_base_and_normal() -> crate::blocks::properties::NiTexturingProperty {
    use crate::blocks::properties::{NiTexturingProperty, TexDesc};
    let desc = |source: u32| TexDesc {
        source_ref: BlockRef(source),
        flags: 0,
        clamp_mode: 3,
        transform: None,
    };
    NiTexturingProperty {
        net: empty_net(),
        flags: 0,
        apply_mode: 2,
        texture_count: 7,
        base_texture: Some(desc(0)),
        dark_texture: None,
        detail_texture: None,
        gloss_texture: None,
        glow_texture: None,
        bump_texture: None,
        normal_texture: Some(desc(1)),
        parallax_texture: None,
        parallax_offset: 0.0,
        decal_textures: Vec::new(),
    }
}

/// Scene for the #4235 tests: legacy texture sources at `[0]`/`[1]`, the
/// `NiTexturingProperty` at `[2]`, the texture set at `[3]`, and a
/// `BSShaderPPLightingProperty` bound to it at `[4]`.
fn scene_with_both_texture_families(texture_set: Vec<String>) -> NifScene {
    let mut pp = pp_lighting_with_clamp_and_env(3, 1.0);
    pp.texture_set_ref = BlockRef(3);
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(source_texture("textures\\legacy_d.dds")),
        Box::new(source_texture("textures\\legacy_n.dds")),
        Box::new(texturing_with_base_and_normal()),
        Box::new(crate::blocks::shader::BSShaderTextureSet {
            textures: texture_set,
        }),
        Box::new(pp),
    ];
    NifScene {
        blocks,
        ..NifScene::default()
    }
}

/// #4235 (FO3-D1-2026-09-11-01) — a `NiTexturingProperty` listed ahead of
/// the shape's `BSShaderPPLightingProperty` used to keep base and normal,
/// discarding the texture set the bound shader samples. The texture set
/// must win both roles.
#[test]
fn texture_set_outranks_an_earlier_texturing_property() {
    let scene = scene_with_both_texture_families(vec![
        "textures\\set_d.dds".to_string(),
        "textures\\set_n.dds".to_string(),
    ]);
    let shape = make_tri_shape_with_props(vec![BlockRef(2), BlockRef(4)]);

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    assert_eq!(
        info.texture_path,
        intern_texture_path(&mut pool, "textures\\set_d.dds"),
        "the shader's own base texture must replace the texturing property's"
    );
    assert_eq!(
        info.normal_map,
        intern_texture_path(&mut pool, "textures\\set_n.dds"),
        "the shader's own normal map must replace the texturing property's"
    );
}

/// …but an empty texture set has nothing to offer, so the legacy paths
/// stay bound rather than being cleared.
#[test]
fn empty_texture_set_keeps_the_texturing_property_paths() {
    let scene = scene_with_both_texture_families(Vec::new());
    let shape = make_tri_shape_with_props(vec![BlockRef(2), BlockRef(4)]);

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    assert_eq!(
        info.texture_path,
        intern_texture_path(&mut pool, "textures\\legacy_d.dds")
    );
    assert_eq!(
        info.normal_map,
        intern_texture_path(&mut pool, "textures\\legacy_n.dds")
    );
}

/// #4401 (NIFAL-D8-2026-09-14-03) — the #4235 displacement fixed the base
/// *path* but left `texture_clamp_mode` latched to whichever property ran
/// first: a `NiTexturingProperty` ahead of the shader in the chain set the
/// latch, and the shader's own clamp never ran. When the shader's texture
/// displaces the legacy base path, its address mode must follow it.
#[test]
fn displacing_the_base_path_re_latches_the_clamp_mode() {
    use crate::blocks::properties::{NiTexturingProperty, TexDesc};

    let desc = |source: u32, clamp: u8| TexDesc {
        source_ref: BlockRef(source),
        flags: 0,
        clamp_mode: clamp,
        transform: None,
    };
    let texturing = NiTexturingProperty {
        net: empty_net(),
        flags: 0,
        apply_mode: 2,
        texture_count: 7,
        // Legacy base with CLAMP_S_WRAP_T (1) — the latch pre-#4401 kept.
        base_texture: Some(desc(0, 1)),
        dark_texture: None,
        detail_texture: None,
        gloss_texture: None,
        glow_texture: None,
        bump_texture: None,
        normal_texture: Some(desc(1, 3)),
        parallax_texture: None,
        parallax_offset: 0.0,
        decal_textures: Vec::new(),
    };
    let mut pp = pp_lighting_with_clamp_and_env(2, 1.0);
    pp.texture_set_ref = BlockRef(3);
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(source_texture("textures\\legacy_d.dds")),
        Box::new(source_texture("textures\\legacy_n.dds")),
        Box::new(texturing),
        Box::new(crate::blocks::shader::BSShaderTextureSet {
            textures: vec![
                "textures\\set_d.dds".to_string(),
                "textures\\set_n.dds".to_string(),
            ],
        }),
        Box::new(pp),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    // Texturing property FIRST — the displacement direction.
    let shape = make_tri_shape_with_props(vec![BlockRef(2), BlockRef(4)]);

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    assert_eq!(
        info.texture_path,
        intern_texture_path(&mut pool, "textures\\set_d.dds"),
        "fixture sanity: the shader's base texture must displace the legacy path"
    );
    assert_eq!(
        info.texture_clamp_mode, 2,
        "the clamp latch must follow the texture that won the slot — the \
         shader's own WRAP_S_CLAMP_T (2), not the displaced legacy \
         TexDesc's CLAMP_S_WRAP_T (1) (#4401)"
    );
}

/// #4401 — the parallax role had the same split as base (two independent
/// first-writer latches), plus a scalar half: the `NiTexturingProperty`
/// slot-7 branch installs generic engine defaults
/// (`DEFAULT_PARALLAX_MAX_PASSES` / `DEFAULT_PARALLAX_HEIGHT_SCALE`), which
/// an authored-POM shader's own converted pair must be able to win when its
/// slot-3 texture displaces the slot-7 path.
#[test]
fn authored_pom_displaces_a_texturing_property_height_map_and_its_default_scalars() {
    use crate::blocks::properties::{NiTexturingProperty, TexDesc};

    let desc = |source: u32, clamp: u8| TexDesc {
        source_ref: BlockRef(source),
        flags: 0,
        clamp_mode: clamp,
        transform: None,
    };
    let texturing = NiTexturingProperty {
        net: empty_net(),
        flags: 0,
        apply_mode: 2,
        texture_count: 7,
        base_texture: Some(desc(0, 3)),
        dark_texture: None,
        detail_texture: None,
        gloss_texture: None,
        glow_texture: None,
        bump_texture: None,
        normal_texture: Some(desc(1, 3)),
        // Slot-7 height map — the legacy path that used to win by chain
        // order and pin the role plus its default scalar pair.
        parallax_texture: Some(desc(5, 3)),
        parallax_offset: 0.0,
        decal_textures: Vec::new(),
    };
    let mut pp = pp_lighting_with_clamp_and_env(3, 1.0);
    pp.texture_set_ref = BlockRef(3);
    // Authored POM (bits 11/28 gate, FO3-D1-02 / #2317) with an authored
    // scalar pair distinct from the generic defaults the slot-7 branch
    // installs: 7.0 passes ≠ 4.0, and scale 2.0 → 0.08 ≠ 0.04.
    pp.shader.shader_flags_1 = crate::shader_flags::fo3nv_f1::PARALLAX;
    pp.parallax_max_passes = 7.0;
    pp.parallax_scale = 2.0;
    // Blocks: [0]/[1] legacy sources, [2] texturing property (base→0,
    // normal→1, parallax slot-7→5), [3] texture set, [4] the shader (both
    // are shape-direct chain properties), [5] the slot-7 source.
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(source_texture("textures\\legacy_d.dds")),
        Box::new(source_texture("textures\\legacy_n.dds")),
        Box::new(texturing),
        Box::new(crate::blocks::shader::BSShaderTextureSet {
            textures: vec![
                "textures\\set_d.dds".to_string(),
                "textures\\set_n.dds".to_string(),
                String::new(),
                "textures\\set_height.dds".to_string(),
            ],
        }),
        Box::new(pp),
        Box::new(source_texture("textures\\legacy_height.dds")),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    // Texturing property FIRST — the displacement direction.
    let shape = make_tri_shape_with_props(vec![BlockRef(2), BlockRef(4)]);

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    assert_eq!(
        info.parallax_map,
        intern_texture_path(&mut pool, "textures\\set_height.dds"),
        "an authored-POM shader's slot-3 height map must displace the \
         texturing property's slot-7 path (#4401)"
    );
    assert_eq!(info.parallax_max_passes, Some(7.0));
    assert_eq!(
        info.parallax_height_scale,
        Some(0.08),
        "the shader's authored scale (2.0 → 0.08) must win over the generic \
         0.04 default the displaced slot-7 branch installed (#4401)"
    );
}

/// …and the control: a shader that does NOT author POM leaves the legacy
/// slot-7 path and its default scalar pair alone — the FO3-D1-02/#2317
/// gate must stay in front of the displacement claim.
#[test]
fn unauthored_pom_keeps_the_texturing_property_height_map() {
    use crate::blocks::properties::{NiTexturingProperty, TexDesc};

    let desc = |source: u32| TexDesc {
        source_ref: BlockRef(source),
        flags: 0,
        clamp_mode: 3,
        transform: None,
    };
    let texturing = NiTexturingProperty {
        net: empty_net(),
        flags: 0,
        apply_mode: 2,
        texture_count: 7,
        base_texture: Some(desc(0)),
        dark_texture: None,
        detail_texture: None,
        gloss_texture: None,
        glow_texture: None,
        bump_texture: None,
        normal_texture: Some(desc(1)),
        parallax_texture: Some(desc(5)),
        parallax_offset: 0.0,
        decal_textures: Vec::new(),
    };
    let mut pp = pp_lighting_with_clamp_and_env(3, 1.0);
    pp.texture_set_ref = BlockRef(3);
    // No PARALLAX/PARALLAX_OCCLUSION bit — slot 3 is present but the
    // material does not author POM.
    // Same block layout as the displacement test above: [5] is the
    // slot-7 source the texturing property binds.
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(source_texture("textures\\legacy_d.dds")),
        Box::new(source_texture("textures\\legacy_n.dds")),
        Box::new(texturing),
        Box::new(crate::blocks::shader::BSShaderTextureSet {
            textures: vec![
                "textures\\set_d.dds".to_string(),
                "textures\\set_n.dds".to_string(),
                String::new(),
                "textures\\set_height.dds".to_string(),
            ],
        }),
        Box::new(pp),
        Box::new(source_texture("textures\\legacy_height.dds")),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(2), BlockRef(4)]);

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    // Fixture sanity: the shader must have applied (its base displaced
    // the legacy path), so the control below is not vacuous.
    assert_eq!(
        info.texture_path,
        intern_texture_path(&mut pool, "textures\\set_d.dds"),
        "fixture sanity: the shader's base claim must still run"
    );
    assert_eq!(
        info.parallax_map,
        intern_texture_path(&mut pool, "textures\\legacy_height.dds"),
        "without authored POM the slot-3 texture must not displace the \
         legacy slot-7 height path (#2317 gate in front of the claim)"
    );
    assert_eq!(
        info.parallax_max_passes,
        Some(byroredux_core::ecs::components::material::DEFAULT_PARALLAX_MAX_PASSES),
        "the slot-7 branch's default pair stays"
    );
    assert_eq!(
        info.parallax_height_scale,
        Some(byroredux_core::ecs::components::material::DEFAULT_PARALLAX_HEIGHT_SCALE)
    );
}
