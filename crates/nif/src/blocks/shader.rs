//! Bethesda shader property blocks.
//!
//! - BSShaderPPLightingProperty / BSShaderNoLightingProperty — Fallout 3/NV
//! - BSLightingShaderProperty / BSEffectShaderProperty — Skyrim+
//! - BSShaderTextureSet — shared texture path list (all games)

use super::base::{BSShaderPropertyData, NiObjectNETData};
use super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use std::any::Any;
use std::io;

/// BSShaderPPLightingProperty — Fallout 3/NV per-pixel lighting shader.
///
/// Inheritance: NiProperty → BSShaderProperty → BSShaderLightingProperty
///              → BSShaderPPLightingProperty.
///
/// The texture set reference points to a BSShaderTextureSet block
/// containing the actual texture file paths.
#[derive(Debug)]
pub struct BSShaderPPLightingProperty {
    pub net: NiObjectNETData,
    pub shader: BSShaderPropertyData,
    pub texture_clamp_mode: u32,
    pub texture_set_ref: BlockRef,
    /// Refraction strength (0.0–1.0). Present when bsver >= 15.
    pub refraction_strength: f32,
    /// Refraction fire period. Present when bsver >= 15.
    pub refraction_fire_period: i32,
    /// Parallax max passes. Present when bsver >= 24.
    pub parallax_max_passes: f32,
    /// Parallax scale. Present when bsver >= 24.
    pub parallax_scale: f32,
}

impl BSShaderPPLightingProperty {
    pub fn shader_flags_1(&self) -> u32 {
        self.shader.shader_flags_1
    }
    pub fn shader_flags_2(&self) -> u32 {
        self.shader.shader_flags_2
    }
}

impl NiObject for BSShaderPPLightingProperty {
    fn block_type_name(&self) -> &'static str {
        "BSShaderPPLightingProperty"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BSShaderPPLightingProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let (shader, texture_clamp_mode) = BSShaderPropertyData::parse_fo3(stream)?;
        let texture_set_ref = stream.read_block_ref()?;

        // nif.xml: Refraction Strength (f32) + Refraction Fire Period (i32) for bsver >= 15.
        let bsver = stream.variant().bsver();
        let (refraction_strength, refraction_fire_period) = if bsver >= 15 {
            (stream.read_f32_le()?, stream.read_i32_le()?)
        } else {
            (0.0, 0)
        };

        // nif.xml: Parallax Max Passes (f32) + Parallax Scale (f32) for bsver >= 24.
        let (parallax_max_passes, parallax_scale) = if bsver >= 24 {
            (stream.read_f32_le()?, stream.read_f32_le()?)
        } else {
            (4.0, 1.0)
        };

        Ok(Self {
            net,
            shader,
            texture_clamp_mode,
            texture_set_ref,
            refraction_strength,
            refraction_fire_period,
            parallax_max_passes,
            parallax_scale,
        })
    }
}

/// BSShaderNoLightingProperty — Fallout 3/NV no-light shader (e.g. UI elements, effects).
///
/// Inheritance: NiProperty → BSShaderProperty → BSShaderLightingProperty
///              → BSShaderNoLightingProperty.
///
/// Instead of a texture set reference, this shader embeds a file name directly
/// and has falloff parameters for alpha blending.
#[derive(Debug)]
pub struct BSShaderNoLightingProperty {
    pub net: NiObjectNETData,
    pub shader: BSShaderPropertyData,
    pub texture_clamp_mode: u32,
    pub file_name: String,
    pub falloff_start_angle: f32,
    pub falloff_stop_angle: f32,
    pub falloff_start_opacity: f32,
    pub falloff_stop_opacity: f32,
}

impl BSShaderNoLightingProperty {
    pub fn shader_flags_1(&self) -> u32 {
        self.shader.shader_flags_1
    }
}

