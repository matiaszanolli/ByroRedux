//! Bethesda shader property blocks.
//!
//! - BSShaderPPLightingProperty / BSShaderNoLightingProperty — Fallout 3/NV
//! - BSLightingShaderProperty / BSEffectShaderProperty — Skyrim+
//! - BSShaderTextureSet — shared texture path list (all games)
//!
//! Split out of the single `blocks/shader.rs` under #4339, which had
//! re-crossed 2000 production lines holding four block families behind one
//! shared head. The axis is the block family, not the game: every growth
//! commit touched exactly one family, while a per-game split would have
//! scattered one struct's parsers across five files.

use crate::impl_ni_object;
use crate::stream::NifStream;
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

mod effect;
mod legacy;
mod lighting;
mod sky_water;

// Glob re-exports: every `crate::blocks::shader::X` path in the tree — the
// dispatcher, the importers and their tests — kept working unchanged.
pub use effect::*;
pub use legacy::*;
pub use lighting::*;
pub use sky_water::*;

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

/// `has_gap_band` selects which shader-flags gate applies at the
/// unattested `bsver == 131` dev stream (#4151):
///
/// * `true` (`BSLightingShaderProperty` / `BSEffectShaderProperty`) — nif.xml
///   splits their typed flags at `#NI_BS_LT_FO4#` (`< 130`) union `#BS_FO4#`
///   (`== 130`), i.e. `bsver <= FALLOUT4`. 131 carries NEITHER encoding — the
///   `carries_typed_shader_flags`/`carries_crc_shader_flags` gap band.
/// * `false` (`BSSkyShaderProperty` / `BSWaterShaderProperty`) — nif.xml
///   instead gates their flags on the un-split `!#BS_GTE_132#` (`< 132`), so
///   131 genuinely carries the typed pair. Routing these two through the
///   BLSP-derived gap band under-read 8 bytes at 131 that nif.xml says are
///   on the wire.
///
/// The CRC-array gate (`bsver >= FO4_CRC_FLAGS` = 132) is identical either
/// way — nif.xml's `#BS_GTE_132#` matches `carries_crc_shader_flags` exactly
/// for all four block types — so only the typed-flags half needs to branch.
pub(super) fn parse_skyrim_shader_base(
    stream: &mut NifStream,
    has_gap_band: bool,
) -> io::Result<SkyrimShaderBase> {
    let bsver = stream.bsver();

    // #2603 / #409 — the gates are the named predicates, not raw BSVER
    // comparisons, because the two encodings are NOT complementary:
    // `FO4_SHADER_GAP` (131) carries neither the typed u32 pair nor the CRC
    // arrays, so `< FO4_CRC_FLAGS` is wrong for the first gate — it reads 8
    // bytes that aren't there and drifts the rest of the block. `version.rs`
    // pins that band (`bsver_shader_flag_band_tests`); keeping the gate here
    // expressed as the predicate is what makes the pin cover this site (#3845).
    // This only applies when `has_gap_band` is true (BLSP/BSEffect); Sky/Water
    // use the un-split `< FO4_CRC_FLAGS` cut instead (#4151).
    let carries_typed_shader_flags = if has_gap_band {
        crate::version::bsver::carries_typed_shader_flags(bsver)
    } else {
        bsver < crate::version::bsver::FO4_CRC_FLAGS
    };
    let (shader_flags_1, shader_flags_2) = if carries_typed_shader_flags {
        (stream.read_u32_le()?, stream.read_u32_le()?)
    } else {
        (0, 0)
    };

    // Counts go through allocate_vec so a corrupt 0xFFFFFFFF can't OOM
    // before the inner u32 reads fail. See #764.
    let (sf1_crcs, sf2_crcs) = if crate::version::bsver::carries_crc_shader_flags(bsver) {
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

/// #1606 — capture the trailing Starfield shader bytes between the
/// parser's stop point and `block_size`, as an opaque tail. Returns empty
/// for non-Starfield files, when `block_size` is unknown, or when the
/// parser already consumed the whole block (no drift). Never over-reads:
/// it consumes exactly `block_size - consumed`, whatever that is, so it is
/// correct for any future tail length without assuming the LOD layout.
pub(super) fn read_starfield_tail(
    stream: &mut NifStream,
    block_start: u64,
    block_size: Option<u32>,
    bsver: u32,
) -> io::Result<Vec<u8>> {
    if bsver < crate::version::bsver::STARFIELD {
        return Ok(Vec::new());
    }
    let Some(block_size) = block_size else {
        return Ok(Vec::new());
    };
    let consumed = stream.position().saturating_sub(block_start);
    let remaining = u64::from(block_size).saturating_sub(consumed);
    if remaining == 0 {
        return Ok(Vec::new());
    }
    stream.read_bytes(remaining as usize)
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
);

#[cfg(test)]
#[path = "../shader_tests/mod.rs"]
mod tests;
