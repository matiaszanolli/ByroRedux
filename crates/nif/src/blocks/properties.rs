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
        // `until=` is inclusive per the version.rs doctrine — field present at v10.0.1.2.
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
    /// Parallax height-map slot (v20.2.0.5+ only, nif.xml `Parallax Texture`).
    /// Rare on vanilla FO3/FNV but shows up on ported / mixed clutter
    /// that retains the legacy `NiTexturingProperty` chain alongside a
    /// `BSShaderPPLightingProperty`. Pre-#450 the parser consumed the
    /// slot + offset to keep stream alignment then dropped the TexDesc.
    pub parallax_texture: Option<TexDesc>,
    /// Trailing f32 from the parallax slot — nif.xml `Parallax Offset`.
    /// Authored sparingly; typical values are 0.0–1.0. Zero when no
    /// parallax slot is present.
    pub parallax_offset: f32,
    /// Decal texture slots (0-indexed from the first decal slot). Oblivion
    /// uses these for blood splatters, wall paintings / map decals, faction
    /// symbols, and other persistent per-triangle overlays. Up to 4 decals
    /// per nif.xml (`Has Decal 0..3 Texture`). Before #400 the parser read
    /// and discarded each `TexDesc`; now they ride through to the importer.
    pub decal_textures: Vec<TexDesc>,
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
        // Gap: versions in (10.0.1.2, 20.1.0.2) have NO flags field.
        // `until=` is inclusive per the version.rs doctrine — present at v10.0.1.2.
        let flags = if stream.version() <= NifVersion(0x0A000102)
            || stream.version() >= NifVersion(0x14010002)
        {
            stream.read_u16_le()?
        } else {
            0
        };

        // Apply Mode: since 3.3.0.13, until 20.1.0.1.
        // `until=` is inclusive per the version.rs doctrine — present at v20.1.0.1.
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
        // Per nif.xml `NiTexturingProperty`, slots 6-7 (`Has Normal
        // Texture` / `Has Parallax Texture`) are gated `since 20.2.0.5`
        // — they don't exist on Oblivion (v20.0.0.5). Pre-#429 the
        // bool reads were unconditional; on Oblivion NIFs with
        // `texture_count > 6` (decal-heavy clutter), each read consumed
        // 1 stray byte from what should have been a decal-slot bool,
        // misaligning every following block. Oblivion has no
        // `block_sizes` table to recover from the drift, so downstream
        // blocks eventually read junk counts and either crashed
        // (#388 OOM-class) or got demoted to NiUnknown via the
        // recovery path (#395). The decal loop below is already
        // correctly version-branched and will absorb the slot
        // accounting once these reads stop poking through.
        let is_v20_2_0_5_plus = stream.version() >= crate::version::NifVersion(0x14020005);

        let normal_texture = if is_v20_2_0_5_plus && texture_count > 6 {
            Self::read_tex_desc(stream)?
        } else {
            None
        };

        // Parallax texture (slot 7) — v20.2.0.5+ only. nif.xml pairs
        // the optional `Parallax Texture` TexDesc with a trailing
        // `Parallax Offset` f32 that is only present when the slot is
        // populated. Pre-#450 both were consumed but discarded; the
        // slot now rides through so the importer can route it to the
        // fragment shader alongside `normal_texture` / `parallax_map`.
        let (parallax_texture, parallax_offset) = if is_v20_2_0_5_plus && texture_count > 7 {
            let parallax = Self::read_tex_desc(stream)?;
            let offset = if parallax.is_some() {
                stream.read_f32_le()?
            } else {
                0.0
            };
            (parallax, offset)
        } else {
            (None, 0.0)
        };
        // Decal texture slots. nif.xml gates each decal at count > 8, > 9, > 10, > 11
        // (v20.2.0.5+) or count > 6, > 7, > 8, > 9 (pre-20.2.0.5). Slot count
        // depends on whether normal+parallax exist:
        //   v20.2.0.5+: slots 0-7 (base/dark/detail/gloss/glow/bump/normal/parallax)
        //               consumed above; decals start at slot 8.
        //   Pre-20.2.0.5 (Oblivion / older): slots 0-5 only — no normal, no
        //               parallax. Decals start at slot 6, num_decals =
        //               texture_count - 6. Pre-#429 the formula was
        //               `texture_count - 7` (off by one), which masked the
        //               unconditional normal_texture read above (`>6` slot
        //               accidentally aligned with what was actually decal 0).
        //               Both bugs needed fixing together; either alone
        //               misaligns downstream blocks.
        let num_decals = if stream.version() >= crate::version::NifVersion(0x14020005) {
            texture_count.saturating_sub(8)
        } else {
            texture_count.saturating_sub(6)
        };
        // #400 — retain decal TexDescs so the importer can surface them
        // to the renderer. `TexDesc::None` entries (empty slot flag) are
        // skipped so downstream consumers only see populated slots.
        let mut decal_textures: Vec<TexDesc> = stream.allocate_vec(num_decals)?;
        for _ in 0..num_decals {
            if let Some(desc) = Self::read_tex_desc(stream)? {
                decal_textures.push(desc);
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
                    // Each shader Map is a full TexDesc (sans leading
                    // `Has Map`, which we already consumed above) plus
                    // a trailing `Map ID` u32. That means the body
                    // includes `Has Texture Transform` + optional
                    // 32-byte transform for version >= 10.1.0.0 — the
                    // old code skipped it, putting the parser 1 or
                    // 33 bytes short per entry and cascading into every
                    // following block. See #119 / audit NIF-302.
                    let _source_ref = stream.read_block_ref()?;
                    if stream.version() >= crate::version::NifVersion(0x14010003) {
                        let _flags = stream.read_u16_le()?;
                    } else {
                        let _clamp = stream.read_u32_le()?;
                        let _filter = stream.read_u32_le()?;
                        let _uv_set = stream.read_u32_le()?;
                    }
                    // nif.xml: `Has Texture Transform` + conditional
                    // 32-byte body are both `since="10.1.0.0"`. Mirrors
                    // the same gate inside `read_tex_desc`.
                    if stream.version() >= crate::version::NifVersion(0x0A010000) {
                        let has_transform = stream.read_byte_bool()?;
                        if has_transform {
                            let _ = Self::read_tex_transform(stream)?;
                        }
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
            parallax_texture,
            parallax_offset,
            decal_textures,
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

            // TexDesc PS2 L/K: nif.xml `until="10.4.0.1"` inclusive per the
            // version.rs doctrine — present at v <= 10.4.0.1.
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

    /// Regression test for issue #400 — NiTexturingProperty decal slots
    /// (Oblivion pre-20.2.0.5 path, slots 6..=texture_count-1) are now
    /// retained on the block instead of silently discarded. Builds a
    /// header with texture_count=8 → 2 populated decal TexDescs, checks
    /// both are reachable.
    #[test]
    fn parse_ni_texturing_property_retains_oblivion_decal_slots() {
        // Oblivion — v20.0.0.5, user_version=11. Pre-20.2.0.5 layout:
        // slots 0..=5 are base/dark/detail/gloss/glow/bump; decals
        // start at slot 6. No normal/parallax slots in this version.
        let header = NifHeader {
            version: NifVersion::V20_0_0_5,
            little_endian: true,
            user_version: 11,
            user_version_2: 11,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };
        let mut data = Vec::new();
        // NiObjectNET base: inline name (empty) + 0 extras + null controller.
        data.extend_from_slice(&0u32.to_le_bytes()); // name: empty inline
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                        // flags u16 (v <= 10.0.1.2 OR v >= 20.1.0.2) — 20.0.0.5 is
                                                        // in the middle gap, so NO flags field. apply_mode u32 reads
                                                        // (v <= 20.1.0.1).
        data.extend_from_slice(&1u32.to_le_bytes()); // apply_mode
                                                     // texture_count = 8 → slots 0..=7 consumed, slots 6 and 7
                                                     // become decals 0 and 1.
        data.extend_from_slice(&8u32.to_le_bytes());
        // Helper: minimal TexDesc for v=20.0.0.5 with has=1.
        // v < 10.1.0.3 → ELSE branch: source_ref + 3 × u32 (clamp / filter /
        // uv_set) + has_transform bool. (20.0.0.5 is below 20.1.0.3.)
        let push_populated = |data: &mut Vec<u8>, source: i32| {
            data.push(1); // has
            data.extend_from_slice(&source.to_le_bytes());
            data.extend_from_slice(&0u32.to_le_bytes()); // clamp_mode
            data.extend_from_slice(&0u32.to_le_bytes()); // filter_mode
            data.extend_from_slice(&0u32.to_le_bytes()); // uv_set
            data.push(0); // has_transform = 0
        };
        let push_empty = |data: &mut Vec<u8>| {
            data.push(0); // has = 0 → TexDesc is None, 1 byte total
        };
        // Slots 0..=5 all empty.
        push_empty(&mut data); // base
        push_empty(&mut data); // dark
        push_empty(&mut data); // detail
        push_empty(&mut data); // gloss
        push_empty(&mut data); // glow
        push_empty(&mut data); // bump
                               // 20.0.0.5 is NOT >= 20.2.0.5 → the normal/parallax slots are
                               // skipped. Decal loop picks up 8-6 = 2 slots.
        push_populated(&mut data, 101); // decal 0
        push_populated(&mut data, 202); // decal 1
                                        // Shader map list trailer (since v >= 10.0.1.0).
        data.extend_from_slice(&0u32.to_le_bytes()); // num_shader_textures = 0

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = NiTexturingProperty::parse(&mut stream)
            .expect("Oblivion NiTexturingProperty should parse");
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "parse consumed {} bytes, expected {}",
            stream.position(),
            expected_len
        );
        assert_eq!(prop.texture_count, 8);
        assert_eq!(
            prop.decal_textures.len(),
            2,
            "expected 2 decal TexDescs (slots 6 + 7) in retained vec"
        );
        assert_eq!(prop.decal_textures[0].source_ref.index(), Some(101));
        assert_eq!(prop.decal_textures[1].source_ref.index(), Some(202));
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

    /// Boundary regression for #935 (post-#769 doctrine flip). nif.xml
    /// gates `Apply Mode` with `until="20.1.0.1"` which is **inclusive**
    /// per niftools/nifly (see version.rs doctrine). The field IS
    /// present at v20.1.0.1 exactly. The first version that drops the
    /// field is v20.1.0.2.
    #[test]
    fn parse_ni_texturing_property_apply_mode_at_v20_1_0_1_exactly() {
        let header = NifHeader {
            version: NifVersion(0x14010001), // v20.1.0.1 — the until= boundary
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };
        let mut data = Vec::new();
        // NiObjectNETData: name = -1 (None), extras count = 0, controller = -1.
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // v20.1.0.1 is still inside `until="20.1.0.1"` (inclusive) so
        // Apply Mode IS read here. Flags is absent (gap version).
        data.extend_from_slice(&1u32.to_le_bytes()); // apply_mode = 1
        data.extend_from_slice(&0u32.to_le_bytes()); // texture_count = 0
        data.push(0); // base_texture has = 0 → None
        data.extend_from_slice(&0u32.to_le_bytes()); // shader_textures count = 0

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = NiTexturingProperty::parse(&mut stream)
            .expect("v20.1.0.1 NiTexturingProperty must consume Apply Mode under inclusive doctrine");
        assert_eq!(stream.position() as usize, expected_len);
        assert_eq!(prop.texture_count, 0);
        assert!(prop.base_texture.is_none());
        assert_eq!(prop.decal_textures.len(), 0);
    }

    /// Boundary above the inclusive `until="20.1.0.1"` — at v20.1.0.2
    /// the Apply Mode field is finally absent and the new TexturingFlags
    /// path is active (`since="20.1.0.2"`).
    #[test]
    fn parse_ni_texturing_property_no_apply_mode_at_v20_1_0_2() {
        let header = NifHeader {
            version: NifVersion(0x14010002),
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };
        let mut data = Vec::new();
        // NiObjectNETData: name = -1 (None), extras count = 0, controller = -1.
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // v20.1.0.2: Flags u16 IS read (since=20.1.0.2 path), Apply Mode absent.
        data.extend_from_slice(&0u16.to_le_bytes()); // flags = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // texture_count = 0
        data.push(0); // base_texture has = 0 → None
        data.extend_from_slice(&0u32.to_le_bytes()); // shader_textures count = 0

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = NiTexturingProperty::parse(&mut stream)
            .expect("v20.1.0.2 NiTexturingProperty must skip Apply Mode under inclusive doctrine");
        assert_eq!(stream.position() as usize, expected_len);
        assert_eq!(prop.texture_count, 0);
    }

    /// Pre-boundary spot check: at v20.1.0.0 the `Apply Mode` field is
    /// present (as it is throughout `[3.3.0.13, 20.1.0.1]` inclusive).
    #[test]
    fn parse_ni_texturing_property_with_apply_mode_below_v20_1_0_1() {
        let header = NifHeader {
            version: NifVersion(0x14010000), // v20.1.0.0 — below the boundary
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };
        let mut data = Vec::new();
        // v20.1.0.0 is BELOW the v20.1.0.1 string-table boundary, so
        // `read_string` uses the length-prefixed inline path: u32 len + bytes.
        data.extend_from_slice(&0u32.to_le_bytes());    // name: empty inline (len = 0)
        data.extend_from_slice(&0u32.to_le_bytes());    // extras count = 0
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref = -1
        data.extend_from_slice(&1u32.to_le_bytes());    // apply_mode = 1 (present pre-20.1.0.1)
        data.extend_from_slice(&0u32.to_le_bytes());    // texture_count = 0
        data.push(0);                                    // base_texture has = 0
        data.extend_from_slice(&0u32.to_le_bytes());    // shader_textures count = 0

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = NiTexturingProperty::parse(&mut stream)
            .expect("v20.1.0.0 NiTexturingProperty must consume Apply Mode");
        assert_eq!(stream.position() as usize, expected_len);
        assert_eq!(prop.texture_count, 0);
    }

    /// Regression: #119 / audit NIF-302 — a shader map entry with
    /// `has_map = 1` at v >= 10.1.0.0 MUST consume its
    /// `Has Texture Transform` bool and, if set, the 32-byte
    /// transform body. Previously the loop skipped straight from
    /// `flags` to `map_id`, putting the parser 1-33 bytes short per
    /// non-empty shader map entry and cascading into every following
    /// block. Two variants: has_transform=0 (just the bool) and
    /// has_transform=1 (bool + 32-byte body).
    #[test]
    fn parse_ni_texturing_property_shader_map_consumes_has_transform_bool() {
        let header = make_header(12, 83); // Skyrim LE — v20.2.0.7, >= 20.1.0.3 flags path
        let mut data = Vec::new();
        // NiObjectNET base
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // NiProperty flags + texture_count = 0 (no slot-0 textures).
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        // `read_tex_desc` for base_texture runs unconditionally — reads
        // `has: bool` even when texture_count=0. Set it to 0 for an empty
        // slot entry.
        data.push(0); // base_texture has = 0
                      // num_shader_textures = 1
        data.extend_from_slice(&1u32.to_le_bytes());
        // Shader map entry — has_map = 1, then body.
        data.push(1); // has_map
        data.extend_from_slice(&7i32.to_le_bytes()); // source_ref
        data.extend_from_slice(&0x0102u16.to_le_bytes()); // flags (v >= 20.1.0.3)
        data.push(0); // has_transform = 0 (no trailing body)
        data.extend_from_slice(&42u32.to_le_bytes()); // map_id

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let _prop = NiTexturingProperty::parse(&mut stream).unwrap();
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "shader map entry with has_transform=0 must consume the bool \
             between flags and map_id"
        );
    }

    #[test]
    fn parse_ni_texturing_property_shader_map_consumes_full_transform() {
        let header = make_header(12, 83);
        let mut data = Vec::new();
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        // base_texture has = 0 (unconditional read).
        data.push(0);
        data.extend_from_slice(&1u32.to_le_bytes());
        // has_map = 1
        data.push(1);
        data.extend_from_slice(&11i32.to_le_bytes()); // source_ref
        data.extend_from_slice(&0x0304u16.to_le_bytes()); // flags
        data.push(1); // has_transform = 1 → 32-byte body follows
                      // TexTransform: translation(2) + scale(2) + rotation(1) + method(1 u32) + center(2) = 8 × 4B.
        for f in [0.25f32, -0.5, 2.0, 3.0, 0.75] {
            data.extend_from_slice(&f.to_le_bytes());
        }
        data.extend_from_slice(&2u32.to_le_bytes()); // transform_method
        data.extend_from_slice(&0.1f32.to_le_bytes()); // center x
        data.extend_from_slice(&0.2f32.to_le_bytes()); // center y
        data.extend_from_slice(&99u32.to_le_bytes()); // map_id

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let _prop = NiTexturingProperty::parse(&mut stream).unwrap();
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "shader map entry with has_transform=1 must consume the \
             32-byte TexTransform body between flags and map_id"
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

    /// Regression: #429 — at v20.0.0.5 (Oblivion), slots 6-7
    /// (`Has Normal Texture` / `Has Parallax Texture`) do NOT exist
    /// per nif.xml — they're gated `since 20.2.0.5`. Pre-fix the
    /// parser read those bools unconditionally, so an Oblivion NIF
    /// with `texture_count == 8` over-consumed 2 bytes of what
    /// should have been decal-slot bools, then potentially more
    /// bytes if those phantom bools came back as `1`. With no
    /// `block_sizes` table to resync, every following block
    /// misaligned. This test exercises the exact failure shape:
    /// build a v20.0.0.5 NiTexturingProperty with `texture_count = 8`
    /// (base + 5 absent slots + 2 decals), assert byte-exact
    /// consumption, and confirm the decal `has = 0` bools after
    /// position survive.
    #[test]
    fn parse_ni_texturing_property_oblivion_skips_normal_parallax_slots() {
        let mut header = make_header(11, 11);
        header.version = NifVersion(0x14000005); // v20.0.0.5 — Oblivion
        let mut data = Vec::new();
        // NiObjectNET on Oblivion (v20.0.0.5 < 20.1.0.1): name is a
        // length-prefixed inline string (u32 length, then bytes), not
        // a string-table index. Write zero-length to mean "no name".
        data.extend_from_slice(&0u32.to_le_bytes()); // name length = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count = 0
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref = NULL
                                                        // No flags field — Oblivion sits in the 10.0.1.3..20.1.0.1
                                                        // gap where NiTexturingProperty has neither the legacy u16
                                                        // flags nor the modern TexturingFlags. Apply mode IS still
                                                        // present (until 20.1.0.1) — write the u32.
        data.extend_from_slice(&0u32.to_le_bytes()); // apply_mode
        data.extend_from_slice(&8u32.to_le_bytes()); // texture_count = 8
                                                     // Slots 0-5: each is a `has = 0` bool (no body). Slots 6-7
                                                     // do NOT exist on this version — pre-fix the parser would
                                                     // also try to read those, eating 2 bytes from below.
        for _ in 0..6 {
            data.push(0); // has = false
        }
        // Pre-v20.2.0.5: decals start at slot 6. With texture_count=8,
        // num_decals = 8-6 = 2 decal slots, each a `has = 0` bool
        // (no body since has=false).
        for _ in 0..2 {
            data.push(0); // decal has = false
        }

        // Trailer: shader textures count = 0 (since v10.0.1.0+).
        data.extend_from_slice(&0u32.to_le_bytes());

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = NiTexturingProperty::parse(&mut stream).unwrap();
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "Oblivion NiTexturingProperty must consume exactly the bytes \
             it authored — pre-#429 the parser ate `has_normal` + \
             `has_parallax` bools that v20.0.0.5 doesn't carry, \
             over-consuming 2 bytes from the decal slot below"
        );
        // Sanity: every slot read came back empty (we wrote `has=0`
        // for everything).
        assert!(prop.base_texture.is_none());
        assert!(prop.dark_texture.is_none());
        assert!(prop.bump_texture.is_none());
        // normal_texture must be `None` because Oblivion doesn't
        // have the slot — pre-#429 it would have been Some(...) or
        // a parse error from over-reading.
        assert!(prop.normal_texture.is_none());
    }

    /// Regression: #484 — pin the `num_decals` boundary for v20.2.0.5+.
    ///
    /// The #400/#429 fix computes `num_decals = texture_count.saturating_sub(8)`
    /// on v20.2.0.5+ (FO3/FNV/SkyrimLE pre-BSTriShape path). `count == 8`
    /// is the exact threshold where slots 0..7 are consumed (base/dark/
    /// detail/gloss/glow/bump/normal/parallax) and no decals remain.
    /// A future rewrite that flips the comparison (e.g. `saturating_sub(7)`,
    /// or `>` instead of `>=`) would silently consume one extra decal
    /// byte here and misalign every downstream block. The next-larger
    /// test pins `count == 9 → 1 decal` from the other side of the boundary.
    #[test]
    fn num_decals_boundary_v20_2_0_5_count_8_yields_zero() {
        let header = make_header(11, 34); // FNV bsver=34
        let mut data = Vec::new();
        // NiObjectNET: string-table index for `name` (v >= 20.1.0.1).
        data.extend_from_slice(&(-1i32).to_le_bytes()); // name index = -1
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                        // NiProperty.Flags (u16) present since 20.1.0.2.
        data.extend_from_slice(&0u16.to_le_bytes());
        // apply_mode omitted (gated `<= 20.1.0.1`) — v20.2.0.7 skips it.
        data.extend_from_slice(&8u32.to_le_bytes()); // texture_count = 8
                                                     // Slots 0..=7: base/dark/detail/gloss/glow/bump/normal/parallax.
                                                     // All `has = 0` — the parser's fixed-slot loop consumes every
                                                     // one so slot accounting lines up. No decals at count=8.
        for _ in 0..8 {
            data.push(0); // has = 0
        }
        // Shader textures trailer.
        data.extend_from_slice(&0u32.to_le_bytes());

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = NiTexturingProperty::parse(&mut stream).expect("parse");
        assert_eq!(stream.position() as usize, expected_len);
        assert_eq!(prop.texture_count, 8);
        assert_eq!(
            prop.decal_textures.len(),
            0,
            "v20.2.0.5+ texture_count=8 must yield zero decals — slots 0..=7 consume the fixed allocation"
        );
    }

    /// Regression: #484 — v20.2.0.5+ `count == 9 → num_decals == 1`.
    /// Pairs with the `count == 8` test above to lock both sides of the
    /// `saturating_sub(8)` threshold.
    #[test]
    fn num_decals_boundary_v20_2_0_5_count_9_yields_one() {
        let header = make_header(11, 34);
        let mut data = Vec::new();
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes()); // flags
        data.extend_from_slice(&9u32.to_le_bytes()); // texture_count = 9
                                                     // Slots 0..=7 empty + 1 populated decal.
        for _ in 0..8 {
            data.push(0);
        }
        // Decal 0 — v20.2.0.7 uses the modern TexDesc (v >= 20.1.0.3):
        //   has(bool) + source_ref(i32) + flags(u16) + has_transform(bool).
        data.push(1);
        data.extend_from_slice(&42i32.to_le_bytes()); // source_ref = 42
        data.extend_from_slice(&0u16.to_le_bytes()); // flags
        data.push(0); // has_transform = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // shader textures trailer

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = NiTexturingProperty::parse(&mut stream).expect("parse");
        assert_eq!(stream.position() as usize, expected_len);
        assert_eq!(prop.texture_count, 9);
        assert_eq!(
            prop.decal_textures.len(),
            1,
            "v20.2.0.5+ texture_count=9 must yield exactly one decal — locks saturating_sub(8) against off-by-one regressions"
        );
        assert_eq!(prop.decal_textures[0].source_ref.index(), Some(42));
    }

    /// Regression: #484 — pre-20.2.0.5 `count == 6 → num_decals == 0`.
    /// Mirrors the v20.2.0.5+ test above but for the Oblivion-era
    /// `saturating_sub(6)` branch (no normal + parallax slots).
    #[test]
    fn num_decals_boundary_pre_v20_2_0_5_count_6_yields_zero() {
        let mut header = make_header(11, 11);
        header.version = NifVersion(0x14000005); // v20.0.0.5 — Oblivion
        let mut data = Vec::new();
        // Oblivion NiObjectNET: inline-string name (u32 length + bytes).
        data.extend_from_slice(&0u32.to_le_bytes()); // name length = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                        // No flags field on v20.0.0.5 (10.0.1.3..20.1.0.1 gap).
        data.extend_from_slice(&0u32.to_le_bytes()); // apply_mode
        data.extend_from_slice(&6u32.to_le_bytes()); // texture_count = 6
        for _ in 0..6 {
            data.push(0); // each slot `has = 0`
        }
        data.extend_from_slice(&0u32.to_le_bytes()); // shader textures trailer

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = NiTexturingProperty::parse(&mut stream).expect("parse");
        assert_eq!(stream.position() as usize, expected_len);
        assert_eq!(prop.texture_count, 6);
        assert_eq!(
            prop.decal_textures.len(),
            0,
            "pre-20.2.0.5 texture_count=6 must yield zero decals — locks saturating_sub(6) threshold"
        );
    }

    /// Regression: #484 — pre-20.2.0.5 `count == 7 → num_decals == 1`.
    /// Pairs with the `count == 6` test above for the Oblivion branch.
    #[test]
    fn num_decals_boundary_pre_v20_2_0_5_count_7_yields_one() {
        let mut header = make_header(11, 11);
        header.version = NifVersion(0x14000005);
        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_le_bytes()); // name length
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // apply_mode
        data.extend_from_slice(&7u32.to_le_bytes()); // texture_count = 7
        for _ in 0..6 {
            data.push(0); // slots 0..=5 empty
        }
        // Decal 0 — v20.0.0.5 is below 20.1.0.3, so TexDesc ELSE branch:
        //   has(bool) + source_ref + 3×u32 (clamp/filter/uv_set) + has_transform
        data.push(1);
        data.extend_from_slice(&99i32.to_le_bytes()); // source_ref = 99
        data.extend_from_slice(&0u32.to_le_bytes()); // clamp
        data.extend_from_slice(&0u32.to_le_bytes()); // filter
        data.extend_from_slice(&0u32.to_le_bytes()); // uv_set
        data.push(0); // has_transform
        data.extend_from_slice(&0u32.to_le_bytes()); // shader textures trailer

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = NiTexturingProperty::parse(&mut stream).expect("parse");
        assert_eq!(stream.position() as usize, expected_len);
        assert_eq!(prop.texture_count, 7);
        assert_eq!(
            prop.decal_textures.len(),
            1,
            "pre-20.2.0.5 texture_count=7 must yield exactly one decal"
        );
        assert_eq!(prop.decal_textures[0].source_ref.index(), Some(99));
    }
}

