//! Bethesda shader property blocks.
//!
//! - BSShaderPPLightingProperty / BSShaderNoLightingProperty — Fallout 3/NV
//! - BSLightingShaderProperty / BSEffectShaderProperty — Skyrim+
//! - BSShaderTextureSet — shared texture path list (all games)

use super::base::{BSShaderPropertyData, NiObjectNETData};
use super::NiObject;
use crate::impl_ni_object;
use crate::stream::NifStream;
use crate::types::BlockRef;
use std::any::Any;
use std::io;
use std::sync::Arc;

/// Returns `true` when `name` looks like a `.bgsm` / `.bgem` / `.mat`
/// material file path. The FO76+/Starfield shader-property stopcond
/// fires only when the editor stored a material-file reference in
/// `Name`; plain editor labels (e.g. "Material_Slot_01") must NOT
/// trigger the short-circuit, otherwise every PBR scalar silently
/// defaults. See #749 / SF-D3-01.
///
/// Trailing `\0` and ASCII whitespace are stripped before the suffix
/// check — artists occasionally export with stale terminators (the
/// path got copy-pasted from a longer string buffer).
pub(crate) fn is_material_reference(name: &str) -> bool {
    let trimmed = name.trim_end_matches(|c: char| c == '\0' || c.is_ascii_whitespace());
    let b = trimmed.as_bytes();
    let n = b.len();
    if n < 4 {
        return false;
    }
    let tail5 = &b[n.saturating_sub(5)..];
    tail5.eq_ignore_ascii_case(b".bgsm")
        || tail5.eq_ignore_ascii_case(b".bgem")
        || b[n.saturating_sub(4)..].eq_ignore_ascii_case(b".mat")
}

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

/// Skyrim-era shader-flags base shared between [`BSSkyShaderProperty`] and
/// [`BSWaterShaderProperty`].
///
/// Per nif.xml lines 6695-6720, both blocks inherit `BSShaderProperty`
/// directly (no `texture_clamp_mode`, no `texture_set_ref`, no PP
/// trailer) and share an identical 4-field prefix on top of
/// `NiObjectNET`:
///
/// * `Shader Flags 1: SkyrimShaderPropertyFlags1`  (u32, BSVER < 132)
/// * `Shader Flags 2: SkyrimShaderPropertyFlags2`  (u32, BSVER < 132)
/// * `Num SF1: uint`  + `SF1: BSShaderCRC32 × Num SF1`  (BSVER >= 132)
/// * `Num SF2: uint`  + `SF2: BSShaderCRC32 × Num SF2`  (BSVER >= 152)
/// * `UV Offset: TexCoord` (2 × f32)
/// * `UV Scale: TexCoord`  (2 × f32)
///
/// Returned in the order `(flags1, flags2, sf1_crcs, sf2_crcs,
/// uv_offset, uv_scale)`. Pre-#713 both block types were aliased to
/// `BSShaderPPLightingProperty::parse`, which over-consumed 12-28 extra
/// bytes (`texture_clamp_mode + texture_set_ref + refraction +
/// parallax`) — the per-block tail (sky filename / sky type / water
/// flags) never reached the importer.
/// Shared Skyrim+ shader-property head: `(shader_flags_1, shader_flags_2,
/// sf1_crcs, sf2_crcs, uv_offset, uv_scale)`.
type SkyrimShaderBase = (u32, u32, Vec<u32>, Vec<u32>, [f32; 2], [f32; 2]);

fn parse_skyrim_shader_base(stream: &mut NifStream) -> io::Result<SkyrimShaderBase> {
    let bsver = stream.bsver();

    let (shader_flags_1, shader_flags_2) = if bsver < crate::version::bsver::FO4_CRC_FLAGS {
        (stream.read_u32_le()?, stream.read_u32_le()?)
    } else {
        (0, 0)
    };

    // Counts go through allocate_vec so a corrupt 0xFFFFFFFF can't OOM
    // before the inner u32 reads fail. See #764.
    let (sf1_crcs, sf2_crcs) = if bsver >= crate::version::bsver::FO4_CRC_FLAGS {
        // #981 — bulk-read CRC arrays via `read_u32_array`.
        let num_sf1 = stream.read_u32_le()? as usize;
        let num_sf2 = if bsver >= crate::version::bsver::FO76_SF2_CRCS {
            stream.read_u32_le()? as usize
        } else {
            0
        };
        let sf1 = stream.read_u32_array(num_sf1)?;
        let sf2 = stream.read_u32_array(num_sf2)?;
        (sf1, sf2)
    } else {
        (Vec::new(), Vec::new())
    };

    let uv_offset = [stream.read_f32_le()?, stream.read_f32_le()?];
    let uv_scale = [stream.read_f32_le()?, stream.read_f32_le()?];

    Ok((
        shader_flags_1,
        shader_flags_2,
        sf1_crcs,
        sf2_crcs,
        uv_offset,
        uv_scale,
    ))
}

/// `BSSkyShaderProperty` — Skyrim-era sky shader (nif.xml line 6708).
///
/// `versions="#SKY_AND_LATER#"`, `inherit="BSShaderProperty"` directly.
/// Carries the Skyrim shader-flags prefix (or BSVER >= 132 CRC arrays),
/// then `UV Offset / UV Scale`, then a per-block tail of
/// `Source Texture: SizedString` + `Sky Object Type: u32`.
///
/// Pre-#713 aliased to `BSShaderPPLightingProperty::parse` which read
/// the FO3 PP trailer — so the sky filename + object type never
/// reached the importer. Drift was masked by `block_sizes` recovery
/// (recurring "consumed N, expected M" warnings).
///
/// Distinct from FO3/FNV [`SkyShaderProperty`] which has its own
/// 6335-line entry — the FO3 variant inherits `BSShaderLightingProperty`
/// (carries `texture_clamp_mode`); the Skyrim variant does not.
#[derive(Debug)]
pub struct BSSkyShaderProperty {
    pub net: NiObjectNETData,
    pub shader_flags_1: u32,
    pub shader_flags_2: u32,
    /// CRC32-hashed shader flag list (BSVER >= 132). Replaces the u32
    /// pair from BSVER 132 onward — same `BSShaderCRC32` enum as on
    /// `BSLightingShaderProperty`.
    pub sf1_crcs: Vec<u32>,
    /// Second CRC32-hashed shader flag list (BSVER >= 152).
    pub sf2_crcs: Vec<u32>,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    /// Sky texture file path (clouds, stars, sun glare, moon, etc.).
    pub source_texture: String,
    /// Per nif.xml `SkyObjectType`: 0=Texture, 1=Sunglare, 2=Sky,
    /// 3=Clouds, 5=Stars, 7=Moon/Stars Mask. Selects which sky function
    /// this property fulfills at render time.
    pub sky_object_type: u32,
}

