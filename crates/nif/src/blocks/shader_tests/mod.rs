//! Shader-property parser regression tests, split by game era (#2056).
//!
//! `mod.rs` owns the shared header/byte-builder helpers; each era
//! submodule (`legacy`/`skyrim`/`fo4`/`fo76`/`starfield`) carries only
//! its `#[test]` functions and reaches the helpers via `use super::*`.
//! Included from `shader.rs` via `#[path = "shader_tests/mod.rs"] mod tests;`.


use super::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;
use std::sync::Arc;

fn make_header(user_version: u32, user_version_2: u32) -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version,
        user_version_2,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("ShaderProp")],
        max_string_length: 10,
        num_groups: 0,
    }
}


/// Build bytes for BSShaderPPLightingProperty, optionally including emissive color.
fn build_bsshader_bytes(user_version_2: u32) -> Vec<u8> {
    build_bsshader_bytes_with_emissive(user_version_2, None)
}


fn build_bsshader_bytes_with_emissive(user_version_2: u32, emissive: Option<[f32; 4]>) -> Vec<u8> {
    let mut data = Vec::new();
    // NiObjectNET: name (string table index 0)
    data.extend_from_slice(&0i32.to_le_bytes());
    // extra_data_refs: count=0
    data.extend_from_slice(&0u32.to_le_bytes());
    // controller_ref: -1
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // NiShadeProperty: shade_flags (u16)
    data.extend_from_slice(&0u16.to_le_bytes());
    // shader_type (u32)
    data.extend_from_slice(&1u32.to_le_bytes());
    // shader_flags_1 (u32)
    data.extend_from_slice(&0x80000000u32.to_le_bytes());
    // shader_flags_2 (u32)
    data.extend_from_slice(&0x00000001u32.to_le_bytes());
    // env_map_scale (f32)
    data.extend_from_slice(&1.0f32.to_le_bytes());
    // texture_clamp_mode (u32)
    data.extend_from_slice(&3u32.to_le_bytes());
    // texture_set_ref (i32)
    data.extend_from_slice(&5i32.to_le_bytes());
    // nif.xml:6245-6248 — refraction fields present for bsver > crate::version::bsver::FO3_REFRACTION, parallax
    // for bsver > crate::version::bsver::FO3_PARALLAX (strict). FNV: bsver=34, both present. FO3 ships some
    // content at bsver=24 (parallax absent — boundary case for #774).
    // Oblivion: bsver=0, neither.
    if user_version_2 > 14 {
        data.extend_from_slice(&0.5f32.to_le_bytes()); // refraction_strength
        data.extend_from_slice(&10i32.to_le_bytes()); // refraction_fire_period
    }
    if user_version_2 > 24 {
        data.extend_from_slice(&4.0f32.to_le_bytes()); // parallax_max_passes
        data.extend_from_slice(&1.5f32.to_le_bytes()); // parallax_scale
    }
    // nif.xml: Emissive Color (Color4) for bsver > crate::version::bsver::FO3_FNV.
    if let Some([r, g, b, a]) = emissive {
        data.extend_from_slice(&r.to_le_bytes());
        data.extend_from_slice(&g.to_le_bytes());
        data.extend_from_slice(&b.to_le_bytes());
        data.extend_from_slice(&a.to_le_bytes());
    }
    data
}


fn make_skyrim_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 83,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("TestShader")],
        max_string_length: 10,
        num_groups: 0,
    }
}


/// Build the common bytes for BSLightingShaderProperty (Skyrim LE, BSVER=83).
fn build_bs_lighting_common(shader_type: u32) -> Vec<u8> {
    let mut data = Vec::new();
    // shader_type (read before NiObjectNET for BSVER 83-130)
    data.extend_from_slice(&shader_type.to_le_bytes());
    // NiObjectNET: name (string table index 0)
    data.extend_from_slice(&0i32.to_le_bytes());
    // extra_data_refs: count=0
    data.extend_from_slice(&0u32.to_le_bytes());
    // controller_ref: -1
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // shader_flags_1, shader_flags_2
    data.extend_from_slice(&0x80000000u32.to_le_bytes());
    data.extend_from_slice(&0x00000010u32.to_le_bytes()); // two-sided flag
                                                          // uv_offset (2x f32)
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    // uv_scale (2x f32)
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    // texture_set_ref
    data.extend_from_slice(&3i32.to_le_bytes());
    // emissive_color (3x f32)
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&0.5f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    // emissive_multiple
    data.extend_from_slice(&2.0f32.to_le_bytes());
    // texture_clamp_mode
    data.extend_from_slice(&3u32.to_le_bytes());
    // alpha
    data.extend_from_slice(&0.8f32.to_le_bytes());
    // refraction_strength
    data.extend_from_slice(&0.0f32.to_le_bytes());
    // glossiness
    data.extend_from_slice(&50.0f32.to_le_bytes());
    // specular_color (3x f32)
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&0.9f32.to_le_bytes());
    data.extend_from_slice(&0.8f32.to_le_bytes());
    // specular_strength
    data.extend_from_slice(&1.5f32.to_le_bytes());
    // lighting_effect_1, lighting_effect_2
    data.extend_from_slice(&0.3f32.to_le_bytes());
    data.extend_from_slice(&0.7f32.to_le_bytes());
    data
}


