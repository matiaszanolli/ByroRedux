//! Regression tests for #1243 (NIF-DIM4-NEW-02) + #1244 (NIF-DIM4-NEW-03)
//! — FO3/FNV legacy non-BS shader subclasses must reach `MaterialInfo`.
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
use crate::blocks::shader::{BSShaderPropertyBaseOnly, WaterShaderProperty};
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
            shader_flags_1: crate::shader_flags::fo3nv_f1::ENVIRONMENT_MAPPING,
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
    assert!(info.is_water_shader);
}

/// `BSShaderPropertyBaseOnly` covers four legacy non-BS subclasses
/// (#1244): `HairShaderProperty`, `VolumetricFogShaderProperty`,
/// `DistantLODShaderProperty`, `BSDistantTreeShaderProperty`. Each
/// inherits `BSShaderProperty` directly (no
/// `BSShaderLightingProperty` layer = no `texture_clamp_mode`), so
/// only `env_map_scale` flows through to MaterialInfo. The shared
/// consumer is exercised once here through the `HairShaderProperty`
/// type-name — Oblivion-era hair NIFs are the most visible case.
#[test]
fn bs_shader_property_base_only_routes_env_map_scale() {
    let net = empty_net();
    let shader_data = BSShaderPropertyData {
        shade_flags: 0,
        shader_type: 0,
        shader_flags_1: crate::shader_flags::fo3nv_f1::ENVIRONMENT_MAPPING,
        shader_flags_2: 0,
        env_map_scale: 0.42,
    };
    let block = BSShaderPropertyBaseOnly::new_for_test(net, shader_data, "HairShaderProperty");
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(block)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = shape_with_property_ref(0);

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    assert_eq!(
        info.env_map_scale, 0.42,
        "pre-#1244: BSShaderPropertyBaseOnly (Hair/VolumetricFog/DistantLOD/DistantTree) \
         parsed cleanly through the shared #717 arm but had no consumer — \
         every Oblivion hair surface's reflective modulator was lost"
    );
}

/// All four `BSShaderPropertyBaseOnly` `type_name` variants resolve
/// through the same downcast site, so the consumer behaves
/// identically regardless of which subclass the parser saw. Sanity-
/// check the type-name discriminator doesn't accidentally gate the
/// consumer (a future maintainer who wants to differentiate behavior
/// per variant would have to read `block.block_type_name()` explicitly,
/// which we don't today).
#[test]
fn bs_shader_property_base_only_consumer_is_type_name_agnostic() {
    let net = empty_net();
    let shader_data = BSShaderPropertyData {
        shade_flags: 0,
        shader_type: 0,
        shader_flags_1: crate::shader_flags::fo3nv_f1::ENVIRONMENT_MAPPING,
        shader_flags_2: 0,
        env_map_scale: 0.7,
    };
    for type_name in &[
        "HairShaderProperty",
        "VolumetricFogShaderProperty",
        "DistantLODShaderProperty",
        "BSDistantTreeShaderProperty",
    ] {
        let block =
            BSShaderPropertyBaseOnly::new_for_test(net.clone(), shader_data.clone(), type_name);
        let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(block)];
        let scene = NifScene {
            blocks,
            ..NifScene::default()
        };
        let shape = shape_with_property_ref(0);
        let mut pool = StringPool::new();
        let info = extract_material_info(&scene, &shape, &[], &mut pool);
        assert_eq!(
            info.env_map_scale, 0.7,
            "type_name={type_name}: BSShaderPropertyBaseOnly consumer must fire for every \
             subclass that routes through the shared #717 parser"
        );
    }
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

/// #1856 — pins the FO3/FNV vs Skyrim+ water split so a future audit
/// doesn't refile "the legacy branch forgot to set
/// `water_shader_flags`" as a wire-up bug. It didn't forget: per
/// nif.xml line 6322 the FO3/FNV `WaterShaderProperty` has *no* fields
/// beyond the `BSShaderProperty` base, so the flag word simply does
/// not exist on that block. It is authored only on the Skyrim-era
/// `BSWaterShaderProperty` (nif.xml line 6705).
///
/// Both halves are asserted together: the legacy block leaves the
/// field at its `0` default while still forwarding `env_map_scale`,
/// and the Skyrim block carries the authored word through. A refactor
/// that "fixed" the legacy branch by folding in the base
/// `BSShaderFlags` (a different flag namespace entirely) fails here.
#[test]
fn water_shader_flags_are_skyrim_only_by_block_shape() {
    use crate::blocks::shader::BSWaterShaderProperty;
    use crate::types::NiTransform as TestNiTransform;

    // FO3/FNV: property-list-bound, no flag word on the block.
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(water_shader_with_env_scale(0.85))];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut pool = StringPool::new();
    let legacy = extract_material_info(&scene, &shape_with_property_ref(0), &[], &mut pool);
    assert_eq!(legacy.env_map_scale, 0.85, "#1243 wire-up still live");
    assert_eq!(
        legacy.water_shader_flags, 0,
        "FO3/FNV WaterShaderProperty carries no WaterShaderPropertyFlags word — \
         anything non-zero here means a flag namespace got crossed"
    );

    // Skyrim+: shader_property_ref-bound, authored flag word (0xC4 =
    // Reflections | Refractions | Cubemap, nif.xml's documented default).
    let bs_water = BSWaterShaderProperty {
        net: empty_net(),
        shader_flags_1: 0,
        shader_flags_2: 0,
        sf1_crcs: Vec::new(),
        sf2_crcs: Vec::new(),
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        water_shader_flags: 0xC4,
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(bs_water)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut shape = NiTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: TestNiTransform::default(),
            properties: vec![],
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef::NULL,
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef(0),
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    };
    shape.av.properties.clear();
    let mut pool = StringPool::new();
    let skyrim = extract_material_info(&scene, &shape, &[], &mut pool);
    assert_eq!(
        skyrim.water_shader_flags, 0xC4,
        "Skyrim+ BSWaterShaderProperty is the only source of this word"
    );
}
