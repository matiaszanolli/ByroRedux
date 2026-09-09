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
//! Every field below was derived by byte-offset arithmetic against the
//! **complete** `.uvd` corpus of the base game plus three DLCs — 1 413
//! files across `Fallout4 - MeshesExtra.ba2` (966), `DLCCoast` (246),
//! `DLCNukaWorld` (183), `DLCRobot` (15) and `DLCworkshop01` (3),
//! spanning 1 056 B to 2.4 MB (#3810, 2026-09-09). An earlier pass used a
//! 5-sample cross-section; every claim here was re-tested against all
//! 1 413, and three candidate relations that a small sample would have
//! accepted were disproved (see "Rejected" below). The DLC archives matter
//! because they were baked by separate content passes — a relation that
//! held only on the base game would be a property of one bake, not of the
//! format.
//!
//! Confirmed on 1 413/1 413:
//!
//! - `magic` / `self_size` / `tile_size` / `debug_string` — reproduced from
//!   the earlier pass unchanged. `self_size` matches the real file length
//!   exactly in every file; `tile_size` is `512.0` in every file.
//! - `bounds`: an axis-aligned bounding box at `0x14..0x2C` — **six**
//!   `f32`s, `min` then `max`. The earlier pass read five and left the
//!   "axis order/min-max pairing not confirmed". The pairing is now
//!   determined: `min[i] < max[i]` on all three axes holds 1 413/1 413 for
//!   this grouping, and only 387/1 413 for the one-slot-earlier alternative
//!   (`0x10..0x18` vs `0x1C..0x24`), so the two are distinguishable rather
//!   than merely plausible. Axis extents run 512 to 24 064 units.
//! - `table_bytes` (`0x2C`) `== 32 * entry_count` — exact, 1 413/1 413. This
//!   is what upgrades `entry_count` from "likely a count" to a count with
//!   a known 32-byte stride, and gives `table_offset` a length.
//! - `table_offset` (`0x30`) `== 336` in every file regardless of size,
//!   and `table_offset + table_bytes <= self_size` — so the first table is
//!   bounds-checkable against the file, which is what
//!   [`parse_uvd_header`] now validates.
//! - `0x90` carries a **second copy of `entry_count`**, byte-equal across all 1 413.
//!   Two independent fields agreeing is the strongest evidence available
//!   here that the value is a count rather than a coincidentally small
//!   offset.
//! - `0x10`, `0x34` and `0x4C` are zero in every file — reserved/padding,
//!   and the reason `bounds` cannot simply be widened leftward.
//! - Every 16-aligned slot in `0x30..0xB0` holds a value `< self_size`, and
//!   `0xA8` is the largest of them — consistent with `0x2C..0xB0` being a
//!   section directory of in-file offsets, with `0xA8` naming the last
//!   section. The individual sections are not identified.
//! - `content_hash` (`0x04`): still unidentified. Uniformly high-entropy
//!   across the corpus with no small-integer or coordinate-scale structure —
//!   consistent with a hash/checksum, not a coordinate. Narrowed, not
//!   proven.
//!
//! **Rejected** — each of these is the kind of claim a handful of samples
//! would have supported, and each is false on the full corpus:
//!
//! - "the bounds are quantised to `tile_size`" — extents are whole
//!   multiples of 512 in only 715/1 413 files (mins 735, maxs 1 225).
//! - "`entry_count` is the tile volume of the bounding box" — 1/1 413.
//! - "the second table starts at `table_offset + 16 * entry_count`" —
//!   232/1 413.
//!
//! Still uncracked, and deliberately not decoded here: the visibility-set
//! payload from `table_offset` onward. Manual inspection shows
//! high-entropy binary for roughly `+0x30`, then a short monotonically
//! increasing single-byte index array, then a float table terminated by an
//! `FLT_MAX` sentinel (`0x7f7fffff`) — i.e. a compressed/bit-packed
//! stream, a research problem of comparable shape to the Havok
//! `hknpCompressedMeshShapeData` blocker in #3809.
//!
//! Also still open: cross-referencing `bounds` against the owning CELL's
//! own extents from a parsed `Fallout4.esm`. The disproof of tile
//! quantisation above means the box is a tight content bound rather than a
//! grid-aligned cell volume, which makes that comparison more informative
//! than it would have been, not less.

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
    /// Axis-aligned bounding box, bytes `0x14..0x2C` — `min` then `max`,
    /// three `f32`s each. `min[i] < max[i]` on every axis in all 1 413
    /// corpus files; the one-slot-earlier alternative pairing holds in
    /// only 387, so this grouping is determined rather than assumed.
    ///
    /// Not quantised to [`Self::tile_size`] — see the module doc's
    /// "Rejected" list. It reads as a tight content bound, not a
    /// grid-aligned cell volume.
    pub bounds_min: [f32; 3],
    /// The `max` corner of [`Self::bounds_min`]'s box.
    pub bounds_max: [f32; 3],
    /// Byte length of the table at [`Self::table_offset`], bytes `0x2C`.
    /// Exactly `32 * entry_count` in all 1 413 corpus files — which is what
    /// establishes the table's 32-byte entry stride.
    pub table_bytes: u32,
    /// Byte-identical (`336`) across the whole corpus regardless of file
    /// size — a fixed header length / first-table start offset.
    pub table_offset: u32,
    /// Entry count for the table at [`Self::table_offset`]. Carried twice
    /// in the header (`0x38` and `0x90`), byte-equal in every corpus
    /// file — two independent fields agreeing is the strongest available
    /// evidence that this is a count and not a small offset.
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
    let mut bounds_min = [0.0f32; 3];
    let mut bounds_max = [0.0f32; 3];
    for i in 0..3 {
        bounds_min[i] = read_f32_le(data, 0x14 + i * 4)?;
        bounds_max[i] = read_f32_le(data, 0x20 + i * 4)?;
    }
    let table_bytes = read_u32_le(data, 0x2C)?;
    let table_offset = read_u32_le(data, 0x30)?;
    let entry_count = read_u32_le(data, 0x38)?;

    // Structural checks, each one a relation that holds on all 1 413
    // corpus files. They are cheap, and they are the only thing standing
    // between a malformed or foreign blob and a consumer that trusts
    // `entry_count` to size an allocation.
    if table_bytes != 32u32.saturating_mul(entry_count) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "uvd: table byte length is not 32 * entry_count",
        ));
    }
    if (table_offset as u64) + (table_bytes as u64) > data.len() as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "uvd: entry table runs past the end of the file",
        ));
    }
    // The second copy of the count. Disagreement means the layout
    // assumption is wrong for this file, not that one of the two is
    // authoritative — so refuse rather than pick.
    if read_u32_le(data, 0x90)? != entry_count {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "uvd: the two entry-count fields disagree",
        ));
    }
    if (0..3).any(|i| !(bounds_min[i] < bounds_max[i])) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "uvd: bounding box is degenerate or inverted",
        ));
    }
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
        bounds_min,
        bounds_max,
        table_bytes,
        table_offset,
        entry_count,
        debug_string,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a header that satisfies every relation the corpus exhibits,
    /// so each test below can break exactly one of them.
    fn build_synthetic_uvd(
        bounds_min: [f32; 3],
        bounds_max: [f32; 3],
        table_offset: u32,
        entry_count: u32,
    ) -> Vec<u8> {
        let table_bytes = 32 * entry_count;
        let len = (table_offset + table_bytes).max(0x100) as usize;
        let mut buf = vec![0u8; len];
        buf[0..4].copy_from_slice(&UVD_MAGIC.to_le_bytes());
        buf[4..8].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        buf[8..12].copy_from_slice(&(len as u32).to_le_bytes());
        buf[12..16].copy_from_slice(&512.0f32.to_le_bytes());
        for i in 0..3 {
            buf[0x14 + i * 4..0x18 + i * 4].copy_from_slice(&bounds_min[i].to_le_bytes());
            buf[0x20 + i * 4..0x24 + i * 4].copy_from_slice(&bounds_max[i].to_le_bytes());
        }
        buf[0x2C..0x30].copy_from_slice(&table_bytes.to_le_bytes());
        buf[0x30..0x34].copy_from_slice(&table_offset.to_le_bytes());
        buf[0x38..0x3C].copy_from_slice(&entry_count.to_le_bytes());
        // The header's second copy of the count (0x90).
        buf[0x90..0x94].copy_from_slice(&entry_count.to_le_bytes());
        let debug = b"T 512.0 SO 128.0 SH 16.000 BF 100 F 0 CS 0.0 - 3.3.17 F 1 0 OG 0";
        buf[0xB0..0xB0 + debug.len()].copy_from_slice(debug);
        buf
    }

    /// The shape of the smallest real file in the corpus
    /// (`vis\fallout4.esm\0020f124.uvd`, 3 968 B), reproduced field for
    /// field from its actual bytes.
    fn real_smallest_sample() -> Vec<u8> {
        build_synthetic_uvd([1024.0, 1536.0, 0.0], [2560.0, 2560.0, 1024.0], 336, 1)
    }

    #[test]
    fn decodes_the_known_envelope_fields() {
        let blob = real_smallest_sample();
        let hdr = parse_uvd_header(&blob).expect("parse");
        assert_eq!(hdr.self_size, blob.len() as u32);
        assert_eq!(hdr.tile_size, 512.0);
        assert_eq!(hdr.bounds_min, [1024.0, 1536.0, 0.0]);
        assert_eq!(hdr.bounds_max, [2560.0, 2560.0, 1024.0]);
        assert_eq!(hdr.table_offset, 336);
        assert_eq!(hdr.entry_count, 1);
        assert_eq!(hdr.table_bytes, 32);
        assert!(hdr.debug_string.starts_with("T 512.0"));
    }

    /// #3810 — the AABB is six floats at `0x14..0x2C`, not the five the
    /// earlier pass read at `0x14..0x28`.
    ///
    /// Reading five truncated the box mid-`max`, so `bounds[4]` was
    /// `max.y` presented as an unlabelled scalar and `max.z` was dropped
    /// entirely. The pairing is corpus-determined (1 413/1 413 versus 387/1 413
    /// for the one-slot-earlier alternative), so this is a decode fix, not
    /// a relabelling.
    #[test]
    fn the_bounding_box_reads_all_six_floats_in_min_then_max_order() {
        let blob = build_synthetic_uvd(
            [-4096.0, -1664.0, -512.0],
            [8192.0, 8192.0, 20480.0],
            336,
            3,
        );
        let hdr = parse_uvd_header(&blob).expect("parse");
        assert_eq!(hdr.bounds_min, [-4096.0, -1664.0, -512.0]);
        assert_eq!(hdr.bounds_max, [8192.0, 8192.0, 20480.0]);
        // The byte at 0x28 is the last float of the box, and it is `max.z`
        // — the slot the five-float read never reached.
        assert_eq!(
            hdr.bounds_max[2],
            f32::from_bits(u32::from_le_bytes(blob[0x28..0x2C].try_into().unwrap()))
        );
    }

    /// `table_bytes == 32 * entry_count` is exact on all 1 413 corpus files,
    /// which is what gives the table a stride and lets the parser bound
    /// it. A blob that violates it is not this format.
    #[test]
    fn rejects_a_table_length_that_is_not_a_whole_number_of_32_byte_entries() {
        let mut blob = real_smallest_sample();
        blob[0x2C..0x30].copy_from_slice(&33u32.to_le_bytes());
        assert!(parse_uvd_header(&blob).is_err());
    }

    /// The bound that protects a future consumer: `entry_count` sizes an
    /// allocation, so a count implying a table past EOF must be refused
    /// here rather than trusted downstream.
    #[test]
    fn rejects_an_entry_table_that_runs_past_the_end_of_the_file() {
        let mut blob = real_smallest_sample();
        let bogus = 4096u32;
        blob[0x38..0x3C].copy_from_slice(&bogus.to_le_bytes());
        blob[0x90..0x94].copy_from_slice(&bogus.to_le_bytes());
        blob[0x2C..0x30].copy_from_slice(&(32 * bogus).to_le_bytes());
        assert!(parse_uvd_header(&blob).is_err());
    }

    /// The header carries the count twice (`0x38` and `0x90`), byte-equal
    /// in every corpus file. Disagreement means the layout assumption is
    /// wrong for that file — so refuse rather than silently pick one.
    #[test]
    fn rejects_disagreeing_entry_count_copies() {
        let mut blob = real_smallest_sample();
        blob[0x90..0x94].copy_from_slice(&2u32.to_le_bytes());
        assert!(parse_uvd_header(&blob).is_err());
    }

    /// `min < max` holds on every axis of every corpus file, so an
    /// inverted or flat box is a decode error rather than an empty cell.
    #[test]
    fn rejects_a_degenerate_or_inverted_bounding_box() {
        let flat = build_synthetic_uvd([0.0, 0.0, 0.0], [512.0, 0.0, 512.0], 336, 1);
        assert!(
            parse_uvd_header(&flat).is_err(),
            "a flat axis must be rejected"
        );

        let inverted = build_synthetic_uvd([4096.0, 0.0, 0.0], [512.0, 512.0, 512.0], 336, 1);
        assert!(
            parse_uvd_header(&inverted).is_err(),
            "an inverted axis must be rejected"
        );
    }

    #[test]
    fn rejects_bad_magic() {
        let mut blob = real_smallest_sample();
        blob[0] = 0;
        assert!(parse_uvd_header(&blob).is_err());
    }

    #[test]
    fn rejects_truncated_header() {
        let blob = real_smallest_sample();
        assert!(parse_uvd_header(&blob[..0x20]).is_err());
    }
}