fn make_fo4_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 130,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("FO4Shader")],
        max_string_length: 9,
        num_groups: 0,
    }
}


/// Build FO4 BSLightingShaderProperty bytes (BSVER=130, shader_type=1 env map).
fn build_bs_lighting_fo4_env_map() -> Vec<u8> {
    let mut data = Vec::new();
    // shader_type (read before NiObjectNET for BSVER 83-130)
    data.extend_from_slice(&1u32.to_le_bytes()); // EnvironmentMap
                                                 // NiObjectNET: name (string table index 0)
    data.extend_from_slice(&0i32.to_le_bytes());
    // extra_data_refs: count=0
    data.extend_from_slice(&0u32.to_le_bytes());
    // controller_ref: -1
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // shader_flags_1, shader_flags_2 (FO4 reads u32 pair)
    data.extend_from_slice(&0x80000000u32.to_le_bytes());
    data.extend_from_slice(&0x00000010u32.to_le_bytes());
    // uv_offset, uv_scale
    for v in [0.0f32, 0.0, 1.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // texture_set_ref
    data.extend_from_slice(&3i32.to_le_bytes());
    // emissive_color (3x f32)
    for v in [0.0f32, 0.5, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // emissive_multiple
    data.extend_from_slice(&2.0f32.to_le_bytes());
    // Root Material (FO4+: NiFixedString = string table index)
    data.extend_from_slice(&(-1i32).to_le_bytes()); // no root material
                                                    // texture_clamp_mode
    data.extend_from_slice(&3u32.to_le_bytes());
    // alpha
    data.extend_from_slice(&0.8f32.to_le_bytes());
    // refraction_strength
    data.extend_from_slice(&0.0f32.to_le_bytes());
    // glossiness (called "smoothness" in FO4, same f32)
    data.extend_from_slice(&0.5f32.to_le_bytes());
    // specular_color (3x f32)
    for v in [1.0f32, 0.9, 0.8] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // specular_strength
    data.extend_from_slice(&1.5f32.to_le_bytes());
    // lighting_effect_1, lighting_effect_2 — NOT present for BSVER >= 130
    // (the parser skips these with (0.0, 0.0))
    // FO4 common fields:
    data.extend_from_slice(&0.3f32.to_le_bytes()); // subsurface_rolloff
                                                   // rimlight_power = FLT_MAX sentinel → has backlight (#1175). Pre-fix
                                                   // this fixture authored a finite 2.5 to match an inverted gate.
    data.extend_from_slice(&f32::MAX.to_le_bytes()); // rimlight_power
    data.extend_from_slice(&1.0f32.to_le_bytes()); // backlight_power
    data.extend_from_slice(&0.7f32.to_le_bytes()); // grayscale_to_palette_scale
    data.extend_from_slice(&5.0f32.to_le_bytes()); // fresnel_power
                                                   // WetnessParams (BSVER=130: 6 floats). Order: spec_scale,
                                                   // spec_power, min_var, fresnel, metalness, unknown_1.
                                                   // #1223 — env_map_scale lives in the shader_type=1
                                                   // (EnvironmentMap) trailing block at BSVER < FO4_DLC_UPPER
                                                   // (140), NOT in wetness. Pre-#1223 this fixture wrote 7
                                                   // floats with a bogus env_map_scale slot, encoding the
                                                   // parser's old over-read gate; corrected here to match the
                                                   // empirical FO4 wire format (5211/6455 vanilla BSLSP @ 140
                                                   // bytes, 1192 @ 146 = 140 + 6 shader-type-1 trailing).
    for v in [0.1f32, 0.2, 0.3, 0.5, 0.6, 0.95] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // Shader type 1 trailing: env_map_scale + 2 bools (FO4 BSVER < FO4_DLC_UPPER)
    data.extend_from_slice(&0.75f32.to_le_bytes()); // env_map_scale
    data.push(1u8); // use_ssr (bool)
    data.push(0u8); // wetness_use_ssr (bool)
    data
}


// ── #409 BSVER-131 / 132 boundary regression tests ───────────────

/// Build a header with a custom BSVER — share the version number
/// path with the existing FO4 / FO76 helpers so only the boundary
/// tests need a fresh fixture. The body is the standard
/// `BSLightingShaderProperty` minus the flag-pair / CRC slots the
/// per-BSVER gate controls; callers assemble the rest.
fn make_fo4_header_with_bsver(bsver: u32) -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: bsver,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("BoundaryShader")],
        max_string_length: 14,
        num_groups: 0,
    }
}


