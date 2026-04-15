//! NIF property blocks — control rendering state.
//!
//! Properties are attached to NiAVObject nodes and propagate down
//! the scene graph unless overridden.

use super::base::NiObjectNETData;
use super::NiObject;
use crate::stream::NifStream;
use crate::types::NiColor;
use crate::version::NifVersion;
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
        // NiMaterialProperty flags: since 3.0, until 10.0.1.2 (NOT present in Oblivion+).
        if stream.version() <= NifVersion(0x0A000102) {
            let _flags = stream.read_u16_le()?;
        }

        // nif.xml line 4366-4367:
        //   <field name="Ambient Color" vercond="#BSVER# #LT# 26">
        //   <field name="Diffuse Color" vercond="#BSVER# #LT# 26">
        // The gate is a raw header-BSVER comparison, not a variant check.
        // `variant().compact_material()` included Fallout3 (bsver=21), which
        // is wrong: 21 < 26, so FO3 files *do* carry ambient/diffuse. Some
        // FNV archives also ship FO3-era content with in-file BSVER 21-25
        // that would be misclassified as Fallout3 and hardcoded to 21 — but
        // either way, using stream.bsver() reflects the file's real layout.
        // See #323.
        let bethesda_compact = stream.bsver() >= 26;

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

        // nif.xml line 4372: `<field name="Emissive Mult" vercond="#BSVER# #GT# 21" />`.
        // Strict >, so FO3 at BSVER=21 is excluded. Use raw file bsver to
        // honor in-file variation for FO3-era content shipped in FNV. #323.
        let emissive_mult = if stream.bsver() > 21 {
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
    /// Optional per-slot UV transform. Populated when the NIF sets
    /// `Has Texture Transform = true` on the slot; `None` when the slot
    /// uses the identity transform. Only the base-texture slot is
    /// currently consumed downstream (see #219).
    pub transform: Option<TexTransform>,
}

/// Per-slot UV transform decoded from a `TexDesc`.
///
/// Corresponds to nif.xml `TexDesc` sub-fields `Translation`, `Scale`,
/// `Rotation`, `Transform Method` and `Center`. Values are in UV space
/// (Translation/Scale/Center are `TexCoord` = 2 × f32; Rotation is a
/// single float in radians).
#[derive(Debug, Clone, Copy)]
pub struct TexTransform {
    /// UV offset applied after the other transforms.
    pub translation: [f32; 2],
    /// UV scale applied around `center`.
    pub scale: [f32; 2],
    /// UV rotation (radians) applied around `center`.
    pub rotation: f32,
    /// Transform order flag from `TexturingTransformMethod` enum.
    /// Preserved for downstream consumers that care about the order
    /// (maya vs max vs milkshape conventions); most engines fold it
    /// into the shader.
    pub transform_method: u32,
    /// Pivot point in UV space for rotation + scale.
    pub center: [f32; 2],
}

impl TexTransform {
    /// Identity transform (no offset, unit scale, no rotation).
    pub const IDENTITY: Self = Self {
        translation: [0.0, 0.0],
        scale: [1.0, 1.0],
        rotation: 0.0,
        transform_method: 0,
        center: [0.0, 0.0],
    };
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

        // Flags: ushort until 10.0.1.2, TexturingFlags since 20.1.0.2.
        // Gap: versions 10.0.1.3 through 20.1.0.1 have NO flags field.
        let flags = if stream.version() <= NifVersion(0x0A000102)
            || stream.version() >= NifVersion(0x14010002)
        {
            stream.read_u16_le()?
        } else {
            0
        };

