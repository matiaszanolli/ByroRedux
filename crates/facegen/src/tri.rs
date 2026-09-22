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
//! #3544 (SK-D3-02) — even the header parsed here has **no
//! consumer**: nothing in the workspace reads `TriHeader` outside
//! this crate's own tests. SIBLING note to `egt`'s doc — same shape,
//! filed together.
//!
//! ## Format (FRTRI003 — header only, per the FaceGen SDK)
//!
//! The SDK names the ten header words, in order:
//! `V, T, Q, LV, LS, X, ext, Md, Ms, K`. We keep those letters as the
//! field names and assert only the meanings the vanilla data pins
//! down — `V` matches headhuman.nif's vertex count, `T` its triangle
//! count, and `Q` (quad faces) is 0 on faces. The remaining letters
//! are stored verbatim; fabricating semantics for them was exactly
//! the pre-#4668 bug (X was being read as "modifier vertices", Md as
//! "uv coords", Ms as "quards", ext as "modifiers").
//!
//! ```text
//! struct Header {                  // 64 bytes read; body deferred
//!     magic: [u8; 8],              // "FRTRI003"
//!     V: u32, T: u32, Q: u32,
//!     LV: u32, LS: u32, X: u32,
//!     ext: u32, Md: u32, Ms: u32, K: u32,
//!     // 24 further bytes, then the body (vertices, faces,
//!     // modifier blocks, …) — deferred to a future milestone
//! }
//! ```
//!
//! Vanilla FNV `headhuman.tri` words (359 972 bytes):
//! `1211, 2294, 0, 0, 0, 1211, 1, 38, 8, 238` — V/T match
//! headhuman.nif's geometry exactly.

use crate::{read_u32_le, FaceGenError};

const TRI_MAGIC: &[u8; 8] = b"FRTRI003";
const HEADER_BYTES: usize = 64;

/// Parsed header of a `.tri` file. Stops at byte 64 — the body
/// (per-vertex data, modifier blocks, named morph targets) will be
/// extracted by a follow-up milestone when lip-sync / expression
/// animation lands. Fields carry the SDK's own letters (#4668); only
/// V/T/Q have documented meanings, pinned by the vanilla words.
#[derive(Debug, Clone)]
pub struct TriHeader {
    /// `V` — vertices in the base mesh (vanilla 1211).
    pub num_vertices: u32,
    /// `T` — triangles (vanilla 2294).
    pub num_triangles: u32,
    /// `Q` — quad faces; 0 on vanilla faces (tri-only).
    pub num_quads: u32,
    /// `LV` — SDK letter, meaning not pinned (vanilla 0).
    pub lv: u32,
    /// `LS` — SDK letter, meaning not pinned (vanilla 0).
    pub ls: u32,
    /// `X` — SDK letter, meaning not pinned (vanilla 1211).
    pub x: u32,
    /// `ext` — SDK letter, meaning not pinned (vanilla 1).
    pub ext: u32,
    /// `Md` — SDK letter, meaning not pinned (vanilla 38).
    pub md: u32,
    /// `Ms` — SDK letter, meaning not pinned (vanilla 8).
    pub ms: u32,
    /// `K` — SDK letter, meaning not pinned (vanilla 238).
    pub k: u32,
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

        // The ten SDK words, in order: V T Q LV LS X ext Md Ms K (#4668).
        let words: [u32; 10] = {
            let mut out = [0u32; 10];
            for (i, word) in out.iter_mut().enumerate() {
                *word = read_u32_le(bytes, 8 + i * 4)?;
            }
            out
        };
        let [num_vertices, num_triangles, num_quads, lv, ls, x, ext, md, ms, k] = words;

        Ok(Self {
            num_vertices,
            num_triangles,
            num_quads,
            lv,
            ls,
            x,
            ext,
            md,
            ms,
            k,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth_tri_header(words: [u32; 10]) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_BYTES);
        out.extend_from_slice(b"FRTRI003");
        for word in words {
            out.extend_from_slice(&word.to_le_bytes());
        }
        // The ten words end at byte 48; pad the remaining 16 header bytes.
        out.extend_from_slice(&[0u8; 16]);
        debug_assert_eq!(out.len(), HEADER_BYTES);
        out
    }

    /// The vanilla `headhuman.tri` header words, verbatim from the audit.
    const VANILLA_WORDS: [u32; 10] = [1211, 2294, 0, 0, 0, 1211, 1, 38, 8, 238];

    /// #4668 — the SDK field order is V T Q LV LS X ext Md Ms K, and the
    /// vanilla words must land in exactly those fields. The old parser
    /// read the same bytes but named them num_modifier_vertices (X),
    /// num_modifiers (ext), num_uv_coords (Md) and num_quads (Ms) —
    /// fabricated semantics, with the real Q/LV/LS/K words discarded.
    #[test]
    fn vanilla_words_land_in_the_sdk_order() {
        let bytes = synth_tri_header(VANILLA_WORDS);
        let hdr = TriHeader::parse(&bytes).expect("parse");
        assert_eq!(hdr.num_vertices, 1211);
        assert_eq!(hdr.num_triangles, 2294);
        assert_eq!(hdr.num_quads, 0);
        assert_eq!(hdr.lv, 0);
        assert_eq!(hdr.ls, 0);
        assert_eq!(hdr.x, 1211);
        assert_eq!(hdr.ext, 1);
        assert_eq!(hdr.md, 38);
        assert_eq!(hdr.ms, 8);
        assert_eq!(hdr.k, 238);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = synth_tri_header([2, 2, 0, 0, 0, 2, 1, 8, 0, 238]);
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
