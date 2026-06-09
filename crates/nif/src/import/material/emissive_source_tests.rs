//! Pin the `EmissiveSource` discriminator at each of the three NIF
//! shader-property set-sites in walker.rs. #1280 step 4 — canonical
//! material convergence.
//!
//! The three sites all flow into the same `MaterialInfo.emissive_mult`
//! slot but carry different semantics:
//!
//! | NIF property class          | Set-site         | Tag              |
//! |-----------------------------|------------------|------------------|
//! | `BSLightingShaderProperty`  | walker.rs:~292   | `Lighting`       |
//! | `BSEffectShaderProperty`    | walker.rs:~347   | `Effect` (tint!) |
//! | `NiMaterialProperty`        | walker.rs:~578   | `Material`       |
//!
//! `BSEffectShaderProperty.base_color_scale` is semantically a diffuse-
//! tint multiplier (see #166), conflated into the emissive slot for the
//! current fragment-shader path. The discriminator makes the conflation
//! type-visible so a future BSEffect-proper render path can branch on it.

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::properties::NiMaterialProperty;
use crate::blocks::shader::{BSEffectShaderProperty, BSLightingShaderProperty, ShaderTypeData};
use crate::blocks::tri_shape::NiTriShape;
use crate::blocks::NiObject;
use crate::types::{BlockRef, NiColor, NiTransform};
use byroredux_core::ecs::components::material::EmissiveSource;
use byroredux_core::string::StringPool;
use std::sync::Arc;

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

fn tri_shape_with_shader_ref(idx: u32) -> NiTriShape {
    NiTriShape {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("TestMesh")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef::NULL,
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef(idx),
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    }
}

/// Variant of `tri_shape_with_shader_ref` that exposes a legacy
/// property cascade (NiMaterialProperty, NiAlphaProperty, …) via the
/// `properties` list on NiAVObject — used by the FO3/FNV/Oblivion path.
fn tri_shape_with_property_list(prop_indices: &[u32]) -> NiTriShape {
    NiTriShape {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("LegacyMesh")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: prop_indices.iter().copied().map(BlockRef).collect(),
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

fn minimal_bslighting() -> BSLightingShaderProperty {
    BSLightingShaderProperty {
        shader_type: 0,
        net: empty_net(),
        material_reference: false,
        shader_flags_1: 0,
        shader_flags_2: 0,
        sf1_crcs: Vec::new(),
        sf2_crcs: Vec::new(),
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        texture_set_ref: BlockRef::NULL,
        emissive_color: [0.5, 0.5, 0.5],
        emissive_multiple: 1.25, // distinctive non-default value
        root_material_path: None,
        texture_clamp_mode: 3,
        alpha: 1.0,
        refraction_strength: 0.0,
        glossiness: 80.0,
        specular_color: [1.0, 1.0, 1.0],
        specular_strength: 1.0,
        lighting_effect_1: 0.0,
        lighting_effect_2: 0.0,
        subsurface_rolloff: 0.0,
        rimlight_power: 0.0,
        backlight_power: 0.0,
        grayscale_to_palette_scale: 0.0,
        fresnel_power: 5.0,
        wetness: None,
        luminance: None,
        do_translucency: false,
        translucency: None,
        texture_arrays: Vec::new(),
        shader_type_data: ShaderTypeData::None,
    }
}

fn minimal_bseffect() -> BSEffectShaderProperty {
    BSEffectShaderProperty {
        net: empty_net(),
        material_reference: false,
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
        falloff_stop_angle: 0.0,
        falloff_start_opacity: 1.0,
        falloff_stop_opacity: 0.0,
        refraction_power: 0.0,
        base_color: [1.0, 0.5, 0.25, 1.0],
        base_color_scale: 2.5, // distinctive — flows into emissive_mult
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

fn minimal_nimaterial() -> NiMaterialProperty {
    NiMaterialProperty {
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
            r: 1.0,
            g: 1.0,
            b: 1.0,
        },
        emissive: NiColor {
            r: 0.2,
            g: 0.4,
            b: 0.6,
        },
        shininess: 40.0,
        alpha: 1.0,
        emissive_mult: 1.75, // distinctive non-default
    }
}

#[test]
fn bslighting_tags_emissive_source_as_lighting() {
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(minimal_bslighting())];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = tri_shape_with_shader_ref(0);
    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);
    assert_eq!(
        info.emissive_source,
        EmissiveSource::Lighting,
        "BSLightingShaderProperty must tag EmissiveSource::Lighting"
    );
    assert!(
        (info.emissive_mult - 1.25).abs() < 1e-5,
        "emissive_mult must come from shader.emissive_multiple"
    );
}

#[test]
fn bseffect_tags_emissive_source_as_effect() {
    // The critical no-cross-vocabulary case: BSEffect's tag must be
    // Effect, NOT Lighting — even though both flow into emissive_mult.
    // A future BSEffect-proper render path will need this to drop the
    // diffuse-tint-as-emissive conflation per #166.
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(minimal_bseffect())];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = tri_shape_with_shader_ref(0);
    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);
    assert_eq!(
        info.emissive_source,
        EmissiveSource::Effect,
        "BSEffectShaderProperty must tag EmissiveSource::Effect (NOT Lighting) \
         so the diffuse-tint-as-emissive conflation stays type-visible"
    );
    assert!(
        (info.emissive_mult - 2.5).abs() < 1e-5,
        "emissive_mult must come from shader.base_color_scale"
    );
}

#[test]
fn nimaterial_tags_emissive_source_as_material() {
    // Legacy Oblivion/FO3/FNV path — NiMaterialProperty on the
    // property-cascade list, not the dedicated shader_property_ref slot.
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(minimal_nimaterial())];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = tri_shape_with_property_list(&[0]);
    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);
    assert_eq!(
        info.emissive_source,
        EmissiveSource::Material,
        "NiMaterialProperty must tag EmissiveSource::Material (legacy path)"
    );
    assert!(
        (info.emissive_mult - 1.75).abs() < 1e-5,
        "emissive_mult must come from mat.emissive_mult"
    );
}

#[test]
fn no_shader_property_defaults_to_none() {
    // Bare mesh with no shader_property_ref and an empty property list
    // — no source authored emissive. Discriminator stays at default.
    let scene = NifScene::default();
    let shape = tri_shape_with_property_list(&[]);
    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);
    assert_eq!(
        info.emissive_source,
        EmissiveSource::None,
        "no shader-property source must leave emissive_source at None"
    );
    assert_eq!(info.emissive_mult, 0.0);
}

#[test]
fn default_material_info_has_none_source() {
    // Sibling check on the Default impl — confirms the field defaults
    // to None even without going through extract_material_info.
    let info = MaterialInfo::default();
    assert_eq!(info.emissive_source, EmissiveSource::None);
}