        // Apply Mode: since 3.3.0.13, until 20.1.0.1.
        if stream.version() <= NifVersion(0x14010001) {
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
        // Decal texture slots. nif.xml gates each decal at count > 8, > 9, > 10, > 11.
        // Slots 0-7 (base through parallax) are already consumed above, so decals
        // start at slot 8: num_decals = texture_count - 8.
        if stream.version() >= crate::version::NifVersion(0x14020005) {
            // v20.2.0.5+: slots 0-7 consumed (base..parallax)
            let num_decals = texture_count.saturating_sub(8);
            for _ in 0..num_decals {
                let _ = Self::read_tex_desc(stream)?;
            }
        } else {
            // Pre-20.2.0.5: slots 0-6 consumed (base..normal), no parallax slot
            let num_decals = texture_count.saturating_sub(7);
            for _ in 0..num_decals {
                let _ = Self::read_tex_desc(stream)?;
            }
        }

        // Shader textures trailer: the authoritative Gamebryo 2.3
        // `NiTexturingProperty::LoadBinary` reads a `uint` count
        // UNCONDITIONALLY (no leading bool gate), then loops over the
        // shader maps. For every entry the loop reads `bool has_map`
        // + optional Map body.
        //
        // This contradicts nif.xml which claims a leading
        // `Has Shader Textures: bool` gate, but the real on-disk
        // data — verified against an Oblivion Quarto03.NIF trace —
        // matches the Gamebryo 2.3 source: the shader-map count is
        // the first 4 bytes of this section. A previous fix (#149)
        // followed nif.xml and read a bool instead, consuming the
        // first byte of the u32 count and leaving the parser 3
        // bytes short on every NiTexturingProperty — which in turn
        // misaligned every following block on Oblivion cell loads
        // (including NiSourceTexture → "failed to fill whole buffer"
        // with huge consumed counts).
        //
        // The version gate (>= 10.0.1.0) matches the historical
        // gate — pre-10.0.1.0 files don't carry the shader map list
        // at all.
        if stream.version() >= crate::version::NifVersion(0x0A000100) {
            let num_shader_textures = stream.read_u32_le()?;
            for _ in 0..num_shader_textures {
                let has = stream.read_byte_bool()?;
                if has {
                    // Each shader Map shares the base Map::LoadBinary
                    // layout: source_ref, clamp/filter/uv or flags,
                    // optional texture transform. The trailing
                    // `map_id` is an extra u32 per shader map that
                    // Gamebryo stores but the importer doesn't need.
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
            // nif.xml: Has Texture Transform (bool) since 10.1.0.0,
            // present in every modern file. We read the 32-byte TexDesc
            // transform body when the bool is set and store it on the
            // returned TexDesc; the old parser skipped it, which caused
            // #219 (per-slot UV transforms lost).
            let transform = if stream.version() >= crate::version::NifVersion(0x0A010000) {
                let has_transform = stream.read_byte_bool()?;
                if has_transform {
                    Some(Self::read_tex_transform(stream)?)
                } else {
                    None
                }
            } else {
                None
            };
            Ok(Some(TexDesc {
                source_ref,
                flags,
                transform,
            }))
        } else {
            let clamp_mode = stream.read_u32_le()?;
            let filter_mode = stream.read_u32_le()?;
            let uv_set = stream.read_u32_le()?;

            if stream.version() <= crate::version::NifVersion(0x0A040001) {
                let _ps2_l = stream.read_u16_le()?;
                let _ps2_k = stream.read_u16_le()?;
            }

            let transform = if stream.version() >= crate::version::NifVersion(0x0A010000) {
                let has_transform = stream.read_byte_bool()?;
                if has_transform {
                    Some(Self::read_tex_transform(stream)?)
                } else {
                    None
                }
            } else {
                None
            };

            let flags = ((clamp_mode & 0xF) as u16)
                | (((filter_mode & 0xF) as u16) << 4)
                | (((uv_set & 0xF) as u16) << 8);
            Ok(Some(TexDesc {
                source_ref,
                flags,
                transform,
            }))
        }
    }

    /// Read a 32-byte `TexTransform`: Translation (2 f32) + Scale (2 f32)
    /// + Rotation (f32) + Transform Method (u32) + Center (2 f32).
    fn read_tex_transform(stream: &mut NifStream) -> io::Result<TexTransform> {
        let tx = stream.read_f32_le()?;
        let ty = stream.read_f32_le()?;
        let sx = stream.read_f32_le()?;
        let sy = stream.read_f32_le()?;
        let rotation = stream.read_f32_le()?;
        let transform_method = stream.read_u32_le()?;
        let cx = stream.read_f32_le()?;
        let cy = stream.read_f32_le()?;
        Ok(TexTransform {
            translation: [tx, ty],
            scale: [sx, sy],
            rotation,
            transform_method,
            center: [cx, cy],
        })
    }
}

#[cfg(test)]
mod tests {
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
            strings: vec![Arc::from("Material")],
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
        // No NiProperty flags — until 10.0.1.2, tests use v20.2.0.7
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
        // No NiProperty flags — until 10.0.1.2, FNV is v20.2.0.7
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

    fn build_flag_property_bytes() -> Vec<u8> {
        let mut data = Vec::new();
        // NiObjectNET: name (string table index 0)
        data.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs: count=0
        data.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref: -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // flags: u16 (bit 0 = enabled)
        data.extend_from_slice(&1u16.to_le_bytes());
        data
    }

    #[test]
    fn parse_flag_property_specular() {
        let header = make_header(11, 34);
        let data = build_flag_property_bytes();
        let mut stream = NifStream::new(&data, &header);
        let prop = NiFlagProperty::parse(&mut stream, "NiSpecularProperty").unwrap();
        assert_eq!(prop.block_type_name(), "NiSpecularProperty");
        assert!(prop.enabled());
        assert_eq!(prop.flags, 1);
        assert_eq!(stream.position() as usize, data.len());
    }

    #[test]
    fn parse_flag_property_wireframe_disabled() {
        let header = make_header(11, 34);
        let mut data = Vec::new();
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes()); // bit 0 = 0 → disabled
        let mut stream = NifStream::new(&data, &header);
        let prop = NiFlagProperty::parse(&mut stream, "NiWireframeProperty").unwrap();
        assert!(!prop.enabled());
    }

    #[test]
    fn parse_string_palette() {
        let header = make_header(11, 34);
        let mut data = Vec::new();
        let palette_str = "Bip01\0Bip01 Head\0Bip01 L Hand\0";
        data.extend_from_slice(&(palette_str.len() as u32).to_le_bytes());
        data.extend_from_slice(palette_str.as_bytes());
        data.extend_from_slice(&(palette_str.len() as u32).to_le_bytes()); // redundant length
        let mut stream = NifStream::new(&data, &header);
        let pal = NiStringPalette::parse(&mut stream).unwrap();
        assert_eq!(pal.get_string(0), Some("Bip01"));
        assert_eq!(pal.get_string(6), Some("Bip01 Head"));
        assert_eq!(pal.get_string(17), Some("Bip01 L Hand"));
        assert_eq!(pal.get_string(999), None);
        assert_eq!(stream.position() as usize, data.len());
    }

    /// Regression test for issue #149 / runtime Oblivion trace:
    /// NiTexturingProperty's shader-map-list tail is a `u32 count`
    /// read unconditionally (no leading bool gate), per the
    /// Gamebryo 2.3 source. An earlier fix (#149) followed nif.xml
    /// and added a leading `has_shader_textures: bool` which
    /// consumed the first byte of the u32 count, leaving the
    /// parser 3 bytes short and misaligning every subsequent
    /// block on Oblivion cell loads. Verify the empty-shader-list
    /// case (count = 0) consumes exactly 4 bytes.
    #[test]
    fn parse_ni_texturing_property_with_zero_shader_maps() {
        let header = make_header(12, 83); // Skyrim LE — v20.2.0.7 path
        let mut data = Vec::new();
        // NiObjectNET: name string index, extra_data count, controller
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // flags u16 (v >= 20.1.0.2 path); no apply_mode at v20.2.0.7
        data.extend_from_slice(&0u16.to_le_bytes());
        // texture_count = 1 → only base_texture is read.
        data.extend_from_slice(&1u32.to_le_bytes());
        // base_texture TexDesc: has_texture = 0 → TexDesc skipped.
        data.push(0);
        // num_decals = texture_count.saturating_sub(8) = 0 → no loop.
        // num_shader_textures = 0 as u32 (4 bytes).
        data.extend_from_slice(&0u32.to_le_bytes());

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = NiTexturingProperty::parse(&mut stream)
            .expect("NiTexturingProperty with zero shader maps should parse");
        assert_eq!(prop.texture_count, 1);
        assert!(prop.base_texture.is_none());
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "NiTexturingProperty consumed {} bytes, expected exactly {}",
            stream.position(),
            expected_len
        );
    }

