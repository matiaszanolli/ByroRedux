//! Bethesda shader property blocks — BSShaderPPLightingProperty, BSShaderTextureSet.
//!
//! These are Fallout 3 / New Vegas shader properties. They replace
//! NiMaterialProperty + NiTexturingProperty for Bethesda's rendering pipeline.

use crate::stream::NifStream;
use crate::types::BlockRef;
use super::NiObject;
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
    pub name: Option<String>,
    pub extra_data_refs: Vec<BlockRef>,
    pub controller_ref: BlockRef,
    pub shader_flags: u16,
    pub shader_type: u32,
    pub shader_flags_1: u32,
    pub shader_flags_2: u32,
    pub env_map_scale: f32,
    pub texture_clamp_mode: u32,
    pub texture_set_ref: BlockRef,
    /// Emissive color (RGBA). Present when user_version_2 >= 34 (FNV+).
    pub emissive_color: [f32; 4],
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
        // NiObjectNET base
        let name = stream.read_string()?;
        let extra_data_refs = stream.read_block_ref_list()?;
        let controller_ref = stream.read_block_ref()?;

        // NiProperty::LoadBinary reads nothing.

        // BSShaderProperty fields:
        let shader_flags = stream.read_u16_le()?;
        let shader_type = stream.read_u32_le()?;
        let shader_flags_1 = stream.read_u32_le()?;
        let shader_flags_2 = stream.read_u32_le()?;
        let env_map_scale = stream.read_f32_le()?;

        // BSShaderLightingProperty: texture clamp mode
        let texture_clamp_mode = stream.read_u32_le()?;

        // BSShaderPPLightingProperty: texture set reference
        let texture_set_ref = stream.read_block_ref()?;

        // Emissive color (RGBA) — Bethesda extension for FNV+.
        let emissive_color = if stream.variant().has_shader_emissive_color() {
            [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ]
        } else {
            [0.0, 0.0, 0.0, 1.0]
        };

        Ok(Self {
            name,
            extra_data_refs,
            controller_ref,
            shader_flags,
            shader_type,
            shader_flags_1,
            shader_flags_2,
            env_map_scale,
            texture_clamp_mode,
            texture_set_ref,
            emissive_color,
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
        // emissive_color (4×f32) — only if user_version_2 >= 34
        if user_version_2 >= 34 {
            data.extend_from_slice(&0.1f32.to_le_bytes());
            data.extend_from_slice(&0.2f32.to_le_bytes());
            data.extend_from_slice(&0.3f32.to_le_bytes());
            data.extend_from_slice(&0.9f32.to_le_bytes());
        }
        data
    }

    #[test]
    fn parse_bsshader_fnv_reads_emissive_color() {
        // Regression: user_version_2 >= 34 (FNV) must read 4 extra floats for emissive color.
        let header = make_header(11, 34);
        let data = build_bsshader_bytes(34);
        let mut stream = NifStream::new(&data, &header);

        let prop = BSShaderPPLightingProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.texture_set_ref.index(), Some(5));
        assert!((prop.emissive_color[0] - 0.1).abs() < 1e-6);
        assert!((prop.emissive_color[1] - 0.2).abs() < 1e-6);
        assert!((prop.emissive_color[2] - 0.3).abs() < 1e-6);
        assert!((prop.emissive_color[3] - 0.9).abs() < 1e-6);
        // All data consumed: 38 base + 16 emissive = 54 bytes
        assert_eq!(stream.position(), 54);
    }

    #[test]
    fn parse_bsshader_oblivion_no_emissive_color() {
        // Regression: Oblivion (user_version=0) must NOT read emissive color.
        let header = make_header(0, 0);
        let data = build_bsshader_bytes(0);
        let mut stream = NifStream::new(&data, &header);

        let prop = BSShaderPPLightingProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.texture_set_ref.index(), Some(5));
        // Default emissive color
        assert_eq!(prop.emissive_color, [0.0, 0.0, 0.0, 1.0]);
        // Only 38 bytes consumed (no emissive)
        assert_eq!(stream.position(), 38);
    }
}
