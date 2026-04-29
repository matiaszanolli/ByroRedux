//! `.egm` — FaceGen geometry-morph sidecar.
//!
//! Companion file to a race base head NIF (e.g. headhuman.egm sits
//! next to headhuman.nif in `meshes\characters\head\`). Stores
//! per-vertex displacement deltas for symmetric and asymmetric face
//! morphs that the engine evaluates at NPC-load time against the
//! `FGGS` (50 sym weights) and `FGGA` (30 asym weights) slider
//! arrays on `NpcRecord`.
//!
//! ## Format (FREGM002)
//!
//! ```text
//! struct Header {                  // 64 bytes total
//!     magic: [u8; 8],              // "FREGM002"
//!     num_vertices:    u32,        //   verified == base head NIF's vertex count
//!     num_sym_morphs:  u32,        //   == FGGS slot count (50 vanilla)
//!     num_asym_morphs: u32,        //   == FGGA slot count (30 vanilla)
//!     geometry_basis: f32,         //   morph-space basis radius (FaceGen-internal)
//!     padding:        [u8; 40],    //   zero
//! }
//! struct Morph {
//!     scale:  f32,                 //   delta = scale * normalized_f16
//!     deltas: [Vec3<f16>; num_vertices],
//! }
//! file = Header
//!      ++ [Morph; num_sym_morphs]
//!      ++ [Morph; num_asym_morphs]
//! ```
//!
//! Verified against vanilla FNV `headhuman.egm` (695 904 bytes,
//! 1449 verts, 50 sym + 30 asym): exact match for
//! `64 + 80 × (4 + 1449 × 6) = 695 904`.

use crate::{half_to_f32, read_f32_le, read_u32_le, FaceGenError};

const EGM_MAGIC: &[u8; 8] = b"FREGM002";
const HEADER_BYTES: usize = 64;
/// Bytes per vertex delta — 3 × half-float.
const DELTA_BYTES: usize = 6;
/// Defensive caps. Vanilla FNV has 1449 vertices and 50+30 morphs;
/// these limits leave plenty of headroom for modded base heads while
/// still rejecting obviously-corrupt counts.
const MAX_VERTICES: u32 = 1 << 20;
const MAX_MORPHS: u32 = 1024;

/// One symmetric or asymmetric morph delta table.
///
/// Rendering applies the morph to a vertex `v_i` as
/// `v_i' = v_i + scale * deltas[i] * weight`, summed across every
/// active morph at the slider value `weight`. The `scale` is a
/// per-morph normalisation that lets the f16 deltas stay in a
/// compact range without losing precision.
#[derive(Debug, Clone)]
pub struct EgmMorph {
    /// Per-morph scale (multiplies the f16 delta before adding to
    /// the base vertex).
    pub scale: f32,
    /// Per-vertex displacement, decoded to f32. `deltas.len() ==
    /// EgmFile::num_vertices`. Each entry is `[dx, dy, dz]` in the
    /// same coordinate frame as the base mesh (NIF Z-up; the engine
    /// converts to Y-up downstream the same way it does for the NIF
    /// vertices themselves).
    pub deltas: Vec<[f32; 3]>,
}

/// Parsed `.egm` file. Carries a flat list of morphs split into a
/// symmetric prefix (`fggs_morphs`) and asymmetric tail
/// (`fgga_morphs`) — the on-disk order is `sym ++ asym` and the
/// split index equals the header's `num_sym_morphs`.
#[derive(Debug, Clone)]
pub struct EgmFile {
    pub num_vertices: u32,
    /// FaceGen geometry-basis radius — opaque metadata, preserved
    /// for round-trip but not consumed by the evaluator. Vanilla
    /// values are wildly varied (the headhuman.egm bytes are
    /// `0x7745c425` ≈ 4.18e+33, suggesting either an SDK-internal
    /// hash or repurposed padding rather than a meaningful float).
    pub geometry_basis: f32,
    pub fggs_morphs: Vec<EgmMorph>,
    pub fgga_morphs: Vec<EgmMorph>,
}

impl EgmFile {
    /// Parse an `.egm` file from its raw bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, FaceGenError> {
        if bytes.len() < HEADER_BYTES {
            return Err(FaceGenError::Truncated {
                needed: HEADER_BYTES,
                offset: 0,
                file_len: bytes.len(),
            });
        }
        if &bytes[..8] != EGM_MAGIC {
            return Err(FaceGenError::BadMagic {
                expected: "FREGM002",
                found: bytes[..8].to_vec(),
            });
        }

        let num_vertices = read_u32_le(bytes, 8)?;
        let num_sym = read_u32_le(bytes, 12)?;
        let num_asym = read_u32_le(bytes, 16)?;
        let geometry_basis = read_f32_le(bytes, 20)?;
        // Bytes 24..64 are padding; we don't validate them (vanilla
        // files have varying garbage there).

        if num_vertices == 0 || num_vertices > MAX_VERTICES {
            return Err(FaceGenError::InconsistentHeader(format!(
                "num_vertices={} (cap {})",
                num_vertices, MAX_VERTICES,
            )));
        }
        if num_sym > MAX_MORPHS || num_asym > MAX_MORPHS {
            return Err(FaceGenError::InconsistentHeader(format!(
                "morph counts sym={} asym={} (cap {})",
                num_sym, num_asym, MAX_MORPHS,
            )));
        }