impl BSSkyShaderProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let (shader_flags_1, shader_flags_2, sf1_crcs, sf2_crcs, uv_offset, uv_scale) =
            parse_skyrim_shader_base(stream)?;
        let source_texture = stream.read_sized_string()?;
        let sky_object_type = stream.read_u32_le()?;
        Ok(Self {
            net,
            shader_flags_1,
            shader_flags_2,
            sf1_crcs,
            sf2_crcs,
            uv_offset,
            uv_scale,
            source_texture,
            sky_object_type,
        })
    }
}

/// `BSWaterShaderProperty` — Skyrim-era water shader (nif.xml line 6695).
///
/// `versions="#SKY_AND_LATER#"`, `inherit="BSShaderProperty"` directly.
/// Carries the Skyrim shader-flags prefix (or BSVER >= 132 CRC arrays),
/// then `UV Offset / UV Scale`, then a single u32
/// `Water Shader Flags: WaterShaderPropertyFlags`.
///
/// Distinct from FO3/FNV [`WaterShaderProperty`] (nif.xml line 6322) —
/// the FO3 variant carries no UV transform, no per-block tail, and a
/// shorter base. Pre-#713 the Skyrim variant was aliased to the FO3 PP
/// parser and over-consumed 24+ bytes; sky-side parser fix uses the
/// same shared base.
#[derive(Debug)]
pub struct BSWaterShaderProperty {
    pub net: NiObjectNETData,
    pub shader_flags_1: u32,
    pub shader_flags_2: u32,
    pub sf1_crcs: Vec<u32>,
    pub sf2_crcs: Vec<u32>,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    /// Water-specific flags per nif.xml `WaterShaderPropertyFlags`
    /// (line 6680). Bit-for-bit: 0=Specular, 1=Reflections, 2=Refractions,
    /// 3=Vertex_UV, 6=Reflections, 7=Refractions, 8=Vertex_UV,
    /// 9=Vertex_Alpha_Depth, 10=Procedural, 11=Fog, 12=Update_Constants,
    /// 13=Cubemap. Default `0xC4` per the spec — Reflections + Refractions
    /// + Cubemap.
    pub water_shader_flags: u32,
}

impl BSWaterShaderProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let (shader_flags_1, shader_flags_2, sf1_crcs, sf2_crcs, uv_offset, uv_scale) =
            parse_skyrim_shader_base(stream)?;
        let water_shader_flags = stream.read_u32_le()?;
        Ok(Self {
            net,
            shader_flags_1,
            shader_flags_2,
            sf1_crcs,
            sf2_crcs,
            uv_offset,
            uv_scale,
            water_shader_flags,
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

impl BSShaderTextureSet {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiObject base reads nothing for modern versions.
        //
        // `Num Textures` is a u32 per nif.xml. Previously we read it as
        // `i32` and clamped `.max(0) as u32`, which silently turned any
        // upstream stream drift that happened to land on a negative u32
        // pattern into an empty texture set — the block then continued
        // parsing from the wrong offset. Reading as u32 matches the spec
        // and lets `allocate_vec`'s budget guard (#388) catch absurd
        // lengths as a loud error, which in turn tells the outer
        // block_sizes recovery path to skip cleanly. See #459.
        let num_textures = stream.read_u32_le()?;
        let mut textures = stream.allocate_vec(num_textures)?;
        for _ in 0..num_textures {
            // Texture paths are always sized strings (u32 len + bytes),
            // NOT string table indices.
            textures.push(stream.read_sized_string()?);
        }

        Ok(Self { textures })
    }
}

/// Shader-type-specific trailing data for BSLightingShaderProperty.
///
/// After the common fields, BSLightingShaderProperty has 0–7 additional fields
/// determined by the `shader_type` value. These carry type-specific rendering
/// parameters (env map scale, skin tint, parallax, eye cubemap, etc.).
///
/// Note: Skyrim/FO4 uses `BSLightingShaderType` enum (type 5=SkinTint, 6=HairTint);
/// FO76 uses `BSShaderType155` enum (type 4=SkinTint as Color4, 5=HairTint as Color3).
#[derive(Debug, Clone)]
pub enum ShaderTypeData {
    /// Type 0 (Default), 2 (Glow), 3 (Parallax), 4 (Face Tint),
    /// 8–10 (Landscape), 12–13 (Tree/LOD), 15 (LOD HD), 17–19 (Cloud/Noise).
    None,
    /// Type 1 (Skyrim/FO4): Environment Map.
    EnvironmentMap { env_map_scale: f32 },
    /// Type 5 (Skyrim/FO4): Skin Tint (Color3).
    SkinTint { skin_tint_color: [f32; 3] },
    /// Type 6 (Skyrim/FO4): Hair Tint.
    HairTint { hair_tint_color: [f32; 3] },
    /// Type 7 (Skyrim/FO4): Parallax Occlusion.
    ParallaxOcc { max_passes: f32, scale: f32 },
    /// Type 11 (Skyrim/FO4): Multi-Layer Parallax.
    MultiLayerParallax {
        inner_layer_thickness: f32,
        refraction_scale: f32,
        inner_layer_texture_scale: [f32; 2],
        envmap_strength: f32,
    },
    /// Type 14 (Skyrim/FO4): Sparkle Snow.
    SparkleSnow { sparkle_parameters: [f32; 4] },
    /// Type 16 (Skyrim/FO4): Eye Environment Map.
    EyeEnvmap {
        eye_cubemap_scale: f32,
        left_eye_reflection_center: [f32; 3],
        right_eye_reflection_center: [f32; 3],
    },
    /// Type 4 (FO76 BSShaderType155): Skin Tint with alpha (Color4).
    Fo76SkinTint { skin_tint_color: [f32; 4] },
}

/// FO4+ wetness parameters (BSSPWetnessParams).
#[derive(Debug, Clone)]
pub struct WetnessParams {
    pub spec_scale: f32,
    pub spec_power: f32,
    pub min_var: f32,
    /// Only present for BSVER == 130 (not FO76+).
    pub env_map_scale: f32,
    pub fresnel_power: f32,
    pub metalness: f32,
    /// Present for BSVER >= 130 (FO4 + DLC + FO76 + Starfield). See
    /// #403 / FO4-D1-C1 — nif.xml gates on `#BS_GT_130#` but the vanilla
    /// FO4 ship stream (226k NIFs audited) carries the field from 130
    /// onward; widening to `>= 130` aligns every game.
    pub unknown_1: f32,
    /// Present for BSVER == 155 (FO76).
    pub unknown_2: f32,
}

/// FO76 luminance parameters (BSSPLuminanceParams).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LuminanceParams {
    pub lum_emittance: f32,
    pub exposure_offset: f32,
    pub final_exposure_min: f32,
    pub final_exposure_max: f32,
}

/// FO76 translucency parameters (BSSPTranslucencyParams).
#[derive(Debug, Clone, Default)]
pub struct TranslucencyParams {
    pub subsurface_color: [f32; 3],
    pub transmissive_scale: f32,
    pub turbulence: f32,
    pub thick_object: bool,
    pub mix_albedo: bool,
}

