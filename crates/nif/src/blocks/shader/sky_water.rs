//! Skyrim+ sky and water shader properties, and the shared
//! `BSShaderTextureSet` texture path list.
//!
//! Split out of the single `blocks/shader.rs` under #4339, which had
//! re-crossed 2000 production lines holding four block families behind one
//! shared head. The axis is the block family, not the game: every growth
//! commit touched exactly one family, while a per-game split would have
//! scattered one struct's parsers across five files.

use super::parse_skyrim_shader_base;
use crate::blocks::base::NiObjectNETData;
use crate::stream::NifStream;
use std::io;

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
        // #4151 — no gap band: nif.xml gates Sky's flags on the un-split
        // `!#BS_GTE_132#`, unlike BLSP/BSEffect's split gates.
        let (shader_flags_1, shader_flags_2, sf1_crcs, sf2_crcs, uv_offset, uv_scale) =
            parse_skyrim_shader_base(stream, false)?;
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
        // #4151 — no gap band: nif.xml gates Water's flags on the un-split
        // `!#BS_GTE_132#`, unlike BLSP/BSEffect's split gates.
        let (shader_flags_1, shader_flags_2, sf1_crcs, sf2_crcs, uv_offset, uv_scale) =
            parse_skyrim_shader_base(stream, false)?;
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