        let total_morphs = num_sym as usize + num_asym as usize;
        let nv = num_vertices as usize;
        let bytes_per_morph = 4 + nv * DELTA_BYTES;
        let needed = HEADER_BYTES + total_morphs * bytes_per_morph;
        if bytes.len() != needed {
            return Err(FaceGenError::InconsistentHeader(format!(
                "file size {} bytes != expected {} \
                 (header {} + {} morphs × ({} scale + {} verts × {} bytes))",
                bytes.len(),
                needed,
                HEADER_BYTES,
                total_morphs,
                4,
                nv,
                DELTA_BYTES,
            )));
        }

        let mut fggs_morphs = Vec::with_capacity(num_sym as usize);
        let mut fgga_morphs = Vec::with_capacity(num_asym as usize);
        let mut offset = HEADER_BYTES;
        for morph_idx in 0..total_morphs {
            let scale = read_f32_le(bytes, offset)?;
            offset += 4;
            let mut deltas = Vec::with_capacity(nv);
            for _ in 0..nv {
                // Three half-floats per vertex, little-endian.
                let dx = half_to_f32(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]));
                let dy = half_to_f32(u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]]));
                let dz = half_to_f32(u16::from_le_bytes([bytes[offset + 4], bytes[offset + 5]]));
                deltas.push([dx, dy, dz]);
                offset += DELTA_BYTES;
            }
            let morph = EgmMorph { scale, deltas };
            if morph_idx < num_sym as usize {
                fggs_morphs.push(morph);
            } else {
                fgga_morphs.push(morph);
            }
        }

        Ok(Self {
            num_vertices,
            geometry_basis,
            fggs_morphs,
            fgga_morphs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic `.egm` byte buffer for tests. Each morph's
    /// scale is `1.0` and every delta is the f16 representation of
    /// `0.5` so the parser's f16→f32 path is exercised.
    fn synth_egm(num_vertices: u32, num_sym: u32, num_asym: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"FREGM002");
        out.extend_from_slice(&num_vertices.to_le_bytes());
        out.extend_from_slice(&num_sym.to_le_bytes());
        out.extend_from_slice(&num_asym.to_le_bytes());
        out.extend_from_slice(&1.0f32.to_le_bytes()); // basis
        out.extend_from_slice(&[0u8; 40]); // padding to 64

        // f16 0.5 = 0x3800 (sign 0, exp 14, mant 0).
        let half_05: u16 = 0x3800;
        let total_morphs = (num_sym + num_asym) as usize;
        for _ in 0..total_morphs {
            out.extend_from_slice(&2.0f32.to_le_bytes()); // scale
            for _ in 0..num_vertices {
                out.extend_from_slice(&half_05.to_le_bytes());
                out.extend_from_slice(&half_05.to_le_bytes());
                out.extend_from_slice(&half_05.to_le_bytes());
            }
        }
        out
    }

    #[test]
    fn parses_synthetic_minimal() {
        let bytes = synth_egm(2, 1, 0);
        let egm = EgmFile::parse(&bytes).expect("parse");
        assert_eq!(egm.num_vertices, 2);
        assert_eq!(egm.fggs_morphs.len(), 1);
        assert_eq!(egm.fgga_morphs.len(), 0);
        assert_eq!(egm.fggs_morphs[0].scale, 2.0);
        assert_eq!(egm.fggs_morphs[0].deltas.len(), 2);
        assert_eq!(egm.fggs_morphs[0].deltas[0], [0.5, 0.5, 0.5]);
    }

    #[test]
    fn parses_synthetic_vanilla_shape() {
        // Mirrors vanilla FNV headhuman.egm shape: 1449 verts, 50 sym
        // + 30 asym morphs. Total = 64 + 80 × (4 + 1449 × 6) = 695904.
        let bytes = synth_egm(1449, 50, 30);
        assert_eq!(bytes.len(), 695904);
        let egm = EgmFile::parse(&bytes).expect("parse");
        assert_eq!(egm.fggs_morphs.len(), 50);
        assert_eq!(egm.fgga_morphs.len(), 30);
        for morph in egm.fggs_morphs.iter().chain(egm.fgga_morphs.iter()) {
            assert_eq!(morph.deltas.len(), 1449);
        }
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = synth_egm(2, 1, 0);
        bytes[0] = b'X';
        let err = EgmFile::parse(&bytes).unwrap_err();
        assert!(matches!(err, FaceGenError::BadMagic { .. }));
    }

    #[test]
    fn rejects_truncated_header() {
        let bytes = vec![0u8; 32]; // shorter than 64-byte header
        let err = EgmFile::parse(&bytes).unwrap_err();
        assert!(matches!(err, FaceGenError::Truncated { .. }));
    }

    #[test]
    fn rejects_size_count_mismatch() {
        let mut bytes = synth_egm(2, 1, 0);
        // Lop off one delta (6 bytes) so the file under-reaches what
        // the header declares.
        bytes.truncate(bytes.len() - 6);
        let err = EgmFile::parse(&bytes).unwrap_err();
        assert!(matches!(err, FaceGenError::InconsistentHeader(_)));
    }

    #[test]
    fn rejects_zero_vertices() {
        let bytes = synth_egm(0, 1, 0);
        let err = EgmFile::parse(&bytes).unwrap_err();
        assert!(matches!(err, FaceGenError::InconsistentHeader(_)));
    }

    #[test]
    fn rejects_morph_count_over_cap() {
        let mut bytes = synth_egm(2, 1, 0);
        // Patch num_sym to 2049 (> MAX_MORPHS=1024).
        bytes[12..16].copy_from_slice(&2049u32.to_le_bytes());
        let err = EgmFile::parse(&bytes).unwrap_err();
        assert!(matches!(err, FaceGenError::InconsistentHeader(_)));
    }
}