/// FO76 texture array entry (BSTextureArray).
#[derive(Debug, Clone, Default)]
pub struct BSTextureArray {
    pub textures: Vec<String>,
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
    /// Shader type. For BSVER 83–139 this is a BSLightingShaderType (Skyrim/FO4);
    /// for BSVER == 155 (FO76) this is a BSShaderType155 with different numeric mapping.
    pub shader_type: u32,
    pub net: NiObjectNETData,
    /// True if stopcond short-circuit fired: BSVER >= 155 and Name is a non-empty
    /// BGSM file path. Everything else is at defaults; the BGSM file holds the real data.
    pub material_reference: bool,
    pub shader_flags_1: u32,
    pub shader_flags_2: u32,
    /// CRC32-hashed shader flag list (BSVER >= 132). Replaces flag pair from BSVER 132 onward.
    pub sf1_crcs: Vec<u32>,
    /// Second CRC32-hashed shader flag list (BSVER >= 152).
    pub sf2_crcs: Vec<u32>,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub texture_set_ref: BlockRef,
    pub emissive_color: [f32; 3],
    pub emissive_multiple: f32,
    /// Root Material (NiFixedString, BSVER >= 130). Sidecar reference into
    /// the `.bgsm` / `.bgem` / `.mat` material file when the editor authored
    /// the material path here instead of via `net.name`. For Starfield this
    /// is the fallback source for `material_path` when the stopcond at
    /// `BSLightingShaderProperty::parse` did NOT fire (i.e. `net.name`
    /// carried a non-material editor label). #1183 / SF-D1-NEW-01.
    pub root_material_path: Option<Arc<str>>,
    pub texture_clamp_mode: u32,
    pub alpha: f32,
    pub refraction_strength: f32,
    pub glossiness: f32,
    pub specular_color: [f32; 3],
    pub specular_strength: f32,
    pub lighting_effect_1: f32,
    pub lighting_effect_2: f32,
    // ── FO4+ common fields (BSVER >= 130) ────────────────────────
    /// Subsurface rolloff (BSVER 130–139).
    pub subsurface_rolloff: f32,
    /// Rimlight power (BSVER 130–139).
    pub rimlight_power: f32,
    /// Backlight power (BSVER 130–139, only if rimlight < FLT_MAX).
    pub backlight_power: f32,
    /// Grayscale to palette scale (BSVER >= 130).
    pub grayscale_to_palette_scale: f32,
    /// Fresnel power (BSVER >= 130).
    pub fresnel_power: f32,
    /// Wetness parameters (BSVER >= 130).
    pub wetness: Option<WetnessParams>,
    // ── FO76 trailing fields (BSVER == 155) ──────────────────────
    /// Luminance parameters (BSVER == 155).
    pub luminance: Option<LuminanceParams>,
    /// Whether FO76 translucency is configured (BSVER == 155).
    pub do_translucency: bool,
    /// Translucency parameters (BSVER == 155, only if do_translucency).
    pub translucency: Option<TranslucencyParams>,
    /// Texture arrays (BSVER == 155).
    pub texture_arrays: Vec<BSTextureArray>,
    /// Shader-type-specific trailing fields (env map, skin tint, eye cubemap, etc.).
    pub shader_type_data: ShaderTypeData,
}

impl BSLightingShaderProperty {
    /// Construct a stub for the FO76+ stopcond short-circuit: when Name is a
    /// non-empty BGSM path, the block body is absent and all other fields stay
    /// at defaults. The BGSM file is parsed separately (out of scope for NIF parsing).
    fn material_reference_stub(net: NiObjectNETData) -> Self {
        Self {
            shader_type: 0,
            net,
            material_reference: true,
            shader_flags_1: 0,
            shader_flags_2: 0,
            sf1_crcs: Vec::new(),
            sf2_crcs: Vec::new(),
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            texture_set_ref: BlockRef::NULL,
            emissive_color: [0.0, 0.0, 0.0],
            emissive_multiple: 1.0,
            // Stopcond fired on `net.name`, which IS the material path —
            // the Root Material sidecar would carry redundant info at best
            // and never gets reached.
            root_material_path: None,
            // 3 = WRAP_S_WRAP_T — the most common Starfield default and safe
            // for the stopcond stub path. The authoritative value lives in the
            // companion .mat JSON file (SF-D6-03 / #762). When that parser
            // lands the asset_provider merge step should overwrite this field.
            texture_clamp_mode: 3,
            alpha: 1.0,
            refraction_strength: 0.0,
            glossiness: 1.0,
            specular_color: [1.0, 1.0, 1.0],
            specular_strength: 1.0,
            lighting_effect_1: 0.0,
            lighting_effect_2: 0.0,
            subsurface_rolloff: 0.0,
            rimlight_power: 0.0,
            backlight_power: 0.0,
            grayscale_to_palette_scale: 1.0,
            fresnel_power: 5.0,
            wetness: None,
            luminance: None,
            do_translucency: false,
            translucency: None,
            texture_arrays: Vec::new(),
            shader_type_data: ShaderTypeData::None,
        }
    }
}