    /// Regression test: `num_shader_textures = 1` + one shader map
    /// with `has = 0` (no body) must parse to exactly `4 (count) +
    /// 1 (has)` = 5 trailing bytes. Exercises the loop logic without
    /// requiring a full shader Map body.
    #[test]
    fn parse_ni_texturing_property_with_empty_shader_map_entry() {
        let header = make_header(12, 83);
        let mut data = Vec::new();
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes()); // flags
        data.extend_from_slice(&1u32.to_le_bytes()); // texture_count
        data.push(0); // base_texture has=0
        data.extend_from_slice(&1u32.to_le_bytes()); // num_shader_textures = 1
        data.push(0); // shader map has = 0

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let _prop = NiTexturingProperty::parse(&mut stream).unwrap();
        assert_eq!(stream.position() as usize, expected_len);
    }

    /// Regression test for issue #219: the TexDesc's per-slot UV transform
    /// must be captured (previously the 32 transform bytes were skipped
    /// and the values discarded). Builds a minimal NiTexturingProperty at
    /// v20.2.0.7 with a base_texture that has `Has Texture Transform = 1`
    /// and verifies that `prop.base_texture.transform` carries the exact
    /// values — and that the stream position matches the payload size.
    #[test]
    fn parse_ni_texturing_property_captures_base_uv_transform() {
        let header = make_header(12, 83); // Skyrim LE — v20.2.0.7, >= 20.1.0.3 flags path
        let mut data = Vec::new();

        // NiObjectNET base.
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());

        // NiProperty flags (u16), texture_count = 1.
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());

        // base_texture TexDesc (has = 1, source_ref = 5, flags = 0x0302,
        // has_transform = 1, then the 32-byte body).
        data.push(1); // has
        data.extend_from_slice(&5i32.to_le_bytes()); // source_ref
        data.extend_from_slice(&0x0302u16.to_le_bytes()); // flags
        data.push(1); // has_transform
                      // Translation (u, v)
        data.extend_from_slice(&0.25f32.to_le_bytes());
        data.extend_from_slice(&(-0.5f32).to_le_bytes());
        // Scale (su, sv)
        data.extend_from_slice(&2.0f32.to_le_bytes());
        data.extend_from_slice(&3.0f32.to_le_bytes());
        // Rotation
        data.extend_from_slice(&0.75f32.to_le_bytes());
        // Transform method (u32 enum)
        data.extend_from_slice(&2u32.to_le_bytes());
        // Center (cu, cv)
        data.extend_from_slice(&0.1f32.to_le_bytes());
        data.extend_from_slice(&0.2f32.to_le_bytes());

        // No decals, no shader map list.
        data.extend_from_slice(&0u32.to_le_bytes());

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = NiTexturingProperty::parse(&mut stream).unwrap();
        assert_eq!(stream.position() as usize, expected_len);

        let base = prop
            .base_texture
            .as_ref()
            .expect("base_texture present (has=1)");
        assert_eq!(base.source_ref.0, 5);
        assert_eq!(base.flags, 0x0302);
        let tx = base
            .transform
            .expect("transform captured (has_transform=1)");
        assert!((tx.translation[0] - 0.25).abs() < 1e-6);
        assert!((tx.translation[1] + 0.5).abs() < 1e-6);
        assert!((tx.scale[0] - 2.0).abs() < 1e-6);
        assert!((tx.scale[1] - 3.0).abs() < 1e-6);
        assert!((tx.rotation - 0.75).abs() < 1e-6);
        assert_eq!(tx.transform_method, 2);
        assert!((tx.center[0] - 0.1).abs() < 1e-6);
        assert!((tx.center[1] - 0.2).abs() < 1e-6);
    }

    /// Parse the same layout with `has_transform = 0` and confirm the
    /// parser leaves `transform = None` instead of inventing identity.
    #[test]
    fn parse_ni_texturing_property_transform_absent() {
        let header = make_header(12, 83);
        let mut data = Vec::new();
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());

        data.push(1); // base_texture has = 1
        data.extend_from_slice(&7i32.to_le_bytes()); // source_ref
        data.extend_from_slice(&0u16.to_le_bytes()); // flags
        data.push(0); // has_transform = 0 (no body bytes)

        data.extend_from_slice(&0u32.to_le_bytes()); // num_shader_textures

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = NiTexturingProperty::parse(&mut stream).unwrap();
        assert_eq!(stream.position() as usize, expected_len);
        let base = prop.base_texture.as_ref().unwrap();
        assert_eq!(base.source_ref.0, 7);
        assert!(base.transform.is_none());
    }
}

