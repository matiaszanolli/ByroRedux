//! `.egt` — FaceGen texture-morph sidecar.
//!
//! Companion file to a race base head NIF; stores per-pixel RGB
//! deltas for the 50 `FGTS` (texture morph) sliders on `NpcRecord`.
//!
//! #3544 (SK-D3-02) — **no consumer exists**. A future face-tint
//! compositor would blend these deltas into the base diffuse texture
//! at NPC load time, but nothing in the workspace calls into this
//! module today; `EgtFile`/`EgtMorph` are parsed and unit-tested only.
//!
//! ## Format (FREGT003, per the FaceGen SDK manual)
//!
//! ```text
//! struct Header {                  // 64 bytes total
//!     magic: [u8; 8],              // "FREGT003"
//!     R: u32,                      // image rows    (width;  256 vanilla)
//!     C: u32,                      // image columns (height; 256 vanilla)
//!     S: u32,                      // shape/morph count (50 vanilla)
//!     A: u32,                      // SDK word A (zero on vanilla; opaque)
//!     texture_basis_version: u32,  // 81 (0x51) on vanilla headhuman.egt
//!     padding: [u8; 32],           // zero
//! }
//! struct Mode {                    // one FGTS slider's delta
//!     scale: f32,
//!     r: [signed char; R * C],     // THREE CONTIGUOUS PLANES, red first —
//!     g: [signed char; R * C],     // NOT interleaved RGB triples
//!     b: [signed char; R * C],
//! }
//! file = Header ++ [Mode; S]
//! ```
//!
//! Samples are **signed chars** (two's-complement i8, zero-centred),
//! not offset-128 unsigned bytes. The audit's vanilla statistics agree:
//! 58.7% of `headhuman.egt` bytes sit within 16 of 0x00/0xFF versus
//! 0.8% near 0x80, and lag-1 correlation (0.993) beats lag-3 (0.952),
//! which is the planar signature — interleaving would make lag-3 the
//! same-channel neighbour. #4668 (PAR-D5-2026-09-21-04).
//!
//! Verified against vanilla FNV `headhuman.egt` (9 830 664 bytes,
//! 256×256, 50 morphs): exact match for
//! `64 + 50 × (4 + 3 × 256 × 256) = 9 830 664`.
//!
//! Note: the EGT file ships under `meshes\characters\head\` in the
//! Meshes BSA (NOT Textures BSA), per the FNV vanilla layout —
//! despite the texture-morph semantic, FaceGen co-locates all four
//! sidecars (`.nif .egm .egt .tri`) with the base head NIF so the
//! engine resolves them as a unit.

use crate::{read_f32_le, read_u32_le, FaceGenError};

const EGT_MAGIC: &[u8; 8] = b"FREGT003";
const HEADER_BYTES: usize = 64;
const PLANES: usize = 3;
const MAX_TEXTURE_DIM: u32 = 4096;
const MAX_MORPHS: u32 = 1024;

/// One FGTS texture-morph delta mode.
///
/// A future compositor (#3544, no implementation exists yet) decodes
/// each stored byte as a **signed char** (`byte as i8`, two's
/// complement, zero-centred) and applies
/// `pixel' = pixel + scale * (sample as f32) * weight / 128`. Storage
/// stays `u8` to keep the in-memory size identical to on-disk; the
/// signed interpretation is the reader's job, and the per-pixel
/// triple is assembled from the file's three contiguous planes so the
/// compositor never sees the planar layout.
#[derive(Debug, Clone)]
pub struct EgtMorph {
    pub scale: f32,
    /// Raw samples assembled per-pixel as `[r, g, b]` from the file's
    /// three contiguous `R × C` planes. Decode each byte as `i8`.
    pub pixels: Vec<[u8; 3]>,
}

