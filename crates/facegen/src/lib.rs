//! FaceGen sidecar parsers — `.egm` (geometry morphs), `.egt` (texture
//! morphs), `.tri` (animated morph targets).
//!
//! These three sidecar files live alongside a race base head NIF
//! (e.g. `meshes\characters\head\headhuman.{egm,egt,tri}`) and feed
//! the FaceGen runtime evaluator that the legacy engine uses to
//! generate per-NPC head meshes from FGGS / FGGA / FGTS slider arrays
//! on `NpcRecord`. ByroRedux M41.0 Phase 3b/3c consumes the EGM output
//! (symmetric FGGS + asymmetric FGGA geometry morphs — see
//! [`eval::apply_morphs`]).
//!
//! #3544 (SK-D3-02) — `egt`/`tri` are parsed but **have no consumer**:
//! this crate exports `EgtFile`/`EgtMorph`/`TriHeader`, but nothing in
//! the workspace reads them. There is no EGT texture-morph compositor
//! today — the runtime-recipe games (Oblivion, FO3/FNV) parse `FGTS`
//! slider weights onto `NpcRecord` (see
//! `crates/plugin/src/esm/records/actor/mod.rs::fgts`) but nothing
//! blends them into the base diffuse texture yet, and `.tri`'s body
//! parse (talk shapes / blinks / expression morphs) is deferred to a
//! future milestone (see `tri`'s own module doc). Both are real,
//! measured feature gaps for per-NPC complexion — not yet wired, not
//! silently working.
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
pub mod eval;
pub mod tri;

pub use egm::{EgmFile, EgmMorph};
pub use egt::{EgtFile, EgtMorph};
pub use eval::apply_morphs;
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
/// here so this crate doesn't depend on `byroredux-nif`'s internals
/// (the canonical impl is `pub(crate)` there, and this crate is
/// deliberately dependency-light). Subnormals are normalised; NaN
/// payloads are preserved.
///
/// #2599 — the two copies are pinned bit-for-bit across all 65_536
/// `u16` inputs by `facegen_half_to_f32_copy_matches_canonical_bit_for_bit`
/// in `byroredux-nif`'s `import::mesh::decode_half_float_tests`. Any
/// edit here that changes behaviour fails that test until the canonical
/// decoder is updated to match (and vice versa).
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

    /// #3544 (SK-D3-02) — `EgtFile`/`EgtMorph`/`TriHeader` have no
    /// consumer anywhere in the workspace; the crate doc used to claim
    /// the EGT compositor phase was already consumed, as though that
    /// compositor shipped. Pins both halves of the correction: the
    /// stale claim is gone, and the honest deferral marker (naming this
    /// issue, so a future reader who greps for it lands here) is
    /// present in all three docs it needs to be in.
    ///
    /// NOTE for future editors: this test's own doc comment and
    /// assertion message deliberately never spell out the stale claim's
    /// full text — `include_str!("lib.rs")` embeds this whole file
    /// including the test module, so the scan is restricted to the
    /// portion before `#[cfg(test)]` specifically to avoid matching its
    /// own describing prose instead of the real (now-fixed) doc.
    #[test]
    fn facegen_docs_do_not_overclaim_an_egt_compositor() {
        let lib_rs_module_doc = include_str!("lib.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("split always yields at least one piece");
        let egt_rs = include_str!("egt.rs");
        let tri_rs = include_str!("tri.rs");

        assert!(
            !lib_rs_module_doc.contains("Phase 3c consumes the EGT compositor output"),
            "the stale claim that a compositor exists must not come back"
        );
        for (name, src) in [
            ("lib.rs", lib_rs_module_doc),
            ("egt.rs", egt_rs),
            ("tri.rs", tri_rs),
        ] {
            assert!(
                src.contains("#3544"),
                "{name} must carry the #3544 deferral marker"
            );
        }
    }
}