// ── Simple flag-only properties (Oblivion) ──────────────────────────

/// Generic flag-only NiProperty subclass.
///
/// NiSpecularProperty, NiWireframeProperty, NiDitherProperty, NiShadeProperty
/// all have identical binary layout: NiObjectNET + flags(u16).
/// This struct is shared; `block_type_name` is set at construction time.
#[derive(Debug)]
pub struct NiFlagProperty {
    pub net: NiObjectNETData,
    pub flags: u16,
    type_name: &'static str,
}

impl NiObject for NiFlagProperty {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiFlagProperty {
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let flags = stream.read_u16_le()?;
        Ok(Self {
            net,
            flags,
            type_name,
        })
    }

    /// Bit 0 of flags — the universal enable/disable toggle for flag properties.
    pub fn enabled(&self) -> bool {
        self.flags & 1 != 0
    }
}

// ── NiStringPalette ─────────────────────────────────────────────────

/// String palette used by Oblivion .kf animation files.
///
/// Contains a single null-separated string buffer that NiControllerSequence
/// ControlledBlock entries index into via byte offsets.
#[derive(Debug)]
pub struct NiStringPalette {
    pub palette: String,
}

impl NiObject for NiStringPalette {
    fn block_type_name(&self) -> &'static str {
        "NiStringPalette"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiStringPalette {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let palette = stream.read_sized_string()?;
        let _length = stream.read_u32_le()?; // redundant length field
        Ok(Self { palette })
    }

