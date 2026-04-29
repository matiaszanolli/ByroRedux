//! `.egt` — FaceGen texture-morph sidecar.
//!
//! Companion file to a race base head NIF; stores per-pixel RGB
//! deltas for the 50 `FGTS` (texture morph) sliders on `NpcRecord`.
//! The face-tint compositor (M41.0 Phase 3c) blends them into the
//! base diffuse texture at NPC load time.
//!
//! ## Format (FREGT003)
//!
//! ```text
//! struct Header {                  // 64 bytes total
//!     magic: [u8; 8],              // "FREGT003"
//!     width:  u32,                 // texture width  (256 vanilla)
//!     height: u32,                 // texture height (256 vanilla)
//!     num_morphs:    u32,          // == FGTS slot count (50 vanilla)
//!     unknown_a: u32,              // zero on vanilla
//!     unknown_b: u32,              // 0x51 on headhuman.egt — opaque
//!     padding: [u8; 32],           // zero
//! }
//! struct Morph {
//!     scale: f32,                  // delta = scale * (rgb - 128) / 128 (signed-byte interpretation)
//!     pixels: [[u8; 3]; width * height],
//! }
//! file = Header ++ [Morph; num_morphs]
//! ```
//!
//! Verified against vanilla FNV `headhuman.egt` (9 830 664 bytes,
//! 256×256, 50 morphs): exact match for
//! `64 + 50 × (4 + 256 × 256 × 3) = 9 830 664`.
//!
//! Note: the EGT file ships under `meshes\characters\head\` in the
//! Meshes BSA (NOT Textures BSA), per the FNV vanilla layout —
//! despite the texture-morph semantic, FaceGen co-locates all four
//! sidecars (`.nif .egm .egt .tri`) with the base head NIF so the
//! engine resolves them as a unit.

use crate::{read_f32_le, read_u32_le, FaceGenError};

const EGT_MAGIC: &[u8; 8] = b"FREGT003";
const HEADER_BYTES: usize = 64;
const RGB_BYTES: usize = 3;
const MAX_TEXTURE_DIM: u32 = 4096;
const MAX_MORPHS: u32 = 1024;

/// One FGTS texture-morph delta plane.
///
/// The compositor (Phase 3c) applies it as
/// `pixel' = pixel + scale * (raw_rgb - 128) * weight / 128`, where
/// `raw_rgb` is read as signed bytes (the 0..255 wire range maps to
/// −1..+1 normalised after subtracting the 128 midpoint).
#[derive(Debug, Clone)]
pub struct EgtMorph {
    pub scale: f32,
    /// Raw RGB bytes, `width × height` entries. Storage stays in u8
    /// to keep the in-memory size identical to on-disk; the
    /// compositor decodes the signed-midpoint form lazily.
    pub pixels: Vec<[u8; 3]>,
}

/// Parsed `.egt` file. The list of morphs maps 1:1 to the
/// `runtime_facegen.fgts` slider array on `NpcRecord` (50 entries
/// on vanilla FNV / FO3).
#[derive(Debug, Clone)]
pub struct EgtFile {
    pub width: u32,
    pub height: u32,
    /// Two opaque header u32s preserved for round-trip / debugging.
    /// Vanilla FNV headhuman.egt has `unknown_a = 0`,
    /// `unknown_b = 0x51`. FaceGen-SDK internal; not consumed by
    /// the compositor.
    pub unknown_a: u32,
    pub unknown_b: u32,
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

        let width = read_u32_le(bytes, 8)?;
        let height = read_u32_le(bytes, 12)?;
        let num_morphs = read_u32_le(bytes, 16)?;
        let unknown_a = read_u32_le(bytes, 20)?;
        let unknown_b = read_u32_le(bytes, 24)?;
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

        let pixels_per_morph = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| {
                FaceGenError::InconsistentHeader(format!(
                    "width × height overflow: {}×{}",
                    width, height,
                ))
            })?;
        let bytes_per_morph = 4 + pixels_per_morph * RGB_BYTES;
        let needed = HEADER_BYTES + (num_morphs as usize) * bytes_per_morph;
        if bytes.len() != needed {
            return Err(FaceGenError::InconsistentHeader(format!(
                "file size {} bytes != expected {} \
                 (header {} + {} morphs × ({} scale + {}×{} pixels × {} bytes))",
                bytes.len(),
                needed,
                HEADER_BYTES,
                num_morphs,
                4,
                width,
                height,
                RGB_BYTES,
            )));
        }

        let mut fgts_morphs = Vec::with_capacity(num_morphs as usize);
        let mut offset = HEADER_BYTES;
        for _ in 0..num_morphs {
            let scale = read_f32_le(bytes, offset)?;
            offset += 4;
            let mut pixels = Vec::with_capacity(pixels_per_morph);
            for _ in 0..pixels_per_morph {
                pixels.push([bytes[offset], bytes[offset + 1], bytes[offset + 2]]);
                offset += RGB_BYTES;
            }
            fgts_morphs.push(EgtMorph { scale, pixels });
        }

        Ok(Self {
            width,
            height,
            unknown_a,
            unknown_b,
            fgts_morphs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth_egt(width: u32, height: u32, num_morphs: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"FREGT003");
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend_from_slice(&num_morphs.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // unknown_a
        out.extend_from_slice(&0x51u32.to_le_bytes()); // unknown_b (matches vanilla)
        // 28..64 = 36 bytes of padding (header total = 64).
        out.extend_from_slice(&[0u8; 36]);
        debug_assert_eq!(out.len(), 64);
        for m in 0..num_morphs {
            out.extend_from_slice(&((m as f32) + 1.0).to_le_bytes()); // scale
            for _ in 0..(width as usize * height as usize) {
                out.push((m & 0xFF) as u8);
                out.push(0x80);
                out.push(((m + 1) & 0xFF) as u8);
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
        assert_eq!(egt.fgts_morphs.len(), 1);
        assert_eq!(egt.fgts_morphs[0].scale, 1.0);
        assert_eq!(egt.fgts_morphs[0].pixels.len(), 4);
        assert_eq!(egt.fgts_morphs[0].pixels[0], [0, 0x80, 1]);
    }

    #[test]
    fn parses_synthetic_vanilla_shape() {
        // Mirrors vanilla FNV headhuman.egt: 256×256, 50 morphs.
        // Total = 64 + 50 × (4 + 65536 × 3) = 9_830_664.
        let bytes = synth_egt(256, 256, 50);
        assert_eq!(bytes.len(), 9_830_664);
        let egt = EgtFile::parse(&bytes).expect("parse");
        assert_eq!(egt.fgts_morphs.len(), 50);
        for morph in &egt.fgts_morphs {
            assert_eq!(morph.pixels.len(), 65_536);
        }
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
