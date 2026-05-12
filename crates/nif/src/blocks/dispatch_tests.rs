//! Tests for ../mod.rs (block dispatch).
//!
//! Extracted from blocks/mod.rs in the monolith-refactor pass — pre-extract
//! mod.rs was 3091 LOC (72% test scaffolding). Same qualified test paths
//! preserved (`blocks::dispatch_tests::FOO`) via `#[path]` declaration.

//! Regression tests for `parse_block` type-name dispatch.
//!
//! These test that the dispatch table routes Oblivion-era shader
//! variants through the right parser — see issue #145.
use super::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;
use std::sync::Arc;

/// Build an Oblivion (bsver=0) header with a single string slot.
fn oblivion_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_0_0_5,
        little_endian: true,
        user_version: 11,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("SkyProp")],
        max_string_length: 8,
        num_groups: 0,
    }
}

/// Minimal Oblivion BSShaderPPLightingProperty-shaped payload: 22 bytes.
/// Matches the no-extra-fields path (no refraction/parallax).
fn oblivion_bsshader_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    // NiObjectNET: name string index
    d.extend_from_slice(&0i32.to_le_bytes());
    // extra_data_refs: count=0
    d.extend_from_slice(&0u32.to_le_bytes());
    // controller_ref: -1
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // BSShaderProperty fields
    d.extend_from_slice(&0u16.to_le_bytes()); // shader_flags
    d.extend_from_slice(&1u32.to_le_bytes()); // shader_type
    d.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_1
    d.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_2
    d.extend_from_slice(&1.0f32.to_le_bytes()); // env_map_scale
    d.extend_from_slice(&3u32.to_le_bytes()); // texture_clamp_mode
    d.extend_from_slice(&5i32.to_le_bytes()); // texture_set_ref
    d
}

#[test]
fn oblivion_shader_variants_route_to_bsshader_pp_lighting() {
    // Every specialized variant named in issue #145 must dispatch
    // through BSShaderPPLightingProperty::parse and produce a
    // downcastable block. #455 moved `TileShaderProperty` onto
    // its own dedicated parser (covered by
    // `tile_shader_property_routes_to_dedicated_parser` below). #474
    // moved `WaterShaderProperty` and `TallGrassShaderProperty` onto
    // their own parsers too (they inherit `BSShaderProperty` directly,
    // not `BSShaderLightingProperty`, so the PPLighting trailer
    // over-read was masked by `block_sizes` recovery).
    // `SkyShaderProperty` moved to its own dedicated parser in #550
    // (inherits `BSShaderLightingProperty` + SizedString + u32 that
    // the PPLighting over-read dropped on the floor).
    // `BSSkyShaderProperty` / `BSWaterShaderProperty` moved to their
    // own parsers in #713 / NIF-D3-01 (Skyrim-era variants that
    // inherit `BSShaderProperty` directly with a Skyrim shader-flags
    // base + UV transform + per-block tail). Both have dedicated
    // dispatch tests — `bs_sky_shader_property_parses_skyrim_layout_exactly`
    // and friends in `shader_tests.rs`.
    // #717 / NIF-D3-02: `HairShaderProperty`, `VolumetricFogShaderProperty`,
    // `DistantLODShaderProperty`, `BSDistantTreeShaderProperty` moved to
    // `BSShaderPropertyBaseOnly` (they inherit `BSShaderProperty` directly,
    // no Lighting fields). Covered by `zero_field_shader_variants_route_to_base_only`.
    let variants = ["BSShaderPPLightingProperty", "Lighting30ShaderProperty"];
    let header = oblivion_header();
    let bytes = oblivion_bsshader_bytes();

    for variant in variants {
        let mut stream = NifStream::new(&bytes, &header);
        let block = parse_block(variant, &mut stream, Some(bytes.len() as u32))
            .unwrap_or_else(|e| panic!("variant '{variant}' failed to parse: {e}"));
        let prop = block
            .as_any()
            .downcast_ref::<BSShaderPPLightingProperty>()
            .unwrap_or_else(|| {
                panic!("variant '{variant}' did not downcast to BSShaderPPLightingProperty")
            });
        assert_eq!(
            prop.texture_set_ref.index(),
            Some(5),
            "variant '{variant}' parsed the wrong texture_set_ref"
        );
    }
}

/// Regression: #455 — `TileShaderProperty` must dispatch through
/// its own `TileShaderProperty::parse`, not get aliased onto
/// `BSShaderPPLightingProperty`. The Oblivion payload here carries
/// the BSShaderLightingProperty base + a SizedString filename and
/// nothing more; routing through PPLighting over-reads by 4 bytes
/// (texture_set_ref) and silently zeros the filename.
#[test]
fn tile_shader_property_routes_to_dedicated_parser() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // NiObjectNET: name string index.
    bytes.extend_from_slice(&0i32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                     // BSShaderProperty fields.
    bytes.extend_from_slice(&0u16.to_le_bytes()); // shader_flags
    bytes.extend_from_slice(&1u32.to_le_bytes()); // shader_type
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_1
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_2
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // env_map_scale
    bytes.extend_from_slice(&3u32.to_le_bytes()); // texture_clamp_mode
    let name = b"textures\\interface\\stealthmeter.dds";
    bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
    bytes.extend_from_slice(name);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("TileShaderProperty", &mut stream, Some(bytes.len() as u32))
        .expect("TileShaderProperty dispatch must reach TileShaderProperty::parse");
    let prop = block
        .as_any()
        .downcast_ref::<crate::blocks::shader::TileShaderProperty>()
        .expect("TileShaderProperty must downcast to its own type, not BSShaderPPLightingProperty");
    assert_eq!(prop.texture_clamp_mode, 3);
    assert_eq!(prop.file_name, "textures\\interface\\stealthmeter.dds");
}

/// Regression for #550 — `SkyShaderProperty` must dispatch through
/// its own `SkyShaderProperty::parse`, not the
/// `BSShaderPPLightingProperty` alias. nif.xml line 6335: inherits
/// `BSShaderLightingProperty` + `File Name: SizedString` + `Sky
/// Object Type: u32`. Pre-fix the aliased parser over-read 20+ bytes
/// (texture_set_ref + refraction + parallax) and silently dropped
/// the sky filename + object type — every sky NIF rendered with
/// default cloud scroll and horizon fade. `block_sizes` kept the
/// outer stream aligned so the defect was silent at parse time but
/// surfaced as the recurring `consumed 54, expected 42-82` warning
/// bucket in the FO3 + FNV corpus stderr logs.
#[test]
fn sky_shader_property_routes_to_dedicated_parser() {
    // FNV header (bsver = 34 — the audit corpus).
    let header = NifHeader {
        version: NifVersion(0x14020007),
        little_endian: true,
        user_version: 11,
        user_version_2: 34,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("SkyProp")],
        max_string_length: 8,
        num_groups: 0,
    };
    let mut bytes = Vec::new();
    // NiObjectNET: name string index = 0
    bytes.extend_from_slice(&0i32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                     // BSShaderProperty fields.
    bytes.extend_from_slice(&0u16.to_le_bytes()); // shade_flags
    bytes.extend_from_slice(&1u32.to_le_bytes()); // shader_type
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_1
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_2
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // env_map_scale
                                                    // BSShaderLightingProperty: texture_clamp_mode
    bytes.extend_from_slice(&3u32.to_le_bytes());
    // SkyShaderProperty: File Name (SizedString) + Sky Object Type
    let name = b"textures\\sky\\skyclouds01.dds";
    bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
    bytes.extend_from_slice(name);
    // Sky Object Type = 3 (BSSM_SKY_CLOUDS)
    bytes.extend_from_slice(&3u32.to_le_bytes());

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("SkyShaderProperty", &mut stream, Some(bytes.len() as u32))
        .expect("SkyShaderProperty dispatch must reach SkyShaderProperty::parse");
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "parser must consume the whole body — the warning bucket was \
             exactly this assertion failing in production"
    );
    assert_eq!(block.block_type_name(), "SkyShaderProperty");
    let prop = block
        .as_any()
        .downcast_ref::<crate::blocks::shader::SkyShaderProperty>()
        .expect("SkyShaderProperty must downcast to its own type, not BSShaderPPLightingProperty");
    assert_eq!(prop.texture_clamp_mode, 3);
    assert_eq!(prop.file_name, "textures\\sky\\skyclouds01.dds");
    assert_eq!(
        prop.sky_object_type, 3,
        "sky_object_type = 3 (BSSM_SKY_CLOUDS) — pre-fix this field \
             was never read and every sky block landed with default 0"
    );
}

/// Regression: #474 — `WaterShaderProperty` inherits `BSShaderProperty`
/// directly per nif.xml line 6322 (no `texture_clamp_mode`, no
/// `texture_set_ref`, no refraction/parallax trailer). Routing through
/// `BSShaderPPLightingProperty::parse` over-read 20+ bytes, masked by
/// `block_sizes` recovery.
#[test]
fn water_shader_property_routes_to_dedicated_parser() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // NiObjectNET.
    bytes.extend_from_slice(&0i32.to_le_bytes()); // name
    bytes.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                     // BSShaderProperty base only — no texture_clamp_mode.
    bytes.extend_from_slice(&0u16.to_le_bytes()); // shade_flags
    bytes.extend_from_slice(&1u32.to_le_bytes()); // shader_type
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_1
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_2
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // env_map_scale
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("WaterShaderProperty", &mut stream, Some(bytes.len() as u32))
        .expect("WaterShaderProperty dispatch must reach dedicated parser");
    let prop = block
        .as_any()
        .downcast_ref::<crate::blocks::shader::WaterShaderProperty>()
        .expect("WaterShaderProperty must downcast to its own type");
    assert_eq!(prop.shader.shader_type, 1);
    assert_eq!(prop.shader.env_map_scale, 1.0);
}

/// Regression: #474 — `TallGrassShaderProperty` inherits `BSShaderProperty`
/// + adds `File Name: SizedString` per nif.xml line 6354. Previously
/// aliased to `BSShaderPPLightingProperty::parse`, dropping the
/// filename on the floor.
#[test]
fn tall_grass_shader_property_routes_to_dedicated_parser() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0i32.to_le_bytes()); // name
    bytes.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
    bytes.extend_from_slice(&0u16.to_le_bytes()); // shade_flags
    bytes.extend_from_slice(&1u32.to_le_bytes()); // shader_type
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_1
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_2
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // env_map_scale
    let name = b"textures\\landscape\\grass01.dds";
    bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
    bytes.extend_from_slice(name);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "TallGrassShaderProperty",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("TallGrassShaderProperty dispatch must reach dedicated parser");
    let prop = block
        .as_any()
        .downcast_ref::<crate::blocks::shader::TallGrassShaderProperty>()
        .expect("TallGrassShaderProperty must downcast to its own type");
    assert_eq!(prop.file_name, "textures\\landscape\\grass01.dds");
}

/// Regression: #474 — `bhkSimpleShapePhantom` carries an 8-byte
/// `Unused 01` field between the bhkWorldObjectCInfo block and the
/// Matrix44 transform (nif.xml line 2793). Pre-#474 the parser
/// skipped straight from CInfo to the 4x4 transform, reading only
/// 92 of 100 declared bytes and leaving `block_sizes` recovery to
/// paper over the gap.
#[test]
fn bhk_simple_shape_phantom_consumes_full_100_bytes() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // bhkWorldObject: shape ref + havok filter + 20-byte CInfo.
    bytes.extend_from_slice(&5i32.to_le_bytes()); // shape_ref
    bytes.extend_from_slice(&0x12345678u32.to_le_bytes()); // havok_filter
    bytes.extend_from_slice(&[0u8; 20]); // bhkWorldObjectCInfo
                                         // bhkSimpleShapePhantom: 8-byte Unused 01 + 64-byte Matrix44.
    bytes.extend_from_slice(&[0u8; 8]); // Unused 01
    for i in 0..16 {
        bytes.extend_from_slice(&(i as f32).to_le_bytes());
    }
    assert_eq!(
        bytes.len(),
        100,
        "test fixture must be 100 bytes per nif.xml"
    );
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "bhkSimpleShapePhantom",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("bhkSimpleShapePhantom must parse without block_sizes recovery");
    let prop = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkSimpleShapePhantom>()
        .expect("bhkSimpleShapePhantom must downcast");
    assert_eq!(prop.shape_ref.index(), Some(5));
    assert_eq!(prop.havok_filter, 0x12345678);
    // Transform column 0 should be [0.0, 1.0, 2.0, 3.0] per the fixture.
    assert_eq!(prop.transform[0], [0.0, 1.0, 2.0, 3.0]);
    assert_eq!(prop.transform[3], [12.0, 13.0, 14.0, 15.0]);
}

// ── #557 / NIF-12 Havok tail type round-trips ───────────────────

/// Regression for #557 — `bhkAabbPhantom` must consume its full
/// 68-byte body (28 B bhkWorldObject prefix + 8 B unused + 32 B
/// hkAabb) and surface shape ref + filter + AABB corners.
#[test]
fn bhk_aabb_phantom_consumes_full_68_bytes() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // bhkWorldObject prefix (28 B).
    bytes.extend_from_slice(&7i32.to_le_bytes()); // shape_ref
    bytes.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes()); // havok_filter
    bytes.extend_from_slice(&[0u8; 20]); // bhkWorldObjectCInfo
                                         // Unused 01 (8 B).
    bytes.extend_from_slice(&[0u8; 8]);
    // hkAabb: min (x=1, y=2, z=3, w=0) + max (x=10, y=20, z=30, w=0).
    for v in [1.0f32, 2.0, 3.0, 0.0, 10.0, 20.0, 30.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    assert_eq!(bytes.len(), 68, "fixture must be 68 bytes per nif.xml");
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkAabbPhantom", &mut stream, Some(bytes.len() as u32))
        .expect("bhkAabbPhantom must parse on Oblivion");
    let phantom = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkAabbPhantom>()
        .expect("dispatch must land on BhkAabbPhantom");
    assert_eq!(phantom.shape_ref.index(), Some(7));
    assert_eq!(phantom.havok_filter, 0xDEAD_BEEF);
    assert_eq!(phantom.aabb_min, [1.0, 2.0, 3.0, 0.0]);
    assert_eq!(phantom.aabb_max, [10.0, 20.0, 30.0, 0.0]);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression for #557 — `bhkLiquidAction` must consume its
/// 28-byte body (12 B unused + 4 × f32 tuning).
#[test]
fn bhk_liquid_action_consumes_full_28_bytes() {
    // FO3+ only, but oblivion_header works for the parse flow
    // since the parser doesn't gate on version. Matches the
    // corpus where FO3/FNV ship these blocks.
    let header = oblivion_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0u8; 12]); // Unused 01
    bytes.extend_from_slice(&25.0f32.to_le_bytes()); // initial stick force
    bytes.extend_from_slice(&100.0f32.to_le_bytes()); // stick strength
    bytes.extend_from_slice(&128.0f32.to_le_bytes()); // neighbor distance
    bytes.extend_from_slice(&500.0f32.to_le_bytes()); // neighbor strength
    assert_eq!(bytes.len(), 28);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkLiquidAction", &mut stream, Some(bytes.len() as u32))
        .expect("bhkLiquidAction dispatch must parse");
    let action = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkLiquidAction>()
        .unwrap();
    assert_eq!(action.initial_stick_force, 25.0);
    assert_eq!(action.stick_strength, 100.0);
    assert_eq!(action.neighbor_distance, 128.0);
    assert_eq!(action.neighbor_strength, 500.0);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression for #557 — `bhkPCollisionObject` wire layout is
/// byte-identical to `bhkCollisionObject` (target + u16 flags +
/// body ref = 10 B) but must surface as its own type so consumers
/// can tell it wraps a phantom, not a rigid body.
#[test]
fn bhk_p_collision_object_consumes_full_10_bytes() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&9i32.to_le_bytes()); // target_ref
    bytes.extend_from_slice(&0x0081u16.to_le_bytes()); // flags (SYNC_ON_UPDATE + SET_LOCAL)
    bytes.extend_from_slice(&3i32.to_le_bytes()); // body_ref (bhkAabbPhantom)
    assert_eq!(bytes.len(), 10);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkPCollisionObject", &mut stream, Some(bytes.len() as u32))
        .expect("bhkPCollisionObject must parse");
    let pco = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkPCollisionObject>()
        .expect("dispatch must land on BhkPCollisionObject, not the sibling bhkCollisionObject");
    assert_eq!(pco.target_ref.index(), Some(9));
    assert_eq!(pco.flags, 0x0081);
    assert_eq!(pco.body_ref.index(), Some(3));
    assert_eq!(pco.block_type_name(), "bhkPCollisionObject");
}

/// Regression for #557 — `bhkConvexListShape` (FO3 only) with a
/// two-sub-shape body. Total size = 37 + 4*N = 45 bytes for N=2.
#[test]
fn bhk_convex_list_shape_consumes_variable_body() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&2u32.to_le_bytes()); // num_sub_shapes
    bytes.extend_from_slice(&11i32.to_le_bytes()); // sub_shape[0]
    bytes.extend_from_slice(&22i32.to_le_bytes()); // sub_shape[1]
    bytes.extend_from_slice(&7u32.to_le_bytes()); // material (FO3 = no Unknown Int prefix)
    bytes.extend_from_slice(&0.5f32.to_le_bytes()); // radius
    bytes.extend_from_slice(&0u32.to_le_bytes()); // Unknown Int 1
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // Unknown Float 1
    bytes.extend_from_slice(&[0u8; 12]); // bhkWorldObjCInfoProperty
    bytes.push(1u8); // use_cached_aabb = true
    bytes.extend_from_slice(&42.0f32.to_le_bytes()); // closest_point_min_distance
    assert_eq!(bytes.len(), 37 + 4 * 2);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkConvexListShape", &mut stream, Some(bytes.len() as u32))
        .expect("bhkConvexListShape dispatch must parse");
    let shape = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkConvexListShape>()
        .unwrap();
    assert_eq!(shape.sub_shapes.len(), 2);
    assert_eq!(shape.sub_shapes[0].index(), Some(11));
    assert_eq!(shape.sub_shapes[1].index(), Some(22));
    assert_eq!(shape.material, 7);
    assert_eq!(shape.radius, 0.5);
    assert!(shape.use_cached_aabb);
    assert_eq!(shape.closest_point_min_distance, 42.0);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression for #557 — `bhkBreakableConstraint` with a Hinge