// ── N23.9: FO76/Starfield tests ──────────────────────────────────

fn make_fo76_header(name: &str) -> NifHeader {
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


/// Build a BSLightingShaderProperty body with FO76 layout (BSVER=155), empty
/// name (so stopcond does NOT fire), shader_type=0 (Default → no trailing),
/// empty SF1/SF2 arrays, wetness + luminance, no translucency, no texture arrays.
fn build_fo76_bs_lighting_minimal() -> Vec<u8> {
    let mut data = Vec::new();
    // NiObjectNET: name = string table index 0 ("")
    data.extend_from_slice(&0i32.to_le_bytes());
    // extra_data_refs list: count=0
    data.extend_from_slice(&0u32.to_le_bytes());
    // controller_ref = -1
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // BSVER == 155: Shader Type (BSShaderType155) = 0 (Default)
    data.extend_from_slice(&0u32.to_le_bytes());
    // Num SF1 = 0 (BSVER >= 132)
    data.extend_from_slice(&0u32.to_le_bytes());
    // Num SF2 = 0 (BSVER >= 152)
    data.extend_from_slice(&0u32.to_le_bytes());
    // (no SF1/SF2 arrays because lengths are zero)
    // uv_offset, uv_scale
    for v in [0.0f32, 0.0, 1.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // texture_set_ref
    data.extend_from_slice(&5i32.to_le_bytes());
    // emissive_color (3×f32)
    for v in [0.1f32, 0.2, 0.3] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // emissive_multiple
    data.extend_from_slice(&1.5f32.to_le_bytes());
    // Root Material (NiFixedString, BSVER >= 130): -1
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // texture_clamp_mode
    data.extend_from_slice(&3u32.to_le_bytes());
    // alpha
    data.extend_from_slice(&0.9f32.to_le_bytes());
    // refraction_strength
    data.extend_from_slice(&0.0f32.to_le_bytes());
    // smoothness (glossiness in struct)
    data.extend_from_slice(&0.6f32.to_le_bytes());
    // specular_color
    for v in [0.7f32, 0.8, 0.9] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // specular_strength
    data.extend_from_slice(&1.25f32.to_le_bytes());
    // (no lighting_effect_1/2 — BSVER >= 130 skips)
    // (no subsurface/rimlight/backlight — not in BS_FO4_2 range)
    // grayscale_to_palette_scale
    data.extend_from_slice(&0.4f32.to_le_bytes());
    // fresnel_power
    data.extend_from_slice(&4.2f32.to_le_bytes());
    // WetnessParams: spec_scale, spec_power, min_var,
    // (env_map_scale only for BSVER==130, skipped here)
    // fresnel, metalness, unknown_1 (>130), unknown_2 (==155)
    for v in [0.11f32, 0.22, 0.33, 0.44, 0.55, 0.66, 0.77] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // FO76 luminance (4×f32)
    for v in [100.0f32, 13.5, 2.0, 3.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // Do Translucency = false (1 byte)
    data.push(0u8);
    // Has Texture Arrays = false (1 byte)
    data.push(0u8);
    // No shader-type trailing for type 0 Default
    data
}


// ── #713 / NIF-D3-01 — Skyrim BSSkyShaderProperty / BSWaterShaderProperty ──

/// Build a synthetic Skyrim LE (BSVER=83) `BSSkyShaderProperty`. Layout:
/// NiObjectNET (12 B) + flags1+flags2 (8 B) + UV offset+scale (16 B) +
/// source-texture sized string + sky_object_type u32. Total = 36 B + string.
fn build_bs_sky_shader_property(source_texture: &str, sky_object_type: u32) -> Vec<u8> {
    let mut data = Vec::new();
    // NiObjectNET: name (string-table 0), 0 extra-data refs, controller -1.
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // Skyrim shader flags: u32 pair on BSVER < 132.
    data.extend_from_slice(&0x80000000u32.to_le_bytes()); // SF1 default
    data.extend_from_slice(&0x00000021u32.to_le_bytes()); // SF2 default
                                                          // UV Offset (2x f32) + UV Scale (2x f32).
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    // Source Texture (sized string).
    data.extend_from_slice(&(source_texture.len() as u32).to_le_bytes());
    data.extend_from_slice(source_texture.as_bytes());
    // Sky Object Type (u32).
    data.extend_from_slice(&sky_object_type.to_le_bytes());
    data
}


/// Build a synthetic Skyrim LE `BSWaterShaderProperty`. Same prefix as
/// the sky variant but the per-block tail is just `Water Shader Flags`
/// (single u32).
fn build_bs_water_shader_property(water_shader_flags: u32) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0x80000008u32.to_le_bytes()); // SF1 default
    data.extend_from_slice(&0x00000021u32.to_le_bytes()); // SF2 default
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&water_shader_flags.to_le_bytes());
    data
}


