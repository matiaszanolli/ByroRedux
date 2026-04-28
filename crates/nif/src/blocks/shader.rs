//! Bethesda shader property blocks.
//!
//! - BSShaderPPLightingProperty / BSShaderNoLightingProperty — Fallout 3/NV
//! - BSLightingShaderProperty / BSEffectShaderProperty — Skyrim+
//! - BSShaderTextureSet — shared texture path list (all games)

use super::base::{BSShaderPropertyData, NiObjectNETData};
use super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use std::any::Any;
use std::io;

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
    /// Refraction strength (0.0–1.0). Present when bsver >= 15.
    pub refraction_strength: f32,
    /// Refraction fire period. Present when bsver >= 15.
    pub refraction_fire_period: i32,
    /// Parallax max passes. Present when bsver >= 24.
    pub parallax_max_passes: f32,
    /// Parallax scale. Present when bsver >= 24.
    pub parallax_scale: f32,
    /// Emissive glow color (RGBA). nif.xml: "Emissive Color" vercond="#BS_GT_FO3#" (bsver > 34).
    /// Defaults to black/opaque when absent (FO3/FNV bsver <= 34).
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
        let net = NiObjectNETData::parse(stream)?;
        let (shader, texture_clamp_mode) = BSShaderPropertyData::parse_fo3(stream)?;
        let texture_set_ref = stream.read_block_ref()?;

        // nif.xml: Refraction Strength (f32) + Refraction Fire Period (i32) for bsver >= 15.
        let bsver = stream.bsver();
        let (refraction_strength, refraction_fire_period) = if bsver >= 15 {
            (stream.read_f32_le()?, stream.read_i32_le()?)
        } else {
            (0.0, 0)
        };

        // nif.xml: Parallax Max Passes (f32) + Parallax Scale (f32) for bsver >= 24.
        let (parallax_max_passes, parallax_scale) = if bsver >= 24 {
            (stream.read_f32_le()?, stream.read_f32_le()?)
        } else {
            (4.0, 1.0)
        };

        // nif.xml:6250 — "Emissive Color" Color4 vercond="#BS_GT_FO3#" (i.e. bsver > 34).
        // FO3/FNV (bsver <= 34) do not carry this field; Skyrim-era PPLighting does.
        let emissive_color = if bsver > 34 {
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

impl NiObject for BSShaderNoLightingProperty {
    fn block_type_name(&self) -> &'static str {
        "BSShaderNoLightingProperty"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BSShaderNoLightingProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let (shader, texture_clamp_mode) = BSShaderPropertyData::parse_fo3(stream)?;
        let file_name = stream.read_sized_string()?;

        let (falloff_start_angle, falloff_stop_angle, falloff_start_opacity, falloff_stop_opacity) =
            if stream.bsver() > 26 {
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

impl NiObject for TileShaderProperty {
    fn block_type_name(&self) -> &'static str {
        "TileShaderProperty"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
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

impl NiObject for SkyShaderProperty {
    fn block_type_name(&self) -> &'static str {
        "SkyShaderProperty"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
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

impl NiObject for WaterShaderProperty {
    fn block_type_name(&self) -> &'static str {
        "WaterShaderProperty"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl WaterShaderProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let shader = BSShaderPropertyData::parse_base(stream)?;
        Ok(Self { net, shader })
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

impl NiObject for TallGrassShaderProperty {
    fn block_type_name(&self) -> &'static str {
        "TallGrassShaderProperty"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
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
fn parse_skyrim_shader_base(
    stream: &mut NifStream,
) -> io::Result<(u32, u32, Vec<u32>, Vec<u32>, [f32; 2], [f32; 2])> {
    let bsver = stream.bsver();

    let (shader_flags_1, shader_flags_2) = if bsver < 132 {
        (stream.read_u32_le()?, stream.read_u32_le()?)
    } else {
        (0, 0)
    };

    let mut sf1_crcs = Vec::new();
    let mut sf2_crcs = Vec::new();
    if bsver >= 132 {
        let num_sf1 = stream.read_u32_le()? as usize;
        let num_sf2 = if bsver >= 152 {
            stream.read_u32_le()? as usize
        } else {
            0
        };
        sf1_crcs.reserve(num_sf1);
        for _ in 0..num_sf1 {
            sf1_crcs.push(stream.read_u32_le()?);
        }
        sf2_crcs.reserve(num_sf2);
        for _ in 0..num_sf2 {
            sf2_crcs.push(stream.read_u32_le()?);
        }
    }

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

impl NiObject for BSSkyShaderProperty {
    fn block_type_name(&self) -> &'static str {
        "BSSkyShaderProperty"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
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

impl NiObject for BSWaterShaderProperty {
    fn block_type_name(&self) -> &'static str {
        "BSWaterShaderProperty"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
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
#[derive(Debug, Clone, Default)]
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

impl NiObject for BSLightingShaderProperty {
    fn block_type_name(&self) -> &'static str {
        "BSLightingShaderProperty"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BSLightingShaderProperty {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let bsver = stream.bsver();

        // NiObjectNET: shader type comes BEFORE name for BSLightingShaderProperty on
        // Skyrim/FO4 (nif.xml onlyT="BSLightingShaderProperty", BSVER 83-139, enum
        // BSLightingShaderType). For FO76+ the shader type is part of the niobject
        // body and typed as BSShaderType155.
        let legacy_shader_type = if (83..=139).contains(&bsver) {
            stream.read_u32_le()?
        } else {
            0
        };

        let net = NiObjectNETData::parse(stream)?;

        // FO76+ stopcond: if Name is a `.bgsm` / `.bgem` / `.mat` material-
        // file reference, the rest of the block is absent (the material
        // file holds the real PBR data). Return a stub and let block_size
        // skip any trailing padding. The suffix gate is critical — pre-
        // #749 this fired on ANY non-empty Name, so every Starfield block
        // with an editor label (e.g. "Material_Slot_01") had its entire
        // PBR body silently defaulted to zero. See SF-D3-01.
        if bsver >= 155 {
            if let Some(name) = net.name.as_deref() {
                if is_material_reference(name) {
                    return Ok(Self::material_reference_stub(net));
                }
            }
        }

        // Shader flags 1/2 — per nif.xml (`#NI_BS_LT_FO4#` =
        // `BSVER < 130` for the "SK" suffix variant; `#BS_FO4#` =
        // `BSVER == 130` strictly for the "FO4" suffix variant). Gate
        // is `bsver <= 130`. At `bsver == 131` the pair is intentionally
        // absent per the spec — dev-stream 131 ships no shader-flag
        // fields at all (neither the Skyrim u32 pair nor the BSVER >=
        // 132 CRC32 arrays). 34,995 FO4 vanilla NIFs parse 100% clean
        // against this gate shape; FO4-D1-H1 (#409) confirmed the gap
        // is correct against nif.xml after initial concern that BSVER
        // 131 would misalign.
        let (shader_flags_1, shader_flags_2) = if bsver <= 130 {
            (stream.read_u32_le()?, stream.read_u32_le()?)
        } else {
            (0, 0)
        };

        // FO76+ BSShaderType155 field. nif.xml gates this on
        // `BSVER #GTE# 155`; pre-#747 the parser used `==` and
        // Starfield (`bsver = 172` per `version.rs:129`) silently
        // skipped, drifting every subsequent block read by 4 bytes
        // and mis-routing the shader-type dispatch through the FO4
        // table at `:990`. SF-D1-DISPATCH.
        let fo76_shader_type = if bsver >= 155 {
            stream.read_u32_le()?
        } else {
            0
        };

        // Num SF1 / Num SF2 (BSVER >= 132 / 152), then both arrays.
        let mut sf1_crcs = Vec::new();
        let mut sf2_crcs = Vec::new();
        if bsver >= 132 {
            let num_sf1 = stream.read_u32_le()? as usize;
            let num_sf2 = if bsver >= 152 {
                stream.read_u32_le()? as usize
            } else {
                0
            };
            sf1_crcs.reserve(num_sf1);
            for _ in 0..num_sf1 {
                sf1_crcs.push(stream.read_u32_le()?);
            }
            sf2_crcs.reserve(num_sf2);
            for _ in 0..num_sf2 {
                sf2_crcs.push(stream.read_u32_le()?);
            }
        }

        // Effective shader type for the downstream dispatch (uses
        // different enums depending on version). #747 / SF-D1-DISPATCH
        // — Starfield reuses the FO76 BSShaderType155 numeric mapping
        // (type 4 = skin tint Color4, type 5 = hair tint Color3 per
        // nif.xml), so the gate is `>= 155`, not `== 155`. Pre-fix
        // Starfield character / hair / face meshes routed through the
        // FO4 dispatch which mis-interprets the type-4/5 payload and
        // drops 12 B of tint data.
        let shader_type = if bsver >= 155 {
            fo76_shader_type
        } else {
            legacy_shader_type
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

        // Root Material (NiFixedString) — FO4+ only (BSVER >= 130).
        if bsver >= 130 {
            let _root_material = stream.read_string()?;
        }

        let texture_clamp_mode = stream.read_u32_le()?;
        let alpha = stream.read_f32_le()?;
        let refraction_strength = stream.read_f32_le()?;

        // Glossiness (Skyrim) or Smoothness (FO4+).
        let glossiness = stream.read_f32_le()?;

        let specular_color = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let specular_strength = stream.read_f32_le()?;

        // Lighting effects — Skyrim only (BSVER < 130).
        let (lighting_effect_1, lighting_effect_2) = if bsver < 130 {
            (stream.read_f32_le()?, stream.read_f32_le()?)
        } else {
            (0.0, 0.0)
        };

        // FO4-only common fields (BS_FO4_2 = BSVER 130–139).
        let (subsurface_rolloff, rimlight_power, backlight_power) = if (130..=139).contains(&bsver)
        {
            let sub = stream.read_f32_le()?;
            let rim = stream.read_f32_le()?;
            // Backlight only present if rimlight is not the FLT_MAX sentinel.
            // Use 3.0e38 threshold (below 3.4028235e38) to handle float precision.
            let back = if rim < 3.0e38 {
                stream.read_f32_le()?
            } else {
                0.0
            };
            (sub, rim, back)
        } else {
            (0.0, 0.0, 0.0)
        };

        let grayscale_to_palette_scale = if bsver >= 130 {
            stream.read_f32_le()?
        } else {
            0.0
        };

        let fresnel_power = if bsver >= 130 {
            stream.read_f32_le()?
        } else {
            0.0
        };

        let wetness = if bsver >= 130 {
            let spec_scale = stream.read_f32_le()?;
            let spec_power = stream.read_f32_le()?;
            let min_var = stream.read_f32_le()?;
            let env_map_scale = if bsver == 130 {
                stream.read_f32_le()?
            } else {
                0.0
            };
            let fresnel = stream.read_f32_le()?;
            let metalness = stream.read_f32_le()?;
            // `Unknown 1` is nominally gated on `#BS_GT_130#` per nif.xml,
            // but the 2026-04-17 FO4 audit (Dim 1 C-1) swept all 8 FO4
            // main + DLC mesh archives (226k NIFs) and found 1,876,931
            // four-byte under-reads on `BSLightingShaderProperty`,
            // every single one at BSVER=130 (the vanilla FO4 ship
            // stream). The field is present from BSVER=130 onward —
            // widen the gate to `>= 130` so the whole wetness tail
            // aligns on FO4 (130), FO4 DLC (131-139), FO76 (155), and
            // Starfield (168+). See #403 / FO4-D1-C1.
            let unknown_1 = if bsver >= 130 {
                stream.read_f32_le()?
            } else {
                0.0
            };
            // #746 / SF-D1-02 — nif.xml gates `Unknown 2` on
            // `BSVER #GTE# 155`. Pre-fix the parser used `==` and
            // every Starfield (`bsver = 172`) WetnessParams under-
            // read by 4 bytes, drifting the rest of the block.
            let unknown_2 = if bsver >= 155 {
                stream.read_f32_le()?
            } else {
                0.0
            };
            Some(WetnessParams {
                spec_scale,
                spec_power,
                min_var,
                env_map_scale,
                fresnel_power: fresnel,
                metalness,
                unknown_1,
                unknown_2,
            })
        } else {
            None
        };

        // FO76+ (BSVER >= 155) trailing fields. #746 / SF-D1-01 —
        // nif.xml gates the LuminanceParams + TranslucencyParams +
        // texture_arrays block on `BSVER #GTE# 155`. Pre-fix the
        // parser used `==` and every Starfield (`bsver = 172`)
        // BLSP under-read by ~24+22+variable bytes, leaving every
        // subsequent block to drift by tens of bytes (block_size
        // skip recovered the cell load but the tail-field captures
        // ended up zeroed).
        let mut luminance = None;
        let mut do_translucency = false;
        let mut translucency = None;
        let mut texture_arrays: Vec<BSTextureArray> = Vec::new();
        if bsver >= 155 {
            luminance = Some(LuminanceParams {
                lum_emittance: stream.read_f32_le()?,
                exposure_offset: stream.read_f32_le()?,
                final_exposure_min: stream.read_f32_le()?,
                final_exposure_max: stream.read_f32_le()?,
            });

            do_translucency = stream.read_byte_bool()?;
            if do_translucency {
                translucency = Some(TranslucencyParams {
                    subsurface_color: [
                        stream.read_f32_le()?,
                        stream.read_f32_le()?,
                        stream.read_f32_le()?,
                    ],
                    transmissive_scale: stream.read_f32_le()?,
                    turbulence: stream.read_f32_le()?,
                    thick_object: stream.read_byte_bool()?,
                    mix_albedo: stream.read_byte_bool()?,
                });
            }

            let has_texture_arrays = stream.read_u8()? != 0;
            if has_texture_arrays {
                // #408 — preferred allocate_vec for both the outer
                // count and per-array width; both are file-driven u32s.
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

        // Shader-type-specific trailing fields. For FO76+ (BSVER >=
        // 155) these use the BSShaderType155 numeric mapping (type 4
        // = skin tint Color4, type 5 = hair tint Color3 per nif.xml).
        // Starfield (`bsver = 172`) reuses the same enum, so the gate
        // is `>= 155` not `== 155`. Pre-#747 Starfield character /
        // hair / face meshes routed through the FO4 dispatch which
        // mis-interpreted the type-4/5 payload and dropped 12 B of
        // tint data.
        let shader_type_data = if bsver >= 155 {
            parse_shader_type_data_fo76(stream, shader_type)?
        } else if bsver < 130 {
            parse_shader_type_data(stream, shader_type)?
        } else {
            parse_shader_type_data_fo4(stream, shader_type, bsver)?
        };

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
            texture_clamp_mode,
            alpha,
            refraction_strength,
            glossiness,
            specular_color,
            specular_strength,
            lighting_effect_1,
            lighting_effect_2,
            subsurface_rolloff,
            rimlight_power,
            backlight_power,
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
            let env_map_scale = stream.read_f32_le()?;
            // FO4-specific: SSR bools (BSVER 130–139).
            if bsver >= 130 && bsver < 140 {
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
            if bsver >= 130 && bsver < 140 {
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

impl NiObject for BSEffectShaderProperty {
    fn block_type_name(&self) -> &'static str {
        "BSEffectShaderProperty"
    }

    fn as_any(&self) -> &dyn Any {
        self
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
        if bsver >= 155 {
            if let Some(name) = net.name.as_deref() {
                if is_material_reference(name) {
                    return Ok(Self::material_reference_stub(net));
                }
            }
        }

        // Shader flags 1/2 — see sibling gate in
        // `BSLightingShaderProperty::parse` for the full nif.xml
        // citation. `bsver == 131` is an intentional gap: neither the
        // u32 pair nor the BSVER >= 132 CRC arrays are present. #409.
        let (shader_flags_1, shader_flags_2) = if bsver <= 130 {
            (stream.read_u32_le()?, stream.read_u32_le()?)
        } else {
            (0, 0)
        };

        let mut sf1_crcs = Vec::new();
        let mut sf2_crcs = Vec::new();
        if bsver >= 132 {
            let num_sf1 = stream.read_u32_le()? as usize;
            let num_sf2 = if bsver >= 152 {
                stream.read_u32_le()? as usize
            } else {
                0
            };
            sf1_crcs.reserve(num_sf1);
            for _ in 0..num_sf1 {
                sf1_crcs.push(stream.read_u32_le()?);
            }
            sf2_crcs.reserve(num_sf2);
            for _ in 0..num_sf2 {
                sf2_crcs.push(stream.read_u32_le()?);
            }
        }

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
        let refraction_power = if bsver >= 155 {
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
        let (env_map_texture, normal_texture, env_mask_texture, env_map_scale) = if bsver >= 130 {
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
        if bsver >= 155 {
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

#[cfg(test)]
#[path = "shader_tests.rs"]
mod tests;
