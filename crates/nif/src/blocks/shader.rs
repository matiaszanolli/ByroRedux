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

        Ok(Self {
            net,
            shader,
            texture_clamp_mode,
            texture_set_ref,
            refraction_strength,
            refraction_fire_period,
            parallax_max_passes,
            parallax_scale,
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

        // FO76+ stopcond: if Name is a non-empty BGSM file path, the rest of the
        // block is absent (the BGSM file holds the real material data). Return
        // a stub and let block_size skip any trailing padding.
        if bsver >= 155 {
            if let Some(name) = net.name.as_deref() {
                if !name.is_empty() {
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

        // FO76 BSShaderType155 field (BSVER == 155 only).
        let fo76_shader_type = if bsver == 155 {
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

        // Effective shader type for the downstream dispatch (uses different enums
        // depending on version).
        let shader_type = if bsver == 155 {
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
            let unknown_2 = if bsver == 155 {
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

        // FO76 (BSVER == 155) trailing fields.
        let mut luminance = None;
        let mut do_translucency = false;
        let mut translucency = None;
        let mut texture_arrays: Vec<BSTextureArray> = Vec::new();
        if bsver == 155 {
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

        // Shader-type-specific trailing fields. For FO76 (BSVER == 155) these use
        // the BSShaderType155 numeric mapping (type 4 = skin tint Color4, type 5 =
        // hair tint Color3). For Skyrim/FO4 we keep the existing dispatch.
        let shader_type_data = if bsver == 155 {
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
    /// FO76+ refraction power (BSVER == 155).
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

        // FO76+ stopcond: non-empty Name means the block is an external BGEM reference.
        if bsver >= 155 {
            if let Some(name) = net.name.as_deref() {
                if !name.is_empty() {
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

        // FO76 refraction power.
        let refraction_power = if bsver == 155 {
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

        // FO76 trailing fields.
        let mut reflectance_texture = String::new();
        let mut lighting_texture = String::new();
        let mut emittance_color = [0.0f32; 3];
        let mut emit_gradient_texture = String::new();
        let mut luminance = None;
        if bsver == 155 {
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
            strings: vec![Arc::from("ShaderProp")],
            max_string_length: 10,
            num_groups: 0,
        }
    }

    /// Build bytes for BSShaderPPLightingProperty, optionally including emissive color.
    fn build_bsshader_bytes(user_version_2: u32) -> Vec<u8> {
        let mut data = Vec::new();
        // NiObjectNET: name (string table index 0)
        data.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs: count=0
        data.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref: -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // NiShadeProperty: shade_flags (u16)
        data.extend_from_slice(&0u16.to_le_bytes());
        // shader_type (u32)
        data.extend_from_slice(&1u32.to_le_bytes());
        // shader_flags_1 (u32)
        data.extend_from_slice(&0x80000000u32.to_le_bytes());
        // shader_flags_2 (u32)
        data.extend_from_slice(&0x00000001u32.to_le_bytes());
        // env_map_scale (f32)
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // texture_clamp_mode (u32)
        data.extend_from_slice(&3u32.to_le_bytes());
        // texture_set_ref (i32)
        data.extend_from_slice(&5i32.to_le_bytes());
        // Refraction/parallax fields — bsver >= 15 reads refraction, bsver >= 24 adds parallax.
        // FNV: bsver=34, so both are present. Oblivion: bsver=0, so neither.
        if user_version_2 >= 15 {
            data.extend_from_slice(&0.5f32.to_le_bytes()); // refraction_strength
            data.extend_from_slice(&10i32.to_le_bytes()); // refraction_fire_period
        }
        if user_version_2 >= 24 {
            data.extend_from_slice(&4.0f32.to_le_bytes()); // parallax_max_passes
            data.extend_from_slice(&1.5f32.to_le_bytes()); // parallax_scale
        }
        data
    }

    /// Regression: #459 — `BSShaderTextureSet::parse` previously read
    /// `Num Textures` as `i32` and clamped `.max(0) as u32`, silently
    /// dropping any negative-interpreted length to an empty set. When
    /// upstream drift flipped the high bit, the block quietly
    /// succeeded at the wrong offset instead of failing loud. Verify
    /// the u32 read still produces an empty set for zero, the expected
    /// set for a normal count, and a loud error for a length that
    /// obviously exceeds the remaining stream (the `allocate_vec`
    /// budget guard from #388 catches it).
    #[test]
    fn parse_bsshader_texture_set_num_textures_as_u32() {
        let header = make_header(11, 34);

        // Case 1: zero textures → empty set, stream fully consumed.
        let zero = 0u32.to_le_bytes();
        let mut stream = NifStream::new(&zero, &header);
        let ts = BSShaderTextureSet::parse(&mut stream).unwrap();
        assert!(ts.textures.is_empty());
        assert_eq!(stream.position(), 4);

        // Case 2: 2 textures — normal path still works.
        let mut data = 2u32.to_le_bytes().to_vec();
        for name in ["diffuse.dds", "normal.dds"] {
            data.extend_from_slice(&(name.len() as u32).to_le_bytes());
            data.extend_from_slice(name.as_bytes());
        }
        let mut stream = NifStream::new(&data, &header);
        let ts = BSShaderTextureSet::parse(&mut stream).unwrap();
        assert_eq!(ts.textures, vec!["diffuse.dds".to_string(), "normal.dds".into()]);

        // Case 3: length of 0xFFFFFFFF (previously silently clamped to 0).
        // Under u32, this exceeds the remaining bytes in the stream and
        // the allocate_vec budget guard short-circuits with InvalidData —
        // loud enough for the outer block_sizes recovery to take over.
        let drift = 0xFFFF_FFFFu32.to_le_bytes();
        let mut stream = NifStream::new(&drift, &header);
        let err = BSShaderTextureSet::parse(&mut stream).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn parse_bsshader_fnv_reads_refraction_parallax() {
        // FNV (bsver=34): reads refraction (bsver>=15) + parallax (bsver>=24) = 16 bytes.
        let header = make_header(11, 34);
        let data = build_bsshader_bytes(34);
        let mut stream = NifStream::new(&data, &header);

        let prop = BSShaderPPLightingProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.texture_set_ref.index(), Some(5));
        assert!((prop.refraction_strength - 0.5).abs() < 1e-6);
        assert_eq!(prop.refraction_fire_period, 10);
        assert!((prop.parallax_max_passes - 4.0).abs() < 1e-6);
        assert!((prop.parallax_scale - 1.5).abs() < 1e-6);
        // All data consumed: 38 base + 16 refraction/parallax = 54 bytes
        assert_eq!(stream.position(), 54);
    }

    /// Regression: #455 — `TileShaderProperty` parses the FO3
    /// `BSShaderLightingProperty` base (NET + shader data + texture
    /// clamp) + a trailing SizedString filename. Pre-fix the dispatch
    /// aliased this type to `BSShaderPPLightingProperty::parse`, which
    /// over-read 20-28 bytes of PP-specific fields and never populated
    /// the filename. HUD overlays (stealth meter / airtimer / quest
    /// markers) lost their texture path as a result.
    #[test]
    fn parse_tile_shader_property_fo3() {
        let header = make_header(11, 34); // FO3/FNV
        let mut data = Vec::new();
        // NiObjectNET: inline name (v <= 20.1.0.0 path)
        data.extend_from_slice(&0i32.to_le_bytes()); // name (string table index)
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
        // BSShaderPropertyData: 18 bytes
        data.extend_from_slice(&0u16.to_le_bytes()); // shade_flags
        data.extend_from_slice(&1u32.to_le_bytes()); // shader_type
        data.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_1
        data.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_2
        data.extend_from_slice(&0.0f32.to_le_bytes()); // env_map_scale
        data.extend_from_slice(&3u32.to_le_bytes()); // texture_clamp_mode
        // file_name SizedString (u32 length + bytes; NO trailing null)
        let name = b"textures\\interface\\airtimer.dds";
        data.extend_from_slice(&(name.len() as u32).to_le_bytes());
        data.extend_from_slice(name);
        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = TileShaderProperty::parse(&mut stream)
            .expect("TileShaderProperty should parse with BSShaderLightingProperty base + filename");
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "TileShaderProperty must consume exactly {expected_len} bytes",
        );
        assert_eq!(prop.texture_clamp_mode, 3);
        assert_eq!(prop.file_name, "textures\\interface\\airtimer.dds");
        assert_eq!(prop.shader.shader_type, 1);
    }

    #[test]
    fn parse_bsshader_oblivion_no_extra_fields() {
        // Oblivion (bsver=0): no refraction or parallax fields.
        let header = make_header(0, 0);
        let data = build_bsshader_bytes(0);
        let mut stream = NifStream::new(&data, &header);

        let prop = BSShaderPPLightingProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.texture_set_ref.index(), Some(5));
        assert_eq!(prop.refraction_strength, 0.0);
        assert_eq!(prop.refraction_fire_period, 0);
        assert!((prop.parallax_max_passes - 4.0).abs() < 1e-6); // defaults
        assert!((prop.parallax_scale - 1.0).abs() < 1e-6);
        // Only 38 bytes consumed (no extras)
        assert_eq!(stream.position(), 38);
    }

    fn make_skyrim_header() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 12,
            user_version_2: 83,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("TestShader")],
            max_string_length: 10,
            num_groups: 0,
        }
    }

    /// Build the common bytes for BSLightingShaderProperty (Skyrim LE, BSVER=83).
    fn build_bs_lighting_common(shader_type: u32) -> Vec<u8> {
        let mut data = Vec::new();
        // shader_type (read before NiObjectNET for BSVER 83-130)
        data.extend_from_slice(&shader_type.to_le_bytes());
        // NiObjectNET: name (string table index 0)
        data.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs: count=0
        data.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref: -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // shader_flags_1, shader_flags_2
        data.extend_from_slice(&0x80000000u32.to_le_bytes());
        data.extend_from_slice(&0x00000010u32.to_le_bytes()); // two-sided flag
                                                              // uv_offset (2x f32)
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // uv_scale (2x f32)
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // texture_set_ref
        data.extend_from_slice(&3i32.to_le_bytes());
        // emissive_color (3x f32)
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // emissive_multiple
        data.extend_from_slice(&2.0f32.to_le_bytes());
        // texture_clamp_mode
        data.extend_from_slice(&3u32.to_le_bytes());
        // alpha
        data.extend_from_slice(&0.8f32.to_le_bytes());
        // refraction_strength
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // glossiness
        data.extend_from_slice(&50.0f32.to_le_bytes());
        // specular_color (3x f32)
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.9f32.to_le_bytes());
        data.extend_from_slice(&0.8f32.to_le_bytes());
        // specular_strength
        data.extend_from_slice(&1.5f32.to_le_bytes());
        // lighting_effect_1, lighting_effect_2
        data.extend_from_slice(&0.3f32.to_le_bytes());
        data.extend_from_slice(&0.7f32.to_le_bytes());
        data
    }

    #[test]
    fn parse_bs_lighting_default_no_trailing() {
        let header = make_skyrim_header();
        let data = build_bs_lighting_common(0); // shader_type=0 (Default)
        let mut stream = NifStream::new(&data, &header);

        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.shader_type, 0);
        assert!((prop.glossiness - 50.0).abs() < 1e-6);
        assert!(matches!(prop.shader_type_data, ShaderTypeData::None));
        // All common data consumed, no trailing fields.
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_lighting_env_map_trailing() {
        let header = make_skyrim_header();
        let mut data = build_bs_lighting_common(1); // shader_type=1 (EnvironmentMap)
        data.extend_from_slice(&0.75f32.to_le_bytes()); // env_map_scale
        let mut stream = NifStream::new(&data, &header);

        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.shader_type, 1);
        match prop.shader_type_data {
            ShaderTypeData::EnvironmentMap { env_map_scale } => {
                assert!((env_map_scale - 0.75).abs() < 1e-6);
            }
            _ => panic!("expected EnvironmentMap"),
        }
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_lighting_skin_tint_trailing() {
        let header = make_skyrim_header();
        let mut data = build_bs_lighting_common(5); // shader_type=5 (SkinTint)
        data.extend_from_slice(&0.9f32.to_le_bytes());
        data.extend_from_slice(&0.7f32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());
        let mut stream = NifStream::new(&data, &header);

        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        match prop.shader_type_data {
            ShaderTypeData::SkinTint { skin_tint_color } => {
                assert!((skin_tint_color[0] - 0.9).abs() < 1e-6);
                assert!((skin_tint_color[1] - 0.7).abs() < 1e-6);
                assert!((skin_tint_color[2] - 0.5).abs() < 1e-6);
            }
            _ => panic!("expected SkinTint"),
        }
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_lighting_eye_envmap_trailing() {
        let header = make_skyrim_header();
        let mut data = build_bs_lighting_common(16); // shader_type=16 (EyeEnvmap)
                                                     // eye_cubemap_scale
        data.extend_from_slice(&1.2f32.to_le_bytes());
        // left_eye_reflection_center (3x f32)
        data.extend_from_slice(&(-0.05f32).to_le_bytes());
        data.extend_from_slice(&0.12f32.to_le_bytes());
        data.extend_from_slice(&0.03f32.to_le_bytes());
        // right_eye_reflection_center (3x f32)
        data.extend_from_slice(&0.05f32.to_le_bytes());
        data.extend_from_slice(&0.12f32.to_le_bytes());
        data.extend_from_slice(&0.03f32.to_le_bytes());
        let mut stream = NifStream::new(&data, &header);

        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        match prop.shader_type_data {
            ShaderTypeData::EyeEnvmap {
                eye_cubemap_scale,
                left_eye_reflection_center,
                right_eye_reflection_center,
            } => {
                assert!((eye_cubemap_scale - 1.2).abs() < 1e-6);
                assert!((left_eye_reflection_center[0] - (-0.05)).abs() < 1e-6);
                assert!((right_eye_reflection_center[0] - 0.05).abs() < 1e-6);
            }
            _ => panic!("expected EyeEnvmap"),
        }
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_lighting_multilayer_parallax_trailing() {
        let header = make_skyrim_header();
        let mut data = build_bs_lighting_common(11); // shader_type=11 (MultiLayerParallax)
        data.extend_from_slice(&0.1f32.to_le_bytes()); // inner_layer_thickness
        data.extend_from_slice(&0.5f32.to_le_bytes()); // refraction_scale
        data.extend_from_slice(&2.0f32.to_le_bytes()); // inner_layer_texture_scale u
        data.extend_from_slice(&2.0f32.to_le_bytes()); // inner_layer_texture_scale v
        data.extend_from_slice(&0.8f32.to_le_bytes()); // envmap_strength
        let mut stream = NifStream::new(&data, &header);

        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        match prop.shader_type_data {
            ShaderTypeData::MultiLayerParallax {
                inner_layer_thickness,
                envmap_strength,
                ..
            } => {
                assert!((inner_layer_thickness - 0.1).abs() < 1e-6);
                assert!((envmap_strength - 0.8).abs() < 1e-6);
            }
            _ => panic!("expected MultiLayerParallax"),
        }
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_effect_shader_soft_falloff_and_greyscale() {
        let header = make_skyrim_header();
        let mut data = Vec::new();
        // NiObjectNET: name (string table index 0)
        data.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs: count=0
        data.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref: -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // shader_flags_1, shader_flags_2
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        // uv_offset, uv_scale
        for _ in 0..4 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // source_texture: sized string "tex/glow.dds"
        let tex = b"tex/glow.dds";
        data.extend_from_slice(&(tex.len() as u32).to_le_bytes());
        data.extend_from_slice(tex);
        // texture_clamp_mode(u8), lighting_influence(u8), env_map_min_lod(u8), unused(u8)
        data.extend_from_slice(&[3u8, 128u8, 5u8, 0u8]);
        // falloff: start_angle, stop_angle, start_opacity, stop_opacity
        for _ in 0..4 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // emissive_color (4x f32)
        for _ in 0..4 {
            data.extend_from_slice(&1.0f32.to_le_bytes());
        }
        // emissive_multiple
        data.extend_from_slice(&2.0f32.to_le_bytes());
        // soft_falloff_depth
        data.extend_from_slice(&5.0f32.to_le_bytes());
        // greyscale_texture: sized string "tex/grey.dds"
        let grey = b"tex/grey.dds";
        data.extend_from_slice(&(grey.len() as u32).to_le_bytes());
        data.extend_from_slice(grey);

        let mut stream = NifStream::new(&data, &header);
        let prop = BSEffectShaderProperty::parse(&mut stream).unwrap();

        assert_eq!(prop.source_texture, "tex/glow.dds");
        assert_eq!(prop.lighting_influence, 128);
        assert_eq!(prop.env_map_min_lod, 5);
        assert!((prop.soft_falloff_depth - 5.0).abs() < 1e-6);
        assert_eq!(prop.greyscale_texture, "tex/grey.dds");
        assert!(prop.env_map_texture.is_empty()); // Not FO4+
        assert_eq!(stream.position(), data.len() as u64);
    }

    fn make_fo4_header() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 12,
            user_version_2: 130,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("FO4Shader")],
            max_string_length: 9,
            num_groups: 0,
        }
    }

    /// Build FO4 BSLightingShaderProperty bytes (BSVER=130, shader_type=1 env map).
    fn build_bs_lighting_fo4_env_map() -> Vec<u8> {
        let mut data = Vec::new();
        // shader_type (read before NiObjectNET for BSVER 83-130)
        data.extend_from_slice(&1u32.to_le_bytes()); // EnvironmentMap
                                                     // NiObjectNET: name (string table index 0)
        data.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs: count=0
        data.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref: -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // shader_flags_1, shader_flags_2 (FO4 reads u32 pair)
        data.extend_from_slice(&0x80000000u32.to_le_bytes());
        data.extend_from_slice(&0x00000010u32.to_le_bytes());
        // uv_offset, uv_scale
        for v in [0.0f32, 0.0, 1.0, 1.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // texture_set_ref
        data.extend_from_slice(&3i32.to_le_bytes());
        // emissive_color (3x f32)
        for v in [0.0f32, 0.5, 1.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // emissive_multiple
        data.extend_from_slice(&2.0f32.to_le_bytes());
        // Root Material (FO4+: NiFixedString = string table index)
        data.extend_from_slice(&(-1i32).to_le_bytes()); // no root material
                                                        // texture_clamp_mode
        data.extend_from_slice(&3u32.to_le_bytes());
        // alpha
        data.extend_from_slice(&0.8f32.to_le_bytes());
        // refraction_strength
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // glossiness (called "smoothness" in FO4, same f32)
        data.extend_from_slice(&0.5f32.to_le_bytes());
        // specular_color (3x f32)
        for v in [1.0f32, 0.9, 0.8] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // specular_strength
        data.extend_from_slice(&1.5f32.to_le_bytes());
        // lighting_effect_1, lighting_effect_2 — NOT present for BSVER >= 130
        // (the parser skips these with (0.0, 0.0))
        // FO4 common fields:
        data.extend_from_slice(&0.3f32.to_le_bytes()); // subsurface_rolloff
        data.extend_from_slice(&2.5f32.to_le_bytes()); // rimlight_power (< FLT_MAX → has backlight)
        data.extend_from_slice(&1.0f32.to_le_bytes()); // backlight_power
        data.extend_from_slice(&0.7f32.to_le_bytes()); // grayscale_to_palette_scale
        data.extend_from_slice(&5.0f32.to_le_bytes()); // fresnel_power
                                                       // WetnessParams (BSVER=130: 7 floats — #403 widened
                                                       // unknown_1 gate to the full 130..155 FO4/FO76 range).
                                                       // Order: spec_scale, spec_power, min_var, env_map_scale,
                                                       // fresnel, metalness, unknown_1.
        for v in [0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.95] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // Shader type 1 trailing: env_map_scale + 2 bools (FO4 BSVER 130)
        data.extend_from_slice(&0.75f32.to_le_bytes()); // env_map_scale
        data.push(1u8); // use_ssr (bool)
        data.push(0u8); // wetness_use_ssr (bool)
        data
    }

    // ── #409 BSVER-131 / 132 boundary regression tests ───────────────

    /// Build a header with a custom BSVER — share the version number
    /// path with the existing FO4 / FO76 helpers so only the boundary
    /// tests need a fresh fixture. The body is the standard
    /// `BSLightingShaderProperty` minus the flag-pair / CRC slots the
    /// per-BSVER gate controls; callers assemble the rest.
    fn make_fo4_header_with_bsver(bsver: u32) -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 12,
            user_version_2: bsver,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("BoundaryShader")],
            max_string_length: 14,
            num_groups: 0,
        }
    }

    /// Regression for #409: at `BSVER == 131` the parser must read
    /// neither the u32 flag pair (gated on `bsver <= 130`) nor the
    /// CRC-array counts (gated on `bsver >= 132`). This is NOT a bug —
    /// nif.xml's `#BS_FO4#` is strict `BSVER == 130` and `#BS_GTE_132#`
    /// starts at 132, leaving 131 as an intentional dev-stream gap
    /// where the flag fields are absent altogether.
    ///
    /// The test constructs a body 8 bytes shorter than BSVER 130 (no
    /// flag pair) and assumes the pre-flag-pair part plus the
    /// post-CRC part line up with `bsver == 131`'s expected layout.
    /// Consumes exactly the authored bytes.
    #[test]
    fn bs_lighting_bsver_131_skips_flag_pair_and_crc_counts() {
        let header = make_fo4_header_with_bsver(131);
        let mut data = Vec::new();

        // shader_type (read before NiObjectNET for BSVER 83-130... but
        // also 131 since the gate at `shader.rs::parse` is `bsver <
        // 155`). See the pre-flag-pair block in the source.
        data.extend_from_slice(&1u32.to_le_bytes()); // EnvironmentMap
        // NiObjectNET: name (string-table idx 0), empty extra_data_refs,
        // no controller.
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // NO flag pair at bsver == 131 (gate is `bsver <= 130`).
        // NO Num SF1/SF2 at bsver == 131 (gate is `bsver >= 132`).
        // Next field: uv_offset + uv_scale.
        for v in [0.0f32, 0.0, 1.0, 1.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // texture_set_ref
        data.extend_from_slice(&3i32.to_le_bytes());
        // emissive_color + emissive_multiple
        for v in [0.0f32, 0.5, 1.0, 2.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // Root Material (FO4+ NiFixedString)
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // texture_clamp_mode, alpha, refraction_strength, glossiness
        data.extend_from_slice(&3u32.to_le_bytes());
        for v in [0.8f32, 0.0, 0.5] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // specular_color + specular_strength
        for v in [1.0f32, 0.9, 0.8, 1.5] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // FO4 common fields (BSVER 130..139): subsurface/rimlight/backlight/
        // grayscale/fresnel.
        data.extend_from_slice(&0.3f32.to_le_bytes());
        data.extend_from_slice(&2.5f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.7f32.to_le_bytes());
        data.extend_from_slice(&5.0f32.to_le_bytes());
        // Wetness (7 floats — same as BSVER 130; the wetness gate is
        // `>= 130` not per-BSVER-specific). `env_map_scale` slot
        // (offset 4 within the wetness block) only present at
        // `bsver == 130` strictly — at 131 the parser reads 6 floats.
        for v in [0.1f32, 0.2, 0.3, 0.5, 0.6, 0.95] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // shader_type=1 (EnvironmentMap) trailing: env_map_scale + 2 bools.
        data.extend_from_slice(&0.75f32.to_le_bytes());
        data.push(1u8);
        data.push(0u8);

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.shader_type, 1);
        // Flag pair stays at the pre-fill-in default `0` because the
        // 131 gate skips the u32 read — this is what the test pins.
        assert_eq!(prop.shader_flags_1, 0, "bsver=131 skips flag pair");
        assert_eq!(prop.shader_flags_2, 0, "bsver=131 skips flag pair");
        // CRC arrays stay empty because Num SF1/SF2 are gated `>= 132`.
        assert!(prop.sf1_crcs.is_empty());
        assert!(prop.sf2_crcs.is_empty());
        // Every authored byte consumed — no under-read into next block.
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "bsver=131 body must consume exactly what was authored"
        );
    }

    /// Regression for #409: at `BSVER == 132` the parser must skip the
    /// flag pair AND read `Num SF1` + the SF1 CRC array (but NOT
    /// `Num SF2` which is gated on `>= 152`). Exercises the other side
    /// of the BSVER 131 gap.
    #[test]
    fn bs_lighting_bsver_132_reads_crc_counts_but_not_flag_pair() {
        let header = make_fo4_header_with_bsver(132);
        let mut data = Vec::new();

        // shader_type + NiObjectNET as usual.
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // NO flag pair at bsver == 132 (gate is `bsver <= 130`).
        // Num SF1 = 2, Num SF2 NOT read (gated on `bsver >= 152`).
        data.extend_from_slice(&2u32.to_le_bytes());
        // Two SF1 CRC32 entries.
        data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
        data.extend_from_slice(&0xCAFEBABEu32.to_le_bytes());
        // Standard post-flag payload.
        for v in [0.0f32, 0.0, 1.0, 1.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(&3i32.to_le_bytes());
        for v in [0.0f32, 0.5, 1.0, 2.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&3u32.to_le_bytes());
        for v in [0.8f32, 0.0, 0.5] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        for v in [1.0f32, 0.9, 0.8, 1.5] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(&0.3f32.to_le_bytes());
        data.extend_from_slice(&2.5f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.7f32.to_le_bytes());
        data.extend_from_slice(&5.0f32.to_le_bytes());
        // Wetness: same 6 floats as bsver 131 (no env_map_scale slot
        // since that's strict `bsver == 130`).
        for v in [0.1f32, 0.2, 0.3, 0.5, 0.6, 0.95] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(&0.75f32.to_le_bytes());
        data.push(1u8);
        data.push(0u8);

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.shader_type, 1);
        assert_eq!(prop.shader_flags_1, 0);
        assert_eq!(prop.shader_flags_2, 0);
        assert_eq!(prop.sf1_crcs, vec![0xDEADBEEF, 0xCAFEBABE]);
        assert!(prop.sf2_crcs.is_empty(), "Num SF2 requires bsver >= 152");
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "bsver=132 must read CRC array but skip flag pair"
        );
    }

    // ── N23.9: FO76/Starfield tests ──────────────────────────────────

    fn make_fo76_header(name: &str) -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 12,
            user_version_2: 155,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from(name)],
            max_string_length: name.len() as u32,
            num_groups: 0,
        }
    }

    /// Build a BSLightingShaderProperty body with FO76 layout (BSVER=155), empty
    /// name (so stopcond does NOT fire), shader_type=0 (Default → no trailing),
    /// empty SF1/SF2 arrays, wetness + luminance, no translucency, no texture arrays.
    fn build_fo76_bs_lighting_minimal() -> Vec<u8> {
        let mut data = Vec::new();
        // NiObjectNET: name = string table index 0 ("")
        data.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs list: count=0
        data.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref = -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // BSVER == 155: Shader Type (BSShaderType155) = 0 (Default)
        data.extend_from_slice(&0u32.to_le_bytes());
        // Num SF1 = 0 (BSVER >= 132)
        data.extend_from_slice(&0u32.to_le_bytes());
        // Num SF2 = 0 (BSVER >= 152)
        data.extend_from_slice(&0u32.to_le_bytes());
        // (no SF1/SF2 arrays because lengths are zero)
        // uv_offset, uv_scale
        for v in [0.0f32, 0.0, 1.0, 1.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // texture_set_ref
        data.extend_from_slice(&5i32.to_le_bytes());
        // emissive_color (3×f32)
        for v in [0.1f32, 0.2, 0.3] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // emissive_multiple
        data.extend_from_slice(&1.5f32.to_le_bytes());
        // Root Material (NiFixedString, BSVER >= 130): -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // texture_clamp_mode
        data.extend_from_slice(&3u32.to_le_bytes());
        // alpha
        data.extend_from_slice(&0.9f32.to_le_bytes());
        // refraction_strength
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // smoothness (glossiness in struct)
        data.extend_from_slice(&0.6f32.to_le_bytes());
        // specular_color
        for v in [0.7f32, 0.8, 0.9] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // specular_strength
        data.extend_from_slice(&1.25f32.to_le_bytes());
        // (no lighting_effect_1/2 — BSVER >= 130 skips)
        // (no subsurface/rimlight/backlight — not in BS_FO4_2 range)
        // grayscale_to_palette_scale
        data.extend_from_slice(&0.4f32.to_le_bytes());
        // fresnel_power
        data.extend_from_slice(&4.2f32.to_le_bytes());
        // WetnessParams: spec_scale, spec_power, min_var,
        // (env_map_scale only for BSVER==130, skipped here)
        // fresnel, metalness, unknown_1 (>130), unknown_2 (==155)
        for v in [0.11f32, 0.22, 0.33, 0.44, 0.55, 0.66, 0.77] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // FO76 luminance (4×f32)
        for v in [100.0f32, 13.5, 2.0, 3.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // Do Translucency = false (1 byte)
        data.push(0u8);
        // Has Texture Arrays = false (1 byte)
        data.push(0u8);
        // No shader-type trailing for type 0 Default
        data
    }

    #[test]
    fn parse_bs_lighting_fo76_minimal() {
        let header = make_fo76_header(""); // empty name → stopcond does NOT fire
        let data = build_fo76_bs_lighting_minimal();
        let mut stream = NifStream::new(&data, &header);

        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        assert!(
            !prop.material_reference,
            "stopcond should not fire for empty name"
        );
        assert_eq!(prop.shader_type, 0); // FO76 Default
        assert!(prop.sf1_crcs.is_empty());
        assert!(prop.sf2_crcs.is_empty());
        assert!((prop.glossiness - 0.6).abs() < 1e-6);
        assert!((prop.grayscale_to_palette_scale - 0.4).abs() < 1e-6);
        assert!((prop.fresnel_power - 4.2).abs() < 1e-6);
        let w = prop
            .wetness
            .as_ref()
            .expect("wetness present for BSVER 155");
        assert!((w.spec_scale - 0.11).abs() < 1e-6);
        assert_eq!(w.env_map_scale, 0.0); // not read for BSVER != 130
        assert!((w.unknown_1 - 0.66).abs() < 1e-6);
        assert!((w.unknown_2 - 0.77).abs() < 1e-6);
        let lum = prop
            .luminance
            .as_ref()
            .expect("luminance present for BSVER 155");
        assert!((lum.lum_emittance - 100.0).abs() < 1e-6);
        assert!((lum.exposure_offset - 13.5).abs() < 1e-6);
        assert!((lum.final_exposure_max - 3.0).abs() < 1e-6);
        assert!(!prop.do_translucency);
        assert!(prop.translucency.is_none());
        assert!(prop.texture_arrays.is_empty());
        assert!(matches!(prop.shader_type_data, ShaderTypeData::None));
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_lighting_fo76_stopcond_short_circuits() {
        // Non-empty name at BSVER >= 155 → stopcond fires, block body is absent.
        let header = make_fo76_header("materials/weapons/rifle.bgsm");
        // Only NiObjectNET bytes are present; no shader fields follow.
        let mut data = Vec::new();
        // name → string table index 0 (→ "materials/weapons/rifle.bgsm")
        data.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs count = 0
        data.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref = -1
        data.extend_from_slice(&(-1i32).to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        assert!(prop.material_reference);
        assert_eq!(
            prop.net.name.as_deref(),
            Some("materials/weapons/rifle.bgsm"),
        );
        // Everything else at defaults.
        assert_eq!(prop.shader_flags_1, 0);
        assert!(prop.wetness.is_none());
        assert!(prop.luminance.is_none());
        // Parser stopped at end of NiObjectNET — no trailing bytes consumed.
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_lighting_fo76_skin_tint_color4() {
        let header = make_fo76_header("");
        let mut data = build_fo76_bs_lighting_minimal();
        // Patch the Shader Type (after 12 bytes NiObjectNET) from 0 → 4 (SkinTint).
        // Layout: name(4) + extra_count(4) + ctrl(4) = 12, then shader_type u32.
        let st_off = 12;
        data[st_off..st_off + 4].copy_from_slice(&4u32.to_le_bytes());
        // Append Color4 skin tint after the base body.
        for v in [0.95f32, 0.72, 0.60, 1.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }

        let mut stream = NifStream::new(&data, &header);
        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        match prop.shader_type_data {
            ShaderTypeData::Fo76SkinTint { skin_tint_color } => {
                assert!((skin_tint_color[0] - 0.95).abs() < 1e-6);
                assert!((skin_tint_color[3] - 1.0).abs() < 1e-6);
            }
            other => panic!("expected Fo76SkinTint, got {other:?}"),
        }
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_lighting_fo76_sf1_crcs() {
        // Build a minimal FO76 body with Num SF1 = 2, Num SF2 = 1.
        let header = make_fo76_header("");
        let mut data = Vec::new();
        // NiObjectNET
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // Shader Type = 0
        data.extend_from_slice(&0u32.to_le_bytes());
        // Num SF1 = 2
        data.extend_from_slice(&2u32.to_le_bytes());
        // Num SF2 = 1
        data.extend_from_slice(&1u32.to_le_bytes());
        // SF1 array
        data.extend_from_slice(&1563274220u32.to_le_bytes()); // CAST_SHADOWS
        data.extend_from_slice(&759557230u32.to_le_bytes()); // TWO_SIDED
                                                             // SF2 array
        data.extend_from_slice(&348504749u32.to_le_bytes()); // VERTEXCOLORS
                                                             // uv_offset, uv_scale
        for v in [0.0f32, 0.0, 1.0, 1.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // texture_set_ref
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // emissive_color + mult
        for _ in 0..4 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // Root Material
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // texture_clamp_mode, alpha, refraction, smoothness
        data.extend_from_slice(&3u32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // specular_color, specular_strength
        for _ in 0..3 {
            data.extend_from_slice(&1.0f32.to_le_bytes());
        }
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // grayscale, fresnel
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&5.0f32.to_le_bytes());
        // wetness: 7 floats (spec, spec_pow, min_var, fresnel, metal, unk1, unk2)
        for _ in 0..7 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // luminance
        for _ in 0..4 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // do_translucency=0, has_texture_arrays=0
        data.push(0u8);
        data.push(0u8);

        let mut stream = NifStream::new(&data, &header);
        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.sf1_crcs, vec![1563274220, 759557230]);
        assert_eq!(prop.sf2_crcs, vec![348504749]);
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_effect_fo76_trailing_textures() {
        let header = make_fo76_header("");
        let mut data = Vec::new();
        // NiObjectNET
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // Num SF1 = 0, Num SF2 = 0 (no flag pair for BSVER >= 132)
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        // uv_offset, uv_scale
        for v in [0.0f32, 0.0, 1.0, 1.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // source_texture
        let src = b"tex/src.dds";
        data.extend_from_slice(&(src.len() as u32).to_le_bytes());
        data.extend_from_slice(src);
        // clamp, light_infl, min_lod, unused
        data.extend_from_slice(&[3u8, 255u8, 0u8, 0u8]);
        // 4 falloff floats
        for _ in 0..4 {
            data.extend_from_slice(&1.0f32.to_le_bytes());
        }
        // refraction_power (FO76)
        data.extend_from_slice(&0.25f32.to_le_bytes());
        // emissive_color (4×f32)
        for _ in 0..4 {
            data.extend_from_slice(&1.0f32.to_le_bytes());
        }
        // emissive_multiple
        data.extend_from_slice(&1.5f32.to_le_bytes());
        // soft_falloff_depth
        data.extend_from_slice(&50.0f32.to_le_bytes());
        // greyscale_texture
        let grey = b"tex/grey.dds";
        data.extend_from_slice(&(grey.len() as u32).to_le_bytes());
        data.extend_from_slice(grey);
        // FO4+ textures (env, normal, mask) + env_map_scale
        for p in [b"tex/env.dds".as_slice(), b"tex/n.dds", b"tex/m.dds"] {
            data.extend_from_slice(&(p.len() as u32).to_le_bytes());
            data.extend_from_slice(p);
        }
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // FO76 trailing: reflectance, lighting textures
        for p in [b"tex/refl.dds".as_slice(), b"tex/lit.dds"] {
            data.extend_from_slice(&(p.len() as u32).to_le_bytes());
            data.extend_from_slice(p);
        }
        // emittance_color (3×f32)
        for v in [0.4f32, 0.5, 0.6] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // emit_gradient_texture
        let grad = b"tex/grad.dds";
        data.extend_from_slice(&(grad.len() as u32).to_le_bytes());
        data.extend_from_slice(grad);
        // luminance (4×f32)
        for v in [100.0f32, 13.5, 2.0, 3.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }

        let mut stream = NifStream::new(&data, &header);
        let prop = BSEffectShaderProperty::parse(&mut stream).unwrap();
        assert!(!prop.material_reference);
        assert!((prop.refraction_power - 0.25).abs() < 1e-6);
        assert_eq!(prop.source_texture, "tex/src.dds");
        assert_eq!(prop.env_map_texture, "tex/env.dds");
        assert_eq!(prop.reflectance_texture, "tex/refl.dds");
        assert_eq!(prop.lighting_texture, "tex/lit.dds");
        assert!((prop.emittance_color[1] - 0.5).abs() < 1e-6);
        assert_eq!(prop.emit_gradient_texture, "tex/grad.dds");
        let lum = prop.luminance.as_ref().unwrap();
        assert!((lum.exposure_offset - 13.5).abs() < 1e-6);
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_effect_fo76_stopcond_short_circuits() {
        let header = make_fo76_header("materials/effects/glow.bgem");
        let mut data = Vec::new();
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let prop = BSEffectShaderProperty::parse(&mut stream).unwrap();
        assert!(prop.material_reference);
        assert_eq!(
            prop.net.name.as_deref(),
            Some("materials/effects/glow.bgem"),
        );
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn parse_bs_lighting_fo4_env_map_with_wetness() {
        let header = make_fo4_header();
        let data = build_bs_lighting_fo4_env_map();
        let mut stream = NifStream::new(&data, &header);

        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.shader_type, 1);
        assert_eq!(prop.shader_flags_1, 0x80000000); // FO4 flags read correctly
        assert!((prop.glossiness - 0.5).abs() < 1e-6); // "smoothness" in FO4
        assert!((prop.subsurface_rolloff - 0.3).abs() < 1e-6);
        assert!((prop.rimlight_power - 2.5).abs() < 1e-6);
        assert!((prop.backlight_power - 1.0).abs() < 1e-6);
        assert!((prop.grayscale_to_palette_scale - 0.7).abs() < 1e-6);
        assert!((prop.fresnel_power - 5.0).abs() < 1e-6);
        // Wetness params — BSVER=130 reads 7 floats (see #403).
        let w = prop.wetness.as_ref().unwrap();
        assert!((w.spec_scale - 0.1).abs() < 1e-6);
        assert!((w.env_map_scale - 0.4).abs() < 1e-6); // BSVER=130 has this
        assert!((w.metalness - 0.6).abs() < 1e-6);
        // #403 regression: unknown_1 is now read for the whole 130..155
        // range (was gated on `> 130` and silently dropped 4 bytes per
        // FO4 lit mesh — observed as 1.87M "4-byte short" warnings on
        // the real FO4 archive sweep).
        assert!(
            (w.unknown_1 - 0.95).abs() < 1e-6,
            "wetness.unknown_1 should round-trip at BSVER=130 (#403)"
        );
        // Shader type data: EnvironmentMap
        match prop.shader_type_data {
            ShaderTypeData::EnvironmentMap { env_map_scale } => {
                assert!((env_map_scale - 0.75).abs() < 1e-6);
            }
            _ => panic!("expected EnvironmentMap"),
        }
        assert_eq!(stream.position(), data.len() as u64);
    }
}