/// inner (type=1, 80 B payload). Oblivion-sized so no block_sizes
/// recovery is needed. Total = 16 (outer CInfo) + 4 (wrapped type)
/// + 16 (inner CInfo) + 80 (Hinge payload) + 4 (threshold) + 1
/// (remove_when_broken) = 121 bytes.
#[test]
fn bhk_breakable_constraint_hinge_inner_consumes_121_bytes() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // Outer bhkConstraintCInfo
    bytes.extend_from_slice(&2u32.to_le_bytes()); // num_entities
    bytes.extend_from_slice(&5i32.to_le_bytes()); // entity_a
    bytes.extend_from_slice(&6i32.to_le_bytes()); // entity_b
    bytes.extend_from_slice(&1u32.to_le_bytes()); // priority
                                                  // Wrapped type = Hinge.
    bytes.extend_from_slice(&1u32.to_le_bytes());
    // Inner bhkConstraintCInfo (16 B) — unused in this parse.
    bytes.extend_from_slice(&[0u8; 16]);
    // Hinge payload (80 B).
    bytes.extend_from_slice(&[0u8; 80]);
    // Threshold + Remove When Broken.
    bytes.extend_from_slice(&256.0f32.to_le_bytes());
    bytes.push(1u8);
    assert_eq!(bytes.len(), 121);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "bhkBreakableConstraint",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("bhkBreakableConstraint must parse");
    let bc = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkBreakableConstraint>()
        .unwrap();
    assert_eq!(bc.entity_a.index(), Some(5));
    assert_eq!(bc.entity_b.index(), Some(6));
    assert_eq!(bc.priority, 1);
    assert_eq!(bc.wrapped_type, 1);
    assert_eq!(bc.threshold, 256.0);
    assert!(bc.remove_when_broken);
    assert_eq!(stream.position() as usize, bytes.len());
}

// ── #394 / OBL-D5-H2 Oblivion-skippable block parsers ──────────

/// Regression for #394 — `bhkMultiSphereShape` with 2 spheres
/// must consume its full 20 + 16*2 = 52-byte body on Oblivion
/// (no block_sizes table to fall back on). Validates material +
/// shape_property + per-sphere (center, radius).
#[test]
fn bhk_multi_sphere_shape_consumes_full_52_bytes_for_2_spheres() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&7u32.to_le_bytes()); // material
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shape_property[0]
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shape_property[1]
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shape_property[2]
    bytes.extend_from_slice(&2u32.to_le_bytes()); // num_spheres
    for v in [1.0f32, 2.0, 3.0, 0.5, 10.0, 20.0, 30.0, 2.5] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    assert_eq!(bytes.len(), 52);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkMultiSphereShape", &mut stream, Some(bytes.len() as u32))
        .expect("bhkMultiSphereShape must parse on Oblivion");
    let sphere = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkMultiSphereShape>()
        .unwrap();
    assert_eq!(sphere.material, 7);
    assert_eq!(sphere.spheres.len(), 2);
    assert_eq!(sphere.spheres[0], [1.0, 2.0, 3.0, 0.5]);
    assert_eq!(sphere.spheres[1], [10.0, 20.0, 30.0, 2.5]);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression for #394 — `NiPathInterpolator` must consume its
/// full 24-byte body. Used by door hinges and environmental spline
/// motion; pre-#394 these tripped the block_sizes-less Oblivion
/// loader.
#[test]
fn ni_path_interpolator_consumes_full_24_bytes() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0x0003u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // bank_dir
    bytes.extend_from_slice(&0.5f32.to_le_bytes()); // max_bank_angle
    bytes.extend_from_slice(&0.2f32.to_le_bytes()); // smoothing
    bytes.extend_from_slice(&1u16.to_le_bytes()); // follow_axis = Y
    bytes.extend_from_slice(&11i32.to_le_bytes()); // path_data_ref
    bytes.extend_from_slice(&22i32.to_le_bytes()); // percent_data_ref
    assert_eq!(bytes.len(), 24);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiPathInterpolator", &mut stream, Some(bytes.len() as u32))
        .expect("NiPathInterpolator must parse on Oblivion");
    let interp = block
        .as_any()
        .downcast_ref::<crate::blocks::interpolator::NiPathInterpolator>()
        .unwrap();
    assert_eq!(interp.flags, 0x0003);
    assert_eq!(interp.bank_dir, -1);
    assert_eq!(interp.follow_axis, 1);
    assert_eq!(interp.path_data_ref.index(), Some(11));
    assert_eq!(interp.percent_data_ref.index(), Some(22));
    assert_eq!(stream.position() as usize, bytes.len());
}

/// `NiLookAtInterpolator` — surfaced by the R3 histogram (18
/// instances per FNV mesh sweep). Layout for our targets (NIF
/// version <= 20.4.0.12 includes the `Transform` field):
/// 2 (flags) + 4 (look_at) + 4 (look_at_name string ref) +
/// 32 (NiQuatTransform) + 4×3 (TRS interp refs) = 54 B.
///
/// Uses a v20.2.0.7 FNV-shaped header so the `look_at_name` field
/// goes through the string-table path (`>= 0x14010001`) — the
/// failing real-world content is FNV-era and uses table indices,
/// not the legacy inline length-prefixed strings.
#[test]
fn ni_look_at_interpolator_consumes_full_54_bytes() {
    let header = NifHeader {
        version: NifVersion(0x14020007),
        little_endian: true,
        user_version: 11,
        user_version_2: 34,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("SkyProp")],
        max_string_length: 8,
        num_groups: 0,
    };
    let mut bytes = Vec::new();
    // Flags: LOOK_FLIP | LOOK_Y_AXIS = 0x0003.
    bytes.extend_from_slice(&0x0003u16.to_le_bytes());
    // Look At Ptr → NiNode index 7.
    bytes.extend_from_slice(&7i32.to_le_bytes());
    // Look At Name → string-table index 0 ("SkyProp" in
    // oblivion_header).
    bytes.extend_from_slice(&0i32.to_le_bytes());
    // NiQuatTransform: translation (1,2,3), rotation (w,x,y,z) =
    // (1,0,0,0), scale = 1.0. 32 bytes.
    for v in [1.0f32, 2.0, 3.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    for v in [1.0f32, 0.0, 0.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    // Three sub-interpolator refs.
    bytes.extend_from_slice(&11i32.to_le_bytes());
    bytes.extend_from_slice(&12i32.to_le_bytes());
    bytes.extend_from_slice(&13i32.to_le_bytes());
    assert_eq!(bytes.len(), 54);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "NiLookAtInterpolator",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("NiLookAtInterpolator must parse on Oblivion");
    let interp = block
        .as_any()
        .downcast_ref::<crate::blocks::interpolator::NiLookAtInterpolator>()
        .unwrap();
    use crate::blocks::interpolator::look_at_flags;
    assert_eq!(interp.flags, 0x0003);
    assert_ne!(interp.flags & look_at_flags::LOOK_FLIP, 0);
    assert_ne!(interp.flags & look_at_flags::LOOK_Y_AXIS, 0);
    assert_eq!(interp.flags & look_at_flags::LOOK_Z_AXIS, 0);
    assert_eq!(interp.look_at.index(), Some(7));
    assert_eq!(interp.look_at_name.as_deref(), Some("SkyProp"));
    assert_eq!(interp.transform.translation.x, 1.0);
    assert_eq!(interp.transform.translation.z, 3.0);
    assert_eq!(interp.transform.scale, 1.0);
    assert_eq!(interp.interp_translation.index(), Some(11));
    assert_eq!(interp.interp_roll.index(), Some(12));
    assert_eq!(interp.interp_scale.index(), Some(13));
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression for #394 — `NiFlipController` on Oblivion (>= 10.1.0.104)
/// gates off `Accum Time` and `Delta` fields, so the disk layout
/// reduces to NiTimeController base (26) + NiSingleInterpController
/// interpolator_ref (4) + texture_slot (4) + num_sources (4) +
/// sources[N] (4 each). Test with N=3 sources → 42 bytes total.
#[test]
fn ni_flip_controller_consumes_full_body_oblivion_layout() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // NiTimeController base: next(i32) + flags(u16) + freq(f32) +
    // phase(f32) + start(f32) + stop(f32) + target(i32) = 26 B
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // stop
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // target
                                                     // NiSingleInterpController: interpolator_ref (4 B).
    bytes.extend_from_slice(&5i32.to_le_bytes());
    // NiFlipController: texture_slot(4) + num_sources(4) + sources.
    bytes.extend_from_slice(&4u32.to_le_bytes()); // texture_slot = GLOW_MAP
    bytes.extend_from_slice(&3u32.to_le_bytes()); // num_sources
    bytes.extend_from_slice(&11i32.to_le_bytes());
    bytes.extend_from_slice(&12i32.to_le_bytes());
    bytes.extend_from_slice(&13i32.to_le_bytes());
    assert_eq!(bytes.len(), 26 + 4 + 4 + 4 + 4 * 3);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiFlipController", &mut stream, Some(bytes.len() as u32))
        .expect("NiFlipController must parse on Oblivion");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiFlipController>()
        .unwrap();
    assert_eq!(ctrl.texture_slot, 4);
    assert_eq!(ctrl.sources.len(), 3);
    assert_eq!(ctrl.sources[0].index(), Some(11));
    assert_eq!(ctrl.sources[2].index(), Some(13));
    assert_eq!(ctrl.base.interpolator_ref.index(), Some(5));
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression for #394 — `NiBSBoneLODController` with one LOD (1
/// bone) + one shape group (1 skin info) + one shape_groups_2
/// entry. Creature-skeleton LOD block on every vanilla Oblivion
/// creature NIF; without this parser, every block after it was
/// truncated.
#[test]
fn ni_bs_bone_lod_controller_consumes_full_body() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // NiTimeController base (26 B).
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // LOD + counts.
    bytes.extend_from_slice(&0u32.to_le_bytes()); // lod
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_lods
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_node_groups (unused)
                                                  // Node Groups: NodeSet { num_nodes=1, nodes=[42] }.
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&42i32.to_le_bytes());
    // Shape Groups 1: SkinInfoSet { num_skin_info=1, [shape_ptr=7, skin_instance=8] }.
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_shape_groups
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_skin_info
    bytes.extend_from_slice(&7i32.to_le_bytes()); // shape_ptr
    bytes.extend_from_slice(&8i32.to_le_bytes()); // skin_instance
                                                  // Shape Groups 2: [ref 99].
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_shape_groups_2
    bytes.extend_from_slice(&99i32.to_le_bytes());
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "NiBSBoneLODController",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("NiBSBoneLODController must parse on Oblivion");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiBsBoneLodController>()
        .unwrap();
    assert_eq!(ctrl.lod, 0);
    assert_eq!(ctrl.node_groups.len(), 1);
    assert_eq!(ctrl.node_groups[0].nodes.len(), 1);
    assert_eq!(ctrl.node_groups[0].nodes[0].index(), Some(42));
    assert_eq!(ctrl.shape_groups_1.len(), 1);
    assert_eq!(ctrl.shape_groups_1[0].skin_infos.len(), 1);
    assert_eq!(
        ctrl.shape_groups_1[0].skin_infos[0].shape_ptr.index(),
        Some(7)
    );
    assert_eq!(ctrl.shape_groups_2.len(), 1);
    assert_eq!(ctrl.shape_groups_2[0].index(), Some(99));
    assert_eq!(stream.position() as usize, bytes.len());
}

/// `NiBSBoneLODController` on Bethesda content (bsver != 0) must
/// stop after `node_groups` and skip the `#NISTREAM#`-gated
/// shape-group tail. Pre-fix the parser ate 4+ extra bytes past
/// the block, hit `0xFFFFFFFF` reading the next block's data as
/// `Num Shape Groups`, and bailed via `allocate_vec`. Surfaced by
/// the R3 per-block histogram on FNV creature skeletons (34
/// instances all advertising as `NiUnknown`). Sized to mirror the
/// failing block 6 from `meshes/characters/_male/skeleton.nif`:
/// 26 (base) + 4 (lod) + 4 (num_lods=1) + 4 (num_node_groups) +
/// 4 (num_nodes=5) + 5×4 (ptrs) = 62 bytes total.
#[test]
fn ni_bs_bone_lod_controller_skips_shape_groups_on_bethesda() {
    // FNV header — bsver=34, the BSVER on every creature skeleton
    // that R3 surfaced.
    let header = NifHeader {
        version: NifVersion(0x14020007),
        little_endian: true,
        user_version: 11,
        user_version_2: 34,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("SkyProp")],
        max_string_length: 8,
        num_groups: 0,
    };
    let mut bytes = Vec::new();
    // NiTimeController base (26 B).
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // LOD + counts.
    bytes.extend_from_slice(&0u32.to_le_bytes()); // lod
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_lods
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_node_groups (unused)
                                                  // Node Groups: NodeSet { num_nodes=5, nodes=[10,11,12,13,14] }.
    bytes.extend_from_slice(&5u32.to_le_bytes()); // num_nodes
    for ptr in 10i32..15 {
        bytes.extend_from_slice(&ptr.to_le_bytes());
    }
    // No shape-group fields — Bethesda content stops here.
    assert_eq!(bytes.len(), 62);
    // Pre-fix tripwire: a sentinel u32 right after the body so a
    // regressed parser that keeps reading past `bytes.len()` would
    // hit `0xFFFFFFFF` and bail in `allocate_vec`. The
    // `Some(bytes.len() as u32)` block-size cap below already
    // bounds the parser; this is belt-and-braces.
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "NiBSBoneLODController",
        &mut stream,
        // Block-size hint covers only the real body — pre-fix
        // parser ignored block_size for end-of-block detection
        // and read past it anyway.
        Some(62),
    )
    .expect("NiBSBoneLODController must parse on Bethesda BSVER!=0");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiBsBoneLodController>()
        .unwrap();
    assert_eq!(ctrl.lod, 0);
    assert_eq!(ctrl.node_groups.len(), 1);
    assert_eq!(ctrl.node_groups[0].nodes.len(), 5);
    assert_eq!(ctrl.node_groups[0].nodes[0].index(), Some(10));
    assert_eq!(ctrl.node_groups[0].nodes[4].index(), Some(14));
    // Shape-groups are absent on Bethesda content per #NISTREAM#.
    assert!(ctrl.shape_groups_1.is_empty());
    assert!(ctrl.shape_groups_2.is_empty());
    // Stream must stop exactly at end of body — no overshoot into
    // the sentinel u32 we stamped past byte 62.
    assert_eq!(stream.position(), 62);
}