impl BSLightingShaderProperty {
    /// Parse a `BSLightingShaderProperty` block, dispatching on the file's
    /// BSVER to the appropriate per-variant parser. Three variants split
    /// at the BSVER boundaries the format actually changes shape at:
    ///
    /// | Range | Parser | Distinguishing features |
    /// |---|---|---|
    /// | BSVER 83-129 | [`parse_skyrim`](Self::parse_skyrim) | u32 shader-flag pair, `lighting_effect_1/2`, no `root_material_path`, no wetness, no FO76 trailing |
    /// | BSVER 130-154 | [`parse_fo4`](Self::parse_fo4) | u32 pair at BSVER=130 only (gap at 131; CRC32 from 132); `root_material_path`; FO4 subsurface block 130-139; wetness; glossiness scale ×100 |
    /// | BSVER ≥ 155 | [`parse_fo76_plus`](Self::parse_fo76_plus) | shader-type comes AFTER name; material-reference stopcond; CRC32 flag arrays; FO76 luminance/translucency/texture-arrays; FO76 shader-type-data table |
    ///
    /// The three per-variant parsers are bit-for-bit equivalent to the
    /// corresponding slice of the pre-#1279 monolithic parse. The split
    /// is a code-organisation refactor — each parser reads top-to-bottom
    /// with no per-BSVER jumps into shared code paths, making the
    /// per-game wire format easy to reason about in isolation.
    ///
    /// **Verification contract**: any change to a per-variant parser must
    /// preserve the `parse_real_nifs --ignored` 100% recoverable rate on
    /// the matching game's real-archive corpus AND the per-game
    /// `m_kind%` / `metO%` fill-rates printed by the
    /// `translation_completeness --ignored` harness within ±2pp.
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let bsver = stream.bsver();
        if bsver >= crate::version::bsver::FO76 {
            Self::parse_fo76_plus(stream, bsver)
        } else if bsver >= crate::version::bsver::FALLOUT4 {
            Self::parse_fo4(stream, bsver)
        } else {
            Self::parse_skyrim(stream, bsver)
        }
    }

    /// Skyrim LE/SE parser (BSVER 83-129).
    ///
    /// - `legacy_shader_type` (u32) precedes `NiObjectNETData` per nif.xml
    ///   `onlyT="BSLightingShaderProperty"` BSVER 83-139.
    /// - u32 shader-flag pair (no CRC32, no stopcond).
    /// - `lighting_effect_1/2` present (Skyrim-only fields).
    /// - No `root_material_path`, no wetness, no FO4 subsurface block,
    ///   no FO76 luminance/translucency/texture-arrays.
    /// - `glossiness` stays raw (0-100 scale authored).
    /// - Shader-type-data dispatches through the legacy `BSLightingShaderType` enum.
    fn parse_skyrim(stream: &mut NifStream, _bsver: u32) -> io::Result<Self> {
        let shader_type = stream.read_u32_le()?;
        let net = NiObjectNETData::parse(stream)?;
        let shader_flags_1 = stream.read_u32_le()?;
        let shader_flags_2 = stream.read_u32_le()?;
        let uv_offset = [stream.read_f32_le()?, stream.read_f32_le()?];
        let uv_scale = [stream.read_f32_le()?, stream.read_f32_le()?];
        let texture_set_ref = stream.read_block_ref()?;
        let emissive_color = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let emissive_multiple = stream.read_f32_le()?;
        let texture_clamp_mode = stream.read_u32_le()?;
        let alpha = stream.read_f32_le()?;
        let refraction_strength = stream.read_f32_le()?;
        let glossiness = stream.read_f32_le()?;
        let specular_color = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let specular_strength = stream.read_f32_le()?;
        let lighting_effect_1 = stream.read_f32_le()?;
        let lighting_effect_2 = stream.read_f32_le()?;
        let shader_type_data = parse_shader_type_data(stream, shader_type)?;

        Ok(Self {
            shader_type,
            net,
            material_reference: false,
            shader_flags_1,
            shader_flags_2,
            sf1_crcs: Vec::new(),
            sf2_crcs: Vec::new(),
            uv_offset,
            uv_scale,
            texture_set_ref,
            emissive_color,
            emissive_multiple,
            root_material_path: None,
            texture_clamp_mode,
            alpha,
            refraction_strength,
            glossiness,
            specular_color,
            specular_strength,
            lighting_effect_1,
            lighting_effect_2,
            subsurface_rolloff: 0.0,
            rimlight_power: 0.0,
            backlight_power: 0.0,
            grayscale_to_palette_scale: 0.0,
            fresnel_power: 0.0,
            wetness: None,
            luminance: None,
            do_translucency: false,
            translucency: None,
            texture_arrays: Vec::new(),
            shader_type_data,
        })
    }

    /// Fallout 4 + dev-band parser (BSVER 130-154).
    ///
    /// - `legacy_shader_type` (u32) precedes `NiObjectNETData`.
    /// - **Shader-flag encoding splits within the range**:
    ///   - BSVER == 130: u32 pair (`shader_flags_1/2`), no CRC32.
    ///   - BSVER == 131 (`FO4_SHADER_GAP`): NEITHER — dev-stream 131 ships
    ///     no shader-flag fields at all. 34,995 FO4 vanilla NIFs parse
    ///     100% clean against this. See #409 / FO4-D1-H1.
    ///   - BSVER ≥ 132: CRC32 arrays (`sf1_crcs/sf2_crcs`); `num_sf2`
    ///     gated separately on BSVER ≥ 152 (never true in FO4 band but
    ///     left in to match the inline shape).
    /// - `root_material_path` always read (NiFixedString).
    /// - `glossiness` scaled ×100 to convert FO4 0-1 smoothness authoring
    ///   to the 0-100 glossiness convention every downstream consumer
    ///   expects. Without this, FO4 BSLightingShader materials whose
    ///   texture path doesn't keyword-match (e.g. Med-Tek polished
    ///   floors) fall through to the glossiness fallback with
    ///   `glossiness=0.8 → roughness=0.95`, killing direct specular and
    ///   the RT-reflection metalness/roughness gate.
    /// - FO4 subsurface block (`subsurface_rolloff`, `rimlight_power`,
    ///   `backlight_power`) ONLY in BSVER 130-139. `backlight_power`
    ///   present iff `rimlight_power >= 3.0e38` per nif.xml 6609 +
    ///   openmw `property.cpp:335` + nifly `Shaders.cpp:477`. See #1175.
    /// - `grayscale_to_palette_scale`, `fresnel_power`, wetness all read.
    /// - Wetness `unknown_2` is FO76-gated (`>= 155`) so always 0.0 here.
    /// - No `lighting_effect_1/2` (Skyrim-only).
    /// - No FO76 luminance/translucency/texture-arrays.
    fn parse_fo4(stream: &mut NifStream, bsver: u32) -> io::Result<Self> {
        let shader_type = stream.read_u32_le()?;
        let net = NiObjectNETData::parse(stream)?;

        let (shader_flags_1, shader_flags_2) = if bsver <= crate::version::bsver::FALLOUT4 {
            (stream.read_u32_le()?, stream.read_u32_le()?)
        } else {
            (0, 0)
        };
        let (sf1_crcs, sf2_crcs) = if bsver >= crate::version::bsver::FO4_CRC_FLAGS {
            let num_sf1 = stream.read_u32_le()? as usize;
            let num_sf2 = if bsver >= crate::version::bsver::FO76_SF2_CRCS {
                stream.read_u32_le()? as usize
            } else {
                0
            };
            let sf1 = stream.read_u32_array(num_sf1)?;
            let sf2 = stream.read_u32_array(num_sf2)?;
            (sf1, sf2)
        } else {
            (Vec::new(), Vec::new())
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
        let root_material_path = stream.read_string()?;
        let texture_clamp_mode = stream.read_u32_le()?;
        let alpha = stream.read_f32_le()?;
        let refraction_strength = stream.read_f32_le()?;
        let glossiness = stream.read_f32_le()? * 100.0;
        let specular_color = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let specular_strength = stream.read_f32_le()?;

        let (subsurface_rolloff, rimlight_power, backlight_power) = if (130..=139).contains(&bsver)
        {
            let sub = stream.read_f32_le()?;
            let rim = stream.read_f32_le()?;
            let back = if rim >= 3.0e38 && rim.is_finite() {
                stream.read_f32_le()?
            } else {
                0.0
            };
            (sub, rim, back)
        } else {
            (0.0, 0.0, 0.0)
        };

        let grayscale_to_palette_scale = stream.read_f32_le()?;
        let fresnel_power = stream.read_f32_le()?;
        let wetness = Some(Self::read_wetness_block(stream, bsver)?);
        let shader_type_data = parse_shader_type_data_fo4(stream, shader_type, bsver)?;

        Ok(Self {
            shader_type,
            net,
            material_reference: false,
            shader_flags_1,
            shader_flags_2,
            sf1_crcs,
            sf2_crcs,
            uv_offset,
            uv_scale,
            texture_set_ref,
            emissive_color,
            emissive_multiple,
            root_material_path,
            texture_clamp_mode,
            alpha,
            refraction_strength,
            glossiness,
            specular_color,
            specular_strength,
            lighting_effect_1: 0.0,
            lighting_effect_2: 0.0,
            subsurface_rolloff,
            rimlight_power,
            backlight_power,
            grayscale_to_palette_scale,
            fresnel_power,
            wetness,
            luminance: None,
            do_translucency: false,
            translucency: None,
            texture_arrays: Vec::new(),
            shader_type_data,
        })
    }

    /// Fallout 76 + Starfield parser (BSVER ≥ 155).
    ///
    /// - NO `legacy_shader_type` before name — the shader-type field
    ///   moves AFTER the flag arrays and uses the `BSShaderType155`
    ///   numeric enum. Starfield (BSVER 172+) reuses the FO76 enum.
    ///   Pre-#747 this gate used `==` and Starfield silently skipped
    ///   the field, mis-routing every character/hair/face shader
    ///   through the FO4 dispatch.
    /// - **Material-reference stopcond**: if `net.name` ends in
    ///   `.bgsm` / `.bgem` / `.mat`, the block body is absent — return
    ///   a stub via [`Self::material_reference_stub`] and let
    ///   `block_size` skip the padding. Pre-#749 the suffix gate was
    ///   "ANY non-empty Name", so every Starfield block with an editor
    ///   label like "Material_Slot_01" silently defaulted its PBR body
    ///   to zero (SF-D3-01).
    /// - CRC32 shader-flag arrays (`sf1_crcs/sf2_crcs`), `num_sf2`
    ///   gate is always true here (`>= 155 > 152`).
    /// - `root_material_path` always read.
    /// - `glossiness` scaled ×100 (FO76 follows FO4's smoothness convention).
    /// - No FO4 subsurface block (gated 130-139).
    /// - Wetness with BOTH `unknown_1` (BSVER >= 130, always true) and
    ///   `unknown_2` (BSVER >= 155, always true here) read.
    /// - FO76 luminance + translucency + texture-arrays trailing block.
    /// - Shader-type-data dispatches through `parse_shader_type_data_fo76`.
    fn parse_fo76_plus(stream: &mut NifStream, bsver: u32) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        if let Some(name) = net.name.as_deref() {
            // #1510 — FO76 keeps the suffix-aware check (#749: an
            // editor-labelled block with no `.mat`/`.bgsm` suffix carries
            // a full inline body, so it must NOT stub). Starfield material
            // references are content-hash paths with NO suffix
            // (`<hash>\<hash>`), so `is_material_reference` misses them and
            // the parser ran the full-body path into the 12-byte stub,
            // over-reading 8 B past EOF. In Starfield a full-body block
            // instead carries an EMPTY name, so `!name.is_empty()` is the
            // correct stub discriminator there — matching the a9c7bc9e
            // baseline (Starfield BSLSP 0 unknown).
            let is_ref = if bsver >= crate::version::bsver::STARFIELD {
                !name.is_empty()
            } else {
                is_material_reference(name)
            };
            if is_ref {
                return Ok(Self::material_reference_stub(net));
            }
        }

        // #1510 / NIF-NEW-05 — the FO76 `BSShaderType155` field lives
        // here for FO76 (bsver 152..171) but Starfield (bsver >= 172) does
        // NOT carry it (its shader_type is implicitly 0, like the legacy
        // pre-name slot). The #1279 `parse_fo76_plus` split read it
        // unconditionally for all bsver >= 155, shifting every later field
        // by 4 B and over-reading — which truncated all 1036 Starfield
        // BSLightingShaderProperty full-body blocks to NiUnknown. The
        // a9c7bc9e baseline (Starfield 0 unknown) gated it on `== 155`.
        let shader_type = if bsver < crate::version::bsver::STARFIELD {
            stream.read_u32_le()?
        } else {
            0
        };
        let num_sf1 = stream.read_u32_le()? as usize;
        let num_sf2 = stream.read_u32_le()? as usize;
        let sf1_crcs = stream.read_u32_array(num_sf1)?;
        let sf2_crcs = stream.read_u32_array(num_sf2)?;

        let uv_offset = [stream.read_f32_le()?, stream.read_f32_le()?];
        let uv_scale = [stream.read_f32_le()?, stream.read_f32_le()?];
        let texture_set_ref = stream.read_block_ref()?;
        let emissive_color = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let emissive_multiple = stream.read_f32_le()?;
        let root_material_path = stream.read_string()?;
        let texture_clamp_mode = stream.read_u32_le()?;
        let alpha = stream.read_f32_le()?;
        let refraction_strength = stream.read_f32_le()?;
        let glossiness = stream.read_f32_le()? * 100.0;
        let specular_color = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let specular_strength = stream.read_f32_le()?;

        let grayscale_to_palette_scale = stream.read_f32_le()?;
        let fresnel_power = stream.read_f32_le()?;
        let wetness = Some(Self::read_wetness_block(stream, bsver)?);

        // #1510 — the luminance / translucency / texture-array tail is
        // FO76-only (a9c7bc9e baseline gated it on `bsver == 155`).
        // Starfield (bsver >= 172) ends after the wetness block, so
        // reading these here over-ran the block into the NIF footer
        // (EOF → "failed to fill whole buffer") on every Starfield
        // BSLightingShaderProperty. Gate on the FO76 era.
        let mut luminance = None;
        let mut do_translucency = false;
        let mut translucency = None;
        let mut texture_arrays: Vec<BSTextureArray> = Vec::new();
        if bsver < crate::version::bsver::STARFIELD {
            luminance = Some(LuminanceParams {
                lum_emittance: stream.read_f32_le()?,
                exposure_offset: stream.read_f32_le()?,
                final_exposure_min: stream.read_f32_le()?,
                final_exposure_max: stream.read_f32_le()?,
            });

            do_translucency = stream.read_byte_bool()?;
            translucency = if do_translucency {
                Some(TranslucencyParams {
                    subsurface_color: [
                        stream.read_f32_le()?,
                        stream.read_f32_le()?,
                        stream.read_f32_le()?,
                    ],
                    transmissive_scale: stream.read_f32_le()?,
                    turbulence: stream.read_f32_le()?,
                    thick_object: stream.read_byte_bool()?,
                    mix_albedo: stream.read_byte_bool()?,
                })
            } else {
                None
            };

            let has_texture_arrays = stream.read_byte_bool()?;
            if has_texture_arrays {
                let num_arrays = stream.read_u32_le()?;
                texture_arrays = stream.allocate_vec(num_arrays)?;
                for _ in 0..num_arrays {
                    let width = stream.read_u32_le()?;
                    let mut textures = stream.allocate_vec(width)?;
                    for _ in 0..width {
                        textures.push(stream.read_sized_string()?);
                    }
                    texture_arrays.push(BSTextureArray { textures });
                }
            }
        }

        let shader_type_data = parse_shader_type_data_fo76(stream, shader_type)?;

        Ok(Self {
            shader_type,
            net,
            material_reference: false,
            shader_flags_1: 0,
            shader_flags_2: 0,
            sf1_crcs,
            sf2_crcs,
            uv_offset,
            uv_scale,
            texture_set_ref,
            emissive_color,
            emissive_multiple,
            root_material_path,
            texture_clamp_mode,
            alpha,
            refraction_strength,
            glossiness,
            specular_color,
            specular_strength,
            lighting_effect_1: 0.0,
            lighting_effect_2: 0.0,
            subsurface_rolloff: 0.0,
            rimlight_power: 0.0,
            backlight_power: 0.0,
            grayscale_to_palette_scale,
            fresnel_power,
            wetness,
            luminance,
            do_translucency,
            translucency,
            texture_arrays,
            shader_type_data,
        })
    }

    /// Shared wetness-block reader used by `parse_fo4` and
    /// `parse_fo76_plus`. The block shape is identical except for
    /// `unknown_2` (FO76+ only). `env_map_scale` is deliberately 0.0
    /// for both — per #1223, the wire field actually lives in
    /// `parse_shader_type_data_fo4`'s shader_type=1 trailing block,
    /// NOT in the wetness block; pre-#1223 reading it here caused a
    /// 4-byte over-read on every vanilla FO4 BSLSP at size=140.
    /// `unknown_1` is widened to `>= FALLOUT4` per #403 / FO4-D1-C1
    /// (2026-04-17 FO4 audit found 1.9M under-reads at BSVER=130
    /// from the original `>` gate).
    fn read_wetness_block(stream: &mut NifStream, bsver: u32) -> io::Result<WetnessParams> {
        let spec_scale = stream.read_f32_le()?;
        let spec_power = stream.read_f32_le()?;
        let min_var = stream.read_f32_le()?;
        let env_map_scale = 0.0f32;
        let fresnel_power = stream.read_f32_le()?;
        let metalness = stream.read_f32_le()?;
        let unknown_1 = stream.read_f32_le()?;
        // #1510 — `unknown_2` is FO76-only (a9c7bc9e baseline gated it
        // `bsver == 155`); Starfield (bsver >= 172) omits it. The old
        // `>= FO76` gate over-read 4 B on every Starfield BSLSP.
        let unknown_2 = if (crate::version::bsver::FO76..crate::version::bsver::STARFIELD)
            .contains(&bsver)
        {
            stream.read_f32_le()?
        } else {
            0.0
        };
        Ok(WetnessParams {
            spec_scale,
            spec_power,
            min_var,
            env_map_scale,
            fresnel_power,
            metalness,
            unknown_1,
            unknown_2,
        })
    }
}

