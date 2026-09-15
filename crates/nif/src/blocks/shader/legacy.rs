//! Fallout 3 / New Vegas shader property blocks.
//!
//! Split out of the single `blocks/shader.rs` under #4339, which had
//! re-crossed 2000 production lines holding four block families behind one
//! shared head. The axis is the block family, not the game: every growth
//! commit touched exactly one family, while a per-game split would have
//! scattered one struct's parsers across five files.

use crate::blocks::base::{BSShaderPropertyData, NiObjectNETData};
use crate::blocks::NiObject;
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
    /// Refraction strength (0.0–1.0). Present when bsver > crate::version::bsver::FO3_REFRACTION.
    pub refraction_strength: f32,
    /// Refraction fire period. Present when bsver > crate::version::bsver::FO3_REFRACTION.
    pub refraction_fire_period: i32,
    /// Parallax max passes. Present when bsver > crate::version::bsver::FO3_PARALLAX.
    pub parallax_max_passes: f32,
    /// Parallax scale. Present when bsver > crate::version::bsver::FO3_PARALLAX.
    pub parallax_scale: f32,
    /// Emissive glow color (RGBA). nif.xml: "Emissive Color" vercond="#BS_GT_FO3#" (bsver > crate::version::bsver::FO3_FNV).
    /// Defaults to black/opaque when absent (FO3/FNV bsver <= crate::version::bsver::FO3_FNV).
    pub emissive_color: [f32; 4],
}

impl BSShaderPPLightingProperty {
    pub fn shader_flags_1(&self) -> u32 {
        self.shader.shader_flags_1
    }
    pub fn shader_flags_2(&self) -> u32 {
        self.shader.shader_flags_2
    }
}

impl BSShaderPPLightingProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let (shader, texture_clamp_mode) = BSShaderPropertyData::parse_fo3(stream)?;
        let texture_set_ref = stream.read_block_ref()?;

        // nif.xml:6245-6246 — Refraction Strength (f32) + Refraction Fire Period (i32)
        // vercond="#BSVER# #GT# 14" (strictly greater).
        let bsver = stream.bsver();
        let (refraction_strength, refraction_fire_period) =
            if bsver > crate::version::bsver::FO3_REFRACTION {
                (stream.read_f32_le()?, stream.read_i32_le()?)
            } else {
                (0.0, 0)
            };

        // nif.xml:6247-6248 — Parallax Max Passes (f32) + Parallax Scale (f32)
        // vercond="#BSVER# #GT# 24" (strictly greater). FO3 ships content at
        // bsver=24 which must NOT carry these fields; the prior `>= 24` gate
        // over-read 8 phantom bytes on those files (#774 / FO3-1-PARGATE).
        let (parallax_max_passes, parallax_scale) = if bsver > crate::version::bsver::FO3_PARALLAX {
            (stream.read_f32_le()?, stream.read_f32_le()?)
        } else {
            (4.0, 1.0)
        };

        // nif.xml:6250 — "Emissive Color" Color4 vercond="#BS_GT_FO3#" (i.e. bsver > crate::version::bsver::FO3_FNV).
        // FO3/FNV (bsver <= crate::version::bsver::FO3_FNV) do not carry this field; Skyrim-era PPLighting does.
        let emissive_color = if bsver > crate::version::bsver::FO3_FNV {
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
            net,
            shader,
            texture_clamp_mode,
            texture_set_ref,
            refraction_strength,
            refraction_fire_period,
            parallax_max_passes,
            parallax_scale,
            emissive_color,
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
    /// Second flag word — FO3/FNV `BSShaderFlags2` semantics (bit 21 =
    /// `Alpha_Decal`, bit 4 = `Refraction_Tint`, etc.). Added for
    /// parity with `BSShaderPPLightingProperty` so callers have a
    /// uniform accessor surface instead of reaching through
    /// `.shader.shader_flags_2`. See #460.
    pub fn shader_flags_2(&self) -> u32 {
        self.shader.shader_flags_2
    }
}