/// Regression for #557 — `bhkOrientHingedBodyAction` must consume
/// its full 68-byte body (12 B bhkUnaryAction + 8 + 16 + 16 + 4 +
/// 4 + 8 = 56 B self).
#[test]
fn bhk_orient_hinged_body_action_consumes_full_68_bytes() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // bhkUnaryAction: Entity Ptr + Unused 01[8].
    bytes.extend_from_slice(&4i32.to_le_bytes()); // entity_ref
    bytes.extend_from_slice(&[0u8; 8]); // Unused 01
                                        // Self body: Unused 02[8] + Hinge Axis LS + Forward LS + S + D + Unused 03[8].
    bytes.extend_from_slice(&[0u8; 8]); // Unused 02
    for v in [1.0f32, 0.0, 0.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    for v in [0.0f32, 1.0, 0.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // strength
    bytes.extend_from_slice(&0.1f32.to_le_bytes()); // damping
    bytes.extend_from_slice(&[0u8; 8]); // Unused 03
    assert_eq!(bytes.len(), 68);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "bhkOrientHingedBodyAction",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("bhkOrientHingedBodyAction must parse");
    let action = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkOrientHingedBodyAction>()
        .unwrap();
    assert_eq!(action.entity_ref.index(), Some(4));
    assert_eq!(action.hinge_axis_ls, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(action.forward_ls, [0.0, 1.0, 0.0, 0.0]);
    assert_eq!(action.strength, 1.0);
    assert_eq!(action.damping, 0.1);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression test for issue #144: Oblivion-era KF animation roots
/// must dispatch through the right parsers.
#[test]
fn oblivion_kf_animation_blocks_route_correctly() {
    // NiKeyframeController: parses as NiSingleInterpController
    // (26-byte NiTimeControllerBase + 4-byte interpolator ref).
    let header = oblivion_header();
    let mut kf_bytes = Vec::new();
    // NiTimeControllerBase: next_controller, flags, frequency, phase,
    // start_time, stop_time, target_ref.
    kf_bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
    kf_bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    kf_bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
    kf_bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
    kf_bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
    kf_bytes.extend_from_slice(&1.0f32.to_le_bytes()); // stop_time
    kf_bytes.extend_from_slice(&(-1i32).to_le_bytes()); // target_ref
    kf_bytes.extend_from_slice(&7i32.to_le_bytes()); // interpolator_ref
    let mut stream = NifStream::new(&kf_bytes, &header);
    let block = parse_block(
        "NiKeyframeController",
        &mut stream,
        Some(kf_bytes.len() as u32),
    )
    .expect("NiKeyframeController should dispatch through NiSingleInterpController");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiSingleInterpController>()
        .expect("NiKeyframeController did not downcast to NiSingleInterpController");
    assert_eq!(ctrl.interpolator_ref.index(), Some(7));

    // NiSequenceStreamHelper: NiObjectNET with no extra fields.
    // name (string table index 0) + extra_data count (0) + controller ref (-1)
    let mut ssh_bytes = Vec::new();
    ssh_bytes.extend_from_slice(&0i32.to_le_bytes()); // name
    ssh_bytes.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
    ssh_bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller
    let mut stream = NifStream::new(&ssh_bytes, &header);
    let block = parse_block(
        "NiSequenceStreamHelper",
        &mut stream,
        Some(ssh_bytes.len() as u32),
    )
    .expect("NiSequenceStreamHelper should dispatch to its own parser");
    assert!(block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiSequenceStreamHelper>()
        .is_some());
}

/// Helper: encode a pre-20.1 inline length-prefixed string (u32 len + bytes).
fn inline_string(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
    out
}

/// Regression test for issue #164: array-form extra data.
#[test]
fn oblivion_strings_and_integers_extra_data_roundtrip() {
    use crate::blocks::extra_data::NiExtraData;

    let header = oblivion_header();

    // NiStringsExtraData: name(empty) + count(3) + 3 inline strings.
    let mut strings_bytes = Vec::new();
    strings_bytes.extend_from_slice(&0u32.to_le_bytes()); // name (empty inline str)
    strings_bytes.extend_from_slice(&3u32.to_le_bytes()); // count
    strings_bytes.extend_from_slice(&inline_string("alpha"));
    strings_bytes.extend_from_slice(&inline_string("beta"));
    strings_bytes.extend_from_slice(&inline_string("gamma"));
    let mut stream = NifStream::new(&strings_bytes, &header);
    let block = parse_block(
        "NiStringsExtraData",
        &mut stream,
        Some(strings_bytes.len() as u32),
    )
    .expect("NiStringsExtraData should dispatch");
    let ed = block
        .as_any()
        .downcast_ref::<NiExtraData>()
        .expect("downcast to NiExtraData");
    let arr = ed.strings_array.as_ref().expect("strings_array populated");
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].as_deref(), Some("alpha"));
    assert_eq!(arr[1].as_deref(), Some("beta"));
    assert_eq!(arr[2].as_deref(), Some("gamma"));

    // NiIntegersExtraData: name(empty) + count(2) + two u32s.
    let mut ints_bytes = Vec::new();
    ints_bytes.extend_from_slice(&0u32.to_le_bytes()); // name
    ints_bytes.extend_from_slice(&2u32.to_le_bytes()); // count
    ints_bytes.extend_from_slice(&42u32.to_le_bytes());
    ints_bytes.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
    let mut stream = NifStream::new(&ints_bytes, &header);
    let block = parse_block(
        "NiIntegersExtraData",
        &mut stream,
        Some(ints_bytes.len() as u32),
    )
    .expect("NiIntegersExtraData should dispatch");
    let ed = block
        .as_any()
        .downcast_ref::<NiExtraData>()
        .expect("downcast to NiExtraData");
    let arr = ed
        .integers_array
        .as_ref()
        .expect("integers_array populated");
    assert_eq!(arr, &vec![42u32, 0xDEADBEEF]);
}

/// Regression test for #615 / SK-D5-04 — `NiStringsExtraData`
/// strings are `SizedString` (always u32-length-prefixed inline)
/// per nif.xml, not the version-aware `string` type. Pre-fix the
/// parser called `read_string`, which on Skyrim+ (v >= 20.1.0.1)
/// reads a 4-byte string-table index. Result: every Skyrim
/// NiStringsExtraData with non-empty contents under-consumed its
/// payload, dropping the entire strings array body.
///
/// Construct a Skyrim-shaped block: name as string-table index
/// (4 bytes) + count + N × SizedString. Pre-fix the parser would
/// read the first 4 bytes of the first SizedString as a string-
/// table index, mis-resolve it, and stop the loop with garbage.
/// Post-fix it must round-trip the strings cleanly.
#[test]
fn skyrim_strings_extra_data_uses_sized_string_not_string_table_index() {
    use crate::blocks::extra_data::NiExtraData;

    let header = NifHeader {
        version: NifVersion(0x14020007),
        little_endian: true,
        user_version: 12,
        user_version_2: 83, // Skyrim LE
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        // Empty string table — proves the strings array does NOT
        // resolve through it. If the parser still used `read_string`
        // here, the first 4 bytes of "alpha" would be misread as
        // an out-of-bounds string-table index and yield None.
        strings: vec![],
        max_string_length: 0,
        num_groups: 0,
    };

    let mut bytes = Vec::new();
    // Name: string-table index = -1 (None) — exercises the modern
    // header path. 4 bytes.
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // Count: 3.
    bytes.extend_from_slice(&3u32.to_le_bytes());
    // Three SizedStrings.
    bytes.extend_from_slice(&inline_string("alpha"));
    bytes.extend_from_slice(&inline_string("beta"));
    bytes.extend_from_slice(&inline_string("gamma"));

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiStringsExtraData", &mut stream, Some(bytes.len() as u32))
        .expect("NiStringsExtraData should dispatch on Skyrim");
    let ed = block
        .as_any()
        .downcast_ref::<NiExtraData>()
        .expect("downcast to NiExtraData");
    let arr = ed
        .strings_array
        .as_ref()
        .expect("strings_array populated on Skyrim path");
    assert_eq!(arr.len(), 3, "all 3 SizedStrings must round-trip");
    assert_eq!(arr[0].as_deref(), Some("alpha"));
    assert_eq!(arr[1].as_deref(), Some("beta"));
    assert_eq!(arr[2].as_deref(), Some("gamma"));
}

/// Regression test for #614 / SK-D5-03 — `BSBoneLODExtraData`
/// must dispatch through `NiExtraData::parse` and populate the
/// `bone_lods` field with the array of `(distance, bone_name)`
/// pairs. Pre-fix the type name had no dispatch arm so every
/// Skyrim SE skeleton.nif (52 files in vanilla Meshes0.bsa) fell
/// into `NiUnknown` and dropped the parse rate from 100% to
/// ~99.7%.
///
/// The block carries the inherited `Name` field (string-table
/// index = -1 for `None`), then `BoneLOD Count: u32`, then N ×
/// `BoneLOD { Distance: u32, Bone Name: NiFixedString }`. The
/// string table here resolves indices 0/1/2 to `bone_a`, `bone_b`,
/// `bone_c` so the parsed names round-trip.
#[test]
fn skyrim_bs_bone_lod_extra_data_dispatches_and_parses() {
    use crate::blocks::extra_data::NiExtraData;

    let header = NifHeader {
        version: NifVersion(0x14020007),
        little_endian: true,
        user_version: 12,
        user_version_2: 83, // Skyrim LE — SKY_AND_LATER gate
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![
            Arc::from("bone_a"),
            Arc::from("bone_b"),
            Arc::from("bone_c"),
        ],
        max_string_length: 6,
        num_groups: 0,
    };

    let mut bytes = Vec::new();
    // Inherited Name: -1 (None) — 4 bytes.
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // BoneLOD Count: 3.
    bytes.extend_from_slice(&3u32.to_le_bytes());
    // 3 × (u32 distance + i32 string_table_index).
    bytes.extend_from_slice(&100u32.to_le_bytes());
    bytes.extend_from_slice(&0i32.to_le_bytes()); // bone_a
    bytes.extend_from_slice(&500u32.to_le_bytes());
    bytes.extend_from_slice(&1i32.to_le_bytes()); // bone_b
    bytes.extend_from_slice(&2000u32.to_le_bytes());
    bytes.extend_from_slice(&2i32.to_le_bytes()); // bone_c

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BSBoneLODExtraData", &mut stream, Some(bytes.len() as u32))
        .expect("BSBoneLODExtraData should dispatch (#614)");
    let ed = block
        .as_any()
        .downcast_ref::<NiExtraData>()
        .expect("downcast to NiExtraData");
    let arr = ed
        .bone_lods
        .as_ref()
        .expect("bone_lods populated for BSBoneLODExtraData");
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].0, 100);
    assert_eq!(arr[0].1.as_deref(), Some("bone_a"));
    assert_eq!(arr[1].0, 500);
    assert_eq!(arr[1].1.as_deref(), Some("bone_b"));
    assert_eq!(arr[2].0, 2000);
    assert_eq!(arr[2].1.as_deref(), Some("bone_c"));
    // Stream must be fully consumed — `block_size` recovery would
    // otherwise mask any drift introduced by a future field add.
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Oblivion-era empty NiNode body (no children, no effects, no
/// properties, identity transform). Used as the base bytes for
/// every NiNode subtype test in this module.
fn oblivion_empty_ninode_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    // NiObjectNET: name (empty inline) + empty extra data list + null controller
    d.extend_from_slice(&0u32.to_le_bytes()); // name len
    d.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
    d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                 // NiAVObject: flags (u16 for bsver<=26), identity transform (13 f32),
                                                 // empty properties list, null collision ref.
    d.extend_from_slice(&0u16.to_le_bytes()); // flags
                                              // transform: translation (3 f32)
    d.extend_from_slice(&0.0f32.to_le_bytes());
    d.extend_from_slice(&0.0f32.to_le_bytes());
    d.extend_from_slice(&0.0f32.to_le_bytes());
    // transform: rotation 3x3 identity
    for (i, row) in (0..3).zip([[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]) {
        let _ = i;
        for v in row {
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    // transform: scale
    d.extend_from_slice(&1.0f32.to_le_bytes());
    // properties list: empty
    d.extend_from_slice(&0u32.to_le_bytes());
    // collision_ref: null
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // NiNode children: empty
    d.extend_from_slice(&0u32.to_le_bytes());
    // NiNode effects: empty (Oblivion has_effects_list = true)
    d.extend_from_slice(&0u32.to_le_bytes());
    d
}

/// Regression test for issue #142: NiNode subtypes with trailing fields.
#[test]
fn oblivion_node_subtypes_dispatch_with_correct_payload() {
    use crate::blocks::node::{
        BsRangeNode, NiBillboardNode, NiLODNode, NiSortAdjustNode, NiSwitchNode,
    };

    let header = oblivion_header();
    let base = oblivion_empty_ninode_bytes();

    // NiBillboardNode: base + billboard_mode u16.
    let mut bb = base.clone();
    bb.extend_from_slice(&3u16.to_le_bytes()); // ALWAYS_FACE_CENTER
    let mut stream = NifStream::new(&bb, &header);
    let block = parse_block("NiBillboardNode", &mut stream, Some(bb.len() as u32))
        .expect("NiBillboardNode dispatch");
    let n = block.as_any().downcast_ref::<NiBillboardNode>().unwrap();
    assert_eq!(n.billboard_mode, 3);
    assert_eq!(stream.position(), bb.len() as u64);

    // NiSwitchNode: base + switch_flags u16 + index u32.
    let mut sw = base.clone();
    sw.extend_from_slice(&0x0003u16.to_le_bytes()); // UpdateOnlyActiveChild | UpdateControllers
    sw.extend_from_slice(&7u32.to_le_bytes());
    let mut stream = NifStream::new(&sw, &header);
    let block = parse_block("NiSwitchNode", &mut stream, Some(sw.len() as u32))
        .expect("NiSwitchNode dispatch");
    let n = block.as_any().downcast_ref::<NiSwitchNode>().unwrap();
    assert_eq!(n.switch_flags, 0x0003);
    assert_eq!(n.index, 7);
    assert_eq!(stream.position(), sw.len() as u64);

    // NiLODNode: NiSwitchNode body + lod_level_data ref i32.
    let mut lod = base.clone();
    lod.extend_from_slice(&0u16.to_le_bytes()); // switch_flags
    lod.extend_from_slice(&0u32.to_le_bytes()); // index
    lod.extend_from_slice(&42i32.to_le_bytes()); // lod_level_data
    let mut stream = NifStream::new(&lod, &header);
    let block =
        parse_block("NiLODNode", &mut stream, Some(lod.len() as u32)).expect("NiLODNode dispatch");
    let n = block.as_any().downcast_ref::<NiLODNode>().unwrap();
    assert_eq!(n.lod_level_data.index(), Some(42));
    assert_eq!(stream.position(), lod.len() as u64);

    // NiSortAdjustNode: base + sorting_mode u32 (v20.0.0.5 > 20.0.0.3 → no
    // trailing accumulator ref).
    let mut sa = base.clone();
    sa.extend_from_slice(&1u32.to_le_bytes()); // SORTING_OFF
    let mut stream = NifStream::new(&sa, &header);
    let block = parse_block("NiSortAdjustNode", &mut stream, Some(sa.len() as u32))
        .expect("NiSortAdjustNode dispatch");
    let n = block.as_any().downcast_ref::<NiSortAdjustNode>().unwrap();
    assert_eq!(n.sorting_mode, 1);
    assert_eq!(stream.position(), sa.len() as u64);

    // BSRangeNode (and its subclasses) — base + 3 bytes.
    for type_name in [
        "BSRangeNode",
        "BSBlastNode",
        "BSDamageStage",
        "BSDebrisNode",
    ] {
        let mut r = base.clone();
        r.push(5); // min
        r.push(10); // max
        r.push(7); // current
        let mut stream = NifStream::new(&r, &header);
        let block = parse_block(type_name, &mut stream, Some(r.len() as u32))
            .unwrap_or_else(|e| panic!("{type_name} dispatch: {e}"));
        let n = block.as_any().downcast_ref::<BsRangeNode>().unwrap();
        assert_eq!(n.min, 5);
        assert_eq!(n.max, 10);
        assert_eq!(n.current, 7);
        assert_eq!(stream.position(), r.len() as u64);
    }

    // Pure-alias variants — parse as plain NiNode with no trailing bytes.
    // BSFaceGenNiNode (Starfield, #727) is an unconfirmed-layout stub: the
    // FaceGen-coefficient trailing fields are unknown, so the alias just
    // catches the dispatch and lets `block_size` recovery skip whatever
    // trailing bytes the real wire layout carries. Test asserts the
    // dispatch lands on `NiNode` so the FaceMeshes.ba2 corpus stops
    // demoting all 1,282 face NIFs to NiUnknown.
    for type_name in [
        "AvoidNode",
        "NiBSAnimationNode",
        "NiBSParticleNode",
        "BSFaceGenNiNode",
    ] {
        let mut stream = NifStream::new(&base, &header);
        let block = parse_block(type_name, &mut stream, Some(base.len() as u32))
            .unwrap_or_else(|e| panic!("{type_name} dispatch: {e}"));
        assert!(block
            .as_any()
            .downcast_ref::<crate::blocks::NiNode>()
            .is_some());
        assert_eq!(stream.position(), base.len() as u64);
    }
}

/// Regression test for issue #125: `NiCollisionObject` (the non-Havok
/// base class) must dispatch to its own parser so Oblivion NIFs that
/// reference it directly don't cascade-fail on the unknown-block
/// fallback. The block is trivially small — a single target ref —
/// and we only need to prove the parser consumes exactly 4 bytes and
/// downcasts cleanly.
#[test]
fn oblivion_ni_collision_object_base_dispatches() {
    use crate::blocks::collision::NiCollisionObjectBase;

    let header = oblivion_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&42i32.to_le_bytes()); // target ref (i32 -> BlockRef(42))

    let expected_len = bytes.len();
    let mut stream = NifStream::new(&bytes, &header);
    // Pass block_size=None to mimic Oblivion where the header has
    // no block_sizes table. Before the fix this arm returned Err.
    let block = parse_block("NiCollisionObject", &mut stream, None)
        .expect("NiCollisionObject must dispatch without block_size on Oblivion");
    let co = block
        .as_any()
        .downcast_ref::<NiCollisionObjectBase>()
        .expect("downcast to NiCollisionObjectBase");
    assert_eq!(co.target_ref.index(), Some(42));
    assert_eq!(stream.position() as usize, expected_len);
}

/// Regression test for issue #117: the 7 Havok constraint types must
/// dispatch to byte-exact parsers on Oblivion so a constraint block
/// on an Oblivion .nif no longer cascade-fails the parse loop.
/// Builds a 16-byte `bhkConstraintCInfo` base + a zero-filled
/// type-specific payload for each constraint type and asserts the
/// parser consumes exactly the expected number of bytes.
#[test]
fn oblivion_havok_constraints_dispatch_byte_exact() {
    use crate::blocks::collision::BhkConstraint;

    let header = oblivion_header();

    /// Construct a valid bhkConstraintCInfo base (16 bytes) with
    /// known entity refs and a non-trivial priority.
    fn base_bytes() -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&2u32.to_le_bytes()); // num_entities
        d.extend_from_slice(&7i32.to_le_bytes()); // entity_a
        d.extend_from_slice(&11i32.to_le_bytes()); // entity_b
        d.extend_from_slice(&1u32.to_le_bytes()); // priority
        d
    }

    // (type_name, payload_size_after_base) — Oblivion sizes per
    // nif.xml with #NI_BS_LTE_16# active. Total = 16 + payload.
    let cases: [(&'static str, usize); 6] = [
        ("bhkBallAndSocketConstraint", 32),
        ("bhkHingeConstraint", 80),
        ("bhkRagdollConstraint", 120),
        ("bhkLimitedHingeConstraint", 124),
        ("bhkPrismaticConstraint", 140),
        ("bhkStiffSpringConstraint", 36),
    ];

    for (type_name, payload) in cases {
        let mut bytes = base_bytes();
        bytes.resize(bytes.len() + payload, 0u8);
        let expected_len = bytes.len();

        let mut stream = NifStream::new(&bytes, &header);
        let block = parse_block(type_name, &mut stream, None)
            .unwrap_or_else(|e| panic!("{type_name} dispatch failed: {e}"));
        let c = block
            .as_any()
            .downcast_ref::<BhkConstraint>()
            .unwrap_or_else(|| panic!("{type_name} didn't downcast to BhkConstraint"));
        assert_eq!(c.type_name, type_name);
        assert_eq!(c.entity_a.index(), Some(7));
        assert_eq!(c.entity_b.index(), Some(11));
        assert_eq!(c.priority, 1);
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "{type_name} consumed {} bytes, expected {}",
            stream.position(),
            expected_len,
        );
    }

    // Malleable constraint — runtime dispatch on the wrapped type.
    // Layout on Oblivion: base(16) + wrapped_type u32(4) + nested
    // bhkConstraintCInfo(16) + inner CInfo(N) + tau+damping(8).
    // Total = 44 + inner. Wrapped type 2 is LimitedHinge (inner=124).
    let mut mbytes = base_bytes();
    mbytes.extend_from_slice(&2u32.to_le_bytes()); // wrapped type = LimitedHinge
    mbytes.extend_from_slice(&2u32.to_le_bytes()); // nested num_entities
    mbytes.extend_from_slice(&3i32.to_le_bytes()); // nested entity_a
    mbytes.extend_from_slice(&4i32.to_le_bytes()); // nested entity_b
    mbytes.extend_from_slice(&0u32.to_le_bytes()); // nested priority
    mbytes.resize(mbytes.len() + 124, 0u8); // inner LimitedHinge CInfo
    mbytes.resize(mbytes.len() + 8, 0u8); // tau + damping
    let expected_len = mbytes.len();

    let mut stream = NifStream::new(&mbytes, &header);
    let block = parse_block("bhkMalleableConstraint", &mut stream, None)
        .expect("bhkMalleableConstraint dispatch failed");
    let c = block
        .as_any()
        .downcast_ref::<BhkConstraint>()
        .expect("malleable didn't downcast to BhkConstraint");
    assert_eq!(c.type_name, "bhkMalleableConstraint");
    assert_eq!(stream.position() as usize, expected_len);
}