impl NiObject for BSShaderNoLightingProperty {
    fn block_type_name(&self) -> &'static str {
        "BSShaderNoLightingProperty"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BSShaderNoLightingProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let (shader, texture_clamp_mode) = BSShaderPropertyData::parse_fo3(stream)?;
        let file_name = stream.read_sized_string()?;

        let (falloff_start_angle, falloff_stop_angle, falloff_start_opacity, falloff_stop_opacity) =
            if stream.variant().bsver() > 26 {
                (
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                )
            } else {
                (0.0, 0.0, 1.0, 0.0)
            };

        Ok(Self {
            net,
            shader,
            texture_clamp_mode,
            file_name,
            falloff_start_angle,
            falloff_stop_angle,
            falloff_start_opacity,
            falloff_stop_opacity,
        })
    }
}

/// BSShaderTextureSet — list of texture file paths for a BSShader.
///
/// Typically 6 textures: diffuse, normal, glow, parallax, env, env mask.
#[derive(Debug)]
pub struct BSShaderTextureSet {
    pub textures: Vec<String>,
}

impl NiObject for BSShaderTextureSet {
    fn block_type_name(&self) -> &'static str {
        "BSShaderTextureSet"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BSShaderTextureSet {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiObject base reads nothing for modern versions.
        let num_textures = stream.read_i32_le()?;
        let mut textures = Vec::with_capacity(num_textures.max(0) as usize);
        for _ in 0..num_textures {
            // Texture paths are always sized strings (u32 len + bytes),
            // NOT string table indices.
            textures.push(stream.read_sized_string()?);
        }

        Ok(Self { textures })
    }
}

/// Shader-type-specific trailing data for BSLightingShaderProperty.
///
/// After the common fields, BSLightingShaderProperty has 0–7 additional fields
/// determined by the `shader_type` value. These carry type-specific rendering
/// parameters (env map scale, skin tint, parallax, eye cubemap, etc.).
#[derive(Debug, Clone)]
pub enum ShaderTypeData {
    /// Type 0 (Default), 2 (Glow), 3 (Parallax), 4 (Face Tint),
    /// 8–10 (Landscape), 12–13 (Tree/LOD), 15 (LOD HD), 17–19 (Cloud/Noise).
    None,
    /// Type 1: Environment Map.
    EnvironmentMap {
        env_map_scale: f32,
    },
    /// Type 5: Skin Tint.
    SkinTint {
        skin_tint_color: [f32; 3],
    },
    /// Type 6: Hair Tint.
    HairTint {
        hair_tint_color: [f32; 3],
    },
    /// Type 7: Parallax Occlusion.
    ParallaxOcc {
        max_passes: f32,
        scale: f32,
    },
    /// Type 11: Multi-Layer Parallax.
    MultiLayerParallax {
        inner_layer_thickness: f32,
        refraction_scale: f32,
        inner_layer_texture_scale: [f32; 2],
        envmap_strength: f32,
    },
    /// Type 14: Sparkle Snow.
    SparkleSnow {
        sparkle_parameters: [f32; 4],
    },
    /// Type 16: Eye Environment Map.
    EyeEnvmap {
        eye_cubemap_scale: f32,
        left_eye_reflection_center: [f32; 3],
        right_eye_reflection_center: [f32; 3],
    },
}