// ── #746 + #747 Starfield BSVER 172 regressions ─────────────────────

/// Starfield header (NIF 20.2.0.7 / `bsver = 172` per
/// `crates/nif/src/version.rs:129`). Mirror of `make_fo76_header`
/// for the regression of #109 captured in #746 / #747.
fn make_starfield_header(name: &str) -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 172,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from(name)],
        max_string_length: name.len() as u32,
        num_groups: 0,
    }
}


/// Build a minimal **Starfield** (BSVER 172) `BSLightingShaderProperty`
/// body — the FO76 shape MINUS every `#BS_F76#`-gated field. Per
/// nif.xml `#BS_F76# = (BSVER == 155)` ("Fallout 76 stream 155 only"),
/// Starfield omits the `BSShaderType155` field, the WetnessParams
/// `unknown_2`, and the Luminance / Translucency / texture-array tail.
/// Name index 0 is "" so the block takes the full-body path (a
/// non-empty Starfield name is a content-hash material reference → stub).
fn build_starfield_bs_lighting_minimal() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes()); // name idx 0 ("")
    data.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                    // NO BSShaderType155 (FO76 == 155 only)
    data.extend_from_slice(&0u32.to_le_bytes()); // num_sf1 (>= 132)
    data.extend_from_slice(&0u32.to_le_bytes()); // num_sf2 (>= 152)
    for v in [0.0f32, 0.0, 1.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes()); // uv_offset, uv_scale
    }
    data.extend_from_slice(&5i32.to_le_bytes()); // texture_set_ref
    for v in [0.1f32, 0.2, 0.3] {
        data.extend_from_slice(&v.to_le_bytes()); // emissive_color
    }
    data.extend_from_slice(&1.5f32.to_le_bytes()); // emissive_multiple
    data.extend_from_slice(&(-1i32).to_le_bytes()); // root_material (>= 130)
    data.extend_from_slice(&3u32.to_le_bytes()); // texture_clamp_mode
    data.extend_from_slice(&0.9f32.to_le_bytes()); // alpha
    data.extend_from_slice(&0.0f32.to_le_bytes()); // refraction_strength
    data.extend_from_slice(&0.6f32.to_le_bytes()); // smoothness
    for v in [0.7f32, 0.8, 0.9] {
        data.extend_from_slice(&v.to_le_bytes()); // specular_color
    }
    data.extend_from_slice(&1.25f32.to_le_bytes()); // specular_strength
    data.extend_from_slice(&0.4f32.to_le_bytes()); // grayscale_to_palette (>= 130)
    data.extend_from_slice(&4.2f32.to_le_bytes()); // fresnel_power (>= 130)
                                                   // wetness: spec_scale, spec_power, min_var, fresnel, metalness,
                                                   // unknown_1 — NO env_map_scale (== 130), NO unknown_2 (== 155).
    for v in [0.11f32, 0.22, 0.33, 0.44, 0.55, 0.66] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // NO luminance / translucency / texture-array tail (FO76 == 155 only).
    // shader_type 0 → ShaderTypeData::None (no trailing fields).
    data
}