/// Parse shader-type-specific trailing fields from BSLightingShaderProperty.
/// Called for Skyrim LE/SE (BSVER < 130) where these fields follow immediately
/// after lighting_effect_2 with no intervening common fields.
fn parse_shader_type_data(stream: &mut NifStream, shader_type: u32) -> io::Result<ShaderTypeData> {
    match shader_type {
        1 => {
            // Environment Map
            let env_map_scale = stream.read_f32_le()?;
            Ok(ShaderTypeData::EnvironmentMap { env_map_scale })
        }
        5 => {
            // Skin Tint
            let skin_tint_color = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            Ok(ShaderTypeData::SkinTint { skin_tint_color })
        }
        6 => {
            // Hair Tint
            let hair_tint_color = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            Ok(ShaderTypeData::HairTint { hair_tint_color })
        }
        7 => {
            // Parallax Occlusion
            let max_passes = stream.read_f32_le()?;
            let scale = stream.read_f32_le()?;
            Ok(ShaderTypeData::ParallaxOcc { max_passes, scale })
        }
        11 => {
            // Multi-Layer Parallax
            let inner_layer_thickness = stream.read_f32_le()?;
            let refraction_scale = stream.read_f32_le()?;
            let inner_layer_texture_scale = [stream.read_f32_le()?, stream.read_f32_le()?];
            let envmap_strength = stream.read_f32_le()?;
            Ok(ShaderTypeData::MultiLayerParallax {
                inner_layer_thickness,
                refraction_scale,
                inner_layer_texture_scale,
                envmap_strength,
            })
        }
        14 => {
            // Sparkle Snow
            let sparkle_parameters = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            Ok(ShaderTypeData::SparkleSnow { sparkle_parameters })
        }
        16 => {
            // Eye Environment Map
            let eye_cubemap_scale = stream.read_f32_le()?;
            let left_eye_reflection_center = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let right_eye_reflection_center = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            Ok(ShaderTypeData::EyeEnvmap {
                eye_cubemap_scale,
                left_eye_reflection_center,
                right_eye_reflection_center,
            })
        }
        // Types 0,2,3,4,8,9,10,12,13,15,17,18,19,20 have no trailing fields.
        _ => Ok(ShaderTypeData::None),
    }
}