    /// Look up a string by byte offset into the palette.
    pub fn get_string(&self, offset: u32) -> Option<&str> {
        let start = offset as usize;
        if start >= self.palette.len() {
            return None;
        }
        let end = self.palette[start..]
            .find('\0')
            .map(|i| start + i)
            .unwrap_or(self.palette.len());
        Some(&self.palette[start..end])
    }
}

// ── NiVertexColorProperty ────────────────────────────────────────────

/// Controls how vertex colors interact with material/lighting.
///
/// `vertex_mode`: 0 = SOURCE_IGNORE, 1 = SOURCE_EMISSIVE, 2 = SOURCE_AMB_DIFF (default)
/// `lighting_mode`: 0 = LIGHTING_E, 1 = LIGHTING_E_A_D (default)
#[derive(Debug)]
pub struct NiVertexColorProperty {
    pub net: NiObjectNETData,
    pub flags: u16,
    pub vertex_mode: u32,
    pub lighting_mode: u32,
}

impl NiObject for NiVertexColorProperty {
    fn block_type_name(&self) -> &'static str {
        "NiVertexColorProperty"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiVertexColorProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let flags = stream.read_u16_le()?;

        let (vertex_mode, lighting_mode) = if stream.version() <= NifVersion::V20_0_0_5 {
            (stream.read_u32_le()?, stream.read_u32_le()?)
        } else {
            // FO3+: packed in flags. bits 4-5 = vertex_mode, bits 3 = lighting_mode.
            let vm = ((flags >> 4) & 0x3) as u32;
            let lm = ((flags >> 3) & 0x1) as u32;
            (vm, lm)
        };

        Ok(Self {
            net,
            flags,
            vertex_mode,
            lighting_mode,
        })
    }
}

// ── NiStencilProperty ────────────────────────────────────────────────