impl BSShaderNoLightingProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let (shader, texture_clamp_mode) = BSShaderPropertyData::parse_fo3(stream)?;
        let file_name = stream.read_sized_string()?;

        // nif.xml gates the four falloff fields on `#BSVER# #GT# 26` (nif.xml
        // line 6236) — the same strict per-file BSVER gate as NiAVObject.Flags.
        // Use the header BSVER, not `variant().avobject_flags_u32()`: a
        // transitional v20.2.0.7/bsver≤26 export detects as the `Fallout3`
        // variant (helper → true) and would read 16 phantom bytes of falloff
        // that aren't on disk. Sibling of the NiAVObject flag-width fix. See #1331.
        let (falloff_start_angle, falloff_stop_angle, falloff_start_opacity, falloff_stop_opacity) =
            if stream.bsver() > crate::version::bsver::FLAGS_U32_THRESHOLD {
                (
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                )
            } else {
                // nif.xml's own per-field defaults for the absent case
                // (lines 6236-6239): start_angle 1.0, stop_angle 0.0,
                // start_opacity 1.0, stop_opacity 0.0. `start_angle` is a
                // cosine-of-angle, so 1.0 means "falloff begins head-on"
                // — 0.0 (the pre-#2331 value) means "begins at grazing",
                // the inverse of what the format specifies.
                (1.0, 0.0, 1.0, 0.0)
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

/// `TileShaderProperty` — FO3-only HUD / UI tile shader. Per nif.xml
/// line 6341 it inherits `BSShaderLightingProperty` (so adds
/// `texture_clamp_mode` on top of the `BSShaderProperty` base) and
/// then appends a single `File Name: SizedString`.
///
/// Pre-#455 `blocks/mod.rs` aliased this type to
/// `BSShaderPPLightingProperty::parse`, which reads 20-28 extra bytes
/// (texture_set_ref + refraction + parallax) that the on-disk
/// TileShaderProperty does NOT carry. FO3's `block_sizes` table kept
/// the outer stream aligned but the PPLighting struct landed with
/// zero-initialized PP-specific fields; the actual `file_name` never
/// reached the struct at all. HUD overlays (stealth meter, airtimer,
/// quest markers) lost their texture path as a result.
#[derive(Debug)]
pub struct TileShaderProperty {
    pub net: NiObjectNETData,
    pub shader: BSShaderPropertyData,
    pub texture_clamp_mode: u32,
    /// HUD / UI tile texture file path. Usually
    /// `textures\interface\<name>.dds`.
    pub file_name: String,
}

impl TileShaderProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let (shader, texture_clamp_mode) = BSShaderPropertyData::parse_fo3(stream)?;
        let file_name = stream.read_sized_string()?;
        Ok(Self {
            net,
            shader,
            texture_clamp_mode,
            file_name,
        })
    }
}

/// `SkyShaderProperty` — FO3 / FNV sky dome, clouds, stars, sun-glare.
/// Per nif.xml line 6335 it inherits `BSShaderLightingProperty` (so
/// adds `texture_clamp_mode` on top of the `BSShaderProperty` base)
/// and then appends `File Name: SizedString` + `Sky Object Type: u32`.
///
/// Pre-#550 `blocks/mod.rs` aliased this type to
/// `BSShaderPPLightingProperty::parse`, which reads 20-28 extra bytes
/// (texture_set_ref + refraction + parallax) that the on-disk
/// SkyShaderProperty does NOT carry — simultaneously losing the real
/// `file_name` and `sky_object_type`. `block_sizes` recovery kept the
/// outer stream aligned so every sky NIF silently rendered with the
/// default cloud scroll and horizon fade. Recurring stderr warning
/// bucket: `consumed 54, expected 42-82` on every SkyShaderProperty
/// block in the FO3 + FNV corpora.
#[derive(Debug)]
pub struct SkyShaderProperty {
    pub net: NiObjectNETData,
    pub shader: BSShaderPropertyData,
    pub texture_clamp_mode: u32,
    /// Sky texture file path (clouds, stars, moon, etc.).
    pub file_name: String,
    /// Per nif.xml `SkyObjectType`: 0=Texture, 1=Sunglare, 2=Sky,
    /// 3=Clouds, 5=Stars, 7=Moon/Stars Mask. Selects which sky
    /// function this property fulfills at render time.
    pub sky_object_type: u32,
}

