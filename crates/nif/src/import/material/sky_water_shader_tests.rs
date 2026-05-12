//! Regression tests for #977 — Skyrim+ `BSSkyShaderProperty` /
//! `BSWaterShaderProperty` consumer wiring in `extract_material_info`.
//!
//! Pre-fix the parser landed every Skyrim sky-dome / sun-glare / moon /
//! star NIF's `source_texture` cleanly into the [`BSSkyShaderProperty`]
//! block, but the importer had no `scene.get_as::<BSSkyShaderProperty>`
//! site — `MaterialInfo.texture_path` stayed `None`, the renderer fell
//! back to the magenta-checker placeholder, and the cell's sky-dome
//! rendered as a 360° magenta sphere. Same defect class as the
//! BSEffectShaderProperty pre-#345 path. The FO3/FNV counterpart
//! (`SkyShaderProperty`, non-BS variant) was wired by #940 — this is
//! the missing Skyrim-era sibling.

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::shader::{BSSkyShaderProperty, BSWaterShaderProperty};
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

fn shape_bound_to_block_zero() -> NiTriShape {
    NiTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: NiTransform::default(),
            properties: vec![],
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef::NULL,
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef(0),
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    }
}

fn sky_shader_clouds() -> BSSkyShaderProperty {
    BSSkyShaderProperty {
        net: empty_net(),
        shader_flags_1: 0,
        shader_flags_2: 0,
        sf1_crcs: Vec::new(),
        sf2_crcs: Vec::new(),
        uv_offset: [0.25, 0.5],
        uv_scale: [2.0, 4.0],
        source_texture: "sky\\sky_clouds.dds".to_string(),
        // SkyObjectType::Clouds per nif.xml.
        sky_object_type: 3,
    }
}

fn water_shader_default_flags() -> BSWaterShaderProperty {
    BSWaterShaderProperty {
        net: empty_net(),
        shader_flags_1: 0,
        shader_flags_2: 0,
        sf1_crcs: Vec::new(),
        sf2_crcs: Vec::new(),
        uv_offset: [0.1, 0.2],
        uv_scale: [3.0, 6.0],
        // 0xC4 = Reflections | Refractions | Cubemap — nif.xml's
        // documented default for `WaterShaderPropertyFlags`.
        water_shader_flags: 0xC4,
    }
}

#[test]
fn default_material_info_has_no_sky_or_water_marker() {
    // Sibling check — non-Sky/Water materials must default to false/0
    // so downstream consumers don't see spurious sky/water surfaces.
    let info = MaterialInfo::default();
    assert!(!info.is_sky_object);
    assert_eq!(info.sky_object_type, 0);
    assert_eq!(info.water_shader_flags, 0);
}

/// Headline regression: a Skyrim sky NIF that previously imported with
/// `texture_path = None` now plumbs `source_texture` all the way through.
/// Without this, every `meshes/sky/*.nif` (clouds, sunglare, moon, stars)
/// renders as the magenta-checker placeholder.
#[test]
fn bs_sky_shader_property_routes_source_texture_to_material_info() {
    let shader = sky_shader_clouds();
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = shape_bound_to_block_zero();

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    let tex = info.texture_path.and_then(|s| pool.resolve(s));
    assert_eq!(
        tex.as_deref(),
        Some("sky\\sky_clouds.dds"),
        "pre-#977: BSSkyShaderProperty.source_texture was parsed but the \
         importer had no consumer — every Skyrim sky-dome rendered as the \
         magenta-checker placeholder"
    );
}

#[test]
fn bs_sky_shader_property_marks_material_as_sky_with_object_type() {
    let shader = sky_shader_clouds();
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = shape_bound_to_block_zero();

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    assert!(info.is_sky_object);
    assert_eq!(info.sky_object_type, 3, "SkyObjectType::Clouds");
    assert!(
        info.has_material_data,
        "BSSkyShaderProperty must mark the material as authoritative so \
         the legacy NiMaterialProperty path doesn't re-determine state"
    );
}

#[test]
fn bs_sky_shader_property_routes_uv_transform() {
    let shader = sky_shader_clouds();
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = shape_bound_to_block_zero();

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    assert_eq!(info.uv_offset, [0.25, 0.5]);
    assert_eq!(info.uv_scale, [2.0, 4.0]);
    assert!(info.has_uv_transform);
}

/// Companion regression for `BSWaterShaderProperty`. M38 cell-driven
/// water spawns via `cell_loader/water.rs` independently, but legacy
/// mesh-driven water (Oblivion `meshes/water/*.nif`, Skyrim river
/// segments) still needs the UV transform + flag bits to reach the
/// renderer. Pre-fix the importer dropped these silently.
#[test]
fn bs_water_shader_property_routes_uv_and_flags_to_material_info() {
    let shader = water_shader_default_flags();
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = shape_bound_to_block_zero();

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    assert_eq!(info.uv_offset, [0.1, 0.2]);
    assert_eq!(info.uv_scale, [3.0, 6.0]);
    assert!(info.has_uv_transform);
    assert_eq!(
        info.water_shader_flags, 0xC4,
        "Reflections | Refractions | Cubemap — nif.xml's documented default"
    );
    assert!(info.has_material_data);
    // Water is NOT a sky surface.
    assert!(!info.is_sky_object);
}

/// Sky and Water share `shader_property_ref` slot semantics. A single
/// shape carries exactly one shader-property reference, so the two
/// types are mutually exclusive at the same `idx`. Pin that the Water
/// branch isn't accidentally writing to a Sky-bound shape (regression
/// guard against `if let { ... } if let { ... }` cross-contamination).
#[test]
fn sky_and_water_consumers_are_mutually_exclusive_per_shape() {
    let shader = sky_shader_clouds();
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = shape_bound_to_block_zero();

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    // Sky branch fired.
    assert!(info.is_sky_object);
    // Water branch did NOT fire on the same shape — `water_shader_flags`
    // stays at the Default value (0). Defending against a future
    // refactor that uses `else if` ordering with the wrong precedence.
    assert_eq!(info.water_shader_flags, 0);
}
