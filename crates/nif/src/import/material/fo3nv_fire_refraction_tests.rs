//! Regression tests for #2321 (FO3-D1-05/D2-01) — FO3/FNV
//! `BSShaderPPLightingProperty` fire-refraction heat-haze must reach
//! `MaterialInfo` the same way the Skyrim+ `BSLightingShaderProperty`
//! path already does (see
//! `lighting_shader_pbr_tests::skyrim_fire_refraction_flags_select_engine_material_kind`).
//!
//! Pre-fix, `blocks/shader.rs` decoded `refraction_strength` /
//! `refraction_fire_period` for every FO3/FNV `BSShaderPPLightingProperty`
//! (bsver > `FO3_REFRACTION`), but `apply_pp_lighting_property` never wrote
//! `refraction_strength` into `MaterialInfo`, and the only site promoting
//! `material_kind = 103` tested Skyrim-only SLSF1 bits with no FO3/FNV
//! equivalent declared in `fo3nv_f1` — so FO3/FNV fire/explosion/plasma
//! heat-haze proxies rendered as flat opaque lit slabs.

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

fn fo3_pp_lighting_with_flags(
    shader_flags_1: u32,
    refraction_strength: f32,
) -> BSShaderPPLightingProperty {
    BSShaderPPLightingProperty {
        net: empty_net(),
        shader: BSShaderPropertyData {
            shade_flags: 0,
            shader_type: 1, // NoLighting-adjacent stub, irrelevant to this path
            shader_flags_1,
            shader_flags_2: 0,
            env_map_scale: 0.0,
        },
        texture_clamp_mode: 0,
        texture_set_ref: BlockRef::NULL,
        refraction_strength,
        refraction_fire_period: 30,
        parallax_max_passes: 4.0,
        parallax_scale: 0.04,
        emissive_color: [0.0, 0.0, 0.0, 1.0],
    }
}

fn extract_with_pool(
    scene: &NifScene,
    shape: &NiTriShape,
    inherited: &[BlockRef],
) -> (MaterialInfo, StringPool) {
    let mut pool = StringPool::new();
    let info = extract_material_info(scene, shape, inherited, &mut pool);
    (info, pool)
}

#[test]
fn fo3nv_fire_refraction_flags_select_engine_material_kind() {
    let shader = fo3_pp_lighting_with_flags(
        crate::shader_flags::fo3nv_f1::REFRACTION | crate::shader_flags::fo3nv_f1::FIRE_REFRACTION,
        0.35,
    );
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);

    assert_eq!(
        info.material_kind, 103,
        "Refraction + Fire_Refraction must select the renderer's fire-haze kind on FO3/FNV"
    );
    assert_eq!(info.refraction_strength, 0.35);
    assert!(
        info.alpha_blend,
        "fire haze is a transparent composition proxy"
    );
    assert_eq!(info.src_blend_mode, 6);
    assert_eq!(info.dst_blend_mode, 7);
    assert!(!info.z_write, "fire haze must not hide the flame cards");
}

#[test]
fn fo3nv_refraction_without_fire_bit_does_not_promote_material_kind() {
    // Refraction alone (no Fire_Refraction) must not trigger the
    // heat-haze promotion — the pair is the discriminator, not either
    // bit alone, matching the Skyrim+ path's own gate.
    let shader = fo3_pp_lighting_with_flags(crate::shader_flags::fo3nv_f1::REFRACTION, 0.20);
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);

    assert_ne!(info.material_kind, 103);
    // The scalar still mirrors even when the promotion doesn't fire —
    // BSRefractionStrengthController can animate it independently.
    assert_eq!(info.refraction_strength, 0.20);
}

#[test]
fn fo3nv_no_refraction_flags_leaves_refraction_strength_at_parsed_default() {
    let shader = fo3_pp_lighting_with_flags(0, 0.0);
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let (info, _pool) = extract_with_pool(&scene, &shape, &[]);

    assert_ne!(info.material_kind, 103);
    assert_eq!(info.refraction_strength, 0.0);
}