/// Regression test for issue #160: `NiAVObject::parse` and
/// `NiNode::parse` must use the raw `bsver()` for binary-layout
/// decisions so that non-Bethesda Gamebryo files classified as
/// `NifVariant::Unknown` still read the correct fields. Previously
/// the variant-based `has_properties_list` / `has_effects_list`
/// helpers returned `false` for `Unknown`, so an Unknown variant
/// with `bsver <= 34` (pre-Skyrim) would skip the properties list
/// and mis-align the stream on every NiAVObject.
#[test]
fn ni_node_parses_unknown_variant_with_low_bsver() {
    use crate::header::NifHeader;
    use crate::stream::NifStream;
    use crate::version::{NifVariant, NifVersion};
    use std::sync::Arc;

    // Craft a header that detects as `Unknown`: the only path into
    // that variant on `detect()` is `uv >= 11` without matching
    // any specific (uv, uv2) arm. uv=13, uv2=0 lands there and
    // also gives us `bsver() == 0` so the pre-Skyrim binary layout
    // applies.
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 13,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("Root")],
        max_string_length: 4,
        num_groups: 0,
    };
    // Sanity: this combo really does classify as Unknown.
    assert_eq!(
        NifVariant::detect(header.version, header.user_version, header.user_version_2),
        NifVariant::Unknown
    );

    // Build a minimal NiNode body matching the pre-Skyrim layout
    // (has properties list + has effects list). Identity transform,
    // empty children / properties / effects lists, null collision
    // ref with the distinctive sentinel value 0xFFFFFFFF so we can
    // detect a stream misalignment at the `collision_ref` field.
    let mut data = Vec::new();
    // NiObjectNET: name index 0, extra_data count 0, controller -1
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // NiAVObject: flags u16 (bsver <= 26), transform, properties list,
    // collision ref. Note flags is u16 here because bsver=0 < 26.
    data.extend_from_slice(&0u16.to_le_bytes()); // flags
    for _ in 0..3 {
        data.extend_from_slice(&0.0f32.to_le_bytes()); // translation
    }
    for row in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        for v in row {
            data.extend_from_slice(&v.to_le_bytes());
        }
    }
    data.extend_from_slice(&1.0f32.to_le_bytes()); // scale
                                                   // Properties list — this is the field `has_properties_list`
                                                   // gates. Old buggy path would skip it and misread the next
                                                   // 4 bytes as `collision_ref`.
    data.extend_from_slice(&0u32.to_le_bytes()); // properties count
    data.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref (null)
                                                    // NiNode children + effects
    data.extend_from_slice(&0u32.to_le_bytes()); // children count
    data.extend_from_slice(&0u32.to_le_bytes()); // effects count

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block("NiNode", &mut stream, Some(data.len() as u32))
        .expect("NiNode must parse under Unknown variant + bsver 0");
    let node = block
        .as_any()
        .downcast_ref::<crate::blocks::NiNode>()
        .expect("downcast to NiNode");
    assert!(
        node.av.collision_ref.is_null(),
        "Unknown variant with bsver=0 must still read properties list \
             so collision_ref lands on the right 4 bytes"
    );
    assert!(node.children.is_empty());
    assert!(node.effects.is_empty());
    assert_eq!(stream.position() as usize, data.len());
}

/// Regression: #159 — BSTreeNode (Skyrim SpeedTree) must dispatch
/// to its own parser and consume the two trailing NiNode ref lists
/// (`Bones 1` + `Bones 2`). Previously aliased to plain NiNode so
/// the two ref lists were silently dropped.
#[test]
fn bs_tree_node_dispatches_with_both_bone_lists() {
    use crate::blocks::node::BsTreeNode;

    let header = oblivion_header();
    let mut bytes = oblivion_empty_ninode_bytes();
    // bones_1: 3 refs (7, 8, 9)
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&7i32.to_le_bytes());
    bytes.extend_from_slice(&8i32.to_le_bytes());
    bytes.extend_from_slice(&9i32.to_le_bytes());
    // bones_2: 2 refs (10, 11)
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&10i32.to_le_bytes());
    bytes.extend_from_slice(&11i32.to_le_bytes());

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BSTreeNode", &mut stream, Some(bytes.len() as u32))
        .expect("BSTreeNode should dispatch through BsTreeNode::parse");
    let tree = block
        .as_any()
        .downcast_ref::<BsTreeNode>()
        .expect("BSTreeNode did not downcast to BsTreeNode");
    assert_eq!(tree.bones_1.len(), 3);
    assert_eq!(tree.bones_1[0].index(), Some(7));
    assert_eq!(tree.bones_1[1].index(), Some(8));
    assert_eq!(tree.bones_1[2].index(), Some(9));
    assert_eq!(tree.bones_2.len(), 2);
    assert_eq!(tree.bones_2[0].index(), Some(10));
    assert_eq!(tree.bones_2[1].index(), Some(11));
    assert_eq!(stream.position(), bytes.len() as u64);
}

/// Regression: #148 — BSMultiBoundNode must dispatch to its own
/// parser and read the trailing `multi_bound_ref` (BlockRef, always)
/// + `culling_mode` (u32, Skyrim+ only). Previously aliased to plain
/// NiNode so the multi-bound linkage was silently dropped.
#[test]
fn bs_multi_bound_node_dispatches_with_multi_bound_ref() {
    use crate::blocks::node::BsMultiBoundNode;

    let header = oblivion_header(); // bsver 0 — no culling_mode field
    let mut bytes = oblivion_empty_ninode_bytes();
    // multi_bound_ref = 42
    bytes.extend_from_slice(&42i32.to_le_bytes());

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BSMultiBoundNode", &mut stream, Some(bytes.len() as u32))
        .expect("BSMultiBoundNode should dispatch through BsMultiBoundNode::parse");
    let node = block
        .as_any()
        .downcast_ref::<BsMultiBoundNode>()
        .expect("BSMultiBoundNode did not downcast to BsMultiBoundNode");
    assert_eq!(node.multi_bound_ref.index(), Some(42));
    assert_eq!(node.culling_mode, 0); // default when bsver < 83
    assert_eq!(stream.position(), bytes.len() as u64);
}

/// Regression #158 / #365: BSPackedCombined[Shared]GeomDataExtra
/// must dispatch to its own parser and fully decode the
/// variable-size per-object tail (not just skip-via-block_size).
///
/// Constructs a valid wire payload with `num_data = 1` per
/// variant — one `BSPackedGeomData` (baked) or one
/// `BSPackedGeomObject` + one `BSPackedSharedGeomData` (shared) —
/// and checks that counts, per-instance combined records, vertex
/// bytes (for the baked variant), and triangle indices all
/// round-trip.
#[test]
fn bs_packed_combined_geom_data_extra_fully_parses_variable_tail() {
    use crate::blocks::extra_data::{BsPackedCombinedGeomDataExtra, BsPackedCombinedPayload};

    let header = oblivion_header();

    // Fixed header — identical between the two variants except for
    // what follows the top-level `num_data`.
    let mut fixed = Vec::new();
    fixed.extend_from_slice(&0u32.to_le_bytes()); // name: empty inline string
                                                  // vertex_desc: low nibble = 4 → 16-byte per-vertex stride.
    let outer_desc: u64 = 0x0000_0000_0000_0004;
    fixed.extend_from_slice(&outer_desc.to_le_bytes());
    fixed.extend_from_slice(&42u32.to_le_bytes()); // num_vertices
    fixed.extend_from_slice(&24u32.to_le_bytes()); // num_triangles
    fixed.extend_from_slice(&1u32.to_le_bytes()); // unknown_flags_1
    fixed.extend_from_slice(&2u32.to_le_bytes()); // unknown_flags_2
    fixed.extend_from_slice(&1u32.to_le_bytes()); // num_data = 1

    // One `BSPackedGeomDataCombined` — 72 bytes: f32 + NiTransform STRUCT + NiBound.
    // NiTransform STRUCT (nif.xml line 1808) ships rotation FIRST (9 f32),
    // then translation (3 f32), then scale (1 f32) — opposite to
    // NiAVObject's inline Translation→Rotation→Scale layout.
    //
    // #767 / 2026-04-30 — non-identity rotation chosen so scrambled
    // field order would assert-fail. Pre-fix the parser used
    // `read_ni_transform()` (NiAVObject order), reading the first 3
    // floats of the rotation matrix as `translation`, the next 3 as
    // rotation row 0, etc. Identity rotation (1,0,0,0,1,0,0,0,1) hides
    // the scrambling because the bytes happen to be plausible. A
    // 90°-Z rotation matrix `[[0,-1,0], [1,0,0], [0,0,1]]` plus
    // distinguishable translation makes any field misorder visible.
    let mut combined = Vec::new();
    combined.extend_from_slice(&0.5f32.to_le_bytes()); // grayscale_to_palette_scale
                                                       // rotation 90° around Z (CCW): row 0=(0,-1,0), row 1=(1,0,0), row 2=(0,0,1)
    for f in [0.0f32, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0] {
        combined.extend_from_slice(&f.to_le_bytes());
    }
    for f in [10.0f32, 20.0, 30.0] {
        // translation — distinguishable from rotation values
        combined.extend_from_slice(&f.to_le_bytes());
    }
    combined.extend_from_slice(&2.5f32.to_le_bytes()); // scale (non-unity)
    for f in [5.0f32, 6.0, 7.0, 42.0] {
        // bounding sphere
        combined.extend_from_slice(&f.to_le_bytes());
    }
    assert_eq!(combined.len(), 72);

    // Baked variant tail: one BSPackedGeomData with num_verts=2,
    // one combined record, vertex_desc (stride 16), 2×16 vertex
    // bytes, and tri_count_lod0=1 triangle.
    let mut baked_tail = Vec::new();
    baked_tail.extend_from_slice(&2u32.to_le_bytes()); // num_verts
    baked_tail.extend_from_slice(&1u32.to_le_bytes()); // lod_levels
    baked_tail.extend_from_slice(&1u32.to_le_bytes()); // tri_count_lod0
    baked_tail.extend_from_slice(&0u32.to_le_bytes()); // tri_offset_lod0
    baked_tail.extend_from_slice(&0u32.to_le_bytes()); // tri_count_lod1
    baked_tail.extend_from_slice(&0u32.to_le_bytes()); // tri_offset_lod1
    baked_tail.extend_from_slice(&0u32.to_le_bytes()); // tri_count_lod2
    baked_tail.extend_from_slice(&0u32.to_le_bytes()); // tri_offset_lod2
    baked_tail.extend_from_slice(&1u32.to_le_bytes()); // num_combined
    baked_tail.extend_from_slice(&combined);
    // Per-vertex stride comes from low nibble of `inner_desc` (4 quads = 16 bytes).
    let inner_desc: u64 = 0x0000_0000_0000_0004;
    baked_tail.extend_from_slice(&inner_desc.to_le_bytes());
    // 2 vertices × 16 bytes = 32 bytes of vertex data.
    baked_tail.extend_from_slice(&[0xAAu8; 32]);
    // 1 triangle: u16 indices [0, 1, 0]
    for idx in [0u16, 1, 0] {
        baked_tail.extend_from_slice(&idx.to_le_bytes());
    }

    // Shared variant tail: one BSPackedGeomObject (8 bytes) then
    // one BSPackedSharedGeomData (header-only, same shape as baked
    // but no vertex / triangle arrays).
    let mut shared_tail = Vec::new();
    shared_tail.extend_from_slice(&0xCAFEBABEu32.to_le_bytes()); // filename_hash
    shared_tail.extend_from_slice(&0x10u32.to_le_bytes()); // data_offset
    shared_tail.extend_from_slice(&2u32.to_le_bytes()); // num_verts
    shared_tail.extend_from_slice(&1u32.to_le_bytes()); // lod_levels
    shared_tail.extend_from_slice(&1u32.to_le_bytes());
    shared_tail.extend_from_slice(&0u32.to_le_bytes());
    shared_tail.extend_from_slice(&0u32.to_le_bytes());
    shared_tail.extend_from_slice(&0u32.to_le_bytes());
    shared_tail.extend_from_slice(&0u32.to_le_bytes());
    shared_tail.extend_from_slice(&0u32.to_le_bytes());
    shared_tail.extend_from_slice(&1u32.to_le_bytes()); // num_combined
    shared_tail.extend_from_slice(&combined);
    shared_tail.extend_from_slice(&inner_desc.to_le_bytes());

    // ---- Baked ----
    let mut baked_bytes = fixed.clone();
    baked_bytes.extend_from_slice(&baked_tail);
    {
        let mut stream = NifStream::new(&baked_bytes, &header);
        let block = parse_block(
            "BSPackedCombinedGeomDataExtra",
            &mut stream,
            Some(baked_bytes.len() as u32),
        )
        .expect("baked parse");
        let extra = block
            .as_any()
            .downcast_ref::<BsPackedCombinedGeomDataExtra>()
            .expect("baked downcast");
        assert_eq!(extra.num_data, 1);
        let baked = match &extra.payload {
            BsPackedCombinedPayload::Baked(v) => v,
            _ => panic!("baked variant should produce Baked payload"),
        };
        assert_eq!(baked.len(), 1);
        assert_eq!(baked[0].num_verts, 2);
        assert_eq!(baked[0].tri_count_lod0, 1);
        assert_eq!(baked[0].combined.len(), 1);
        let c = &baked[0].combined[0];
        assert!((c.grayscale_to_palette_scale - 0.5).abs() < 1e-6);
        // #767 regression: NiTransform STRUCT field order (Rotation →
        // Translation → Scale). With the pre-fix `read_ni_transform()`
        // (NiAVObject order), translation would read the first 3
        // rotation floats (0, -1, 0) and rotation row 0 would shift
        // into rotation row 1's slot — these assertions fail in that
        // case.
        assert!((c.transform.translation.x - 10.0).abs() < 1e-6);
        assert!((c.transform.translation.y - 20.0).abs() < 1e-6);
        assert!((c.transform.translation.z - 30.0).abs() < 1e-6);
        assert!((c.transform.scale - 2.5).abs() < 1e-6);
        // 90°-Z rotation: rows[0] = (0, -1, 0), rows[1] = (1, 0, 0)
        assert!((c.transform.rotation.rows[0][0] - 0.0).abs() < 1e-6);
        assert!((c.transform.rotation.rows[0][1] - (-1.0)).abs() < 1e-6);
        assert!((c.transform.rotation.rows[1][0] - 1.0).abs() < 1e-6);
        assert_eq!(baked[0].vertex_data.len(), 32);
        assert_eq!(baked[0].triangles, vec![[0, 1, 0]]);
        assert_eq!(stream.position() as usize, baked_bytes.len());
    }

    // ---- Shared ----
    let mut shared_bytes = fixed.clone();
    shared_bytes.extend_from_slice(&shared_tail);
    {
        let mut stream = NifStream::new(&shared_bytes, &header);
        let block = parse_block(
            "BSPackedCombinedSharedGeomDataExtra",
            &mut stream,
            Some(shared_bytes.len() as u32),
        )
        .expect("shared parse");
        let extra = block
            .as_any()
            .downcast_ref::<BsPackedCombinedGeomDataExtra>()
            .expect("shared downcast");
        assert_eq!(extra.num_data, 1);
        let (objects, data) = match &extra.payload {
            BsPackedCombinedPayload::Shared { objects, data } => (objects, data),
            _ => panic!("shared variant should produce Shared payload"),
        };
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].filename_hash, 0xCAFEBABE);
        assert_eq!(objects[0].data_offset, 0x10);
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].num_verts, 2);
        assert_eq!(data[0].combined.len(), 1);
        assert_eq!(stream.position() as usize, shared_bytes.len());
    }
}

/// Regression test for issue #108: `BSConnectPoint::Children.Skinned`
/// is a `byte` per nif.xml, not a `uint`. The previous parser read
/// 4 bytes instead of 1, eating the first 3 bytes of the following
/// count field. Verifies the byte read preserves the subsequent
/// count and string fields exactly.
#[test]
fn bs_connect_point_children_reads_skinned_as_byte() {
    use crate::blocks::extra_data::BsConnectPointChildren;

    let header = oblivion_header(); // inline-string path (pre-20.1.0.1)
    let mut data = Vec::new();
    // NiExtraData base: empty inline name
    data.extend_from_slice(&0u32.to_le_bytes());
    // Skinned: 1 (true) — ONE byte, not four.
    data.push(1u8);
    // Num Connect Points: u32 = 2
    data.extend_from_slice(&2u32.to_le_bytes());
    // Two sized-string entries.
    let s1 = b"HEAD";
    data.extend_from_slice(&(s1.len() as u32).to_le_bytes());
    data.extend_from_slice(s1);
    let s2 = b"CAMERA";
    data.extend_from_slice(&(s2.len() as u32).to_le_bytes());
    data.extend_from_slice(s2);

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "BSConnectPoint::Children",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("BSConnectPoint::Children should dispatch");
    let cp = block
        .as_any()
        .downcast_ref::<BsConnectPointChildren>()
        .expect("downcast to BsConnectPointChildren");
    assert!(cp.skinned, "skinned byte should decode to true");
    assert_eq!(cp.point_names.len(), 2);
    assert_eq!(cp.point_names[0], "HEAD");
    assert_eq!(cp.point_names[1], "CAMERA");
    assert_eq!(
        stream.position() as usize,
        expected_len,
        "BSConnectPoint::Children over-read the skinned flag"
    );
}

/// Build an "empty NiAVObject" body sized for Oblivion. Same prefix
/// as the NiNode helper, minus the NiNode-specific children+effects
/// trailers. Used for NiLight bodies.
fn oblivion_niavobject_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&0u32.to_le_bytes()); // name len (empty inline)
    d.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
    d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
    d.extend_from_slice(&0u16.to_le_bytes()); // flags
    for _ in 0..3 {
        d.extend_from_slice(&0.0f32.to_le_bytes()); // translation
    }
    for row in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        for v in row {
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
    d.extend_from_slice(&0u32.to_le_bytes()); // empty properties list
    d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
    d
}