/// BSLightingShaderProperty — Skyrim+ per-pixel lighting shader.
///
/// Inheritance: NiObjectNET → NiProperty → BSShaderProperty → BSLightingShaderProperty.
/// Replaces BSShaderPPLightingProperty starting with Skyrim (BSVER >= 83).
///
/// For Skyrim LE/SE, BSShaderProperty base adds no fields (its FO3-only fields
/// are skipped). The shader type is a Skyrim-specific field read before the name
/// in NiObjectNET (per nif.xml `onlyT` condition).
#[derive(Debug)]
pub struct BSLightingShaderProperty {
    pub shader_type: u32,
    pub net: NiObjectNETData,
    pub shader_flags_1: u32,
    pub shader_flags_2: u32,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub texture_set_ref: BlockRef,
    pub emissive_color: [f32; 3],
    pub emissive_multiple: f32,
    pub texture_clamp_mode: u32,
    pub alpha: f32,
    pub refraction_strength: f32,
    pub glossiness: f32,
    pub specular_color: [f32; 3],
    pub specular_strength: f32,
    pub lighting_effect_1: f32,
    pub lighting_effect_2: f32,
    /// Shader-type-specific trailing fields (env map, skin tint, eye cubemap, etc.).
    pub shader_type_data: ShaderTypeData,
}

impl NiObject for BSLightingShaderProperty {
    fn block_type_name(&self) -> &'static str {
        "BSLightingShaderProperty"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BSLightingShaderProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiObjectNET: shader type comes BEFORE name for BSLightingShaderProperty
        // (nif.xml onlyT="BSLightingShaderProperty", BSVER 83-130).
        let shader_type = if stream.variant().bsver() >= 83 && stream.variant().bsver() <= 130 {
            stream.read_u32_le()?
        } else {
            0
        };

        let net = NiObjectNETData::parse(stream)?;

        // BSLightingShaderProperty fields.
        // Shader flags — u32 pair for Skyrim and FO4. FO76+ uses a different format.
        let (shader_flags_1, shader_flags_2) = if !stream.variant().uses_fo76_shader_flags() {
            (stream.read_u32_le()?, stream.read_u32_le()?)
        } else {
            // FO76/Starfield: variable-length flag arrays — skip via block size adjustment.
            (0, 0)
        };

        let uv_offset = [stream.read_f32_le()?, stream.read_f32_le()?];
        let uv_scale = [stream.read_f32_le()?, stream.read_f32_le()?];
        let texture_set_ref = stream.read_block_ref()?;
        let emissive_color = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let emissive_multiple = stream.read_f32_le()?;

        // Root Material (NiFixedString) — FO4+ only (BSVER >= 130).
        if stream.variant().bsver() >= 130 {
            let _root_material = stream.read_string()?;
        }

        let texture_clamp_mode = stream.read_u32_le()?;
        let alpha = stream.read_f32_le()?;
        let refraction_strength = stream.read_f32_le()?;

        // Glossiness (Skyrim) or Smoothness (FO4+).
        let glossiness = stream.read_f32_le()?;

        let specular_color = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let specular_strength = stream.read_f32_le()?;

        // Lighting effects — Skyrim only (BSVER < 130).
        let (lighting_effect_1, lighting_effect_2) = if stream.variant().bsver() < 130 {
            (stream.read_f32_le()?, stream.read_f32_le()?)
        } else {
            (0.0, 0.0)
        };

        // Shader-type-specific trailing fields (Skyrim LE/SE only, BSVER < 130).
        // FO4+ has additional common fields before these — deferred to N23.2 Phase 2.
        let shader_type_data = if stream.variant().bsver() < 130 {
            parse_shader_type_data(stream, shader_type)?
        } else {
            ShaderTypeData::None
        };

        Ok(Self {
            shader_type,
            net,
            shader_flags_1,
            shader_flags_2,
            uv_offset,
            uv_scale,
            texture_set_ref,
            emissive_color,
            emissive_multiple,
            texture_clamp_mode,
            alpha,
            refraction_strength,
            glossiness,
            specular_color,
            specular_strength,
            lighting_effect_1,
            lighting_effect_2,
            shader_type_data,
        })
    }
}

