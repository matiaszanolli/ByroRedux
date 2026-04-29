//! `.tri` — FaceGen animated morph-target sidecar (header-only stub).
//!
//! `.tri` files carry animated face morphs (talk shapes, blinks,
//! emotion presets) keyed by string label. Format is significantly
//! richer than `.egm` / `.egt` (per-morph variable-length name +
//! multiple modifier classes + diff/abs delta toggles) and the full
//! parse isn't on the M41.0 critical path — Phase 3b uses EGM for
//! geometry morphs at NPC load time, the `.tri` plays a different
//! role (lip-sync / facial expression). Filed as a follow-up
//! milestone (M47-tier work, dialogue + AI animation).
//!
//! ## Format (FRTRI003 — header only)
//!
//! ```text
//! struct Header {                  // ≥ 64 bytes
//!     magic: [u8; 8],              // "FRTRI003"
//!     num_vertices:    u32,        //   matches base mesh
//!     num_triangles:   u32,
//!     unknown_a: [u32; 3],         //   zero on vanilla
//!     num_quads:       u32,        //   typically 0 on faces (tri-only)
//!     num_modifiers:   u32,        //   number of named morph categories
//!     num_modifier_verts: u32,     //   verts driven by modifiers
//!     num_uv_coords:   u32,
//!     unknown_b:       u32,
//!     // ... more fields, then the body (vertices, faces,
//!     //     modifier blocks, …) — deferred to a future milestone
//! }
//! ```
//!
//! Header verified on vanilla FNV `headhuman.tri` (359 972 bytes,
//! `num_vertices = 1211`, `num_triangles = 2294` — matching
//! headhuman.nif's geometry exactly).

use crate::{read_u32_le, FaceGenError};

const TRI_MAGIC: &[u8; 8] = b"FRTRI003";
const HEADER_BYTES: usize = 64;

/// Parsed header of a `.tri` file. Stops at byte 64 — the body
/// (per-vertex data, modifier blocks, named morph targets) will be
/// extracted by a follow-up milestone when lip-sync / expression
/// animation lands.
#[derive(Debug, Clone)]
pub struct TriHeader {
    pub num_vertices: u32,
    pub num_triangles: u32,
    pub num_quads: u32,
    pub num_modifiers: u32,
    pub num_modifier_vertices: u32,
    pub num_uv_coords: u32,
    /// Trailing header fields kept opaque so future parser work
    /// doesn't have to decode them retroactively. Indexed by their
    /// little-endian u32 offset.
    pub unknown_words: [u32; 9],
}

impl TriHeader {
    /// Parse the 64-byte `.tri` header. Body bytes (vertex /
    /// triangle / modifier tables) past the header are not consumed.
    pub fn parse(bytes: &[u8]) -> Result<Self, FaceGenError> {
        if bytes.len() < HEADER_BYTES {
            return Err(FaceGenError::Truncated {
                needed: HEADER_BYTES,
                offset: 0,
                file_len: bytes.len(),
            });
        }
        if &bytes[..8] != TRI_MAGIC {
            return Err(FaceGenError::BadMagic {
                expected: "FRTRI003",
                found: bytes[..8].to_vec(),
            });
        }

        let num_vertices = read_u32_le(bytes, 8)?;
        let num_triangles = read_u32_le(bytes, 12)?;
        // Bytes 16..28 are three opaque u32s (zero on vanilla).
        let unk_a0 = read_u32_le(bytes, 16)?;
        let unk_a1 = read_u32_le(bytes, 20)?;
        let unk_a2 = read_u32_le(bytes, 24)?;
        // 28..32 = num_modifier_vertices (vanilla headhuman.tri = 1211, same as num_vertices).
        let num_modifier_vertices = read_u32_le(bytes, 28)?;
        let num_modifiers = read_u32_le(bytes, 32)?;
        let num_uv_coords = read_u32_le(bytes, 36)?;
        let num_quads = read_u32_le(bytes, 40)?;
        // Trailing words (44..64) are also opaque on vanilla.
        let unk_b0 = read_u32_le(bytes, 44)?;
        let unk_b1 = read_u32_le(bytes, 48)?;
        let unk_b2 = read_u32_le(bytes, 52)?;
        let unk_b3 = read_u32_le(bytes, 56)?;
        let unk_b4 = read_u32_le(bytes, 60)?;

        Ok(Self {
            num_vertices,
            num_triangles,
            num_quads,
            num_modifiers,
            num_modifier_vertices,
            num_uv_coords,
            unknown_words: [
                unk_a0, unk_a1, unk_a2, unk_b0, unk_b1, unk_b2, unk_b3, unk_b4, 0,
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth_tri_header(num_vertices: u32, num_triangles: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_BYTES);
        out.extend_from_slice(b"FRTRI003");
        out.extend_from_slice(&num_vertices.to_le_bytes());
        out.extend_from_slice(&num_triangles.to_le_bytes());
        out.extend_from_slice(&[0u8; 12]); // unknown_a triple
        out.extend_from_slice(&num_vertices.to_le_bytes()); // modifier_verts
        out.extend_from_slice(&1u32.to_le_bytes()); // num_modifiers
        out.extend_from_slice(&38u32.to_le_bytes()); // num_uv_coords
        out.extend_from_slice(&8u32.to_le_bytes()); // num_quads
        // Pad trailing 5 × u32 unknowns to 64 bytes.
        out.extend_from_slice(&238u32.to_le_bytes());
        out.extend_from_slice(&[0u8; 16]);
        debug_assert_eq!(out.len(), HEADER_BYTES);
        out
    }

    #[test]
    fn parses_synthetic_header() {
        let bytes = synth_tri_header(1211, 2294);
        let hdr = TriHeader::parse(&bytes).expect("parse");
        assert_eq!(hdr.num_vertices, 1211);
        assert_eq!(hdr.num_triangles, 2294);
        assert_eq!(hdr.num_modifier_vertices, 1211);
        assert_eq!(hdr.num_modifiers, 1);
        assert_eq!(hdr.num_uv_coords, 38);
        assert_eq!(hdr.num_quads, 8);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = synth_tri_header(2, 2);
        bytes[0] = b'Z';
        let err = TriHeader::parse(&bytes).unwrap_err();
        assert!(matches!(err, FaceGenError::BadMagic { .. }));
    }

    #[test]
    fn rejects_truncated() {
        let bytes = vec![0u8; 32];
        let err = TriHeader::parse(&bytes).unwrap_err();
        assert!(matches!(err, FaceGenError::Truncated { .. }));
    }
}