/// Parsed `.egt` file. The list of modes maps 1:1 to the
/// `runtime_facegen.fgts` slider array on `NpcRecord` (50 entries
/// on vanilla FNV / FO3).
#[derive(Debug, Clone)]
pub struct EgtFile {
    /// SDK header word R — image width.
    pub width: u32,
    /// SDK header word C — image height.
    pub height: u32,
    /// SDK header word S — mode (morph) count.
    pub num_morphs: u32,
    /// SDK header word A. Zero on vanilla; opaque to us.
    pub unknown_a: u32,
    /// Texture Basis Version. 81 (0x51) on vanilla `headhuman.egt`.
    pub texture_basis_version: u32,
    pub fgts_morphs: Vec<EgtMorph>,
}

impl EgtFile {
    /// Parse an `.egt` file from its raw bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, FaceGenError> {
        if bytes.len() < HEADER_BYTES {
            return Err(FaceGenError::Truncated {
                needed: HEADER_BYTES,
                offset: 0,
                file_len: bytes.len(),
            });
        }
        if &bytes[..8] != EGT_MAGIC {
            return Err(FaceGenError::BadMagic {
                expected: "FREGT003",
                found: bytes[..8].to_vec(),
            });
        }

        let width = read_u32_le(bytes, 8)?; // R
        let height = read_u32_le(bytes, 12)?; // C
        let num_morphs = read_u32_le(bytes, 16)?; // S
        let unknown_a = read_u32_le(bytes, 20)?; // A
        let texture_basis_version = read_u32_le(bytes, 24)?;
        // Bytes 28..64 are padding; not validated.

        if width == 0 || height == 0 || width > MAX_TEXTURE_DIM || height > MAX_TEXTURE_DIM {
            return Err(FaceGenError::InconsistentHeader(format!(
                "width={} height={} (cap {}×{})",
                width, height, MAX_TEXTURE_DIM, MAX_TEXTURE_DIM,
            )));
        }
        if num_morphs > MAX_MORPHS {
            return Err(FaceGenError::InconsistentHeader(format!(
                "num_morphs={} (cap {})",
                num_morphs, MAX_MORPHS,
            )));
        }

