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
    /// Emissive multiplier. Present when user_version_2 >= 27 (FO3/FNV+).
    pub emissive_mult: f32,
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

        // Bethesda optimization (FO3/FNV+): ambient and diffuse are omitted.
        let bethesda_compact = stream.variant().compact_material();

        let ambient = if bethesda_compact {
            NiColor { r: 0.5, g: 0.5, b: 0.5 }
        } else {
            stream.read_ni_color()?
        };
        let diffuse = if bethesda_compact {
            NiColor { r: 0.5, g: 0.5, b: 0.5 }
        } else {
            stream.read_ni_color()?
        };

        let specular = stream.read_ni_color()?;
        let emissive = stream.read_ni_color()?;
        let shininess = stream.read_f32_le()?;
        let alpha = stream.read_f32_le()?;

        // Emissive multiplier — Bethesda extension (FO3/FNV+).
        let emissive_mult = if stream.variant().has_emissive_mult() {
            stream.read_f32_le()?
        } else {
            1.0
        };

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
            emissive_mult,
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
            strings: vec!["Material".to_string()],
            max_string_length: 8,
            num_groups: 0,
        }
    }

    fn write_color(buf: &mut Vec<u8>, r: f32, g: f32, b: f32) {
        buf.extend_from_slice(&r.to_le_bytes());
        buf.extend_from_slice(&g.to_le_bytes());
        buf.extend_from_slice(&b.to_le_bytes());
    }

    /// Build NiMaterialProperty bytes for the classic (Oblivion) format:
    /// ambient + diffuse + specular + emissive + shininess + alpha.
    fn build_material_oblivion() -> Vec<u8> {
        let mut data = Vec::new();
        // NiObjectNET: name, extra_data count=0, controller=-1
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // ambient (0.2, 0.2, 0.2)
        write_color(&mut data, 0.2, 0.2, 0.2);
        // diffuse (0.8, 0.6, 0.4)
        write_color(&mut data, 0.8, 0.6, 0.4);
        // specular
        write_color(&mut data, 1.0, 1.0, 1.0);
        // emissive
        write_color(&mut data, 0.0, 0.0, 0.0);
        // shininess
        data.extend_from_slice(&25.0f32.to_le_bytes());
        // alpha
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data
    }

    /// Build NiMaterialProperty bytes for the FNV format:
    /// no ambient/diffuse, + emissive_mult.
    fn build_material_fnv() -> Vec<u8> {
        let mut data = Vec::new();
        // NiObjectNET: name, extra_data count=0, controller=-1
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // NO ambient, NO diffuse (Bethesda optimization)
        // specular (0.5, 0.5, 0.5)
        write_color(&mut data, 0.5, 0.5, 0.5);
        // emissive (0.1, 0.0, 0.0)
        write_color(&mut data, 0.1, 0.0, 0.0);
        // shininess
        data.extend_from_slice(&10.0f32.to_le_bytes());
        // alpha
        data.extend_from_slice(&0.8f32.to_le_bytes());
        // emissive_mult
        data.extend_from_slice(&2.5f32.to_le_bytes());
        data
    }

    #[test]
    fn parse_material_oblivion_reads_ambient_diffuse() {
        // Regression: Oblivion (user_version < 11) reads all 4 colors.
        let header = make_header(0, 0);
        let data = build_material_oblivion();
        let mut stream = NifStream::new(&data, &header);

        let mat = NiMaterialProperty::parse(&mut stream).unwrap();
        assert!((mat.ambient.r - 0.2).abs() < 1e-6);
        assert!((mat.diffuse.r - 0.8).abs() < 1e-6);
        assert!((mat.diffuse.g - 0.6).abs() < 1e-6);
        assert!((mat.shininess - 25.0).abs() < 1e-6);
        assert!((mat.emissive_mult - 1.0).abs() < 1e-6); // default
    }

    #[test]
    fn parse_material_fnv_skips_ambient_diffuse() {
        // Regression: FNV (user_version=11, user_version_2=34) skips ambient/diffuse.
        let header = make_header(11, 34);
        let data = build_material_fnv();
        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);

        let mat = NiMaterialProperty::parse(&mut stream).unwrap();
        // Ambient and diffuse should be defaults (not read from stream)
        assert!((mat.ambient.r - 0.5).abs() < 1e-6);
        assert!((mat.diffuse.r - 0.5).abs() < 1e-6);
        // Specular should be read from stream
        assert!((mat.specular.r - 0.5).abs() < 1e-6);
        assert!((mat.emissive.r - 0.1).abs() < 1e-6);
        assert!((mat.shininess - 10.0).abs() < 1e-6);
        assert!((mat.alpha - 0.8).abs() < 1e-6);
        assert!((mat.emissive_mult - 2.5).abs() < 1e-6);
        // All bytes consumed
        assert_eq!(stream.position() as usize, expected_len);
    }

    #[test]
    fn parse_material_fo3_also_skips_ambient_diffuse() {
        // Fallout 3 (uv=11, uv2=34) uses the same compact format as FNV.
        let header = make_header(11, 34);
        let data = build_material_fnv();
        let mut stream = NifStream::new(&data, &header);

        let mat = NiMaterialProperty::parse(&mut stream).unwrap();
        // Ambient/diffuse are defaults (not read from stream)
        assert!((mat.ambient.r - 0.5).abs() < 1e-6);
        assert!((mat.diffuse.r - 0.5).abs() < 1e-6);
        assert!((mat.emissive_mult - 2.5).abs() < 1e-6);
    }
}
