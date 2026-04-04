//! NIF property blocks — control rendering state.
//!
//! Properties are attached to NiAVObject nodes and propagate down
//! the scene graph unless overridden.

use super::base::NiObjectNETData;
use super::NiObject;
use crate::stream::NifStream;
use crate::types::NiColor;
use std::any::Any;
use std::io;

/// Material properties (ambient, diffuse, specular, emissive colors).
#[derive(Debug)]
pub struct NiMaterialProperty {
    pub net: NiObjectNETData,
    pub ambient: NiColor,
    pub diffuse: NiColor,
    pub specular: NiColor,
    pub emissive: NiColor,
    pub shininess: f32,
    pub alpha: f32,
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
        let net = NiObjectNETData::parse(stream)?;

        let bethesda_compact = stream.variant().compact_material();

        let ambient = if bethesda_compact {
            NiColor {
                r: 0.5,
                g: 0.5,
                b: 0.5,
            }
        } else {
            stream.read_ni_color()?
        };
        let diffuse = if bethesda_compact {
            NiColor {
                r: 0.5,
                g: 0.5,
                b: 0.5,
            }
        } else {
            stream.read_ni_color()?
        };

        let specular = stream.read_ni_color()?;
        let emissive = stream.read_ni_color()?;
        let shininess = stream.read_f32_le()?;
        let alpha = stream.read_f32_le()?;

        let emissive_mult = if stream.variant().has_emissive_mult() {
            stream.read_f32_le()?
        } else {
            1.0
        };

        Ok(Self {
            net,
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
    pub net: NiObjectNETData,
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
        let net = NiObjectNETData::parse(stream)?;
        let flags = stream.read_u16_le()?;
        let threshold = stream.read_u8()?;
        Ok(Self {
            net,
            flags,
            threshold,
        })
    }
}

/// Texture mapping property — references NiSourceTexture blocks.
#[derive(Debug)]
pub struct NiTexturingProperty {
    pub net: NiObjectNETData,
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
    pub source_ref: crate::types::BlockRef,
    pub flags: u16,
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
        let net = NiObjectNETData::parse(stream)?;
        let flags = stream.read_u16_le()?;

        if stream.version() < crate::version::NifVersion::V20_2_0_7 {
            let _apply_mode = stream.read_u32_le()?;
        }

        let texture_count = stream.read_u32_le()?;

        let base_texture = Self::read_tex_desc(stream)?;
        let dark_texture = if texture_count > 1 {
            Self::read_tex_desc(stream)?
        } else {
            None
        };
        let detail_texture = if texture_count > 2 {
            Self::read_tex_desc(stream)?
        } else {
            None
        };
        let gloss_texture = if texture_count > 3 {
            Self::read_tex_desc(stream)?
        } else {
            None
        };
        let glow_texture = if texture_count > 4 {
            Self::read_tex_desc(stream)?
        } else {
            None
        };
        let bump_texture = if texture_count > 5 {
            Self::read_tex_desc(stream)?
        } else {
            None
        };
        // nif.xml: bump texture has 3 extra fields after TexDesc.
        if bump_texture.is_some() {
            let _luma_scale = stream.read_f32_le()?;
            let _luma_offset = stream.read_f32_le()?;
            // Bump Map Matrix: 2x2 floats (Matrix22)
            let _m00 = stream.read_f32_le()?;
            let _m01 = stream.read_f32_le()?;
            let _m10 = stream.read_f32_le()?;
            let _m11 = stream.read_f32_le()?;
        }
        let normal_texture = if texture_count > 6 {
            Self::read_tex_desc(stream)?
        } else {
            None
        };

        if texture_count > 7 {
            // Parallax texture (slot 7).
            let parallax = Self::read_tex_desc(stream)?;
            // nif.xml: Parallax Offset float after parallax TexDesc.
            if parallax.is_some() {
                let _parallax_offset = stream.read_f32_le()?;
            }
        }
        // Decal texture slots. For v20.2.0.5+ (FNV), nif.xml gates each decal
        // at count > 8, > 9, > 10, > 11. In practice, Bethesda serializes
        // (texture_count - 7) decal has-booleans (with TexDesc if true).
        if stream.version() >= crate::version::NifVersion(0x14020005) {
            // v20.2.0.5+: decals start after slot 7 (parallax)
            let num_decals = texture_count.saturating_sub(7);
            for _ in 0..num_decals {
                let _ = Self::read_tex_desc(stream)?;
            }
        } else {
            // Pre-20.2.0.5: decals start after slot 6 (normal)
            let num_decals = texture_count.saturating_sub(6);
            for _ in 0..num_decals {
                let _ = Self::read_tex_desc(stream)?;
            }
        }

