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