/// Regression test for issue #156: NiLight hierarchy dispatch + payload.
#[test]
fn oblivion_lights_parse_with_attenuation_and_color() {
    use crate::blocks::light::{NiAmbientLight, NiPointLight, NiSpotLight};

    let header = oblivion_header();
    let av = oblivion_niavobject_bytes();

    // Common NiDynamicEffect + NiLight tail for an Oblivion torch:
    //   switch_state:u8=1, num_affected_nodes:u32=0,
    //   dimmer:f32=1.0,
    //   ambient:(0,0,0), diffuse:(1.0, 0.6, 0.2), specular:(0,0,0)
    fn dynamic_light_tail() -> Vec<u8> {
        let mut d = Vec::new();
        d.push(1u8); // switch_state
        d.extend_from_slice(&0u32.to_le_bytes()); // affected nodes count
        d.extend_from_slice(&1.0f32.to_le_bytes()); // dimmer
        for _ in 0..3 {
            d.extend_from_slice(&0.0f32.to_le_bytes()); // ambient color
        }
        for &c in &[1.0f32, 0.6, 0.2] {
            d.extend_from_slice(&c.to_le_bytes()); // diffuse color
        }
        for _ in 0..3 {
            d.extend_from_slice(&0.0f32.to_le_bytes()); // specular color
        }
        d
    }

    // NiAmbientLight: base + dynamic_light_tail, nothing else.
    let mut amb = av.clone();
    amb.extend_from_slice(&dynamic_light_tail());
    let mut stream = NifStream::new(&amb, &header);
    let block = parse_block("NiAmbientLight", &mut stream, Some(amb.len() as u32))
        .expect("NiAmbientLight dispatch");
    let light = block.as_any().downcast_ref::<NiAmbientLight>().unwrap();
    assert_eq!(light.base.dimmer, 1.0);
    assert!((light.base.diffuse_color.g - 0.6).abs() < 1e-6);
    assert_eq!(stream.position(), amb.len() as u64);

    // NiPointLight: base + tail + (const=1.0, lin=0.01, quad=0.0).
    let mut pl = av.clone();
    pl.extend_from_slice(&dynamic_light_tail());
    pl.extend_from_slice(&1.0f32.to_le_bytes()); // constant
    pl.extend_from_slice(&0.01f32.to_le_bytes()); // linear
    pl.extend_from_slice(&0.0f32.to_le_bytes()); // quadratic
    let mut stream = NifStream::new(&pl, &header);
    let block = parse_block("NiPointLight", &mut stream, Some(pl.len() as u32))
        .expect("NiPointLight dispatch");
    let p = block.as_any().downcast_ref::<NiPointLight>().unwrap();
    assert_eq!(p.constant_attenuation, 1.0);
    assert!((p.linear_attenuation - 0.01).abs() < 1e-6);
    assert_eq!(stream.position(), pl.len() as u64);

    // NiSpotLight: NiPointLight body + outer + exponent (Oblivion
    // v20.0.0.5 < 20.2.0.5, so no inner_spot_angle).
    let mut sl = av.clone();
    sl.extend_from_slice(&dynamic_light_tail());
    sl.extend_from_slice(&1.0f32.to_le_bytes()); // constant
    sl.extend_from_slice(&0.01f32.to_le_bytes()); // linear
    sl.extend_from_slice(&0.0f32.to_le_bytes()); // quadratic
    sl.extend_from_slice(&(std::f32::consts::FRAC_PI_4).to_le_bytes()); // outer
    sl.extend_from_slice(&2.0f32.to_le_bytes()); // exponent
    let mut stream = NifStream::new(&sl, &header);
    let block = parse_block("NiSpotLight", &mut stream, Some(sl.len() as u32))
        .expect("NiSpotLight dispatch");
    let s = block.as_any().downcast_ref::<NiSpotLight>().unwrap();
    assert!((s.outer_spot_angle - std::f32::consts::FRAC_PI_4).abs() < 1e-6);
    assert_eq!(s.inner_spot_angle, 0.0); // not in this version
    assert_eq!(s.exponent, 2.0);
    assert_eq!(stream.position(), sl.len() as u64);
}

/// Build an "empty NiAVObject" body sized for FO4+ (NIF 20.2.0.7,
/// bsver 130). Same field order as Oblivion but: `name` is a string-
/// table index (i32, -1 = absent), `flags` is u32, properties list is
/// gone (bsver > 34), and the collision_ref is still present (NIF v
/// >= 10.0.1.0).
fn fo4_niavobject_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&(-1i32).to_le_bytes()); // name idx (none)
    d.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
    d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
    d.extend_from_slice(&0u32.to_le_bytes()); // flags (u32 since bsver > 26)
    for _ in 0..3 {
        d.extend_from_slice(&0.0f32.to_le_bytes()); // translation
    }
    for row in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        for v in row {
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
                                                // No properties list (bsver=130 > 34, dropped per nif.xml).
    d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
    d
}

/// Regression for #721 (NIF-D5-06): FO4+ NiLight reparents directly
/// onto NiAVObject — `Switch State` and the `Affected Nodes` list both
/// carry `vercond="#NI_BS_LT_FO4#"` in nif.xml line 3499-3504. Pre-fix
/// the parser keyed only on NIF version (always true for FO4) and
/// consumed 5 bytes of NiLight color data as the dropped fields,
/// throwing every mesh-embedded light through `block_size` recovery.
/// 681 light blocks across FO4 / FO76 / SF Meshes archives demoted.
///
/// This fixture has NO `switch_state` byte and NO `affected_nodes`
/// count/list — directly into NiLight `dimmer + 3 colors` after the
/// NiAVObject base. A pre-#721 parser would over-read by 5 bytes, the
/// `dimmer` would land on garbage, and the per-block size assertion
/// at the end would fail.
#[test]
fn fo4_point_light_skips_dynamic_effect_tail() {
    use crate::blocks::light::NiPointLight;

    let header = fo4_header();
    let mut bytes = fo4_niavobject_bytes();
    // No NiDynamicEffect tail on FO4+.
    bytes.extend_from_slice(&0.85f32.to_le_bytes()); // dimmer
    for _ in 0..3 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // ambient
    }
    for &c in &[1.0f32, 0.4, 0.1] {
        bytes.extend_from_slice(&c.to_le_bytes()); // diffuse
    }
    for _ in 0..3 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // specular
    }
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // constant_attenuation
    bytes.extend_from_slice(&0.02f32.to_le_bytes()); // linear
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // quadratic

    let expected_len = bytes.len();
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiPointLight", &mut stream, Some(bytes.len() as u32))
        .expect("FO4 NiPointLight must dispatch and consume the stream cleanly");
    let p = block.as_any().downcast_ref::<NiPointLight>().unwrap();
    // dimmer reads correctly only when the parser SKIPPED the FO4-
    // dropped NiDynamicEffect tail. Pre-fix this lands on the
    // ambient_r byte and reads as garbage.
    assert!(
        (p.base.dimmer - 0.85).abs() < 1e-6,
        "dimmer must read 0.85 (got {}) — parser likely over-read the FO4 NiDynamicEffect tail",
        p.base.dimmer
    );
    assert!((p.base.diffuse_color.r - 1.0).abs() < 1e-6);
    assert!((p.base.diffuse_color.g - 0.4).abs() < 1e-6);
    assert_eq!(p.constant_attenuation, 1.0);
    assert!((p.linear_attenuation - 0.02).abs() < 1e-6);
    assert_eq!(stream.position(), expected_len as u64);
}

/// Regression for #722 (NIF-D5-07): BSClothExtraData inherits
/// BSExtraData, which nif.xml line 3222 explicitly excludes from the
/// `Name` field via `excludeT="BSExtraData"`. Pre-fix the parser
/// called `read_extra_data_name` here, consuming 4 bytes (string-
/// table index) of the cloth payload as a name reference and then
/// reading the next 4 bytes as the length. 1,523 / 1,523 cloth-
/// bearing FO4 / FO76 / SF NIFs failed through `block_size` recovery
/// — capes, flags, curtains, hair fell back to rigid geometry.
#[test]
fn fo4_bs_cloth_extra_data_omits_name_field() {
    use crate::blocks::extra_data::BsClothExtraData;

    let header = fo4_header();
    // BSExtraData omits the NiExtraData Name. Wire layout reduces to
    // `length: u32 + data: u8[length]`. Use a sentinel payload so an
    // off-by-4 (pre-fix) consumes the length bytes as part of the
    // payload and trips the consume-exact assertion below.
    let payload: &[u8] = b"CLOTHBLOB";
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);

    let expected_len = bytes.len();
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BSClothExtraData", &mut stream, Some(bytes.len() as u32))
        .expect("BSClothExtraData must dispatch on FO4 without consuming a phantom Name field");
    let cloth = block.as_any().downcast_ref::<BsClothExtraData>().unwrap();
    assert!(
        cloth.name.is_none(),
        "BSExtraData explicitly excludes Name (nif.xml line 3222 `excludeT`); name must stay None"
    );
    assert_eq!(cloth.data.as_slice(), payload);
    assert_eq!(stream.position(), expected_len as u64);
}

/// Regression test for issue #154: NiUVController + NiUVData.
#[test]
fn oblivion_uv_controller_and_data_roundtrip() {
    use crate::blocks::controller::NiUVController;
    use crate::blocks::interpolator::NiUVData;

    let header = oblivion_header();

    // NiUVController: NiTimeControllerBase (26 bytes) + u16 target + i32 data ref.
    let mut uvc = Vec::new();
    uvc.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
    uvc.extend_from_slice(&0u16.to_le_bytes()); // flags
    uvc.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
    uvc.extend_from_slice(&0.0f32.to_le_bytes()); // phase
    uvc.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
    uvc.extend_from_slice(&2.5f32.to_le_bytes()); // stop_time
    uvc.extend_from_slice(&(-1i32).to_le_bytes()); // target_ref
    uvc.extend_from_slice(&0u16.to_le_bytes()); // target_attribute
    uvc.extend_from_slice(&42i32.to_le_bytes()); // data ref
    let mut stream = NifStream::new(&uvc, &header);
    let block = parse_block("NiUVController", &mut stream, Some(uvc.len() as u32))
        .expect("NiUVController dispatch");
    let c = block.as_any().downcast_ref::<NiUVController>().unwrap();
    assert_eq!(c.target_attribute, 0);
    assert_eq!(c.data_ref.index(), Some(42));
    assert!((c.base.stop_time - 2.5).abs() < 1e-6);
    assert_eq!(stream.position(), uvc.len() as u64);

    // NiUVData: four KeyGroup<FloatKey>. First group has 2 linear
    // keys scrolling U from 0→1; the rest are empty.
    let mut uvd = Vec::new();
    // Group 0: num_keys=2, key_type=Linear(1), key (time, value)×2
    uvd.extend_from_slice(&2u32.to_le_bytes());
    uvd.extend_from_slice(&1u32.to_le_bytes()); // KeyType::Linear
    uvd.extend_from_slice(&0.0f32.to_le_bytes()); // t=0
    uvd.extend_from_slice(&0.0f32.to_le_bytes()); // v=0
    uvd.extend_from_slice(&1.0f32.to_le_bytes()); // t=1
    uvd.extend_from_slice(&1.0f32.to_le_bytes()); // v=1
                                                  // Groups 1-3: num_keys=0 (no key_type field when empty).
    for _ in 0..3 {
        uvd.extend_from_slice(&0u32.to_le_bytes());
    }
    let mut stream = NifStream::new(&uvd, &header);
    let block =
        parse_block("NiUVData", &mut stream, Some(uvd.len() as u32)).expect("NiUVData dispatch");
    let d = block.as_any().downcast_ref::<NiUVData>().unwrap();
    assert_eq!(d.groups[0].keys.len(), 2);
    assert_eq!(d.groups[0].keys[1].value, 1.0);
    assert!(d.groups[1].keys.is_empty());
    assert!(d.groups[2].keys.is_empty());
    assert!(d.groups[3].keys.is_empty());
    assert_eq!(stream.position(), uvd.len() as u64);
}

/// Regression test for issue #153: NiCamera parsing.
#[test]
fn oblivion_ni_camera_roundtrip() {
    use crate::blocks::node::NiCamera;

    let header = oblivion_header();
    let mut bytes = oblivion_niavobject_bytes();
    // camera_flags u16
    bytes.extend_from_slice(&0u16.to_le_bytes());
    // frustum left/right/top/bottom
    bytes.extend_from_slice(&(-0.5f32).to_le_bytes());
    bytes.extend_from_slice(&0.5f32.to_le_bytes());
    bytes.extend_from_slice(&0.3f32.to_le_bytes());
    bytes.extend_from_slice(&(-0.3f32).to_le_bytes());
    // frustum near / far
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&5000.0f32.to_le_bytes());
    // use_orthographic byte bool = 0
    bytes.push(0u8);
    // viewport left/right/top/bottom
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    // lod_adjust
    bytes.extend_from_slice(&1.5f32.to_le_bytes());
    // scene_ref
    bytes.extend_from_slice(&9i32.to_le_bytes());
    // num_screen_polygons, num_screen_textures (both u32, both 0 on disk)
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());

    let mut stream = NifStream::new(&bytes, &header);
    let block =
        parse_block("NiCamera", &mut stream, Some(bytes.len() as u32)).expect("NiCamera dispatch");
    let c = block.as_any().downcast_ref::<NiCamera>().unwrap();
    assert!((c.frustum_right - 0.5).abs() < 1e-6);
    assert!((c.frustum_far - 5000.0).abs() < 1e-6);
    assert!(!c.use_orthographic);
    assert!((c.lod_adjust - 1.5).abs() < 1e-6);
    assert_eq!(c.scene_ref.index(), Some(9));
    assert_eq!(c.num_screen_polygons, 0);
    assert_eq!(c.num_screen_textures, 0);
    assert_eq!(stream.position(), bytes.len() as u64);
}

/// Regression test for issue #163: NiTextureEffect.
#[test]
fn oblivion_ni_texture_effect_roundtrip() {
    use crate::blocks::texture::NiTextureEffect;

    let header = oblivion_header();
    let mut bytes = oblivion_niavobject_bytes();
    // NiDynamicEffect base: switch_state=1, num_affected_nodes=0
    bytes.push(1u8);
    bytes.extend_from_slice(&0u32.to_le_bytes());
    // model_projection_matrix: 3x3 identity
    for row in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        for v in row {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    // model_projection_translation: (0, 0, 0)
    for _ in 0..3 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
    }
    // texture_filtering = 2 (trilerp)
    bytes.extend_from_slice(&2u32.to_le_bytes());
    // NO max_anisotropy at 20.0.0.5 (< 20.5.0.4)
    // texture_clamping = 0
    bytes.extend_from_slice(&0u32.to_le_bytes());
    // texture_type = 4 (env map)
    bytes.extend_from_slice(&4u32.to_le_bytes());
    // coordinate_generation_type = 0 (sphere map)
    bytes.extend_from_slice(&0u32.to_le_bytes());
    // source_texture_ref = 17
    bytes.extend_from_slice(&17i32.to_le_bytes());
    // enable_plane = 0
    bytes.push(0u8);
    // plane: normal (0, 1, 0), constant 0.5
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.5f32.to_le_bytes());
    // NO ps2_l / ps2_k at 20.0.0.5 (> 10.2.0.0)

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiTextureEffect", &mut stream, Some(bytes.len() as u32))
        .expect("NiTextureEffect dispatch");
    let e = block.as_any().downcast_ref::<NiTextureEffect>().unwrap();
    assert_eq!(e.texture_filtering, 2);
    assert_eq!(e.texture_type, 4);
    assert_eq!(e.coordinate_generation_type, 0);
    assert_eq!(e.source_texture_ref.index(), Some(17));
    assert!(!e.enable_plane);
    assert!((e.plane[1] - 1.0).abs() < 1e-6);
    assert!((e.plane[3] - 0.5).abs() < 1e-6);
    assert_eq!(e.max_anisotropy, 0); // absent for Oblivion
    assert_eq!(e.ps2_l, 0); // absent for Oblivion
    assert_eq!(stream.position(), bytes.len() as u64);
}

