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
        // Shader flags — Skyrim format (BSVER < 130). FO4+ uses different flag format.
        let (shader_flags_1, shader_flags_2) = if !stream.variant().uses_fo4_shader_flags()
            && !stream.variant().uses_fo76_shader_flags()
        {
            (stream.read_u32_le()?, stream.read_u32_le()?)
        } else {
            // FO4/FO76 flags: not supported yet, skip via block size adjustment.
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

        // Remaining shader-type-specific fields (env map scale, skin tint, parallax,
        // eye cubemap, etc.) are skipped — the block size check in parse_nif will
        // adjust the stream position.

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
        })
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
    pub falloff_start_angle: f32,
    pub falloff_stop_angle: f32,
    pub falloff_start_opacity: f32,
    pub falloff_stop_opacity: f32,
    pub emissive_color: [f32; 4],
    pub emissive_multiple: f32,
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

        // Shader flags — Skyrim format (BSVER < 130).
        let (shader_flags_1, shader_flags_2) = if !stream.variant().uses_fo4_shader_flags()
            && !stream.variant().uses_fo76_shader_flags()
        {
            (stream.read_u32_le()?, stream.read_u32_le()?)
        } else {
            (0, 0)
        };

        let uv_offset = [stream.read_f32_le()?, stream.read_f32_le()?];
        let uv_scale = [stream.read_f32_le()?, stream.read_f32_le()?];

        // Source texture as sized string (NOT a texture set reference).
        let source_texture = stream.read_sized_string()?;

        // 4 bytes packed: texture_clamp_mode(u8), lighting_influence(u8),
        // env_map_min_lod(u8), unused(u8).
        let texture_clamp_mode = stream.read_u8()?;
        let _lighting_influence = stream.read_u8()?;
        let _env_map_min_lod = stream.read_u8()?;
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

        // Remaining fields (soft falloff depth, greyscale texture, env/normal/mask
        // textures for FO4+, etc.) are skipped — block size check adjusts stream.

        Ok(Self {
            net,
            shader_flags_1,
            shader_flags_2,
            uv_offset,
            uv_scale,
            source_texture,
            texture_clamp_mode,
            falloff_start_angle,
            falloff_stop_angle,
            falloff_start_opacity,
            falloff_stop_opacity,
            emissive_color,
            emissive_multiple,
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
}