/// Controls stencil testing and face culling (two-sided rendering).
///
/// Version-aware: Oblivion uses expanded fields, FO3+ packs into flags.
/// The key field for rendering is `draw_mode`:
///   0 = CCW_OR_BOTH (application default, treated as BOTH)
///   1 = CCW (standard backface cull)
///   2 = CW
///   3 = BOTH (double-sided)
#[derive(Debug)]
pub struct NiStencilProperty {
    pub net: NiObjectNETData,
    pub flags: u16,
    pub stencil_enabled: bool,
    pub stencil_function: u32,
    pub stencil_ref: u32,
    pub stencil_mask: u32,
    pub fail_action: u32,
    pub z_fail_action: u32,
    pub pass_action: u32,
    pub draw_mode: u32,
}

impl NiObject for NiStencilProperty {
    fn block_type_name(&self) -> &'static str {
        "NiStencilProperty"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiStencilProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;

        if stream.version() <= NifVersion::V20_0_0_5 {
            // Oblivion format: expanded fields.
            let stencil_enabled = stream.read_u8()? != 0;
            let stencil_function = stream.read_u32_le()?;
            let stencil_ref = stream.read_u32_le()?;
            let stencil_mask = stream.read_u32_le()?;
            let fail_action = stream.read_u32_le()?;
            let z_fail_action = stream.read_u32_le()?;
            let pass_action = stream.read_u32_le()?;
            let draw_mode = stream.read_u32_le()?;

            Ok(Self {
                net,
                flags: 0,
                stencil_enabled,
                stencil_function,
                stencil_ref,
                stencil_mask,
                fail_action,
                z_fail_action,
                pass_action,
                draw_mode,
            })
        } else {
            // FO3/FNV/Skyrim format: packed flags.
            let flags = stream.read_u16_le()?;
            let stencil_ref = stream.read_u32_le()?;
            let stencil_mask = stream.read_u32_le()?;

            // Unpack from flags:
            // bit 0: stencil enable
            // bits 1-3: fail action
            // bits 4-6: z-fail action
            // bits 7-9: pass action
            // bits 10-11: draw mode
            // bits 12-14: stencil function
            let stencil_enabled = flags & 1 != 0;
            let fail_action = ((flags >> 1) & 0x7) as u32;
            let z_fail_action = ((flags >> 4) & 0x7) as u32;
            let pass_action = ((flags >> 7) & 0x7) as u32;
            let draw_mode = ((flags >> 10) & 0x3) as u32;
            let stencil_function = ((flags >> 12) & 0x7) as u32;

            Ok(Self {
                net,
                flags,
                stencil_enabled,
                stencil_function,
                stencil_ref,
                stencil_mask,
                fail_action,
                z_fail_action,
                pass_action,
                draw_mode,
            })
        }
    }

    /// Returns true if draw_mode indicates double-sided rendering.
    pub fn is_two_sided(&self) -> bool {
        // 0 = CCW_OR_BOTH (app default → treat as BOTH), 3 = BOTH
        self.draw_mode == 0 || self.draw_mode == 3
    }
}

// ── NiZBufferProperty ────────────────────────────────────────────────

/// Controls depth (Z-buffer) testing and writing.
///
/// flags bit 0: z-buffer test enable
/// flags bit 1: z-buffer write enable
/// bits 2-5: test function (on Oblivion, separate field instead)
#[derive(Debug)]
pub struct NiZBufferProperty {
    pub net: NiObjectNETData,
    pub flags: u16,
    pub z_test_enabled: bool,
    pub z_write_enabled: bool,
    pub z_function: u32,
}

impl NiObject for NiZBufferProperty {
    fn block_type_name(&self) -> &'static str {
        "NiZBufferProperty"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiZBufferProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let flags = stream.read_u16_le()?;

        let z_test_enabled = flags & 1 != 0;
        let z_write_enabled = flags & 2 != 0;

        let z_function = if stream.version() <= NifVersion::V20_0_0_5 {
            // Oblivion: separate field.
            stream.read_u32_le()?
        } else {
            // FO3+: packed in flags bits 2-5.
            ((flags >> 2) & 0xF) as u32
        };

        Ok(Self {
            net,
            flags,
            z_test_enabled,
            z_write_enabled,
            z_function,
        })
    }
}