impl SkyShaderProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let (shader, texture_clamp_mode) = BSShaderPropertyData::parse_fo3(stream)?;
        let file_name = stream.read_sized_string()?;
        let sky_object_type = stream.read_u32_le()?;
        Ok(Self {
            net,
            shader,
            texture_clamp_mode,
            file_name,
            sky_object_type,
        })
    }
}

/// `WaterShaderProperty` — FO3/FNV water shader (nif.xml line 6322).
///
/// Inherits `BSShaderProperty` **directly** (not `BSShaderLightingProperty`)
/// so it carries no `texture_clamp_mode` and no additional fields of its
/// own. Pre-#474 this block was aliased to `BSShaderPPLightingProperty::
/// parse` which over-read the `texture_clamp_mode` + `texture_set_ref` +
/// refraction + parallax trailer — 24 extra bytes masked by
/// `block_sizes` recovery.
#[derive(Debug)]
pub struct WaterShaderProperty {
    pub net: NiObjectNETData,
    pub shader: BSShaderPropertyData,
}

impl WaterShaderProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let shader = BSShaderPropertyData::parse_base(stream)?;
        Ok(Self { net, shader })
    }
}

/// Zero-field `BSShaderProperty` subclasses (nif.xml lines 6346, 6350, 6359, 6363).
///
/// `HairShaderProperty`, `VolumetricFogShaderProperty`,
/// `DistantLODShaderProperty`, `BSDistantTreeShaderProperty` all inherit
/// `BSShaderProperty` directly with no additional fields — only the NET +
/// `BSShaderPropertyData` base. Pre-#717 all four were aliased to
/// `BSShaderPPLightingProperty::parse` which over-read up to 24 bytes
/// (`texture_clamp_mode` + `texture_set_ref` + refraction + parallax),
/// masked by `block_sizes` recovery but silently drifting on any modded NIF
/// that carries one of these types.
#[derive(Debug)]
pub struct BSShaderPropertyBaseOnly {
    pub net: NiObjectNETData,
    pub shader: BSShaderPropertyData,
    type_name: &'static str,
}

impl NiObject for BSShaderPropertyBaseOnly {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BSShaderPropertyBaseOnly {
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let shader = BSShaderPropertyData::parse_base(stream)?;
        Ok(Self {
            net,
            shader,
            type_name,
        })
    }

    /// Direct constructor for synthetic-scene tests that bypass the
    /// wire parser. Production code reaches this struct only through
    /// [`Self::parse`]. The `type_name` field is private to keep
    /// callers from accidentally constructing a block with a name
    /// the dispatcher table doesn't recognise.
    #[cfg(test)]
    pub(crate) fn new_for_test(
        net: NiObjectNETData,
        shader: BSShaderPropertyData,
        type_name: &'static str,
    ) -> Self {
        Self {
            net,
            shader,
            type_name,
        }
    }
}

/// `TallGrassShaderProperty` — FO3/FNV grass shader (nif.xml line 6354).
///
/// Inherits `BSShaderProperty` directly and adds a single
/// `File Name: SizedString` (grass texture path). Pre-#474 aliased to
/// `BSShaderPPLightingProperty::parse`, losing both the filename and
/// reading the wrong trailer — block_sizes recovery kept the stream
/// aligned but the filename never reached the struct.
#[derive(Debug)]
pub struct TallGrassShaderProperty {
    pub net: NiObjectNETData,
    pub shader: BSShaderPropertyData,
    /// Grass texture file path (typically `textures\landscape\*.dds`).
    pub file_name: String,
}

impl TallGrassShaderProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let shader = BSShaderPropertyData::parse_base(stream)?;
        let file_name = stream.read_sized_string()?;
        Ok(Self {
            net,
            shader,
            file_name,
        })
    }
}