// ── NiFogProperty ────────────────────────────────────────────────────

/// Per-node fog override (legacy; 1 FO3 block observed in the wild).
///
/// nif.xml: NiProperty → NiObjectNET → NiObject.
/// NiProperty.Flags (until 10.0.1.2) NOT present in FO3+.
/// Own field: FogFlags (u16) + fog_depth (f32) + fog_color (Color3).
#[derive(Debug)]
pub struct NiFogProperty {
    pub net: NiObjectNETData,
    pub flags: u16,
    pub fog_depth: f32,
    pub fog_color: [f32; 3],
}

impl NiObject for NiFogProperty {
    fn block_type_name(&self) -> &'static str {
        "NiFogProperty"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiFogProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        // NiProperty.Flags: since 3.0, until 10.0.1.2 — not present in FO3+.
        // `until=` is inclusive per the version.rs doctrine — present at v10.0.1.2.
        if stream.version() <= NifVersion(0x0A000102) {
            let _prop_flags = stream.read_u16_le()?;
        }
        let flags = stream.read_u16_le()?;
        let fog_depth = stream.read_f32_le()?;
        let fog_color = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        Ok(Self {
            net,
            flags,
            fog_depth,
            fog_color,
        })
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

    /// Test-only constructor — synthesise a NiFlagProperty without
    /// going through `parse`. Used by `material.rs` regression tests
    /// that need to feed NiSpecularProperty / NiWireframeProperty /
    /// NiDitherProperty / NiShadeProperty into `extract_material_info`
    /// without hand-rolling the NiObjectNETData byte layout.
    #[doc(hidden)]
    pub fn for_test(flags: u16, type_name: &'static str) -> Self {
        Self {
            net: NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: crate::types::BlockRef::NULL,
            },
            flags,
            type_name,
        }
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
            //
            // #723 / NIF-D2-05 — pre-Gamebryo NetImmerse NIFs prefix the
            // expanded fields with a u16 `flags` carried over from the
            // NiProperty base (`Flags: ushort, until=10.0.1.2` per nif.xml
            // line 5149, inclusive per the version.rs doctrine — present
            // at v <= 10.0.1.2). Pre-fix the Oblivion-format branch
            // claimed the legacy u16 flags belonged to `stencil_enabled`'s
            // first byte and drifted the rest of the record by 2 bytes.
            // No Bethesda title ships in this band; this guards pre-
            // Gamebryo compat.
            let flags = if stream.version() <= NifVersion(0x0A000102) {
                stream.read_u16_le()?
            } else {
                0
            };
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

#[cfg(test)]
mod fog_property_tests {
    use super::*;
    use crate::header::NifHeader;
    use crate::stream::NifStream;
    use crate::version::NifVersion;
    use std::sync::Arc;

    fn make_fo3_header() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 11,
            user_version_2: 21,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("")],
            max_string_length: 0,
            num_groups: 0,
        }
    }

    #[test]
    fn parse_ni_fog_property_fo3() {
        let header = make_fo3_header();
        let mut data = Vec::new();
        // NiObjectNET: name (string index u32) + num_extra u32 + controller_ref i32
        data.extend_from_slice(&0u32.to_le_bytes()); // name index 0
        data.extend_from_slice(&0u32.to_le_bytes()); // num extra data
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller ref = null
                                                        // No NiProperty.Flags (v20.2.0.7 > 10.0.1.2)
                                                        // FogFlags: 1 (enabled)
        data.extend_from_slice(&1u16.to_le_bytes());
        // fog_depth: 0.5
        data.extend_from_slice(&0.5f32.to_le_bytes());
        // fog_color: grey
        data.extend_from_slice(&0.5f32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let prop = NiFogProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.flags, 1);
        assert!((prop.fog_depth - 0.5).abs() < 1e-6);
        assert!((prop.fog_color[0] - 0.5).abs() < 1e-6);
        assert_eq!(stream.position() as usize, data.len());
    }
}