/// Build a minimal **Starfield** (BSVER 172) full-body `BSEffectShaderProperty`.
/// Name index 0 is "" so the block takes the full-body path (a non-empty
/// Starfield name is a content-hash material reference → stub). The NiObjectNET
/// name is a header-table INDEX (`0i32`); every `BSEffectShaderProperty` texture
/// field is a length-prefixed INLINE sized string (`0u32` length = empty), and
/// `controller_ref` is a `-1` BlockRef.
fn build_starfield_bs_effect_minimal() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes()); // name idx 0 ("")
    data.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref (BlockRef)
                                                    // BSVER >= 132: CRC arrays, both empty
    data.extend_from_slice(&0u32.to_le_bytes()); // num_sf1
    data.extend_from_slice(&0u32.to_le_bytes()); // num_sf2 (>= 152)
    for v in [0.0f32, 0.0, 1.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes()); // uv_offset, uv_scale
    }
    data.extend_from_slice(&0u32.to_le_bytes()); // source_texture (empty sized string)
    data.extend_from_slice(&[3u8, 0u8, 0u8, 0u8]); // clamp, light_infl, min_lod, unused
    for _ in 0..4 {
        data.extend_from_slice(&1.0f32.to_le_bytes()); // falloff start/stop angle+opacity
    }
    data.extend_from_slice(&0.25f32.to_le_bytes()); // refraction_power (>= 155)
    for _ in 0..4 {
        data.extend_from_slice(&1.0f32.to_le_bytes()); // base_color
    }
    data.extend_from_slice(&1.0f32.to_le_bytes()); // base_color_scale
    data.extend_from_slice(&50.0f32.to_le_bytes()); // soft_falloff_depth
    data.extend_from_slice(&0u32.to_le_bytes()); // greyscale_texture (empty)
                                                 // FO4+ (>= 130): env / normal / mask (empty) + env_map_scale
    for _ in 0..3 {
        data.extend_from_slice(&0u32.to_le_bytes());
    }
    data.extend_from_slice(&1.0f32.to_le_bytes()); // env_map_scale
                                                   // FO76+ (>= 155): reflectance / lighting (empty), emittance, gradient (empty), luminance
    data.extend_from_slice(&0u32.to_le_bytes()); // reflectance_texture
    data.extend_from_slice(&0u32.to_le_bytes()); // lighting_texture
    for v in [0.4f32, 0.5, 0.6] {
        data.extend_from_slice(&v.to_le_bytes()); // emittance_color
    }
    data.extend_from_slice(&0u32.to_le_bytes()); // emit_gradient_texture
    for v in [100.0f32, 13.5, 2.0, 3.0] {
        data.extend_from_slice(&v.to_le_bytes()); // luminance
    }
    data
}


// ── #1331 sibling: BSShaderNoLightingProperty falloff width per-file BSVER ──

/// Build a `BSShaderNoLightingProperty` block: NiObjectNET (name idx 0) +
/// FO3 shader base + texture_clamp_mode + sized `file_name`, optionally
/// followed by the four falloff floats.
fn build_no_lighting_bytes(file_name: &str, falloff: Option<[f32; 4]>) -> Vec<u8> {
    let mut d = Vec::new();
    // NiObjectNET
    d.extend_from_slice(&0i32.to_le_bytes()); // name string index 0
    d.extend_from_slice(&0u32.to_le_bytes()); // extra_data list count = 0
    d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref = -1
                                                 // BSShaderPropertyData::parse_base
    d.extend_from_slice(&0u16.to_le_bytes()); // shade_flags
    d.extend_from_slice(&1u32.to_le_bytes()); // shader_type
    d.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_1
    d.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_2
    d.extend_from_slice(&1.0f32.to_le_bytes()); // env_map_scale
                                                // BSShaderLightingProperty texture_clamp_mode
    d.extend_from_slice(&3u32.to_le_bytes());
    // file_name (sized string)
    d.extend_from_slice(&(file_name.len() as u32).to_le_bytes());
    d.extend_from_slice(file_name.as_bytes());
    if let Some(f) = falloff {
        for v in f {
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    d
}

mod legacy;
mod skyrim;
mod fo4;
mod fo76;
mod starfield;