/// Regression test for issue #143: legacy particle modifier chain
/// and NiParticleSystemController. These types ship in every
/// Oblivion magic FX / fire / dust / blood mesh and hard-fail the
/// whole file when one is missing (no block_sizes fallback).
#[test]
fn oblivion_legacy_particle_modifier_chain_roundtrip() {
    use crate::blocks::legacy_particle::{
        NiGravity, NiParticleBomb, NiParticleColorModifier, NiParticleGrowFade, NiParticleRotation,
        NiPlanarCollider, NiSphericalCollider,
    };

    let header = oblivion_header();

    // Helpers.
    fn niptr_modifier_prefix() -> Vec<u8> {
        // next_modifier = -1, controller = -1
        let mut d = Vec::new();
        d.extend_from_slice(&(-1i32).to_le_bytes());
        d.extend_from_slice(&(-1i32).to_le_bytes());
        d
    }
    fn collider_prefix() -> Vec<u8> {
        let mut d = niptr_modifier_prefix();
        d.extend_from_slice(&0.5f32.to_le_bytes()); // bounce
        d.push(0u8); // spawn_on_collide
        d.push(1u8); // die_on_collide
        d
    }

    // NiParticleColorModifier: base + color_data_ref.
    let mut bytes = niptr_modifier_prefix();
    bytes.extend_from_slice(&7i32.to_le_bytes());
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiParticleColorModifier", &mut s, Some(bytes.len() as u32)).unwrap();
    let m = b
        .as_any()
        .downcast_ref::<NiParticleColorModifier>()
        .unwrap();
    assert_eq!(m.color_data_ref.index(), Some(7));
    assert_eq!(s.position(), bytes.len() as u64);

    // NiParticleGrowFade: base + grow + fade.
    let mut bytes = niptr_modifier_prefix();
    bytes.extend_from_slice(&0.25f32.to_le_bytes());
    bytes.extend_from_slice(&0.75f32.to_le_bytes());
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiParticleGrowFade", &mut s, Some(bytes.len() as u32)).unwrap();
    let m = b.as_any().downcast_ref::<NiParticleGrowFade>().unwrap();
    assert!((m.grow - 0.25).abs() < 1e-6);
    assert!((m.fade - 0.75).abs() < 1e-6);
    assert_eq!(s.position(), bytes.len() as u64);

    // NiParticleRotation: base + random_initial_axis + Vec3 axis + speed.
    let mut bytes = niptr_modifier_prefix();
    bytes.push(1u8);
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&2.5f32.to_le_bytes());
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiParticleRotation", &mut s, Some(bytes.len() as u32)).unwrap();
    let m = b.as_any().downcast_ref::<NiParticleRotation>().unwrap();
    assert!(m.random_initial_axis);
    assert_eq!(m.initial_axis, [0.0, 1.0, 0.0]);
    assert!((m.rotation_speed - 2.5).abs() < 1e-6);
    assert_eq!(s.position(), bytes.len() as u64);

    // NiParticleBomb: base + decay + duration + delta_v + start +
    // decay_type + symmetry_type + position + direction.
    let mut bytes = niptr_modifier_prefix();
    for v in [0.1f32, 1.0, 2.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&1u32.to_le_bytes()); // decay_type
    bytes.extend_from_slice(&0u32.to_le_bytes()); // symmetry_type
    for v in [0.0f32, 0.0, 0.0, 0.0, 0.0, 1.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiParticleBomb", &mut s, Some(bytes.len() as u32)).unwrap();
    let m = b.as_any().downcast_ref::<NiParticleBomb>().unwrap();
    assert_eq!(m.decay_type, 1);
    assert_eq!(m.direction, [0.0, 0.0, 1.0]);
    assert_eq!(s.position(), bytes.len() as u64);

    // NiGravity: base + decay + force + field_type + position + direction.
    let mut bytes = niptr_modifier_prefix();
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // decay
    bytes.extend_from_slice(&9.81f32.to_le_bytes()); // force
    bytes.extend_from_slice(&1u32.to_le_bytes()); // planar field
    for v in [0.0f32, 0.0, 0.0, 0.0, -1.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiGravity", &mut s, Some(bytes.len() as u32)).unwrap();
    let m = b.as_any().downcast_ref::<NiGravity>().unwrap();
    assert!((m.force - 9.81).abs() < 1e-6);
    assert_eq!(m.field_type, 1);
    assert_eq!(m.direction[1], -1.0);
    assert_eq!(s.position(), bytes.len() as u64);

    // NiPlanarCollider: collider_prefix + height + width + position +
    // x_vector + y_vector + NiPlane (vec3 normal + f32 constant).
    let mut bytes = collider_prefix();
    bytes.extend_from_slice(&10.0f32.to_le_bytes()); // height
    bytes.extend_from_slice(&5.0f32.to_le_bytes()); // width
    for v in [0.0f32; 3] {
        bytes.extend_from_slice(&v.to_le_bytes());
    } // position
    for v in [1.0f32, 0.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    } // x_vector
    for v in [0.0f32, 0.0, 1.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    } // y_vector
    for v in [0.0f32, 1.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    } // plane normal
    bytes.extend_from_slice(&0.25f32.to_le_bytes()); // plane constant
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiPlanarCollider", &mut s, Some(bytes.len() as u32)).unwrap();
    let m = b.as_any().downcast_ref::<NiPlanarCollider>().unwrap();
    assert!(m.die_on_collide);
    assert!((m.height - 10.0).abs() < 1e-6);
    assert_eq!(m.plane, [0.0, 1.0, 0.0, 0.25]);
    assert_eq!(s.position(), bytes.len() as u64);

    // NiSphericalCollider: collider_prefix + radius + position.
    let mut bytes = collider_prefix();
    bytes.extend_from_slice(&3.5f32.to_le_bytes()); // radius
    for v in [1.0f32, 2.0, 3.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiSphericalCollider", &mut s, Some(bytes.len() as u32)).unwrap();
    let m = b.as_any().downcast_ref::<NiSphericalCollider>().unwrap();
    assert!((m.radius - 3.5).abs() < 1e-6);
    assert_eq!(m.position, [1.0, 2.0, 3.0]);
    assert_eq!(s.position(), bytes.len() as u64);
}

/// Regression test for issue #143: NiParticleSystemController with
/// zero particles. Verifies the huge scalar field chain consumes
/// the expected byte count.
#[test]
fn oblivion_legacy_particle_system_controller_roundtrip() {
    use crate::blocks::legacy_particle::NiParticleSystemController;

    let header = oblivion_header();

    // NiTimeControllerBase: 26 bytes.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
    bytes.extend_from_slice(&3.0f32.to_le_bytes()); // stop_time
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // target_ref

    // Controller body scalar soup — mostly zeros, non-zero marker
    // values to verify specific field offsets.
    for v in [
        50.0f32, // speed
        5.0,     // speed_variation
        0.0,     // declination
        0.5,     // declination_variation
        0.0,     // planar_angle
        6.28,    // planar_angle_variation
    ] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    // initial_normal (vec3)
    for v in [0.0f32, 0.0, 1.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    // initial_color (RGBA)
    for v in [1.0f32, 0.5, 0.25, 1.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&1.5f32.to_le_bytes()); // initial_size
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // emit_start_time
    bytes.extend_from_slice(&10.0f32.to_le_bytes()); // emit_stop_time
    bytes.push(0u8); // reset_particle_system
    bytes.extend_from_slice(&25.0f32.to_le_bytes()); // birth_rate
    bytes.extend_from_slice(&2.0f32.to_le_bytes()); // lifetime
    bytes.extend_from_slice(&0.5f32.to_le_bytes()); // lifetime_variation
    bytes.push(1u8); // use_birth_rate
    bytes.push(0u8); // spawn_on_death
    for v in [0.0f32; 3] {
        bytes.extend_from_slice(&v.to_le_bytes());
    } // emitter_dimensions
    bytes.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // emitter ptr hash
    bytes.extend_from_slice(&1u16.to_le_bytes()); // num_spawn_generations
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // percentage_spawned
    bytes.extend_from_slice(&1u16.to_le_bytes()); // spawn_multiplier
    bytes.extend_from_slice(&0.1f32.to_le_bytes()); // spawn_speed_chaos
    bytes.extend_from_slice(&0.1f32.to_le_bytes()); // spawn_dir_chaos

    bytes.extend_from_slice(&0u16.to_le_bytes()); // num_particles
    bytes.extend_from_slice(&0u16.to_le_bytes()); // num_valid
                                                  // No particle records.
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // unknown_ref
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num_emitter_points
    bytes.extend_from_slice(&0u32.to_le_bytes()); // trailer_emitter_type
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // unknown_trailer_float
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // trailer_emitter_modifier

    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block(
        "NiParticleSystemController",
        &mut s,
        Some(bytes.len() as u32),
    )
    .expect("NiParticleSystemController dispatch");
    let c = b
        .as_any()
        .downcast_ref::<NiParticleSystemController>()
        .unwrap();
    assert!((c.speed - 50.0).abs() < 1e-6);
    assert!((c.birth_rate - 25.0).abs() < 1e-6);
    assert!((c.lifetime - 2.0).abs() < 1e-6);
    assert_eq!(c.emitter, 0xDEADBEEF);
    assert_eq!(c.num_particles, 0);
    assert_eq!(s.position(), bytes.len() as u64);

    // NiBSPArrayController aliases to the same parser with the
    // identical payload — verify it dispatches.
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiBSPArrayController", &mut s, Some(bytes.len() as u32))
        .expect("NiBSPArrayController dispatch");
    assert!(b
        .as_any()
        .downcast_ref::<NiParticleSystemController>()
        .is_some());
}

// ── #124 / audit NIF-513 — bhkNPCollisionObject family ──────────

/// FO4 header (bsver=130) used by the NP-physics dispatch tests.
fn fo4_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 130,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

#[test]
fn fo4_bhk_np_collision_object_dispatches_and_consumes() {
    let header = fo4_header();
    // NiCollisionObject::target_ref (i32) + flags (u16) + data_ref (i32) + body_id (u32).
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0x01020304i32.to_le_bytes()); // target_ref
    bytes.extend_from_slice(&0x0080u16.to_le_bytes()); // flags (default 0x80)
    bytes.extend_from_slice(&0x00000005i32.to_le_bytes()); // data_ref = 5
    bytes.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // body_id
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "bhkNPCollisionObject",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("bhkNPCollisionObject should dispatch through a real parser");
    let obj = block
        .as_any()
        .downcast_ref::<collision::BhkNPCollisionObject>()
        .expect("bhkNPCollisionObject did not downcast");
    assert_eq!(obj.flags, 0x0080);
    assert_eq!(obj.body_id, 0xDEADBEEF);
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "bhkNPCollisionObject must consume the stream exactly"
    );
}

#[test]
fn fo4_bhk_physics_system_keeps_byte_array_verbatim() {
    let header = fo4_header();
    // ByteArray: u32 size + raw bytes.
    let payload: &[u8] = b"PHYSICS-BLOB-123";
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkPhysicsSystem", &mut stream, Some(bytes.len() as u32))
        .expect("bhkPhysicsSystem dispatch");
    let sys = block
        .as_any()
        .downcast_ref::<collision::BhkSystemBinary>()
        .expect("bhkPhysicsSystem downcast");
    assert_eq!(sys.type_name, "bhkPhysicsSystem");
    assert_eq!(sys.data.as_slice(), payload);
    assert_eq!(stream.position() as usize, bytes.len());
}

#[test]
fn fo4_bhk_ragdoll_system_keeps_byte_array_verbatim() {
    let header = fo4_header();
    let payload: &[u8] = b"RAGDOLL";
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkRagdollSystem", &mut stream, Some(bytes.len() as u32))
        .expect("bhkRagdollSystem dispatch");
    let sys = block
        .as_any()
        .downcast_ref::<collision::BhkSystemBinary>()
        .expect("bhkRagdollSystem downcast");
    assert_eq!(sys.type_name, "bhkRagdollSystem");
    assert_eq!(sys.data.as_slice(), payload);
    assert_eq!(stream.position() as usize, bytes.len());
}

// ── #708 / NIF-D5-01 + NIF-D5-02 + NIF-D5-08 — Starfield BSGeometry triple ──

/// Starfield header (bsver=172, uv=12). Per
/// `crates/nif/src/version.rs::NifVariant::detect`. Skyrim+ string-table
/// shape — `read_string` resolves `0` to `strings[0]`.
fn starfield_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 172,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("BSGeometry_Test")],
        max_string_length: 16,
        num_groups: 0,
    }
}

/// Build the NiAVObject (no-properties) prefix every BSGeometry shares.
/// `flags` lands on the parent NiAVObject; bit 0x200 is the
/// internal-geom-data gate.
fn starfield_av_prefix(flags: u32) -> Vec<u8> {
    let mut d = Vec::new();
    // NiObjectNET — name index, extra-data ref count, controller ref.
    d.extend_from_slice(&0i32.to_le_bytes()); // name = strings[0]
    d.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
    d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                 // NiAVObject (parse_no_properties): flags(u32) + transform + collision_ref
    d.extend_from_slice(&flags.to_le_bytes());
    // NiTransform: rotation 3×3 matrix (9×f32) + translation (3×f32) + scale (f32)
    for v in [
        1.0f32, 0.0, 0.0, // row 0
        0.0, 1.0, 0.0, // row 1
        0.0, 0.0, 1.0, // row 2
        0.0, 0.0, 0.0, // translation
        1.0, // scale
    ] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
    d
}

/// Append the BSGeometry trailer (bounds + boundMinMax + 3 refs) and
/// `mesh_count` external-mesh slots (each: 3×u32 + sized-string).
fn starfield_external_geometry_bytes(flags: u32, mesh_names: &[&str]) -> Vec<u8> {
    assert!(mesh_names.len() <= 4);
    let mut d = starfield_av_prefix(flags);
    // bounds: Vector3 center + f32 radius
    for v in [0.0f32, 0.0, 0.0, 1.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    // boundMinMax: 6 × f32
    for v in [-1.0f32, -1.0, -1.0, 1.0, 1.0, 1.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    // 3 refs: skin / shader / alpha
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // 4 mesh slots — `mesh_names.len()` populated, rest absent.
    for i in 0..4 {
        if i < mesh_names.len() {
            d.push(1u8); // present
            d.extend_from_slice(&123u32.to_le_bytes()); // tri_size
            d.extend_from_slice(&456u32.to_le_bytes()); // num_verts
            d.extend_from_slice(&64u32.to_le_bytes()); // flags (nifly: "often 64")
                                                       // sized string: u32 length + bytes
            let name = mesh_names[i].as_bytes();
            d.extend_from_slice(&(name.len() as u32).to_le_bytes());
            d.extend_from_slice(name);
        } else {
            d.push(0u8); // absent
        }
    }
    d
}

/// Regression for #708 / NIF-D5-01: BSGeometry now dispatches and
/// captures the external-mesh-path branch (the 99% Starfield case).
/// Pre-fix it fell into NiUnknown and the entire mesh body was lost.
#[test]
fn starfield_bs_geometry_external_mesh_dispatches() {
    let header = starfield_header();
    let bytes = starfield_external_geometry_bytes(
        0, // no internal-geom flag → external-mesh branch
        &[
            "abcdef0123456789abcdef0123456789abcdef012",
            "secondary.mesh",
        ],
    );
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BSGeometry", &mut stream, Some(bytes.len() as u32))
        .expect("BSGeometry must dispatch");
    let geo = block
        .as_any()
        .downcast_ref::<bs_geometry::BSGeometry>()
        .expect("BSGeometry downcast");

    assert_eq!(geo.bounding_sphere.1, 1.0, "bounds.radius");
    assert!(!geo.has_internal_geom_data());
    assert_eq!(geo.meshes.len(), 2, "2 of 4 slots populated");
    match &geo.meshes[0].kind {
        bs_geometry::BSGeometryMeshKind::External { mesh_name } => {
            assert_eq!(mesh_name, "abcdef0123456789abcdef0123456789abcdef012");
        }
        _ => panic!("expected external mesh kind"),
    }
    assert_eq!(geo.meshes[0].tri_size, 123);
    assert_eq!(geo.meshes[0].num_verts, 456);
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "BSGeometry must consume the whole block exactly"
    );
}

/// Internal-geom-data branch: bit 0x200 of NiAVObject flags switches
/// the per-mesh slot from external-name to inline `BSGeometryMeshData`.
/// Build a minimal-version body (`version > 2` early-out) so the test
/// stays compact while still exercising the internal-mesh dispatch.
#[test]
fn starfield_bs_geometry_internal_geom_data_branch_dispatches() {
    let header = starfield_header();
    // av-prefix flags with bit 0x200 set.
    let mut d = starfield_av_prefix(0x200);
    // bounds + boundMinMax + 3 refs (same trailer as external case).
    for v in [0.0f32, 0.0, 0.0, 0.5] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    for v in [-1.0f32, -1.0, -1.0, 1.0, 1.0, 1.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // One populated mesh slot followed by 3 absent.
    d.push(1u8);
    d.extend_from_slice(&0u32.to_le_bytes()); // tri_size
    d.extend_from_slice(&0u32.to_le_bytes()); // num_verts
    d.extend_from_slice(&64u32.to_le_bytes()); // flags
                                               // BSGeometryMeshData: version=99 (>2 → early-out, no body follows).
    d.extend_from_slice(&99u32.to_le_bytes());
    d.push(0u8);
    d.push(0u8);
    d.push(0u8);

    let mut stream = NifStream::new(&d, &header);
    let block = parse_block("BSGeometry", &mut stream, Some(d.len() as u32))
        .expect("BSGeometry must dispatch (internal branch)");
    let geo = block
        .as_any()
        .downcast_ref::<bs_geometry::BSGeometry>()
        .expect("BSGeometry downcast");

    assert!(geo.has_internal_geom_data());
    assert_eq!(geo.meshes.len(), 1);
    match &geo.meshes[0].kind {
        bs_geometry::BSGeometryMeshKind::Internal { mesh_data } => {
            assert_eq!(mesh_data.version, 99);
            assert!(mesh_data.vertices.is_empty(), "version > 2 → empty body");
        }
        _ => panic!("expected internal mesh kind"),
    }
}

/// Regression for #708 / NIF-D5-02: SkinAttach now dispatches and
/// reaches the `bones` NiStringVector decode. Pre-fix every Starfield
/// SkinAttach extra-data block fell into NiUnknown alongside its
/// parent BSGeometry.
#[test]
fn starfield_skin_attach_dispatches() {
    let header = starfield_header();
    let mut bytes = Vec::new();
    // NiExtraData prefix: name string-table index = 0.
    bytes.extend_from_slice(&0i32.to_le_bytes());
    // bones: NiStringVector — u32 count + count × NiString(u32 length + bytes).
    bytes.extend_from_slice(&3u32.to_le_bytes()); // count
    for s in ["Spine", "Head", "L_Hand"] {
        bytes.extend_from_slice(&(s.len() as u32).to_le_bytes());
        bytes.extend_from_slice(s.as_bytes());
    }
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("SkinAttach", &mut stream, Some(bytes.len() as u32))
        .expect("SkinAttach must dispatch");
    let extra = block
        .as_any()
        .downcast_ref::<extra_data::NiExtraData>()
        .expect("SkinAttach downcast to NiExtraData");
    let bones = extra
        .skin_attach_bones
        .as_ref()
        .expect("skin_attach_bones populated");
    assert_eq!(
        bones,
        &vec!["Spine".to_string(), "Head".into(), "L_Hand".into()]
    );
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression for #708 / NIF-D5-08: BoneTranslations dispatches and
/// captures `(bone, translation)` pairs.
#[test]
fn starfield_bone_translations_dispatches() {
    let header = starfield_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0i32.to_le_bytes()); // name index
    bytes.extend_from_slice(&2u32.to_le_bytes()); // numTranslations
    for (name, trans) in [("Spine", [0.1f32, 0.2, 0.3]), ("Head", [-0.4, 0.5, -0.6])] {
        bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
        bytes.extend_from_slice(name.as_bytes());
        for v in trans {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BoneTranslations", &mut stream, Some(bytes.len() as u32))
        .expect("BoneTranslations must dispatch");
    let extra = block
        .as_any()
        .downcast_ref::<extra_data::NiExtraData>()
        .expect("BoneTranslations downcast");
    let translations = extra
        .bone_translations
        .as_ref()
        .expect("bone_translations populated");
    assert_eq!(translations.len(), 2);
    assert_eq!(translations[0].0, "Spine");
    assert!((translations[0].1[0] - 0.1).abs() < 1e-6);
    assert_eq!(translations[1].0, "Head");
    assert!((translations[1].1[2] + 0.6).abs() < 1e-6);
    assert_eq!(stream.position() as usize, bytes.len());
}

// ── O5-3 / #688 — early-Gamebryo NiObject groupID prefix ─────────

/// Build a header for an early-Gamebryo NIF (file version in the
/// `[10.0.0.0, 10.1.0.114)` window). Every block in this range is
/// prefixed with a 4-byte `NiObject.groupID` field per nifly's
/// `NiObject::Get`. Pre-#688 the byte was misread as the first u32
/// of the block payload (typically `NiObjectNET.Name`'s SizedString
/// length), causing 145 / 8032 Oblivion-era files to truncate at
/// root with "failed to fill whole buffer".
fn early_gamebryo_header(packed_version: u32) -> NifHeader {
    NifHeader {
        version: NifVersion(packed_version),
        little_endian: true,
        user_version: 0,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

/// O5-3 / #688: a v10.0.1.0 NiNode with the leading `groupID` u32
/// must parse all of NiObjectNET (name + extra_data + controller)
/// + NiAVObject (flags + transform + properties + collision) +
/// NiNode (children + effects). Pre-fix the parser swallowed the
/// 4-byte groupID as the start of `Name.length`, then drifted by
/// 4 bytes through every downstream field — eventually failing
/// the buffer-fill check far past the actual layout.
#[test]
fn ni_node_v10_0_1_0_consumes_groupid_prefix_and_full_payload() {
    let header = early_gamebryo_header(0x0A000100);
    let mut bytes = Vec::new();
    // NiObject.groupID — vanilla Bethesda content always ships zero.
    bytes.extend_from_slice(&0u32.to_le_bytes());
    // NiObjectNET.Name (SizedString = u32 length + bytes).
    bytes.extend_from_slice(&6u32.to_le_bytes());
    bytes.extend_from_slice(b"HornsA");
    // NiObjectNET.NumExtraDataList + Extra Data List.
    bytes.extend_from_slice(&1u32.to_le_bytes()); // count
    bytes.extend_from_slice(&1i32.to_le_bytes()); // ref[0]
                                                  // NiObjectNET.Controller — NULL.
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // NiAVObject.Flags (u16, BSVER == 0 ≤ 26).
    bytes.extend_from_slice(&0x0010u16.to_le_bytes());
    // NiAVObject.Transform: translation (3×f32) + rotation (9×f32) + scale.
    for _ in 0..3 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
    }
    for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // scale
                                                    // NiAVObject.NumProperties + Properties (count=0 → empty).
    bytes.extend_from_slice(&0u32.to_le_bytes());
    // NiAVObject.CollisionObject (since 10.0.1.0).
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // NiNode.NumChildren + Children + NumEffects + Effects (all 0).
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());

    let consumed_expected = bytes.len();
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiNode", &mut stream, None)
        .expect("v10.0.1.0 NiNode with groupID prefix must parse");
    let node = block
        .as_any()
        .downcast_ref::<crate::blocks::node::NiNode>()
        .expect("downcast to NiNode");
    assert_eq!(node.av.net.name.as_deref(), Some("HornsA"));
    assert_eq!(node.av.flags, 0x0010);
    assert_eq!(node.av.net.extra_data_refs.len(), 1);
    assert_eq!(stream.position() as usize, consumed_expected);
}

/// O5-3 / #688: same payload at v10.1.0.106 — the upper edge of the
/// reported failing bucket (77 of 145 files). The fix uses an
/// inclusive-low / exclusive-high gate, so 10.1.0.106 (= 0x0A01006A,
/// just below the 10.1.0.114 = 0x0A010072 cap) must still consume
/// the prefix.
#[test]
fn ni_node_v10_1_0_106_consumes_groupid_prefix() {
    let header = early_gamebryo_header(0x0A01006A);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u32.to_le_bytes()); // groupID
    bytes.extend_from_slice(&0u32.to_le_bytes()); // name length 0
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num extra data
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    for _ in 0..13 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // transform
    }
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num properties
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // collision
    bytes.extend_from_slice(&0u32.to_le_bytes()); // children
    bytes.extend_from_slice(&0u32.to_le_bytes()); // effects

    let consumed_expected = bytes.len();
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiNode", &mut stream, None)
        .expect("v10.1.0.106 NiNode with groupID prefix must parse");
    assert!(block.as_any().is::<crate::blocks::node::NiNode>());
    assert_eq!(stream.position() as usize, consumed_expected);
}

/// O5-3 / #688: above the 10.1.0.114 cap (e.g. v20.0.0.5 / Oblivion
/// mainstream) the groupID prefix is gone — the parser must NOT
/// consume an extra 4 bytes. This pins the upper bound; without the
/// gate every Oblivion / FO3 / FNV / Skyrim block would lose 4 bytes
/// at the head.
#[test]
fn ni_node_v20_0_0_5_does_not_consume_groupid_prefix() {
    // Use the existing oblivion_header() (V20_0_0_5 / user_version=11).
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // No groupID — name index goes first.
    bytes.extend_from_slice(&0i32.to_le_bytes()); // name index = 0
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num extra data
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller
    bytes.extend_from_slice(&0u32.to_le_bytes()); // flags (u32 since BSVER=0… wait, header sets user_version=11, bsver=0)
    for _ in 0..13 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
    }
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num properties
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // collision
    bytes.extend_from_slice(&0u32.to_le_bytes()); // children
    bytes.extend_from_slice(&0u32.to_le_bytes()); // effects
                                                  // The oblivion_header has bsver=0, so flags is u16 not u32 — fix:
                                                  // re-build with u16 flags.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0i32.to_le_bytes()); // name index = 0
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num extra data
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags (u16, bsver=0 ≤ 26)
    for _ in 0..13 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
    }
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num properties
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // collision
    bytes.extend_from_slice(&0u32.to_le_bytes()); // children
    bytes.extend_from_slice(&0u32.to_le_bytes()); // effects

    let consumed_expected = bytes.len();
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiNode", &mut stream, None)
        .expect("v20.0.0.5 NiNode without groupID prefix must parse");
    assert!(block.as_any().is::<crate::blocks::node::NiNode>());
    assert_eq!(
        stream.position() as usize,
        consumed_expected,
        "v20.0.0.5 must NOT consume a phantom groupID — pre-#688 it \
         stopped at the right offset because the byte was \
         (mis-)read into NiObjectNET.Name.length"
    );
}