/// Parse FO4+ shader-type-specific trailing fields.
/// Same types as Skyrim but type 1 (EnvironmentMap) adds two bools (BSVER 130–139)
/// and type 5 (SkinTint) adds a skin tint alpha float (BSVER 130–139).
fn parse_shader_type_data_fo4(
    stream: &mut NifStream,
    shader_type: u32,
    bsver: u32,
) -> io::Result<ShaderTypeData> {
    match shader_type {
        1 => {
            // nif.xml gates env_map_scale `#NI_BS_LTE_FO4#` = BSVER <= 139
            // (i.e. < FO4_DLC_UPPER); for BSVER 140–154 (a dead band, no
            // shipping game) the field is absent, so reading it
            // unconditionally over-read 4 bytes. Mirror the SSR-bool upper
            // bound below. Default to the neutral 1.0 multiplier when absent.
            // See #1552 / SK-D2-01.
            let env_map_scale = if bsver < crate::version::bsver::FO4_DLC_UPPER {
                stream.read_f32_le()?
            } else {
                1.0
            };
            // FO4-specific: SSR bools (BSVER 130–139).
            if (crate::version::bsver::FALLOUT4..crate::version::bsver::FO4_DLC_UPPER)
                .contains(&bsver)
            {
                let _use_ssr = stream.read_byte_bool()?;
                let _wetness_use_ssr = stream.read_byte_bool()?;
            }
            Ok(ShaderTypeData::EnvironmentMap { env_map_scale })
        }
        5 => {
            let skin_tint_color = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            // FO4-specific: skin tint alpha (BSVER 130–139).
            if (crate::version::bsver::FALLOUT4..crate::version::bsver::FO4_DLC_UPPER)
                .contains(&bsver)
            {
                let _skin_tint_alpha = stream.read_f32_le()?;
            }
            Ok(ShaderTypeData::SkinTint { skin_tint_color })
        }
        // All other types same as Skyrim.
        6 => {
            let hair_tint_color = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            Ok(ShaderTypeData::HairTint { hair_tint_color })
        }
        7 => {
            let max_passes = stream.read_f32_le()?;
            let scale = stream.read_f32_le()?;
            Ok(ShaderTypeData::ParallaxOcc { max_passes, scale })
        }
        11 => {
            let inner_layer_thickness = stream.read_f32_le()?;
            let refraction_scale = stream.read_f32_le()?;
            let inner_layer_texture_scale = [stream.read_f32_le()?, stream.read_f32_le()?];
            let envmap_strength = stream.read_f32_le()?;
            Ok(ShaderTypeData::MultiLayerParallax {
                inner_layer_thickness,
                refraction_scale,
                inner_layer_texture_scale,
                envmap_strength,
            })
        }
        14 => {
            let sparkle_parameters = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            Ok(ShaderTypeData::SparkleSnow { sparkle_parameters })
        }
        16 => {
            let eye_cubemap_scale = stream.read_f32_le()?;
            let left_eye_reflection_center = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let right_eye_reflection_center = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            Ok(ShaderTypeData::EyeEnvmap {
                eye_cubemap_scale,
                left_eye_reflection_center,
                right_eye_reflection_center,
            })
        }
        _ => Ok(ShaderTypeData::None),
    }
}

