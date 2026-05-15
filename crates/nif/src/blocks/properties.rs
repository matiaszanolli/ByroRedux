//! NIF property blocks — control rendering state.
//!
//! Properties are attached to NiAVObject nodes and propagate down
//! the scene graph unless overridden.

use super::base::NiObjectNETData;
use super::NiObject;
use crate::impl_ni_object;
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

impl NiMaterialProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        // NiMaterialProperty flags: since 3.0, until 10.0.1.2 (NOT present in Oblivion+).
        // `until=` is inclusive per the version.rs doctrine — field present at v10.0.1.2.
        if stream.version() <= NifVersion::V10_0_1_2 {
            let _flags = stream.read_u16_le()?;
        }

        // nif.xml line 4366-4367:
        //   <field name="Ambient Color" vercond="#BSVER# #LT# 26">
        //   <field name="Diffuse Color" vercond="#BSVER# #LT# 26">
        // Use the file's in-header BSVER directly so files whose
        // `NifVariant` classification puts them in a different
        // boundary bucket than their actual `user_version_2` still
        // parse correctly. See #323 / #938.
        let bethesda_compact = stream.bsver() >= crate::version::bsver::FLAGS_U32_THRESHOLD;

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

impl NiTexturingProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;

        // Flags: ushort until 10.0.1.2, TexturingFlags since 20.1.0.2.
        // Gap: versions in (10.0.1.2, 20.1.0.2) have NO flags field.
        // `until=` is inclusive per the version.rs doctrine — present at v10.0.1.2.
        let flags = if stream.version() <= NifVersion::V10_0_1_2
            || stream.version() >= NifVersion::V20_1_0_2
        {
            stream.read_u16_le()?
        } else {
            0
        };

        // Apply Mode: since 3.3.0.13, until 20.1.0.1.
        // `until=` is inclusive per the version.rs doctrine — present at v20.1.0.1.
        if stream.version() <= NifVersion::STRING_TABLE_THRESHOLD {
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
        let is_v20_2_0_5_plus = stream.version() >= NifVersion::V20_2_0_5;

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
        let num_decals = if stream.version() >= NifVersion::V20_2_0_5 {
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
        if stream.version() >= NifVersion::V10_0_1_0 {
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
                    if stream.version() >= NifVersion::V20_1_0_3 {
                        let _flags = stream.read_u16_le()?;
                    } else {
                        let _clamp = stream.read_u32_le()?;
                        let _filter = stream.read_u32_le()?;
                        let _uv_set = stream.read_u32_le()?;
                    }
                    // nif.xml: `Has Texture Transform` + conditional
                    // 32-byte body are both `since="10.1.0.0"`. Mirrors
                    // the same gate inside `read_tex_desc`.
                    if stream.version() >= crate::version::NifVersion::V10_1_0_0 {
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

        if stream.version() >= NifVersion::V20_1_0_3 {
            let flags = stream.read_u16_le()?;
            // nif.xml: Has Texture Transform (bool) since 10.1.0.0,
            // present in every modern file. We read the 32-byte TexDesc
            // transform body when the bool is set and store it on the
            // returned TexDesc; the old parser skipped it, which caused
            // #219 (per-slot UV transforms lost).
            let transform = if stream.version() >= crate::version::NifVersion::V10_1_0_0 {
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
            if stream.version() <= NifVersion::V10_4_0_1 {
                let _ps2_l = stream.read_u16_le()?;
                let _ps2_k = stream.read_u16_le()?;
            }

            let transform = if stream.version() >= crate::version::NifVersion::V10_1_0_0 {
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

impl NiFogProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        // NiProperty.Flags: since 3.0, until 10.0.1.2 — not present in FO3+.
        // `until=` is inclusive per the version.rs doctrine — present at v10.0.1.2.
        if stream.version() <= NifVersion::V10_0_1_2 {
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
            let flags = if stream.version() <= NifVersion::V10_0_1_2 {
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

impl NiZBufferProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let flags = stream.read_u16_le()?;

        let z_test_enabled = flags & 1 != 0;
        let z_write_enabled = flags & 2 != 0;

        let z_function = if stream.version() < NifVersion::V4_1_0_12 {
            // Pre-4.1.0.12 NIFs predate the `z_function` field
            // entirely — no Bethesda title ships in this range (all
            // are v10+), but parsing tools / pre-Morrowind reference
            // content can land here. Mirror the engine default
            // (LESS_OR_EQUAL = 3) so consumers don't see garbage
            // when no field was authored. See NIF-D4-NEW-09 (audit
            // 2026-05-12).
            3
        } else if stream.version() <= NifVersion::V20_0_0_5 {
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


impl_ni_object!(
    NiMaterialProperty,
    NiAlphaProperty,
    NiTexturingProperty,
    NiFogProperty,
    NiStringPalette,
    NiVertexColorProperty,
    NiStencilProperty,
    NiZBufferProperty,
);

#[cfg(test)]
#[path = "properties_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "properties_fog_tests.rs"]
mod fog_property_tests;