/// Parse shader-type-specific trailing fields from BSLightingShaderProperty.
/// Called for Skyrim LE/SE (BSVER < 130) where these fields follow immediately
/// after lighting_effect_2 with no intervening common fields.
fn parse_shader_type_data(stream: &mut NifStream, shader_type: u32) -> io::Result<ShaderTypeData> {
    match shader_type {
        1 => {
            // Environment Map
            let env_map_scale = stream.read_f32_le()?;
            Ok(ShaderTypeData::EnvironmentMap { env_map_scale })
        }
        5 => {
            // Skin Tint
            let skin_tint_color = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            Ok(ShaderTypeData::SkinTint { skin_tint_color })
        }
        6 => {
            // Hair Tint
            let hair_tint_color = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            Ok(ShaderTypeData::HairTint { hair_tint_color })
        }
        7 => {
            // Parallax Occlusion
            let max_passes = stream.read_f32_le()?;
            let scale = stream.read_f32_le()?;
            Ok(ShaderTypeData::ParallaxOcc { max_passes, scale })
        }
        11 => {
            // Multi-Layer Parallax
            let inner_layer_thickness = stream.read_f32_le()?;
            let refraction_scale = stream.read_f32_le()?;
            let inner_layer_texture_scale = [stream.read_f32_le()?, stream.read_f32_le()?];
            let envmap_strength = stream.read_f32_le()?;
            Ok(ShaderTypeData::MultiLayerParallax {
                inner_layer_thickness,
                refraction_scale,
                inner_layer_texture_scale,
                envmap_strength,
            })
        }
        14 => {
            // Sparkle Snow
            let sparkle_parameters = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            Ok(ShaderTypeData::SparkleSnow {
                sparkle_parameters,
            })
        }
        16 => {
            // Eye Environment Map
            let eye_cubemap_scale = stream.read_f32_le()?;
            let left_eye_reflection_center = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let right_eye_reflection_center = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            Ok(ShaderTypeData::EyeEnvmap {
                eye_cubemap_scale,
                left_eye_reflection_center,
                right_eye_reflection_center,
            })
        }
        // Types 0,2,3,4,8,9,10,12,13,15,17,18,19,20 have no trailing fields.
        _ => Ok(ShaderTypeData::None),
    }
}

/// BSEffectShaderProperty — Skyrim+ effect/VFX shader.
///
/// Unlike BSLightingShaderProperty, this shader embeds a source texture
/// filename as a sized string rather than referencing a BSShaderTextureSet.
#[derive(Debug)]
pub struct BSEffectShaderProperty {
    pub net: NiObjectNETData,
    pub shader_flags_1: u32,
    pub shader_flags_2: u32,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub source_texture: String,
    pub texture_clamp_mode: u8,
    pub lighting_influence: u8,
    pub env_map_min_lod: u8,
    pub falloff_start_angle: f32,
    pub falloff_stop_angle: f32,
    pub falloff_start_opacity: f32,
    pub falloff_stop_opacity: f32,
    pub emissive_color: [f32; 4],
    pub emissive_multiple: f32,
    pub soft_falloff_depth: f32,
    pub greyscale_texture: String,
    /// Environment map texture path (FO4+ only, BSVER >= 130).
    pub env_map_texture: String,
    /// Normal texture path (FO4+ only, BSVER >= 130).
    pub normal_texture: String,
    /// Environment mask texture path (FO4+ only, BSVER >= 130).
    pub env_mask_texture: String,
    /// Environment map scale (FO4+ only, BSVER >= 130).
    pub env_map_scale: f32,
}

impl NiObject for BSEffectShaderProperty {
    fn block_type_name(&self) -> &'static str {
        "BSEffectShaderProperty"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BSEffectShaderProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;

        // Shader flags — u32 pair for Skyrim and FO4. FO76+ uses a different format.
        let (shader_flags_1, shader_flags_2) = if !stream.variant().uses_fo76_shader_flags() {
            (stream.read_u32_le()?, stream.read_u32_le()?)
        } else {
            // FO76/Starfield: variable-length flag arrays — skip via block size adjustment.
            (0, 0)
        };

