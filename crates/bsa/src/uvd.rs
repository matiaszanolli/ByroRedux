//! `.uvd` — Fallout 4 previs/occlusion (visibility-set) header reader
//! (#3810, EX-14/15 item C3).
//!
//! `vis\<plugin>\<cell_formid:08x>.uvd` stores the precomputed
//! potentially-visible-set (PVS) data the CK's previs pass bakes per
//! cell/cluster. This module decodes only the **outer envelope** —
//! magic, tile size, the embedded generator-tool debug string, and two
//! structural fields useful for bounds-checking (`table_offset`,
//! `entry_count`). The visibility-set payload itself (from
//! `table_offset` onward) is high-entropy, evidently bit-packed data —
//! genuinely uncracked, and **not** decoded here. See the module doc on
//! [`crate::csg`] for the sibling FO4 format this pairs with, and
//! `byroredux/src/cell_loader/precombined.rs` for the (currently
//! nonexistent) consumer.
//!
//! Every field below was derived by byte-offset arithmetic against
//! real `.uvd` samples pulled from `Fallout4 - MeshesExtra.ba2`
//! (5-sample cross-section spanning 1 056 B to 1.45 MB, 2026-08-31),
//! cross-checked for consistency across wildly different file sizes:
//!
//! - `magic` / `tile_size` / `self_size` / `debug_string`: already
//!   confirmed in an earlier pass (2026-08-23) — reproduced here
//!   unchanged.
//! - `content_hash`: the earlier pass left this "not yet identified
//!   (candidate: a content hash/checksum, or a per-cell coordinate)".
//!   All 5 new samples show uniformly high-entropy 32-bit values with
//!   no small-integer or coordinate-scale structure, which is
//!   consistent with a hash/checksum and *not* a coordinate — narrows
//!   but does not prove the earlier candidate list.
//! - `bounds`: bytes `0x14..0x28`, 5 `f32`s at a scale (multiples of
//!   512/4096) consistent with FO4 exterior world-space coordinates —
//!   very likely an axis-aligned bounding volume for the cluster, but
//!   the exact axis order/min-max pairing is not confirmed (would need
//!   cross-referencing against known CELL bounds from a parsed ESM,
//!   left for follow-up).
//! - `table_offset`: offset `0x30` — **byte-identical (`336`/`0x150`)
//!   across every sample regardless of total file size** (1 056 B to
//!   1 450 992 B). This is new: almost certainly a fixed header length
//!   / first-variable-table start pointer, not previously identified.
//! - `entry_count`: offset `0x38` — scales with file size/complexity
//!   across the corpus (1, 1, 11, 133, 191 for the 5 samples, small to
//!   large) — very likely an object/visibility-entry count, not
//!   previously identified.
//!
//! Byte `0x150` onward (`table_offset`) was inspected manually and is
//! high-entropy binary through roughly `+0x30`, followed by a short
//! monotonically increasing single-byte index array, then a second
//! float table terminated by an `FLT_MAX` sentinel (`0x7f7fffff`) —
//! i.e. the real payload is itself a compressed/bit-packed stream, a
//! research problem of comparable shape to the Havok
//! `hknpCompressedMeshShapeData` blocker in #3809. No further decode is
//! attempted here.

use std::io;

/// Byte-identical prefix confirmed across every sampled `.uvd` file.
pub const UVD_MAGIC: u32 = 0xD600_0012;

fn read_u32_le(data: &[u8], offset: usize) -> io::Result<u32> {
    data.get(offset..offset + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "uvd: truncated"))
}

fn read_f32_le(data: &[u8], offset: usize) -> io::Result<f32> {
    read_u32_le(data, offset).map(f32::from_bits)
}