/// Regression for #717 / NIF-D3-02: `HairShaderProperty`,
/// `VolumetricFogShaderProperty`, `DistantLODShaderProperty`, and
/// `BSDistantTreeShaderProperty` all inherit `BSShaderProperty` directly
/// with **no additional fields**.  Pre-fix they were aliased to
/// `BSShaderPPLightingProperty::parse`, which over-read up to 24 bytes
/// (`texture_clamp_mode` + `texture_set_ref` + refraction + parallax)
/// that are absent on these wire layouts — masked by `block_sizes`
/// recovery but producing silent stream drift on any modded NIF that
/// carries one of these types.
#[test]
fn zero_field_shader_variants_route_to_base_only() {
    let header = oblivion_header();

    // Minimal payload for a zero-field BSShaderProperty subclass:
    // NiObjectNET (12 bytes) + BSShaderPropertyData.parse_base (18 bytes) = 30 bytes.
    let mut bytes = Vec::new();
    // NiObjectNET
    bytes.extend_from_slice(&0i32.to_le_bytes()); // name string index
    bytes.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                     // BSShaderPropertyData (parse_base)
    bytes.extend_from_slice(&0u16.to_le_bytes()); // shade_flags
    bytes.extend_from_slice(&3u32.to_le_bytes()); // shader_type (Tall_Grass=3 for visibility)
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_1
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_2
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // env_map_scale

    let variants = [
        "HairShaderProperty",
        "VolumetricFogShaderProperty",
        "DistantLODShaderProperty",
        "BSDistantTreeShaderProperty",
    ];

    for variant in variants {
        let mut stream = NifStream::new(&bytes, &header);
        let block = parse_block(variant, &mut stream, Some(bytes.len() as u32))
            .unwrap_or_else(|e| panic!("variant '{variant}' failed: {e}"));

        // Must downcast to BSShaderPropertyBaseOnly, NOT BSShaderPPLightingProperty.
        let base = block
            .as_any()
            .downcast_ref::<crate::blocks::shader::BSShaderPropertyBaseOnly>()
            .unwrap_or_else(|| {
                panic!(
                    "variant '{variant}' must downcast to BSShaderPropertyBaseOnly, \
                     not BSShaderPPLightingProperty (pre-#717 regression)"
                )
            });

        assert_eq!(
            base.block_type_name(),
            variant,
            "block_type_name must reflect the wire type, not the Rust wrapper"
        );
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "variant '{variant}' must consume exactly {} bytes (pre-#717 \
             over-read 24 extra bytes of PPLighting fields)",
            bytes.len()
        );
    }
}

/// Regression for #710 / NIF-D5-03. `BSPositionData` is an FO4 / FO76
/// extra-data block carrying a per-vertex blend factor array (half-
/// floats). Pre-fix it had no dispatch arm — 2,961 vanilla instances
/// (372 in `Fallout4 - Meshes.ba2`, 2,589 in `SeventySix - Meshes.ba2`)
/// fell into NiUnknown and lost their per-vertex morph data. This
/// test builds a synthetic 4-vertex payload, dispatches via
/// `parse_block`, asserts the downcast succeeds, and pins the
/// half-float decode (0x3C00 ↔ 1.0, 0x0000 ↔ 0.0, 0xBC00 ↔ -1.0).
#[test]
fn bs_position_data_dispatches_and_decodes_half_float_array() {
    use crate::blocks::extra_data::BsPositionData;

    // FO4 header: v20.2.0.7, user_version=12, user_version_2=130 (FO4 BSVER).
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 130,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        // BSPositionData reads NiObjectNET's name via the string
        // table on v >= 20.1.0.1; supply slot 0 so the read succeeds.
        strings: vec![Arc::from("ClothBlend")],
        max_string_length: 16,
        num_groups: 0,
    };

    let mut data = Vec::new();
    // NiObjectNET name string-table index = 0 → "ClothBlend".
    data.extend_from_slice(&0i32.to_le_bytes());
    // num_vertices = 4
    data.extend_from_slice(&4u32.to_le_bytes());
    // 4 half-float blend factors: 1.0, 0.5, 0.0, -1.0
    // 1.0   = 0x3C00
    // 0.5   = 0x3800
    // 0.0   = 0x0000
    // -1.0  = 0xBC00
    for h in [0x3C00u16, 0x3800, 0x0000, 0xBC00] {
        data.extend_from_slice(&h.to_le_bytes());
    }

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block("BSPositionData", &mut stream, Some(data.len() as u32))
        .expect("BSPositionData must dispatch (#710)");
    assert_eq!(block.block_type_name(), "BSPositionData");
    let pos = block
        .as_any()
        .downcast_ref::<BsPositionData>()
        .expect("dispatch must produce BsPositionData");

    assert_eq!(pos.name.as_deref(), Some("ClothBlend"));
    assert_eq!(pos.vertex_data.len(), 4);
    assert!((pos.vertex_data[0] - 1.0).abs() < 1e-6);
    assert!((pos.vertex_data[1] - 0.5).abs() < 1e-3);
    assert!((pos.vertex_data[2] - 0.0).abs() < 1e-6);
    assert!((pos.vertex_data[3] - (-1.0)).abs() < 1e-6);

    assert_eq!(
        stream.position() as usize,
        data.len(),
        "BSPositionData must consume exactly {} bytes",
        data.len()
    );
}

/// Companion: hostile `num_vertices = 0xFFFFFFFF` must error out via
/// the `allocate_vec` budget guard, not OOM-allocate ~12 GB before
/// the inner half-float reads fail. Per the issue's ALLOCATE_VEC
/// completeness check (#764 sweep).
#[test]
fn bs_position_data_hostile_num_vertices_returns_err_not_panic() {
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 130,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("Hostile")],
        max_string_length: 8,
        num_groups: 0,
    };

    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes()); // name index
    data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // hostile num_vertices

    let mut stream = NifStream::new(&data, &header);
    let result = parse_block("BSPositionData", &mut stream, Some(data.len() as u32));
    assert!(
        result.is_err(),
        "hostile num_vertices must error gracefully, not panic"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("only") && msg.contains("bytes remain"),
        "expected `allocate_vec` budget rejection, got: {msg}"
    );
}

/// Regression for #720 / NIF-D5-04. `BSEyeCenterExtraData` is an FO4
/// / FO76 extra-data block carrying eye-pivot positions consumed by
/// FaceGen + dialogue-camera framing. Pre-fix it had no dispatch arm
/// — 625 vanilla instances (623 in `Fallout4 - Meshes.ba2`, 2 in
/// `SeventySix - Meshes.ba2`) fell into NiUnknown, so dialogue
/// eye-tracking pointed at the NIF origin instead of the eye centroid.
/// Synthetic FO4-shaped header + the canonical 4-float payload
/// (left+right eye XY) round-trip through dispatch + decode.
#[test]
fn bs_eye_center_extra_data_dispatches_and_decodes_4_floats() {
    use crate::blocks::extra_data::BsEyeCenterExtraData;

    // FO4 header: v20.2.0.7, user_version=12, user_version_2=130 (FO4 BSVER).
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 130,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        // BSEyeCenterExtraData reads NiObjectNET's name via the string
        // table on v >= 20.1.0.1; supply slot 0 so the read succeeds.
        strings: vec![Arc::from("EyeCenter")],
        max_string_length: 16,
        num_groups: 0,
    };

    let mut data = Vec::new();
    // NiObjectNET name string-table index = 0 → "EyeCenter".
    data.extend_from_slice(&0i32.to_le_bytes());
    // num_floats = 4 (canonical: left.x, left.y, right.x, right.y).
    data.extend_from_slice(&4u32.to_le_bytes());
    for f in [-2.5f32, 4.0, 2.5, 4.0] {
        data.extend_from_slice(&f.to_le_bytes());
    }

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block("BSEyeCenterExtraData", &mut stream, Some(data.len() as u32))
        .expect("BSEyeCenterExtraData must dispatch (#720)");
    assert_eq!(block.block_type_name(), "BSEyeCenterExtraData");
    let eye = block
        .as_any()
        .downcast_ref::<BsEyeCenterExtraData>()
        .expect("dispatch must produce BsEyeCenterExtraData");

    assert_eq!(eye.name.as_deref(), Some("EyeCenter"));
    assert_eq!(eye.floats, vec![-2.5, 4.0, 2.5, 4.0]);

    assert_eq!(
        stream.position() as usize,
        data.len(),
        "BSEyeCenterExtraData must consume exactly {} bytes",
        data.len()
    );
}

/// Companion: hostile `num_floats = 0xFFFFFFFF` must error out via
/// the `allocate_vec` budget guard, not OOM-allocate ~16 GB before
/// the inner `read_f32_le` fails. Per the issue's ALLOCATE_VEC
/// completeness check (#764 sweep).
#[test]
fn bs_eye_center_extra_data_hostile_num_floats_returns_err_not_panic() {
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 130,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("Hostile")],
        max_string_length: 8,
        num_groups: 0,
    };

    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes()); // name index
    data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // hostile num_floats

    let mut stream = NifStream::new(&data, &header);
    let result = parse_block("BSEyeCenterExtraData", &mut stream, Some(data.len() as u32));
    assert!(
        result.is_err(),
        "hostile num_floats must error gracefully, not panic"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("only") && msg.contains("bytes remain"),
        "expected `allocate_vec` budget rejection, got: {msg}"
    );
}

// ── #942 / NIF-D5-NEW-03 — BSDistantObjectLargeRefExtraData (SSE) ──

/// SSE header with the `BSExtraData.name` slot populated for the
/// `read_extra_data_name` lookup. SSE bsver=100, version=20.2.0.7.
fn sse_header_with_name(name: &str) -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 100,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from(name)],
        max_string_length: name.len() as u32,
        num_groups: 0,
    }
}

#[test]
fn sse_bs_distant_object_large_ref_extra_data_round_trips_true() {
    let header = sse_header_with_name("LargeRefMarker");
    let mut data = Vec::new();
    // NiExtraData.name: string-table index 0 → "LargeRefMarker".
    data.extend_from_slice(&0i32.to_le_bytes());
    // Large Ref bool — single byte at v20.2.0.7.
    data.push(1);

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "BSDistantObjectLargeRefExtraData",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("BSDistantObjectLargeRefExtraData must dispatch");
    assert_eq!(block.block_type_name(), "BSDistantObjectLargeRefExtraData");
    let large = block
        .as_any()
        .downcast_ref::<extra_data::BsDistantObjectLargeRefExtraData>()
        .expect("dispatch must produce BsDistantObjectLargeRefExtraData");
    assert!(large.large_ref);
    assert_eq!(large.name.as_deref(), Some("LargeRefMarker"));
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "must consume the 5-byte body exactly"
    );
}

#[test]
fn sse_bs_distant_object_large_ref_extra_data_round_trips_false() {
    let header = sse_header_with_name("");
    let mut data = Vec::new();
    data.extend_from_slice(&(-1i32).to_le_bytes()); // name index = -1 (None)
    data.push(0); // large_ref = false

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "BSDistantObjectLargeRefExtraData",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("BSDistantObjectLargeRefExtraData with false flag must dispatch");
    let large = block
        .as_any()
        .downcast_ref::<extra_data::BsDistantObjectLargeRefExtraData>()
        .unwrap();
    assert!(!large.large_ref);
    assert!(large.name.is_none());
}

// ── #942 / NIF-D5-NEW-03 — BSDistantObjectInstancedNode (FO76) ──────

/// FO76 header — bsver=155, version=20.2.0.7, with named string slots
/// for the NiObjectNET / texture-array `SizedString` paths used by the
/// BSMultiBoundNode base and BSDistantObjectInstancedNode trailing
/// texture arrays. Strings inside texture arrays are inline
/// `SizedString` (length-prefixed bytes) — they don't look up against
/// the string table, so an empty `strings` field is fine for them.
fn fo76_header_with_name(name: &str) -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 155,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from(name)],
        max_string_length: name.len() as u32,
        num_groups: 0,
    }
}