        let uv_offset = [stream.read_f32_le()?, stream.read_f32_le()?];
        let uv_scale = [stream.read_f32_le()?, stream.read_f32_le()?];

        // Source texture as sized string (NOT a texture set reference).
        let source_texture = stream.read_sized_string()?;

        // 4 bytes packed: texture_clamp_mode(u8), lighting_influence(u8),
        // env_map_min_lod(u8), unused(u8).
        let texture_clamp_mode = stream.read_u8()?;
        let lighting_influence = stream.read_u8()?;
        let env_map_min_lod = stream.read_u8()?;
        let _unused = stream.read_u8()?;

        let falloff_start_angle = stream.read_f32_le()?;
        let falloff_stop_angle = stream.read_f32_le()?;
        let falloff_start_opacity = stream.read_f32_le()?;
        let falloff_stop_opacity = stream.read_f32_le()?;

        // FO76+ has refraction power here — skip for Skyrim.
        if stream.variant().uses_fo76_shader_flags() {
            let _refraction_power = stream.read_f32_le()?;
        }

        let emissive_color = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let emissive_multiple = stream.read_f32_le()?;

        // Soft falloff depth — present in all versions.
        let soft_falloff_depth = stream.read_f32_le()?;

        // Greyscale texture — sized string, present in all versions.
        let greyscale_texture = stream.read_sized_string()?;

        // FO4+ additional textures (BSVER >= 130).
        let bsver = stream.variant().bsver();
        let (env_map_texture, normal_texture, env_mask_texture, env_map_scale) = if bsver >= 130 {
            let env = stream.read_sized_string()?;
            let norm = stream.read_sized_string()?;
            let mask = stream.read_sized_string()?;
            let scale = stream.read_f32_le()?;
            (env, norm, mask, scale)
        } else {
            (String::new(), String::new(), String::new(), 0.0)
        };

        // Remaining FO76+ fields (reflectance/lighting/emit textures, luminance)
        // are skipped — block size check adjusts stream.

        Ok(Self {
            net,
            shader_flags_1,
            shader_flags_2,
            uv_offset,
            uv_scale,
            source_texture,
            texture_clamp_mode,
            lighting_influence,
            env_map_min_lod,
            falloff_start_angle,
            falloff_stop_angle,
            falloff_start_opacity,
            falloff_stop_opacity,
            emissive_color,
            emissive_multiple,
            soft_falloff_depth,
            greyscale_texture,
            env_map_texture,
            normal_texture,
            env_mask_texture,
            env_map_scale,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::NifHeader;
    use crate::stream::NifStream;
    use crate::version::NifVersion;

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
            strings: vec!["ShaderProp".to_string()],
            max_string_length: 10,
            num_groups: 0,
        }
    }

    /// Build bytes for BSShaderPPLightingProperty, optionally including emissive color.
    fn build_bsshader_bytes(user_version_2: u32) -> Vec<u8> {
        let mut data = Vec::new();
        // NiObjectNET: name (string table index 0)
        data.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs: count=0
        data.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref: -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // BSShaderProperty: shader_flags (u16)
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
        // Refraction/parallax fields — bsver >= 15 reads refraction, bsver >= 24 adds parallax.
        // FNV: bsver=34, so both are present. Oblivion: bsver=0, so neither.
        if user_version_2 >= 15 {
            data.extend_from_slice(&0.5f32.to_le_bytes()); // refraction_strength
            data.extend_from_slice(&10i32.to_le_bytes()); // refraction_fire_period
        }
        if user_version_2 >= 24 {
            data.extend_from_slice(&4.0f32.to_le_bytes()); // parallax_max_passes
            data.extend_from_slice(&1.5f32.to_le_bytes()); // parallax_scale
        }
        data
    }