        let samples_per_plane = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| {
                FaceGenError::InconsistentHeader(format!(
                    "width × height overflow: {}×{}",
                    width, height,
                ))
            })?;
        let bytes_per_mode = 4 + samples_per_plane * PLANES;
        let needed = HEADER_BYTES + (num_morphs as usize) * bytes_per_mode;
        if bytes.len() != needed {
            return Err(FaceGenError::InconsistentHeader(format!(
                "file size {} bytes != expected {} \
                 (header {} + {} modes × ({} scale + 3 planes × {}×{} samples))",
                bytes.len(),
                needed,
                HEADER_BYTES,
                num_morphs,
                4,
                width,
                height,
            )));
        }

        let mut fgts_morphs = Vec::with_capacity(num_morphs as usize);
        let mut offset = HEADER_BYTES;
        for _ in 0..num_morphs {
            let scale = read_f32_le(bytes, offset)?;
            offset += 4;
            // Planar: three contiguous R×C signed-char planes (r, g, b),
            // NOT interleaved triples (#4668).
            let planes = &bytes[offset..offset + samples_per_plane * PLANES];
            offset += samples_per_plane * PLANES;
            let (r_plane, rest) = planes.split_at(samples_per_plane);
            let (g_plane, b_plane) = rest.split_at(samples_per_plane);
            let mut pixels = Vec::with_capacity(samples_per_plane);
            for i in 0..samples_per_plane {
                pixels.push([r_plane[i], g_plane[i], b_plane[i]]);
            }
            fgts_morphs.push(EgtMorph { scale, pixels });
        }

        Ok(Self {
            width,
            height,
            num_morphs,
            unknown_a,
            texture_basis_version,
            fgts_morphs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Planar synth: per mode, `scale` then three contiguous R×C planes.
    fn synth_egt(width: u32, height: u32, num_morphs: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"FREGT003");
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend_from_slice(&num_morphs.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // A
        out.extend_from_slice(&81u32.to_le_bytes()); // texture basis version (vanilla)
                                                     // 28..64 = 36 bytes of padding (header total = 64).
        out.extend_from_slice(&[0u8; 36]);
        debug_assert_eq!(out.len(), 64);
        for m in 0..num_morphs {
            out.extend_from_slice(&((m as f32) + 1.0).to_le_bytes()); // scale
            let samples = (width as usize) * (height as usize);
            for plane in 0..3u8 {
                for i in 0..samples {
                    out.push(((m as usize + plane as usize + i) & 0xFF) as u8);
                }
            }
        }
        out
    }

    #[test]
    fn parses_synthetic_minimal() {
        let bytes = synth_egt(2, 2, 1);
        let egt = EgtFile::parse(&bytes).expect("parse");
        assert_eq!(egt.width, 2);
        assert_eq!(egt.height, 2);
        assert_eq!(egt.num_morphs, 1);
        assert_eq!(egt.unknown_a, 0);
        assert_eq!(egt.texture_basis_version, 81);
        assert_eq!(egt.fgts_morphs.len(), 1);
        assert_eq!(egt.fgts_morphs[0].scale, 1.0);
        assert_eq!(egt.fgts_morphs[0].pixels.len(), 4);
        // Planar layout: pixel 0 assembles sample 0 of each plane.
        // r samples: (0+0+i)&0xFF; g: (0+1+i)&0xFF; b: (0+2+i)&0xFF.
        assert_eq!(egt.fgts_morphs[0].pixels[0], [0, 1, 2]);
        assert_eq!(egt.fgts_morphs[0].pixels[1], [1, 2, 3]);
        assert_eq!(egt.fgts_morphs[0].pixels[2], [2, 3, 4]);
    }

    #[test]
    fn parses_synthetic_vanilla_shape() {
        // Mirrors vanilla FNV headhuman.egt: 256×256, 50 modes.
        // Total = 64 + 50 × (4 + 3 × 65536) = 9_830_664.
        let bytes = synth_egt(256, 256, 50);
        assert_eq!(bytes.len(), 9_830_664);
        let egt = EgtFile::parse(&bytes).expect("parse");
        assert_eq!(egt.fgts_morphs.len(), 50);
        for morph in &egt.fgts_morphs {
            assert_eq!(morph.pixels.len(), 65_536);
        }
    }

    /// #4668 — the vanilla header words (R=256, C=256, S=50, A=0,
    /// Texture Basis Version=81) must land in the fields the SDK names.
    /// A synthetic file carries them so the assertion runs without the
    /// game archive; the exact-size check makes the words load-bearing.
    #[test]
    fn vanilla_header_words_reach_their_named_fields() {
        let bytes = synth_egt(256, 256, 50);
        let egt = EgtFile::parse(&bytes).expect("parse");
        assert_eq!(egt.width, 256);
        assert_eq!(egt.height, 256);
        assert_eq!(egt.num_morphs, 50);
        assert_eq!(egt.unknown_a, 0);
        assert_eq!(egt.texture_basis_version, 81);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = synth_egt(2, 2, 1);
        bytes[0] = b'Z';
        let err = EgtFile::parse(&bytes).unwrap_err();
        assert!(matches!(err, FaceGenError::BadMagic { .. }));
    }

    #[test]
    fn rejects_zero_dim() {
        let bytes = synth_egt(0, 2, 1);
        let err = EgtFile::parse(&bytes).unwrap_err();
        assert!(matches!(err, FaceGenError::InconsistentHeader(_)));
    }

    #[test]
    fn rejects_truncated() {
        let mut bytes = synth_egt(2, 2, 1);
        bytes.truncate(bytes.len() - 3);
        let err = EgtFile::parse(&bytes).unwrap_err();
        assert!(matches!(err, FaceGenError::InconsistentHeader(_)));
    }
}
