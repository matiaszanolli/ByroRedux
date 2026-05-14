//! Shader-property dispatch tests.
//!
//! BSShaderPPLighting / Tile / Sky / Water / TallGrass / zero-field variants
//! — each pinned to its dedicated parser per #145 / #455 / #474 / #550 / #713
//! / #717.

use super::{oblivion_bsshader_bytes, oblivion_header};
use crate::blocks::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;
use std::sync::Arc;

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
