//! Regression tests for #1243 (NIF-DIM4-NEW-02) — FO3/FNV legacy
//! `WaterShaderProperty` (non-BS variant) must reach `MaterialInfo`.
//!
//! Pre-fix the parser landed `WaterShaderProperty` cleanly via its
//! dedicated arm at `blocks/mod.rs` (added in #474 to stop the over-read
//! against `BSShaderPPLightingProperty::parse`), but `walker.rs` had no
//! `scene.get_as::<WaterShaderProperty>(idx)` site — `env_map_scale`
//! never reached `MaterialInfo`. The #940 wire-up pass that consumed
//! Tile / Sky / TallGrass shaders omitted Water on the (stale)
//! reasoning that "BSShaderProperty base data isn't yet plumbed into
//! MaterialInfo" — but those same Tile / Sky / TallGrass branches
//! reach exactly the same `shader.shader.env_map_scale`. Same defect
//! class as the BSEffectShaderProperty pre-#345 path.
//!
//! The Skyrim+ sibling `BSWaterShaderProperty` is exercised by
//! `sky_water_shader_tests` (#977 closure); this file covers the
//! FO3/FNV non-BS counterpart.

use super::*;
use crate::blocks::base::{BSShaderPropertyData, NiAVObjectData, NiObjectNETData};
use crate::blocks::shader::WaterShaderProperty;
use crate::blocks::tri_shape::NiTriShape;
use crate::blocks::NiObject;
use crate::types::{BlockRef, NiTransform};
use byroredux_core::string::StringPool;

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

/// FO3/FNV walker iterates `shape.av.properties`, not
/// `shader_property_ref`. `WaterShaderProperty` is a FO3/FNV-era
/// `NiProperty` subclass so it binds via the property list, mirroring
/// the Tile / Sky / TallGrass branch shape.
fn shape_with_property_ref(block_idx: u32) -> NiTriShape {
    NiTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: NiTransform::default(),
            properties: vec![BlockRef(block_idx)],
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

fn water_shader_with_env_scale(env_map_scale: f32) -> WaterShaderProperty {
    WaterShaderProperty {
        net: empty_net(),
        shader: BSShaderPropertyData {
            shade_flags: 0,
            shader_type: 0,
            shader_flags_1: 0,
            shader_flags_2: 0,
            env_map_scale,
        },
    }
}

/// Headline regression: a FO3/FNV mesh-driven water plane that previously
/// imported with `env_map_scale = 0.0` (the MaterialInfo default) now
/// receives the authored value through the new consumer.
#[test]
fn water_shader_property_routes_env_map_scale_to_material_info() {
    let shader = water_shader_with_env_scale(0.85);
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = shape_with_property_ref(0);

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    assert_eq!(
        info.env_map_scale, 0.85,
        "pre-#1243: WaterShaderProperty.shader.env_map_scale was parsed \
         cleanly but the importer had no consumer — every mesh-driven \
         FO3/FNV water plane lost its authored env reflection contribution"
    );
}

/// A FO3/FNV mesh with no shader-property bound must keep the
/// `MaterialInfo::default()` env_map_scale (0.0 — no env reflection).
/// Guards against a future refactor that always overwrites the field.
#[test]
fn no_water_shader_property_keeps_default_env_map_scale() {
    let blocks: Vec<Box<dyn NiObject>> = Vec::new();
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut shape = shape_with_property_ref(0);
    shape.av.properties.clear();

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    assert_eq!(info.env_map_scale, MaterialInfo::default().env_map_scale);
}
