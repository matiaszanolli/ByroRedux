//! NIF property blocks — control rendering state.
//!
//! Properties are attached to NiAVObject nodes and propagate down
//! the scene graph unless overridden.

use crate::stream::NifStream;
use crate::types::{BlockRef, NiColor};
use super::NiObject;
use std::any::Any;
use std::io;

/// Material properties (ambient, diffuse, specular, emissive colors).
#[derive(Debug)]
pub struct NiMaterialProperty {
    pub name: Option<String>,
    pub extra_data_refs: Vec<BlockRef>,
    pub controller_ref: BlockRef,
    pub ambient: NiColor,
    pub diffuse: NiColor,
    pub specular: NiColor,
    pub emissive: NiColor,
    pub shininess: f32,
    pub alpha: f32,
}

impl NiObject for NiMaterialProperty {
    fn block_type_name(&self) -> &'static str {
        "NiMaterialProperty"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiMaterialProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiObjectNET
        let name = stream.read_string()?;
        let extra_data_refs = stream.read_block_ref_list()?;
        let controller_ref = stream.read_block_ref()?;

        // NiProperty::LoadBinary reads NOTHING — pure pass-through to NiObjectNET.

        // NiMaterialProperty
        let ambient = stream.read_ni_color()?;
        let diffuse = stream.read_ni_color()?;
        let specular = stream.read_ni_color()?;
        let emissive = stream.read_ni_color()?;
        let shininess = stream.read_f32_le()?;
        let alpha = stream.read_f32_le()?;

        Ok(Self {
            name,
            extra_data_refs,
            controller_ref,
            ambient,
            diffuse,
            specular,
            emissive,
            shininess,
            alpha,
        })
    }
}

/// Alpha blending property.
#[derive(Debug)]
pub struct NiAlphaProperty {
    pub name: Option<String>,
    pub extra_data_refs: Vec<BlockRef>,
    pub controller_ref: BlockRef,
    pub flags: u16,
    pub threshold: u8,
}

impl NiObject for NiAlphaProperty {
    fn block_type_name(&self) -> &'static str {
        "NiAlphaProperty"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiAlphaProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let name = stream.read_string()?;
        let extra_data_refs = stream.read_block_ref_list()?;
        let controller_ref = stream.read_block_ref()?;
        let flags = stream.read_u16_le()?;
        let threshold = stream.read_u8()?;

        Ok(Self { name, extra_data_refs, controller_ref, flags, threshold })
    }
}

/// Texture mapping property — references NiSourceTexture blocks.
#[derive(Debug)]
pub struct NiTexturingProperty {
    pub name: Option<String>,
    pub extra_data_refs: Vec<BlockRef>,
    pub controller_ref: BlockRef,
    pub flags: u16,
    pub texture_count: u32,
    pub base_texture: Option<TexDesc>,
    pub dark_texture: Option<TexDesc>,
    pub detail_texture: Option<TexDesc>,
    pub gloss_texture: Option<TexDesc>,
    pub glow_texture: Option<TexDesc>,
    pub bump_texture: Option<TexDesc>,
    pub normal_texture: Option<TexDesc>,
}

/// Description of a single texture slot.
#[derive(Debug)]
pub struct TexDesc {
    pub source_ref: BlockRef,
    pub clamp_mode: u32,
    pub filter_mode: u32,
    pub uv_set: u32,
}

impl NiObject for NiTexturingProperty {
    fn block_type_name(&self) -> &'static str {
        "NiTexturingProperty"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiTexturingProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let name = stream.read_string()?;
        let extra_data_refs = stream.read_block_ref_list()?;
        let controller_ref = stream.read_block_ref()?;
        let flags = stream.read_u16_le()?;

        // Apply mode (present in older versions)
        if stream.version() < crate::version::NifVersion::V20_2_0_7 {
            let _apply_mode = stream.read_u32_le()?;
        }

        let texture_count = stream.read_u32_le()?;

        let base_texture = Self::read_tex_desc(stream)?;
        let dark_texture = if texture_count > 1 { Self::read_tex_desc(stream)? } else { None };
        let detail_texture = if texture_count > 2 { Self::read_tex_desc(stream)? } else { None };
        let gloss_texture = if texture_count > 3 { Self::read_tex_desc(stream)? } else { None };
        let glow_texture = if texture_count > 4 { Self::read_tex_desc(stream)? } else { None };
        let bump_texture = if texture_count > 5 { Self::read_tex_desc(stream)? } else { None };
        let normal_texture = if texture_count > 6 { Self::read_tex_desc(stream)? } else { None };

        // Skip remaining texture slots
        for _ in 7..texture_count {
            let _ = Self::read_tex_desc(stream)?;
        }

        Ok(Self {
            name,
            extra_data_refs,
            controller_ref,
            flags,
            texture_count,
            base_texture,
            dark_texture,
            detail_texture,
            gloss_texture,
            glow_texture,
            bump_texture,
            normal_texture,
        })
    }

    fn read_tex_desc(stream: &mut NifStream) -> io::Result<Option<TexDesc>> {
        let has = stream.read_bool()?;
        if !has {
            return Ok(None);
        }
        let source_ref = stream.read_block_ref()?;
        let clamp_mode = stream.read_u32_le()?;
        let filter_mode = stream.read_u32_le()?;
        let uv_set = stream.read_u32_le()?;

        // PS2-specific fields in older versions — skip
        if stream.version() <= crate::version::NifVersion(0x0A040001) {
            let _ps2_l = stream.read_u16_le()?;
            let _ps2_k = stream.read_u16_le()?;
        }

        // Has texture transform (version >= 10.1.0.0)
        if stream.version() >= crate::version::NifVersion(0x0A010000) {
            let has_transform = stream.read_bool()?;
            if has_transform {
                // Translation (2 floats), tiling (2 floats), w rotation (1 float),
                // transform type (u32), center offset (2 floats)
                stream.skip(4 * 5 + 4 + 4 * 2);
            }
        }

        Ok(Some(TexDesc { source_ref, clamp_mode, filter_mode, uv_set }))
    }
}