/// Build the BSMultiBoundNode wire body (NiNode body + multi_bound_ref
/// + culling_mode for bsver >= 83). Returns the byte vector ready to
/// concatenate inside a BSDistantObjectInstancedNode payload.
fn build_bs_multi_bound_node_body() -> Vec<u8> {
    let mut b = Vec::new();
    // NiObjectNET: name (string-table index 0 → name string), 0 extra
    // data refs, controller_ref = -1.
    b.extend_from_slice(&0i32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&(-1i32).to_le_bytes());
    // NiAVObject (FO76/v20.2.0.7 + bsver>=83 layout): u32 flags +
    // transform (3 floats translation + 9 floats rotation + 1 scale) +
    // 0 properties + collision_ref = -1.
    b.extend_from_slice(&0u32.to_le_bytes()); // flags
    for _ in 0..3 {
        b.extend_from_slice(&0.0f32.to_le_bytes());
    }
    for r in &[1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        b.extend_from_slice(&r.to_le_bytes());
    }
    b.extend_from_slice(&1.0f32.to_le_bytes()); // scale
                                                // Properties list is gated `bsver <= 34` (FO3/FNV/Oblivion); FO76
                                                // bsver=155 skips it entirely — emitting a `0u32` here would shift
                                                // every downstream field forward 4 bytes and the multi_bound_ref
                                                // (-1) would be misread as a children count of 0xFFFFFFFF.
    b.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
                                                 // NiNode: 0 children. The `effects` array is gated on bsver — FO4+
                                                 // (bsver=130) drops it. FO76 (bsver=155) drops it too.
    b.extend_from_slice(&0u32.to_le_bytes()); // children count
                                              // BSMultiBoundNode: multi_bound_ref (-1) + culling_mode (Skyrim+).
    b.extend_from_slice(&(-1i32).to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes()); // culling_mode
    b
}

#[test]
fn fo76_bs_distant_object_instanced_node_round_trips_two_instances() {
    let header = fo76_header_with_name("DistantRoot");

    let mut data = Vec::new();
    data.extend_from_slice(&build_bs_multi_bound_node_body());

    // num_instances = 2.
    data.extend_from_slice(&2u32.to_le_bytes());

    // Instance 0: resource_id (file_hash=0xCAFEBABE, ext="nif\0",
    // dir_hash=0xDEADBEEF), 1 unknown_data entry, 2 transforms.
    data.extend_from_slice(&0xCAFEBABEu32.to_le_bytes()); // file_hash
    data.extend_from_slice(b"nif\0"); // extension
    data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // dir_hash
    data.extend_from_slice(&1u32.to_le_bytes()); // num_unknown_data
    data.extend_from_slice(&0x0102030405060708u64.to_le_bytes()); // unknown 1
    data.extend_from_slice(&0x11223344u32.to_le_bytes()); // unknown 2
    data.extend_from_slice(&2u32.to_le_bytes()); // num_transforms
                                                 // Two diagnostic matrices (16 f32 each) — first element differs so
                                                 // round-trip checks can distinguish them.
    for tag in [10.0f32, 20.0] {
        for j in 0..16 {
            data.extend_from_slice(&(tag + j as f32).to_le_bytes());
        }
    }

    // Instance 1: empty unknown_data, single transform.
    data.extend_from_slice(&0x00000001u32.to_le_bytes()); // file_hash
    data.extend_from_slice(b"bgs\0"); // extension
    data.extend_from_slice(&0x00000002u32.to_le_bytes()); // dir_hash
    data.extend_from_slice(&0u32.to_le_bytes()); // num_unknown_data
    data.extend_from_slice(&1u32.to_le_bytes()); // num_transforms
    for j in 0..16 {
        data.extend_from_slice(&(100.0f32 + j as f32).to_le_bytes());
    }

    // 3 BSShaderTextureArray slots — each is unknown_byte + count.
    // Slot 0: 1 BSTextureArray with width=2 ("foo", "bar").
    data.push(1u8); // unknown_byte
    data.extend_from_slice(&1u32.to_le_bytes()); // num_texture_arrays
    data.extend_from_slice(&2u32.to_le_bytes()); // width
    data.extend_from_slice(&3u32.to_le_bytes()); // SizedString length
    data.extend_from_slice(b"foo");
    data.extend_from_slice(&3u32.to_le_bytes());
    data.extend_from_slice(b"bar");
    // Slots 1 + 2: empty (count=0).
    for _ in 0..2 {
        data.push(1u8);
        data.extend_from_slice(&0u32.to_le_bytes());
    }

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "BSDistantObjectInstancedNode",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("BSDistantObjectInstancedNode must dispatch");
    assert_eq!(block.block_type_name(), "BSDistantObjectInstancedNode");
    let inst = block
        .as_any()
        .downcast_ref::<node::BsDistantObjectInstancedNode>()
        .expect("dispatch must produce BsDistantObjectInstancedNode");

    // Multi-bound base intact.
    assert_eq!(inst.base.culling_mode, 0);
    assert!(inst.base.multi_bound_ref.is_null());
    assert_eq!(inst.base.base.av.net.name.as_deref(), Some("DistantRoot"));

    // Per-instance payload intact.
    assert_eq!(inst.instances.len(), 2);
    let a = &inst.instances[0];
    assert_eq!(a.resource_file_hash, 0xCAFEBABE);
    assert_eq!(a.resource_extension, *b"nif\0");
    assert_eq!(a.resource_dir_hash, 0xDEADBEEF);
    assert_eq!(a.unknown_data, vec![(0x0102030405060708, 0x11223344)]);
    assert_eq!(a.transforms.len(), 2);
    assert_eq!(a.transforms[0][0], 10.0);
    assert_eq!(a.transforms[1][0], 20.0);

    let b = &inst.instances[1];
    assert_eq!(b.resource_file_hash, 1);
    assert_eq!(b.unknown_data.len(), 0);
    assert_eq!(b.transforms.len(), 1);
    assert_eq!(b.transforms[0][0], 100.0);

    // Whole payload consumed — texture arrays are parsed-and-consumed,
    // so the drift detector stays silent on this fixture.
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "BSDistantObjectInstancedNode must consume the entire payload"
    );
}

#[test]
fn fo76_bs_distant_object_instanced_node_root_recognised_by_is_ni_node_subclass() {
    // SK-D5-02 / #611 — the root-selection helper must include the new
    // subclass so a NIF rooted at BSDistantObjectInstancedNode picks
    // block 0 instead of skipping past it to the first plain NiNode
    // child.
    assert!(crate::is_ni_node_subclass("BSDistantObjectInstancedNode"));
}

// ── #936 / NIF-D5-NEW-01 — NiBSplineComp{Float,Point3}Interpolator ──

/// FNV header (bsver=34, v20.2.0.7) used by the B-spline dispatch
/// tests. Compact B-spline interpolators are reachable on FO3/FNV per
/// the "B-splines aren't Skyrim+ only" feedback memory, so the float +
/// point3 fixtures use the FNV-style header.
fn fnv_header_bspline() -> NifHeader {
    NifHeader {
        version: NifVersion(0x14020007),
        little_endian: true,
        user_version: 11,
        user_version_2: 34,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

#[test]
fn ni_bspline_comp_float_interpolator_round_trip() {
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    // NiBSplineInterpolator: start_time, stop_time, spline_data_ref,
    // basis_data_ref.
    data.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
    data.extend_from_slice(&1.5f32.to_le_bytes()); // stop_time
    data.extend_from_slice(&7i32.to_le_bytes()); // spline_data_ref
    data.extend_from_slice(&8i32.to_le_bytes()); // basis_data_ref
                                                 // NiBSplineFloatInterpolator: value, handle.
    data.extend_from_slice(&0.25f32.to_le_bytes()); // fallback value
    data.extend_from_slice(&0u32.to_le_bytes()); // handle
                                                 // NiBSplineCompFloatInterpolator: float_offset, float_half_range.
    data.extend_from_slice(&0.5f32.to_le_bytes()); // offset
    data.extend_from_slice(&0.5f32.to_le_bytes()); // half_range

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "NiBSplineCompFloatInterpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("NiBSplineCompFloatInterpolator must dispatch");
    assert_eq!(block.block_type_name(), "NiBSplineCompFloatInterpolator");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::NiBSplineCompFloatInterpolator>()
        .expect("dispatch must produce NiBSplineCompFloatInterpolator");

    assert_eq!(interp.start_time, 0.0);
    assert_eq!(interp.stop_time, 1.5);
    assert_eq!(interp.spline_data_ref.index(), Some(7));
    assert_eq!(interp.basis_data_ref.index(), Some(8));
    assert_eq!(interp.value, 0.25);
    assert_eq!(interp.handle, 0);
    assert_eq!(interp.float_offset, 0.5);
    assert_eq!(interp.float_half_range, 0.5);
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "32-byte body must be consumed exactly"
    );
}

#[test]
fn ni_bspline_comp_point3_interpolator_round_trip() {
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    // NiBSplineInterpolator base.
    data.extend_from_slice(&0.5f32.to_le_bytes()); // start_time
    data.extend_from_slice(&2.5f32.to_le_bytes()); // stop_time
    data.extend_from_slice(&3i32.to_le_bytes()); // spline_data_ref
    data.extend_from_slice(&4i32.to_le_bytes()); // basis_data_ref
                                                 // NiBSplinePoint3Interpolator: Vector3 value + handle.
    data.extend_from_slice(&0.1f32.to_le_bytes());
    data.extend_from_slice(&0.2f32.to_le_bytes());
    data.extend_from_slice(&0.3f32.to_le_bytes());
    data.extend_from_slice(&12u32.to_le_bytes()); // handle (non-invalid)
                                                  // NiBSplineCompPoint3Interpolator: position_offset, position_half_range.
    data.extend_from_slice(&1.0f32.to_le_bytes()); // offset
    data.extend_from_slice(&2.0f32.to_le_bytes()); // half_range

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "NiBSplineCompPoint3Interpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("NiBSplineCompPoint3Interpolator must dispatch");
    assert_eq!(block.block_type_name(), "NiBSplineCompPoint3Interpolator");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::NiBSplineCompPoint3Interpolator>()
        .expect("dispatch must produce NiBSplineCompPoint3Interpolator");

    assert_eq!(interp.start_time, 0.5);
    assert_eq!(interp.stop_time, 2.5);
    assert_eq!(interp.spline_data_ref.index(), Some(3));
    assert_eq!(interp.basis_data_ref.index(), Some(4));
    assert_eq!(interp.value, [0.1, 0.2, 0.3]);
    assert_eq!(interp.handle, 12);
    assert_eq!(interp.position_offset, 1.0);
    assert_eq!(interp.position_half_range, 2.0);
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "36-byte body must be consumed exactly"
    );
}

#[test]
fn ni_bspline_comp_float_interpolator_invalid_handle_static_fallback() {
    // handle = 0xFFFFFFFF + a non-FLT_MAX `value` means the channel is
    // static — pin that the body still parses cleanly so the anim
    // emitter's static-key path has data to read.
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes()); // null spline_data_ref
    data.extend_from_slice(&(-1i32).to_le_bytes()); // null basis_data_ref
    data.extend_from_slice(&0.75f32.to_le_bytes()); // value
    data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // INVALID handle
    data.extend_from_slice(&0.0f32.to_le_bytes()); // float_offset
    data.extend_from_slice(&0.0f32.to_le_bytes()); // float_half_range

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "NiBSplineCompFloatInterpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("static-handle NiBSplineCompFloatInterpolator must dispatch");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::NiBSplineCompFloatInterpolator>()
        .unwrap();
    assert_eq!(interp.handle, u32::MAX);
    assert_eq!(interp.value, 0.75);
}

// ── #941 / NIF-D5-NEW-02 — BSTreadTransfInterpolator (FO3+) ──────

#[test]
fn fnv_bs_tread_transf_interpolator_round_trip_two_tread_transforms() {
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    // num_tread_transforms = 2
    data.extend_from_slice(&2u32.to_le_bytes());

    // Tread 0: name string-table index = -1 (None), two NiQuatTransforms.
    data.extend_from_slice(&(-1i32).to_le_bytes()); // name
    // T1: translation (1,2,3) + rotation (w=1,x=0,y=0,z=0 identity) + scale=1
    for v in [1.0f32, 2.0, 3.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // T2: translation (4,5,6) + rotation identity + scale=2
    for v in [4.0f32, 5.0, 6.0, 1.0, 0.0, 0.0, 0.0, 2.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }

    // Tread 1: same shape, different values.
    data.extend_from_slice(&(-1i32).to_le_bytes());
    for v in [10.0f32, 20.0, 30.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    for v in [40.0f32, 50.0, 60.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }

    // data_ref → block 5
    data.extend_from_slice(&5i32.to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "BSTreadTransfInterpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("BSTreadTransfInterpolator must dispatch");
    assert_eq!(block.block_type_name(), "BSTreadTransfInterpolator");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::BsTreadTransfInterpolator>()
        .expect("dispatch must produce BsTreadTransfInterpolator");

    assert_eq!(interp.tread_transforms.len(), 2);
    assert_eq!(interp.tread_transforms[0].transform_1.translation.x, 1.0);
    assert_eq!(interp.tread_transforms[0].transform_2.scale, 2.0);
    assert_eq!(interp.tread_transforms[1].transform_1.translation.x, 10.0);
    assert_eq!(interp.tread_transforms[1].transform_2.translation.z, 60.0);
    assert_eq!(interp.data_ref.index(), Some(5));
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "BSTreadTransfInterpolator must consume the payload exactly — \
         each tread is 68 bytes (4 + 32 + 32), 2 treads + 4-byte count + \
         4-byte data ref = 144 bytes"
    );
    assert_eq!(data.len(), 4 + 2 * 68 + 4, "fixture size sanity check");
}

#[test]
fn fnv_bs_tread_transf_interpolator_empty_array() {
    // num_tread_transforms = 0 — zero-tread case. Edge case for the
    // allocate_vec(0) path and the immediately-following data_ref.
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    data.extend_from_slice(&0u32.to_le_bytes()); // count
    data.extend_from_slice(&(-1i32).to_le_bytes()); // null data_ref

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "BSTreadTransfInterpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("zero-tread BSTreadTransfInterpolator must dispatch");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::BsTreadTransfInterpolator>()
        .unwrap();
    assert!(interp.tread_transforms.is_empty());
    assert!(interp.data_ref.is_null());
    assert_eq!(stream.position() as usize, data.len());
}

// ── #728 / NIF-D5-10 — BSCollisionQueryProxyExtraData (FO76) ─────

#[test]
fn fo76_bs_collision_query_proxy_extra_data_round_trips_byte_array() {
    let header = fnv_header_bspline(); // wire layout doesn't depend on bsver — ByteArray only
    let payload: &[u8] = b"\xDE\xAD\xBE\xEF\xCA\xFE";
    let mut data = Vec::new();
    data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    data.extend_from_slice(payload);

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "BSCollisionQueryProxyExtraData",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("BSCollisionQueryProxyExtraData must dispatch");
    assert_eq!(block.block_type_name(), "BSCollisionQueryProxyExtraData");
    let proxy = block
        .as_any()
        .downcast_ref::<extra_data::BsCollisionQueryProxyExtraData>()
        .expect("dispatch must produce BsCollisionQueryProxyExtraData");
    assert_eq!(proxy.data.as_slice(), payload);
    assert_eq!(stream.position() as usize, data.len());
}

// ── #978 / NIF-D5-NEW-02 — uncompressed B-spline interpolator dispatch ──

#[test]
fn ni_bspline_transform_interpolator_uncompressed_round_trip() {
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    // NiBSplineInterpolator base.
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&5i32.to_le_bytes());
    data.extend_from_slice(&6i32.to_le_bytes());
    // NiQuatTransform (32 bytes).
    data.extend_from_slice(&0.0f32.to_le_bytes()); // tx
    data.extend_from_slice(&0.0f32.to_le_bytes()); // ty
    data.extend_from_slice(&0.0f32.to_le_bytes()); // tz
    data.extend_from_slice(&1.0f32.to_le_bytes()); // qw (identity)
    data.extend_from_slice(&0.0f32.to_le_bytes()); // qx
    data.extend_from_slice(&0.0f32.to_le_bytes()); // qy
    data.extend_from_slice(&0.0f32.to_le_bytes()); // qz
    data.extend_from_slice(&1.0f32.to_le_bytes()); // scale
    // Three handles.
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&u32::MAX.to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "NiBSplineTransformInterpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("NiBSplineTransformInterpolator must dispatch");
    assert_eq!(block.block_type_name(), "NiBSplineTransformInterpolator");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::NiBSplineTransformInterpolator>()
        .expect("dispatch must produce NiBSplineTransformInterpolator");
    assert_eq!(interp.spline_data_ref.index(), Some(5));
    assert_eq!(interp.basis_data_ref.index(), Some(6));
    assert_eq!(interp.translation_handle, 0);
    assert_eq!(interp.rotation_handle, 1);
    assert_eq!(interp.scale_handle, u32::MAX);
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "60-byte uncompressed body must be consumed exactly"
    );
}

#[test]
fn ni_bspline_float_interpolator_uncompressed_round_trip() {
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    data.extend_from_slice(&0.0f32.to_le_bytes()); // start
    data.extend_from_slice(&1.0f32.to_le_bytes()); // stop
    data.extend_from_slice(&7i32.to_le_bytes()); // spline_data
    data.extend_from_slice(&8i32.to_le_bytes()); // basis_data
    data.extend_from_slice(&0.42f32.to_le_bytes()); // value
    data.extend_from_slice(&3u32.to_le_bytes()); // handle

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "NiBSplineFloatInterpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("NiBSplineFloatInterpolator must dispatch");
    assert_eq!(block.block_type_name(), "NiBSplineFloatInterpolator");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::NiBSplineFloatInterpolator>()
        .expect("dispatch must produce NiBSplineFloatInterpolator");
    assert_eq!(interp.value, 0.42);
    assert_eq!(interp.handle, 3);
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "24-byte uncompressed body must be consumed exactly"
    );
}

#[test]
fn ni_bspline_point3_interpolator_uncompressed_round_trip() {
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&9i32.to_le_bytes());
    data.extend_from_slice(&10i32.to_le_bytes());
    data.extend_from_slice(&0.5f32.to_le_bytes()); // vx
    data.extend_from_slice(&0.6f32.to_le_bytes()); // vy
    data.extend_from_slice(&0.7f32.to_le_bytes()); // vz
    data.extend_from_slice(&15u32.to_le_bytes()); // handle

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "NiBSplinePoint3Interpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("NiBSplinePoint3Interpolator must dispatch");
    assert_eq!(block.block_type_name(), "NiBSplinePoint3Interpolator");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::NiBSplinePoint3Interpolator>()
        .expect("dispatch must produce NiBSplinePoint3Interpolator");
    assert_eq!(interp.value, [0.5, 0.6, 0.7]);
    assert_eq!(interp.handle, 15);
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "32-byte uncompressed body must be consumed exactly"
    );
}