    #[test]
    fn parse_bsshader_fnv_reads_refraction_parallax() {
        // FNV (bsver=34): reads refraction (bsver>=15) + parallax (bsver>=24) = 16 bytes.
        let header = make_header(11, 34);
        let data = build_bsshader_bytes(34);
        let mut stream = NifStream::new(&data, &header);

        let prop = BSShaderPPLightingProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.texture_set_ref.index(), Some(5));
        assert!((prop.refraction_strength - 0.5).abs() < 1e-6);
        assert_eq!(prop.refraction_fire_period, 10);
        assert!((prop.parallax_max_passes - 4.0).abs() < 1e-6);
        assert!((prop.parallax_scale - 1.5).abs() < 1e-6);
        // All data consumed: 38 base + 16 refraction/parallax = 54 bytes
        assert_eq!(stream.position(), 54);
    }

    #[test]
    fn parse_bsshader_oblivion_no_extra_fields() {
        // Oblivion (bsver=0): no refraction or parallax fields.
        let header = make_header(0, 0);
        let data = build_bsshader_bytes(0);
        let mut stream = NifStream::new(&data, &header);

        let prop = BSShaderPPLightingProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.texture_set_ref.index(), Some(5));
        assert_eq!(prop.refraction_strength, 0.0);
        assert_eq!(prop.refraction_fire_period, 0);
        assert!((prop.parallax_max_passes - 4.0).abs() < 1e-6); // defaults
        assert!((prop.parallax_scale - 1.0).abs() < 1e-6);
        // Only 38 bytes consumed (no extras)
        assert_eq!(stream.position(), 38);
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
            strings: vec!["TestShader".to_string()],
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

    #[test]
    fn parse_bs_lighting_default_no_trailing() {
        let header = make_skyrim_header();
        let data = build_bs_lighting_common(0); // shader_type=0 (Default)
        let mut stream = NifStream::new(&data, &header);

        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.shader_type, 0);
        assert!((prop.glossiness - 50.0).abs() < 1e-6);
        assert!(matches!(prop.shader_type_data, ShaderTypeData::None));
        // All common data consumed, no trailing fields.
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_lighting_env_map_trailing() {
        let header = make_skyrim_header();
        let mut data = build_bs_lighting_common(1); // shader_type=1 (EnvironmentMap)
        data.extend_from_slice(&0.75f32.to_le_bytes()); // env_map_scale
        let mut stream = NifStream::new(&data, &header);

        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.shader_type, 1);
        match prop.shader_type_data {
            ShaderTypeData::EnvironmentMap { env_map_scale } => {
                assert!((env_map_scale - 0.75).abs() < 1e-6);
            }
            _ => panic!("expected EnvironmentMap"),
        }
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_lighting_skin_tint_trailing() {
        let header = make_skyrim_header();
        let mut data = build_bs_lighting_common(5); // shader_type=5 (SkinTint)
        data.extend_from_slice(&0.9f32.to_le_bytes());
        data.extend_from_slice(&0.7f32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());
        let mut stream = NifStream::new(&data, &header);

        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        match prop.shader_type_data {
            ShaderTypeData::SkinTint { skin_tint_color } => {
                assert!((skin_tint_color[0] - 0.9).abs() < 1e-6);
                assert!((skin_tint_color[1] - 0.7).abs() < 1e-6);
                assert!((skin_tint_color[2] - 0.5).abs() < 1e-6);
            }
            _ => panic!("expected SkinTint"),
        }
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_lighting_eye_envmap_trailing() {
        let header = make_skyrim_header();
        let mut data = build_bs_lighting_common(16); // shader_type=16 (EyeEnvmap)
        // eye_cubemap_scale
        data.extend_from_slice(&1.2f32.to_le_bytes());
        // left_eye_reflection_center (3x f32)
        data.extend_from_slice(&(-0.05f32).to_le_bytes());
        data.extend_from_slice(&0.12f32.to_le_bytes());
        data.extend_from_slice(&0.03f32.to_le_bytes());
        // right_eye_reflection_center (3x f32)
        data.extend_from_slice(&0.05f32.to_le_bytes());
        data.extend_from_slice(&0.12f32.to_le_bytes());
        data.extend_from_slice(&0.03f32.to_le_bytes());
        let mut stream = NifStream::new(&data, &header);

        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        match prop.shader_type_data {
            ShaderTypeData::EyeEnvmap {
                eye_cubemap_scale,
                left_eye_reflection_center,
                right_eye_reflection_center,
            } => {
                assert!((eye_cubemap_scale - 1.2).abs() < 1e-6);
                assert!((left_eye_reflection_center[0] - (-0.05)).abs() < 1e-6);
                assert!((right_eye_reflection_center[0] - 0.05).abs() < 1e-6);
            }
            _ => panic!("expected EyeEnvmap"),
        }
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_lighting_multilayer_parallax_trailing() {
        let header = make_skyrim_header();
        let mut data = build_bs_lighting_common(11); // shader_type=11 (MultiLayerParallax)
        data.extend_from_slice(&0.1f32.to_le_bytes()); // inner_layer_thickness
        data.extend_from_slice(&0.5f32.to_le_bytes()); // refraction_scale
        data.extend_from_slice(&2.0f32.to_le_bytes()); // inner_layer_texture_scale u
        data.extend_from_slice(&2.0f32.to_le_bytes()); // inner_layer_texture_scale v
        data.extend_from_slice(&0.8f32.to_le_bytes()); // envmap_strength
        let mut stream = NifStream::new(&data, &header);

        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        match prop.shader_type_data {
            ShaderTypeData::MultiLayerParallax {
                inner_layer_thickness,
                envmap_strength,
                ..
            } => {
                assert!((inner_layer_thickness - 0.1).abs() < 1e-6);
                assert!((envmap_strength - 0.8).abs() < 1e-6);
            }
            _ => panic!("expected MultiLayerParallax"),
        }
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_effect_shader_soft_falloff_and_greyscale() {
        let header = make_skyrim_header();
        let mut data = Vec::new();
        // NiObjectNET: name (string table index 0)
        data.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs: count=0
        data.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref: -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // shader_flags_1, shader_flags_2
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        // uv_offset, uv_scale
        for _ in 0..4 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // source_texture: sized string "tex/glow.dds"
        let tex = b"tex/glow.dds";
        data.extend_from_slice(&(tex.len() as u32).to_le_bytes());
        data.extend_from_slice(tex);
        // texture_clamp_mode(u8), lighting_influence(u8), env_map_min_lod(u8), unused(u8)
        data.extend_from_slice(&[3u8, 128u8, 5u8, 0u8]);
        // falloff: start_angle, stop_angle, start_opacity, stop_opacity
        for _ in 0..4 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // emissive_color (4x f32)
        for _ in 0..4 {
            data.extend_from_slice(&1.0f32.to_le_bytes());
        }
        // emissive_multiple
        data.extend_from_slice(&2.0f32.to_le_bytes());
        // soft_falloff_depth
        data.extend_from_slice(&5.0f32.to_le_bytes());
        // greyscale_texture: sized string "tex/grey.dds"
        let grey = b"tex/grey.dds";
        data.extend_from_slice(&(grey.len() as u32).to_le_bytes());
        data.extend_from_slice(grey);

        let mut stream = NifStream::new(&data, &header);
        let prop = BSEffectShaderProperty::parse(&mut stream).unwrap();

        assert_eq!(prop.source_texture, "tex/glow.dds");
        assert_eq!(prop.lighting_influence, 128);
        assert_eq!(prop.env_map_min_lod, 5);
        assert!((prop.soft_falloff_depth - 5.0).abs() < 1e-6);
        assert_eq!(prop.greyscale_texture, "tex/grey.dds");
        assert!(prop.env_map_texture.is_empty()); // Not FO4+
        assert_eq!(stream.position(), data.len() as u64);
    }
}
