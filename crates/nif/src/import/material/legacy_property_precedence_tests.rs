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