/// Parse FO76 (BSVER == 155) shader-type-specific trailing fields.
/// The BSShaderType155 enum has a different numeric mapping than BSLightingShaderType:
/// 4 = Skin Tint (Color4), 5 = Hair Tint (Color3). Other types have no trailing data.
fn parse_shader_type_data_fo76(
    stream: &mut NifStream,
    shader_type: u32,
) -> io::Result<ShaderTypeData> {
    match shader_type {
        4 => {
            // Skin Tint (Color4 — includes alpha).
            let skin_tint_color = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            Ok(ShaderTypeData::Fo76SkinTint { skin_tint_color })
        }
        5 => {
            // Hair Tint (Color3).
            let hair_tint_color = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            Ok(ShaderTypeData::HairTint { hair_tint_color })
        }
        // 0 Default, 2 Glow, 3 Face Tint, 12 Eye Envmap, 17 Terrain — no trailing.
        //
        // Verified against nif.xml `BSShaderType155` (the FO76 enum) at
        // /mnt/data/src/reference/nifxml/nif.xml:1425-1434: it admits only
        // values {0, 2, 3, 4, 5, 12, 17}. The Skyrim/FO4 Eye-Envmap payload
        // (Eye Cubemap Scale + Left/Right reflection centers) is gated in
        // BSLightingShaderProperty on `Shader Type == 16` (nif.xml:6634-6636),
        // which `BSShaderType155` cannot produce — so FO76 eye meshes carry
        // no trailing bytes here. See #623 / SK-D3-06.
        _ => Ok(ShaderTypeData::None),
    }
}

/// BSEffectShaderProperty — Skyrim+ effect/VFX shader.
///
/// Unlike BSLightingShaderProperty, this shader embeds a source texture
/// filename as a sized string rather than referencing a BSShaderTextureSet.
#[derive(Debug)]
pub struct BSEffectShaderProperty {
    pub net: NiObjectNETData,
    /// True if stopcond short-circuit fired: BSVER >= 155 and Name is a non-empty
    /// BGEM file path. Other fields are at defaults.
    pub material_reference: bool,
    pub shader_flags_1: u32,
    pub shader_flags_2: u32,
    /// CRC32-hashed shader flag list (BSVER >= 132).
    pub sf1_crcs: Vec<u32>,
    /// Second CRC32-hashed shader flag list (BSVER >= 152).
    pub sf2_crcs: Vec<u32>,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub source_texture: String,
    pub texture_clamp_mode: u8,
    pub lighting_influence: u8,
    pub env_map_min_lod: u8,
    pub falloff_start_angle: f32,
    pub falloff_stop_angle: f32,
    pub falloff_start_opacity: f32,
    pub falloff_stop_opacity: f32,
    /// FO76+ refraction power (BSVER >= 155 — fixed in #746 to
    /// also pick up the Starfield 168/172 streams).
    pub refraction_power: f32,
    /// Base color (Color4) — multiplicative diffuse tint applied on
    /// top of the source texture sample. Pre-#166 this was called
    /// `emissive_color`, a holdover from an early nif.xml misread
    /// that conflated BSLightingShader's emissive slot with
    /// BSEffect's base-color slot. Per nif.xml `BSEffectShaderProperty`,
    /// byte offsets align with `emissive_color` — this is a
    /// semantic-name fix only, not a parse layout change.
    /// Downstream consumers in `import/material.rs` and
    /// `import/mesh.rs` still map it into [`MaterialInfo::emissive_color`]
    /// because the effect shader's visible "glow" is driven by
    /// `base_color * base_color_scale` with the current fragment
    /// shader path — a proper diffuse-tint remapping is downstream
    /// work once the effect shader gets its own render path.
    pub base_color: [f32; 4],
    /// Base color scale — scalar multiplier for `base_color`.
    /// Renamed from `emissive_multiple` alongside `base_color` (#166).
    pub base_color_scale: f32,
    pub soft_falloff_depth: f32,
    pub greyscale_texture: String,
    /// Environment map texture path (FO4+ only, BSVER >= 130).
    pub env_map_texture: String,
    /// Normal texture path (FO4+ only, BSVER >= 130).
    pub normal_texture: String,
    /// Environment mask texture path (FO4+ only, BSVER >= 130).
    pub env_mask_texture: String,
    /// Environment map scale (FO4+ only, BSVER >= 130).
    pub env_map_scale: f32,
    /// FO76 reflectance texture (BSVER == 155).
    pub reflectance_texture: String,
    /// FO76 lighting texture (BSVER == 155).
    pub lighting_texture: String,
    /// FO76 emittance color (BSVER == 155).
    pub emittance_color: [f32; 3],
    /// FO76 emit gradient texture (BSVER == 155).
    pub emit_gradient_texture: String,
    /// FO76 luminance params (BSVER == 155).
    pub luminance: Option<LuminanceParams>,
}

impl BSEffectShaderProperty {
    fn material_reference_stub(net: NiObjectNETData) -> Self {
        Self {
            net,
            material_reference: true,
            shader_flags_1: 0,
            shader_flags_2: 0,
            sf1_crcs: Vec::new(),
            sf2_crcs: Vec::new(),
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            source_texture: String::new(),
            texture_clamp_mode: 3,
            lighting_influence: 0,
            env_map_min_lod: 0,
            falloff_start_angle: 1.0,
            falloff_stop_angle: 1.0,
            falloff_start_opacity: 0.0,
            falloff_stop_opacity: 0.0,
            refraction_power: 0.0,
            base_color: [1.0, 1.0, 1.0, 1.0],
            base_color_scale: 1.0,
            soft_falloff_depth: 100.0,
            greyscale_texture: String::new(),
            env_map_texture: String::new(),
            normal_texture: String::new(),
            env_mask_texture: String::new(),
            env_map_scale: 1.0,
            reflectance_texture: String::new(),
            lighting_texture: String::new(),
            emittance_color: [0.0, 0.0, 0.0],
            emit_gradient_texture: String::new(),
            luminance: None,
        }
    }
}