/// The decoded `.uvd` outer envelope. See the module doc for which
/// fields are confirmed vs. best-effort candidates.
#[derive(Debug, Clone, PartialEq)]
pub struct UvdHeader {
    /// Unidentified per-file value at bytes `4..8` — high-entropy
    /// across the corpus, more consistent with a hash/checksum than a
    /// coordinate. Semantic unconfirmed.
    pub content_hash: u32,
    /// Self-reported total file length (bytes `8..12`) — verified
    /// exact match to the real file size in every sampled file.
    pub self_size: u32,
    /// Tile size in game units (bytes `12..16`), always `512.0` in the
    /// corpus — matches the `T 512.0` term in `debug_string`.
    pub tile_size: f32,
    /// Candidate axis-aligned bounding volume, bytes `0x14..0x28` (5
    /// `f32`s) — axis order/min-max pairing not confirmed.
    pub bounds: [f32; 5],
    /// Byte-identical (`336`) across the whole sampled corpus
    /// regardless of file size — likely a fixed header length / first
    /// variable-table start offset.
    pub table_offset: u32,
    /// Scales with file complexity across the corpus — likely an
    /// object/visibility-entry count.
    pub entry_count: u32,
    /// Null-padded ASCII generator-tool fingerprint at bytes
    /// `0xB0..0x100`, byte-identical across the whole corpus (a build
    /// tool/version string, not per-cell content).
    pub debug_string: String,
}

/// Parse a `.uvd` file's outer envelope. Returns `Err` on magic
/// mismatch or a buffer too short to contain the fields above — the
/// (uncracked) payload past `table_offset` is not validated.
pub fn parse_uvd_header(data: &[u8]) -> io::Result<UvdHeader> {
    if data.len() < 0x100 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "uvd: too short for header",
        ));
    }
    let magic = read_u32_le(data, 0)?;
    if magic != UVD_MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "uvd: magic mismatch",
        ));
    }
    let content_hash = read_u32_le(data, 4)?;
    let self_size = read_u32_le(data, 8)?;
    let tile_size = read_f32_le(data, 12)?;
    let mut bounds = [0.0f32; 5];
    for (i, slot) in bounds.iter_mut().enumerate() {
        *slot = read_f32_le(data, 0x14 + i * 4)?;
    }
    let table_offset = read_u32_le(data, 0x30)?;
    let entry_count = read_u32_le(data, 0x38)?;
    let debug_bytes = &data[0xB0..0x100];
    let nul = debug_bytes
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(debug_bytes.len());
    let debug_string = String::from_utf8_lossy(&debug_bytes[..nul]).into_owned();

    Ok(UvdHeader {
        content_hash,
        self_size,
        tile_size,
        bounds,
        table_offset,
        entry_count,
        debug_string,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_synthetic_uvd(bounds: [f32; 5], table_offset: u32, entry_count: u32) -> Vec<u8> {
        let mut buf = vec![0u8; 0x100];
        let total_len = buf.len() as u32;
        buf[0..4].copy_from_slice(&UVD_MAGIC.to_le_bytes());
        buf[4..8].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        buf[8..12].copy_from_slice(&total_len.to_le_bytes());
        buf[12..16].copy_from_slice(&512.0f32.to_le_bytes());
        for (i, v) in bounds.iter().enumerate() {
            buf[0x14 + i * 4..0x18 + i * 4].copy_from_slice(&v.to_le_bytes());
        }
        buf[0x30..0x34].copy_from_slice(&table_offset.to_le_bytes());
        buf[0x38..0x3c].copy_from_slice(&entry_count.to_le_bytes());
        let debug = b"T 512.0 SO 128.0 SH 16.000 BF 100 F 0 CS 0.0 - 3.3.17 F 1 0 OG 0";
        buf[0xB0..0xB0 + debug.len()].copy_from_slice(debug);
        buf
    }

    #[test]
    fn decodes_known_fields() {
        let bounds = [-4096.0, 8192.0, -1664.0, 8192.0, 20480.0];
        let blob = build_synthetic_uvd(bounds, 336, 191);
        let hdr = parse_uvd_header(&blob).expect("parse");
        assert_eq!(hdr.self_size, blob.len() as u32);
        assert_eq!(hdr.tile_size, 512.0);
        assert_eq!(hdr.bounds, bounds);
        assert_eq!(hdr.table_offset, 336);
        assert_eq!(hdr.entry_count, 191);
        assert!(hdr.debug_string.starts_with("T 512.0"));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut blob = build_synthetic_uvd([0.0; 5], 0, 0);
        blob[0] = 0;
        assert!(parse_uvd_header(&blob).is_err());
    }

    #[test]
    fn rejects_truncated_header() {
        let blob = build_synthetic_uvd([0.0; 5], 0, 0);
        assert!(parse_uvd_header(&blob[..0x20]).is_err());
    }
}
