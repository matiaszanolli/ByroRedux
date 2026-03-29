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

        // Emissive color (RGBA) — Bethesda extension for FNV+ (user_version_2 >= 34).
        let emissive_color = if stream.user_version_2() >= 34 {
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