impl BSEffectShaderProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let bsver = stream.bsver();
        let net = NiObjectNETData::parse(stream)?;

        // FO76+ stopcond: Name is an external `.bgem` / `.mat` material-file
        // reference (sibling of the BSLightingShaderProperty gate above).
        // The suffix-aware test ensures editor labels with no path suffix
        // continue through to the full body parse — see #749 / SF-D3-01.
        if bsver >= crate::version::bsver::FO76 {
            if let Some(name) = net.name.as_deref() {
                if is_material_reference(name) {
                    return Ok(Self::material_reference_stub(net));
                }
            }
        }

        // Shader flags 1/2 — see sibling gate in
        // `BSLightingShaderProperty::parse` for the full nif.xml
        // citation. `bsver == crate::version::bsver::FO4_SHADER_GAP` is an intentional gap: neither the
        // u32 pair nor the BSVER >= 132 CRC arrays are present. #409.
        let (shader_flags_1, shader_flags_2) = if bsver <= crate::version::bsver::FALLOUT4 {
            (stream.read_u32_le()?, stream.read_u32_le()?)
        } else {
            (0, 0)
        };

        // #981 — bulk-read CRC arrays via `read_u32_array`; same
        // byte-budget guarantee as the BSEffectShaderData variant above.
        let (sf1_crcs, sf2_crcs) = if bsver >= crate::version::bsver::FO4_CRC_FLAGS {
            let num_sf1 = stream.read_u32_le()? as usize;
            let num_sf2 = if bsver >= crate::version::bsver::FO76_SF2_CRCS {
                stream.read_u32_le()? as usize
            } else {
                0
            };
            let sf1 = stream.read_u32_array(num_sf1)?;
            let sf2 = stream.read_u32_array(num_sf2)?;
            (sf1, sf2)
        } else {
            (Vec::new(), Vec::new())
        };

        let uv_offset = [stream.read_f32_le()?, stream.read_f32_le()?];
        let uv_scale = [stream.read_f32_le()?, stream.read_f32_le()?];

        // Source texture as sized string (NOT a texture set reference).
        let source_texture = stream.read_sized_string()?;

        // 4 bytes packed: texture_clamp_mode(u8), lighting_influence(u8),
        // env_map_min_lod(u8), unused(u8).
        let texture_clamp_mode = stream.read_u8()?;
        let lighting_influence = stream.read_u8()?;
        let env_map_min_lod = stream.read_u8()?;
        let _unused = stream.read_u8()?;

        let falloff_start_angle = stream.read_f32_le()?;
        let falloff_stop_angle = stream.read_f32_le()?;
        let falloff_start_opacity = stream.read_f32_le()?;
        let falloff_stop_opacity = stream.read_f32_le()?;

        // FO76+ refraction power. #746 / SF-D1-04 — nif.xml gates
        // this on `BSVER #GTE# 155`. Pre-fix the parser used `==`
        // and every Starfield (`bsver = 172`) BSEffect block under-
        // read by 4 B, drifting the rest of the block. See #746.
        let refraction_power = if bsver >= crate::version::bsver::FO76 {
            stream.read_f32_le()?
        } else {
            0.0
        };

        // Per nif.xml `BSEffectShaderProperty`, these fields are
        // Base Color (Color4) + Base Color Scale (float) — NOT
        // emissive. BSEffect's visible "glow" comes from the base
        // color multiplied by the base-color-scale tint over the
        // source texture. Pre-#166 these were named emissive_* and
        // material.rs folded them into MaterialInfo.emissive_*;
        // byte layout identical so downstream behavior unchanged.
        let base_color = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let base_color_scale = stream.read_f32_le()?;

        // Soft falloff depth — present in all versions.
        let soft_falloff_depth = stream.read_f32_le()?;

        // Greyscale texture — sized string, present in all versions.
        let greyscale_texture = stream.read_sized_string()?;

        // FO4+ additional textures (BSVER >= 130).
        let (env_map_texture, normal_texture, env_mask_texture, env_map_scale) =
            if bsver >= crate::version::bsver::FALLOUT4 {
                let env = stream.read_sized_string()?;
                let norm = stream.read_sized_string()?;
                let mask = stream.read_sized_string()?;
                let scale = stream.read_f32_le()?;
                (env, norm, mask, scale)
            } else {
                (String::new(), String::new(), String::new(), 0.0)
            };

        // FO76+ trailing fields. #746 / SF-D1-04 — same value-gate
        // regression as `refraction_power` and the BLSP tail. nif.xml
        // gates this block on `BSVER #GTE# 155`; pre-fix the parser
        // used `==` and Starfield (`bsver = 172`) BSEffect blocks
        // under-read by ≥40 B + 4 sized strings.
        let mut reflectance_texture = String::new();
        let mut lighting_texture = String::new();
        let mut emittance_color = [0.0f32; 3];
        let mut emit_gradient_texture = String::new();
        let mut luminance = None;
        if bsver >= crate::version::bsver::FO76 {
            reflectance_texture = stream.read_sized_string()?;
            lighting_texture = stream.read_sized_string()?;
            emittance_color = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            emit_gradient_texture = stream.read_sized_string()?;
            luminance = Some(LuminanceParams {
                lum_emittance: stream.read_f32_le()?,
                exposure_offset: stream.read_f32_le()?,
                final_exposure_min: stream.read_f32_le()?,
                final_exposure_max: stream.read_f32_le()?,
            });
        }

        Ok(Self {
            net,
            material_reference: false,
            shader_flags_1,
            shader_flags_2,
            sf1_crcs,
            sf2_crcs,
            uv_offset,
            uv_scale,
            source_texture,
            texture_clamp_mode,
            lighting_influence,
            env_map_min_lod,
            falloff_start_angle,
            falloff_stop_angle,
            falloff_start_opacity,
            falloff_stop_opacity,
            refraction_power,
            base_color,
            base_color_scale,
            soft_falloff_depth,
            greyscale_texture,
            env_map_texture,
            normal_texture,
            env_mask_texture,
            env_map_scale,
            reflectance_texture,
            lighting_texture,
            emittance_color,
            emit_gradient_texture,
            luminance,
        })
    }
}

impl_ni_object!(
    BSShaderPPLightingProperty,
    BSShaderNoLightingProperty,
    TileShaderProperty,
    SkyShaderProperty,
    WaterShaderProperty,
    TallGrassShaderProperty,
    BSSkyShaderProperty,
    BSWaterShaderProperty,
    BSShaderTextureSet,
    BSLightingShaderProperty,
    BSEffectShaderProperty,
);

#[cfg(test)]
#[path = "shader_tests.rs"]
mod tests;
