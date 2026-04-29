//! FaceGen sidecar parsers — `.egm` (geometry morphs), `.egt` (texture
//! morphs), `.tri` (animated morph targets).
//!
//! These three sidecar files live alongside a race base head NIF
//! (e.g. `meshes\characters\head\headhuman.{egm,egt,tri}`) and feed
//! the FaceGen runtime evaluator that the legacy engine uses to
//! generate per-NPC head meshes from FGGS / FGGA / FGTS slider arrays
//! on `NpcRecord`. ByroRedux M41.0 Phase 3b consumes the EGM output;
//! Phase 3c consumes the EGT compositor output.
//!
//! ## Format references
//!
//! All three formats are FaceGen-SDK proprietary and **not in
//! nif.xml**. Layouts here are reverse-engineered from real vanilla
//! FNV files (verified by exact-byte-count round-trip in the unit
//! tests below) — `headhuman.egm` (695 904 bytes) decomposes
//! exactly into `64 + 80 morphs × (4 + 1449 verts × 6 bytes)` and
//! `headhuman.egt` (9 830 664 bytes) into
//! `64 + 50 morphs × (4 + 256 × 256 × 3)`.
//!
//! No `unsafe`. No external deps beyond `thiserror` for the error
//! type. Half-float decoding is hand-rolled (`half_to_f32` below)
//! to avoid pulling in the `half` crate for a 30-line algorithm.

pub mod egm;
pub mod egt;
pub mod tri;

pub use egm::{EgmFile, EgmMorph};
pub use egt::{EgtFile, EgtMorph};
pub use tri::TriHeader;

/// Errors surfaced by all three FaceGen sidecar parsers. Variants
/// describe the structural failure shape (truncated, wrong magic,
/// inconsistent counts) — the calling layer wraps them with file
/// path context.
#[derive(Debug, thiserror::Error)]
pub enum FaceGenError {
    /// First 8 bytes of the file don't match the expected magic
    /// string (`FREGM002` / `FREGT003` / `FRTRI003`).
    #[error("FaceGen magic mismatch: expected '{expected}', got {found:?}")]
    BadMagic {
        expected: &'static str,
        found: Vec<u8>,
    },
    /// File ended before the parser finished consuming all declared
    /// morphs / vertices / pixels.
    #[error(
        "FaceGen truncated: needed {needed} bytes at offset {offset}, file is {file_len} bytes"
    )]
    Truncated {
        needed: usize,
        offset: usize,
        file_len: usize,
    },
    /// Header field declared a count incompatible with the rest of
    /// the file's size — e.g. EGM with `num_vertices = 0` or
    /// `num_morphs > 1024`. Caps are conservative; raise as content
    /// proves them too tight.
    #[error("FaceGen header inconsistent: {0}")]
    InconsistentHeader(String),
}

/// Decode an IEEE 754 binary16 ("half-float") into f32.
///
/// Mirrors `byroredux_nif::import::mesh::half_to_f32` — re-declared
/// here so this crate doesn't depend on `byroredux-nif`'s internals.
/// Subnormals are normalised; NaN payloads are preserved.
#[inline]
pub fn half_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as i32;
    let mant = (h & 0x3FF) as u32;
    let bits = if exp == 0 {
        if mant == 0 {
            sign << 31
        } else {
            // Subnormal — normalise.
            let mut m = mant;
            let mut e = -14_i32;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            m &= 0x3FF;
            (sign << 31) | (((e + 127) as u32) << 23) | (m << 13)
        }
    } else if exp == 31 {
        // Inf / NaN — preserve mantissa for NaN payloads.
        (sign << 31) | (0xFFu32 << 23) | (mant << 13)
    } else {
        (sign << 31) | (((exp - 15 + 127) as u32) << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}

/// Read a little-endian `u32` at `offset` from `bytes`. Returns
/// `Truncated` when the read would run past the buffer end.
pub(crate) fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, FaceGenError> {
    if offset + 4 > bytes.len() {
        return Err(FaceGenError::Truncated {
            needed: 4,
            offset,
            file_len: bytes.len(),
        });
    }
    Ok(u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

/// Read a little-endian `f32` at `offset` from `bytes`.
pub(crate) fn read_f32_le(bytes: &[u8], offset: usize) -> Result<f32, FaceGenError> {
    Ok(f32::from_bits(read_u32_le(bytes, offset)?))
}

/// Read a little-endian `u16` at `offset`.
pub(crate) fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16, FaceGenError> {
    if offset + 2 > bytes.len() {
        return Err(FaceGenError::Truncated {
            needed: 2,
            offset,
            file_len: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

#[cfg(test)]
mod tests {
    use super::half_to_f32;

    #[test]
    fn half_to_f32_canonical_values() {
        // 0x3C00 = 1.0
        assert_eq!(half_to_f32(0x3C00), 1.0);
        // 0xC000 = -2.0
        assert_eq!(half_to_f32(0xC000), -2.0);
        // 0x0000 = +0.0
        assert_eq!(half_to_f32(0x0000), 0.0);
        // 0x8000 = -0.0
        assert_eq!(half_to_f32(0x8000).to_bits(), (-0.0_f32).to_bits());
        // 0x7C00 = +inf
        assert!(half_to_f32(0x7C00).is_infinite() && half_to_f32(0x7C00).is_sign_positive());
        // 0xFC00 = -inf
        assert!(half_to_f32(0xFC00).is_infinite() && half_to_f32(0xFC00).is_sign_negative());
        // Smallest subnormal: 0x0001 = 2^-24 ≈ 5.96e-8
        let subnormal = half_to_f32(0x0001);
        assert!(subnormal > 0.0 && subnormal < 1e-7);
    }
}