        if stream.version() >= crate::version::NifVersion(0x0A000100) {
            let num_shader_textures = stream.read_u32_le()?;
            for _ in 0..num_shader_textures {
                let has = stream.read_byte_bool()?;
                if has {
                    let _source_ref = stream.read_block_ref()?;
                    if stream.version() >= crate::version::NifVersion(0x14010003) {
                        let _flags = stream.read_u16_le()?;
                    } else {
                        let _clamp = stream.read_u32_le()?;
                        let _filter = stream.read_u32_le()?;
                        let _uv_set = stream.read_u32_le()?;
                    }
                    let _map_id = stream.read_u32_le()?;
                }
            }
        }

        Ok(Self {
            net,
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
        let has = stream.read_byte_bool()?;
        if !has {
            return Ok(None);
        }
        let source_ref = stream.read_block_ref()?;

        if stream.version() >= crate::version::NifVersion(0x14010003) {
            let flags = stream.read_u16_le()?;
            // nif.xml: Has Texture Transform (bool) since 10.1.0.0, NO until — present in ALL modern versions.
            if stream.version() >= crate::version::NifVersion(0x0A010000) {
                let has_transform = stream.read_byte_bool()?;
                if has_transform {
                    // Translation (2 floats) + Scale (2 floats) + Rotation (1 float)
                    // + Transform Method (u32) + Center (2 floats) = 32 bytes
                    stream.skip(4 * 2 + 4 * 2 + 4 + 4 + 4 * 2);
                }
            }
            Ok(Some(TexDesc { source_ref, flags }))
        } else {
            let clamp_mode = stream.read_u32_le()?;
            let filter_mode = stream.read_u32_le()?;
            let uv_set = stream.read_u32_le()?;

            if stream.version() <= crate::version::NifVersion(0x0A040001) {
                let _ps2_l = stream.read_u16_le()?;
                let _ps2_k = stream.read_u16_le()?;
            }

            if stream.version() >= crate::version::NifVersion(0x0A010000) {
                let has_transform = stream.read_byte_bool()?;
                if has_transform {
                    stream.skip(4 * 5 + 4 + 4 * 2);
                }
            }

            let flags = ((clamp_mode & 0xF) as u16)
                | (((filter_mode & 0xF) as u16) << 4)
                | (((uv_set & 0xF) as u16) << 8);
            Ok(Some(TexDesc { source_ref, flags }))
        }
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

    fn build_material_oblivion() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        write_color(&mut data, 0.2, 0.2, 0.2);
        write_color(&mut data, 0.8, 0.6, 0.4);
        write_color(&mut data, 1.0, 1.0, 1.0);
        write_color(&mut data, 0.0, 0.0, 0.0);
        data.extend_from_slice(&25.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data
    }

    fn build_material_fnv() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        write_color(&mut data, 0.5, 0.5, 0.5);
        write_color(&mut data, 0.1, 0.0, 0.0);
        data.extend_from_slice(&10.0f32.to_le_bytes());
        data.extend_from_slice(&0.8f32.to_le_bytes());
        data.extend_from_slice(&2.5f32.to_le_bytes());
        data
    }

    #[test]
    fn parse_material_oblivion_reads_ambient_diffuse() {
        let header = make_header(0, 0);
        let data = build_material_oblivion();
        let mut stream = NifStream::new(&data, &header);
        let mat = NiMaterialProperty::parse(&mut stream).unwrap();
        assert!((mat.ambient.r - 0.2).abs() < 1e-6);
        assert!((mat.diffuse.r - 0.8).abs() < 1e-6);
        assert!((mat.diffuse.g - 0.6).abs() < 1e-6);
        assert!((mat.shininess - 25.0).abs() < 1e-6);
        assert!((mat.emissive_mult - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parse_material_fnv_skips_ambient_diffuse() {
        let header = make_header(11, 34);
        let data = build_material_fnv();
        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let mat = NiMaterialProperty::parse(&mut stream).unwrap();
        assert!((mat.ambient.r - 0.5).abs() < 1e-6);
        assert!((mat.diffuse.r - 0.5).abs() < 1e-6);
        assert!((mat.specular.r - 0.5).abs() < 1e-6);
        assert!((mat.emissive.r - 0.1).abs() < 1e-6);
        assert!((mat.shininess - 10.0).abs() < 1e-6);
        assert!((mat.alpha - 0.8).abs() < 1e-6);
        assert!((mat.emissive_mult - 2.5).abs() < 1e-6);
        assert_eq!(stream.position() as usize, expected_len);
    }

    #[test]
    fn parse_material_fo3_also_skips_ambient_diffuse() {
        let header = make_header(11, 34);
        let data = build_material_fnv();
        let mut stream = NifStream::new(&data, &header);
        let mat = NiMaterialProperty::parse(&mut stream).unwrap();
        assert!((mat.ambient.r - 0.5).abs() < 1e-6);
        assert!((mat.diffuse.r - 0.5).abs() < 1e-6);
        assert!((mat.emissive_mult - 2.5).abs() < 1e-6);
    }
}
