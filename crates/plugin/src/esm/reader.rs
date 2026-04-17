//! Low-level binary reader for the TES4 record format.
//!
//! ESM/ESP files are sequences of records and groups. Each record has a
//! 4-char type code, data size, flags, and form ID. Records contain
//! sub-records (type + size + data). Groups contain other records/groups.
//!
//! **Per-game header layout.** Oblivion (TES4) uses a 20-byte record
//! header and 20-byte group header, ending after `vc_info`. Every later
//! game (Fallout 3, New Vegas, Skyrim, FO4, etc.) extends both to 24
//! bytes with a trailing version + unknown field. The first 16 bytes are
//! identical in either layout, so we only need to branch on the
//! additional skip at the end.

use anyhow::{ensure, Context, Result};
use flate2::read::ZlibDecoder;
use std::io::Read;

/// Record flag: data is zlib-compressed.
const FLAG_COMPRESSED: u32 = 0x00040000;

/// ESM format variant — determines record / group header size.
///
/// The two surviving layouts across the Bethesda lineage:
/// - [`Oblivion`](Self::Oblivion) — 20-byte headers (TES4, Oblivion.esm)
/// - [`Tes5Plus`](Self::Tes5Plus) — 24-byte headers (FO3 / FNV / Skyrim /
///   FO4 / FO76 / Starfield)
///
/// Morrowind's TES3 format is entirely different and not supported here;
/// it would need its own reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EsmVariant {
    /// Oblivion — 20-byte record and group headers.
    Oblivion,
    /// FO3 / FNV / Skyrim LE+SE / FO4 / FO76 / Starfield — 24-byte headers.
    Tes5Plus,
}

impl EsmVariant {
    /// Auto-detect the ESM variant from a file buffer.
    ///
    /// The heuristic looks at byte offset 20 in the file. Every Bethesda
    /// ESM begins with a `TES4` record, and the first sub-record inside
    /// its data area is always `HEDR`. In Oblivion, the record header is
    /// 20 bytes, so bytes 20-23 spell out `"HEDR"`. In every later game
    /// the header is 24 bytes, so bytes 20-23 are the version u16 +
    /// unknown u16 (small integers, never ASCII). Test the four ASCII
    /// bytes and you have a deterministic, one-shot detector.
    pub fn detect(data: &[u8]) -> Self {
        if data.len() >= 24 && &data[20..24] == b"HEDR" {
            Self::Oblivion
        } else {
            Self::Tes5Plus
        }
    }

    /// Record header size in bytes (`type + data_size + flags + form_id`
    /// plus trailing metadata).
    pub fn record_header_size(self) -> usize {
        match self {
            Self::Oblivion => 20,
            Self::Tes5Plus => 24,
        }
    }

    /// Group header size in bytes (`GRUP + size + label + group_type`
    /// plus trailing metadata).
    pub fn group_header_size(self) -> usize {
        match self {
            Self::Oblivion => 20,
            Self::Tes5Plus => 24,
        }
    }
}

/// Fine-grained game identity for sub-record layout dispatch.
///
/// [`EsmVariant`] only splits "Oblivion (20-byte headers)" from "everything
/// else (24-byte headers)" because that's what the low-level walker needs.
/// Per-record layouts diverge within the Tes5Plus family: FO3/FNV share one
/// schema for ARMO/WEAP/AMMO DATA, Skyrim uses a different one (no health
/// field, BOD2 instead of BMDT, DNAM as packed armor rating), and FO4 adds
/// its own variants again. Callers that parse body data need this finer
/// distinction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameKind {
    /// Oblivion (TES4, HEDR 1.0).
    Oblivion,
    /// Fallout 3 (HEDR 0.85) and Fallout: New Vegas (HEDR 1.34). These two
    /// share their DATA/DNAM layouts everywhere the current parser cares
    /// about, so they collapse to one game kind.
    #[default]
    Fallout3NV,
    /// Skyrim LE + SE (HEDR 1.7). New ARMO/WEAP/AMMO sub-record schemas.
    Skyrim,
    /// Fallout 4 (HEDR 0.95). SCOL/PKIN/TXST and yet another item schema.
    Fallout4,
    /// Fallout 76 (HEDR 68.0 — unusually large).
    Fallout76,
    /// Starfield (HEDR 0.96).
    Starfield,
}

impl GameKind {
    /// Derive the game kind from the ESM variant plus the HEDR `Version`
    /// f32 (sub-record offset 0 of the TES4 record's HEDR). Callers that
    /// don't have a HEDR version should pass `0.0`, which falls back to
    /// [`GameKind::Fallout3NV`] (the most common Tes5Plus case — keeps
    /// existing synthetic test fixtures working).
    pub fn from_header(variant: EsmVariant, hedr_version: f32) -> Self {
        match variant {
            EsmVariant::Oblivion => Self::Oblivion,
            EsmVariant::Tes5Plus => {
                // HEDR versions (cross-referenced against UESP + real
                // vanilla master files):
                //   FO3       = 0.85
                //   FO4       = 0.95
                //   Starfield = 0.96
                //   FNV       = 1.34
                //   Skyrim    = 1.7 (LE and SE both)
                //   FO76      = 68.0
                // Exact float equality is unsafe — match on small bands.
                if hedr_version >= 60.0 {
                    Self::Fallout76
                } else if (1.6..=1.8).contains(&hedr_version) {
                    Self::Skyrim
                } else if (0.94..=0.955).contains(&hedr_version) {
                    Self::Fallout4
                } else if (0.955..=0.975).contains(&hedr_version) {
                    Self::Starfield
                } else {
                    // FO3 (0.85), FNV (1.34), or unknown → treat as the
                    // legacy "Fallout" family.
                    Self::Fallout3NV
                }
            }
        }
    }
}

/// Binary reader for ESM/ESP files.
pub struct EsmReader<'a> {
    data: &'a [u8],
    pos: usize,
    variant: EsmVariant,
}

/// Parsed record header (CELL, REFR, STAT, TES4, etc.).
#[derive(Debug, Clone)]
pub struct RecordHeader {
    pub record_type: [u8; 4],
    pub data_size: u32,
    pub flags: u32,
    pub form_id: u32,
}

/// Parsed group header (GRUP).
#[derive(Debug, Clone)]
pub struct GroupHeader {
    pub label: [u8; 4],
    pub group_type: u32,
    /// Total size including this header.
    pub total_size: u32,
}

/// A sub-record within a record.
#[derive(Debug, Clone)]
pub struct SubRecord {
    pub sub_type: [u8; 4],
    pub data: Vec<u8>,
}

/// File header data from the TES4 record.
#[derive(Debug)]
pub struct FileHeader {
    pub master_files: Vec<String>,
    pub record_count: u32,
    /// HEDR `Version` f32 (sub-record offset 0). 0.0 when absent (synthetic
    /// test fixtures often omit HEDR). Feed into [`GameKind::from_header`].
    pub hedr_version: f32,
}

impl<'a> EsmReader<'a> {
    /// Create a reader, auto-detecting the game variant from the file
    /// header. Oblivion gets 20-byte record/group headers; everything
    /// else gets 24. See [`EsmVariant::detect`].
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            variant: EsmVariant::detect(data),
        }
    }

    /// Create a reader with an explicit variant — used by the unit
    /// tests which build synthetic 24-byte records regardless of game.
    pub fn with_variant(data: &'a [u8], variant: EsmVariant) -> Self {
        Self {
            data,
            pos: 0,
            variant,
        }
    }

    pub fn variant(&self) -> EsmVariant {
        self.variant
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    /// Peek at the next 4 bytes without advancing.
    pub fn peek_type(&self) -> Option<[u8; 4]> {
        if self.pos + 4 <= self.data.len() {
            Some([
                self.data[self.pos],
                self.data[self.pos + 1],
                self.data[self.pos + 2],
                self.data[self.pos + 3],
            ])
        } else {
            None
        }
    }

    /// Check if the next record is a GRUP.
    pub fn is_group(&self) -> bool {
        self.peek_type() == Some(*b"GRUP")
    }

    /// Read a record header (20 bytes on Oblivion, 24 on FO3+).
    pub fn read_record_header(&mut self) -> Result<RecordHeader> {
        let header_size = self.variant.record_header_size();
        ensure!(self.remaining() >= header_size, "Truncated record header");
        let record_type = self.read_bytes_4();
        let data_size = self.read_u32();
        let flags = self.read_u32();
        let form_id = self.read_u32();
        // Trailing metadata: Oblivion = 4 bytes (vc_info); FO3+ = 8 bytes
        // (vc_info + unknown + version + unknown). We don't consume any
        // of it today, just skip past.
        self.skip(header_size - 16);
        Ok(RecordHeader {
            record_type,
            data_size,
            flags,
            form_id,
        })
    }

    /// Read a group header (20 bytes on Oblivion, 24 on FO3+). Caller
    /// must verify `peek_type() == "GRUP"` first.
    pub fn read_group_header(&mut self) -> Result<GroupHeader> {
        let header_size = self.variant.group_header_size();
        ensure!(self.remaining() >= header_size, "Truncated group header");
        let typ = self.read_bytes_4();
        ensure!(
            &typ == b"GRUP",
            "Expected GRUP, got {:?}",
            std::str::from_utf8(&typ)
        );
        let total_size = self.read_u32();
        let label = self.read_bytes_4();
        let group_type = self.read_u32();
        // Trailing metadata: Oblivion = 4 bytes (stamp); FO3+ = 8 bytes
        // (stamp + unknown + version + unknown).
        self.skip(header_size - 16);
        Ok(GroupHeader {
            label,
            group_type,
            total_size,
        })
    }

    /// Read the sub-records within a record's data section.
    ///
    /// If the record is compressed (FLAG_COMPRESSED), decompresses first.
    pub fn read_sub_records(&mut self, header: &RecordHeader) -> Result<Vec<SubRecord>> {
        let data_start = self.pos;
        let raw_data = if header.flags & FLAG_COMPRESSED != 0 {
            // First 4 bytes = uncompressed size, rest is zlib.
            ensure!(header.data_size >= 4, "Compressed record too small");
            let decompressed_size = self.read_u32() as usize;
            let compressed_len = header.data_size as usize - 4;
            ensure!(
                self.remaining() >= compressed_len,
                "Truncated compressed data"
            );
            let compressed = &self.data[self.pos..self.pos + compressed_len];
            self.pos += compressed_len;

            let mut decoder = ZlibDecoder::new(compressed);
            let mut decompressed = Vec::with_capacity(decompressed_size);
            decoder
                .read_to_end(&mut decompressed)
                .context("Failed to decompress ESM record")?;
            decompressed
        } else {
            let size = header.data_size as usize;
            ensure!(self.remaining() >= size, "Truncated record data");
            let slice = self.data[self.pos..self.pos + size].to_vec();
            self.pos += size;
            slice
        };

        // Parse sub-records from the (possibly decompressed) data.
        let mut sub_pos = 0;
        let mut subs = Vec::new();
        while sub_pos + 6 <= raw_data.len() {
            let sub_type = [
                raw_data[sub_pos],
                raw_data[sub_pos + 1],
                raw_data[sub_pos + 2],
                raw_data[sub_pos + 3],
            ];
            let sub_size =
                u16::from_le_bytes([raw_data[sub_pos + 4], raw_data[sub_pos + 5]]) as usize;
            sub_pos += 6;

            if sub_pos + sub_size > raw_data.len() {
                // Tolerate truncated final sub-record.
                break;
            }
            let data = raw_data[sub_pos..sub_pos + sub_size].to_vec();
            sub_pos += sub_size;
            subs.push(SubRecord { sub_type, data });
        }

        // Ensure we consumed exactly data_size from the outer stream.
        let consumed = self.pos - data_start;
        if consumed != header.data_size as usize {
            // Adjust position if we over/under-read (shouldn't happen, but defensive).
            self.pos = data_start + header.data_size as usize;
        }

        Ok(subs)
    }

    /// Skip a record's data section without parsing.
    pub fn skip_record(&mut self, header: &RecordHeader) {
        self.pos += header.data_size as usize;
    }

    /// Skip a group's remaining content. `total_size` in the group
    /// header includes the (20- or 24-byte) header that the caller has
    /// already read, so subtract the variant's header size to get the
    /// remaining content length.
    pub fn skip_group(&mut self, header: &GroupHeader) {
        let remaining = header.total_size as usize - self.variant.group_header_size();
        self.pos += remaining;
    }

    /// Parse the TES4 file header record.
    pub fn read_file_header(&mut self) -> Result<FileHeader> {
        let header = self.read_record_header()?;
        ensure!(
            &header.record_type == b"TES4",
            "ESM file must start with TES4 record, got {:?}",
            std::str::from_utf8(&header.record_type),
        );

        let subs = self.read_sub_records(&header)?;
        let mut masters = Vec::new();
        let mut record_count = 0;
        let mut hedr_version = 0.0f32;

        for sub in &subs {
            match &sub.sub_type {
                b"HEDR" if sub.data.len() >= 12 => {
                    hedr_version =
                        f32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                    record_count =
                        u32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]);
                }
                b"MAST" => {
                    // Null-terminated string.
                    let name = sub.data.split(|&b| b == 0).next().unwrap_or(&sub.data);
                    masters.push(String::from_utf8_lossy(name).to_string());
                }
                _ => {}
            }
        }

        Ok(FileHeader {
            master_files: masters,
            record_count,
            hedr_version,
        })
    }

    // ── Primitives ──────────────────────────────────────────────────

    fn read_u32(&mut self) -> u32 {
        let v = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        v
    }

    fn read_bytes_4(&mut self) -> [u8; 4] {
        let v = [
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ];
        self.pos += 4;
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic Tes5Plus (24-byte header) record.
    fn build_record(typ: &[u8; 4], form_id: u32, sub_records: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        build_record_for(EsmVariant::Tes5Plus, typ, form_id, sub_records)
    }

    /// Build a synthetic record with explicit variant — Oblivion's
    /// 20-byte header has 4 bytes of vc_info padding where the Tes5Plus
    /// layout has 8.
    fn build_record_for(
        variant: EsmVariant,
        typ: &[u8; 4],
        form_id: u32,
        sub_records: &[(&[u8; 4], &[u8])],
    ) -> Vec<u8> {
        // Build sub-record data first.
        let mut sub_data = Vec::new();
        for (sub_type, data) in sub_records {
            sub_data.extend_from_slice(*sub_type);
            sub_data.extend_from_slice(&(data.len() as u16).to_le_bytes());
            sub_data.extend_from_slice(data);
        }

        let mut buf = Vec::new();
        buf.extend_from_slice(typ);
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes()); // data_size
        buf.extend_from_slice(&0u32.to_le_bytes()); // flags
        buf.extend_from_slice(&form_id.to_le_bytes());
        // Trailing metadata: 4 bytes for Oblivion, 8 for Tes5Plus.
        buf.resize(buf.len() + (variant.record_header_size() - 16), 0);
        buf.extend_from_slice(&sub_data);
        buf
    }

    /// Build a synthetic Tes5Plus (24-byte header) group.
    fn build_group(label: &[u8; 4], group_type: u32, content: &[u8]) -> Vec<u8> {
        build_group_for(EsmVariant::Tes5Plus, label, group_type, content)
    }

    fn build_group_for(
        variant: EsmVariant,
        label: &[u8; 4],
        group_type: u32,
        content: &[u8],
    ) -> Vec<u8> {
        let total_size = variant.group_header_size() + content.len();
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GRUP");
        buf.extend_from_slice(&(total_size as u32).to_le_bytes());
        buf.extend_from_slice(label);
        buf.extend_from_slice(&group_type.to_le_bytes());
        buf.resize(buf.len() + (variant.group_header_size() - 16), 0);
        buf.extend_from_slice(content);
        buf
    }

    #[test]
    fn read_record_header_basic() {
        let data = build_record(b"STAT", 0x12345, &[]);
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        assert_eq!(&header.record_type, b"STAT");
        assert_eq!(header.form_id, 0x12345);
        assert_eq!(header.data_size, 0);
    }

    #[test]
    fn read_sub_records() {
        let data = build_record(
            b"STAT",
            0x100,
            &[(b"EDID", b"TestStatic\0"), (b"MODL", b"meshes\\test.nif\0")],
        );
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        let subs = reader.read_sub_records(&header).unwrap();

        assert_eq!(subs.len(), 2);
        assert_eq!(&subs[0].sub_type, b"EDID");
        assert_eq!(&subs[0].data, b"TestStatic\0");
        assert_eq!(&subs[1].sub_type, b"MODL");
        assert_eq!(&subs[1].data, b"meshes\\test.nif\0");
    }

    #[test]
    fn read_group_header_basic() {
        let group = build_group(b"CELL", 0, &[]);
        let mut reader = EsmReader::with_variant(&group, EsmVariant::Tes5Plus);
        let header = reader.read_group_header().unwrap();
        assert_eq!(&header.label, b"CELL");
        assert_eq!(header.group_type, 0);
        assert_eq!(header.total_size, 24);
    }

    #[test]
    fn is_group_detects_grup() {
        let group = build_group(b"CELL", 0, &[]);
        let reader = EsmReader::with_variant(&group, EsmVariant::Tes5Plus);
        assert!(reader.is_group());

        let record = build_record(b"STAT", 0, &[]);
        let reader = EsmReader::with_variant(&record, EsmVariant::Tes5Plus);
        assert!(!reader.is_group());
    }

    #[test]
    fn skip_record_advances_position() {
        let data = build_record(b"STAT", 0, &[(b"EDID", b"Test\0")]);
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        reader.skip_record(&header);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn file_header_parses_tes4() {
        let tes4 = build_record(
            b"TES4",
            0,
            &[
                // HEDR: version(f32) + record_count(u32) + next_object_id(u32)
                (b"HEDR", &{
                    let mut d = Vec::new();
                    d.extend_from_slice(&1.0f32.to_le_bytes()); // version
                    d.extend_from_slice(&42u32.to_le_bytes()); // record count
                    d.extend_from_slice(&0u32.to_le_bytes()); // next object id
                    d
                }),
                (b"MAST", b"FalloutNV.esm\0"),
                (b"DATA", &0u64.to_le_bytes()),
            ],
        );
        let mut reader = EsmReader::with_variant(&tes4, EsmVariant::Tes5Plus);
        let fh = reader.read_file_header().unwrap();
        assert_eq!(fh.record_count, 42);
        assert_eq!(fh.master_files, vec!["FalloutNV.esm"]);
    }

    // ── Oblivion (20-byte header) tests ────────────────────────────────

    #[test]
    fn variant_detect_oblivion() {
        // Build a real Oblivion TES4 record: 20-byte header + HEDR subrecord.
        let tes4 = build_record_for(
            EsmVariant::Oblivion,
            b"TES4",
            0,
            &[(b"HEDR", &{
                let mut d = Vec::new();
                d.extend_from_slice(&1.0f32.to_le_bytes()); // Oblivion version = 1.0
                d.extend_from_slice(&0u32.to_le_bytes()); // record count
                d.extend_from_slice(&0u32.to_le_bytes()); // next object id
                d
            })],
        );
        // At offset 20 we should see "HEDR" — the sub-record type.
        assert_eq!(&tes4[20..24], b"HEDR");
        assert_eq!(EsmVariant::detect(&tes4), EsmVariant::Oblivion);
    }

    #[test]
    fn variant_detect_tes5_plus() {
        // FNV-style TES4 — HEDR lands at offset 24.
        let tes4 = build_record_for(
            EsmVariant::Tes5Plus,
            b"TES4",
            0,
            &[(b"HEDR", b"placeholder\0")],
        );
        assert_eq!(&tes4[24..28], b"HEDR");
        assert_eq!(EsmVariant::detect(&tes4), EsmVariant::Tes5Plus);
    }

    #[test]
    fn read_oblivion_record_header_has_20_byte_layout() {
        let data = build_record_for(EsmVariant::Oblivion, b"STAT", 0xAB, &[]);
        assert_eq!(data.len(), 20); // no sub-records → 20 header bytes total
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Oblivion);
        let header = reader.read_record_header().unwrap();
        assert_eq!(&header.record_type, b"STAT");
        assert_eq!(header.form_id, 0xAB);
        assert_eq!(header.data_size, 0);
        assert_eq!(reader.position(), 20);
    }

    #[test]
    fn read_oblivion_group_header_has_20_byte_layout() {
        let group = build_group_for(EsmVariant::Oblivion, b"CELL", 0, &[]);
        assert_eq!(group.len(), 20);
        let mut reader = EsmReader::with_variant(&group, EsmVariant::Oblivion);
        let header = reader.read_group_header().unwrap();
        assert_eq!(&header.label, b"CELL");
        assert_eq!(header.total_size, 20);
        assert_eq!(reader.position(), 20);
    }

    #[test]
    fn read_oblivion_sub_records() {
        let data = build_record_for(
            EsmVariant::Oblivion,
            b"STAT",
            0x100,
            &[
                (b"EDID", b"TestOblivion\0"),
                (b"MODL", b"meshes\\stat.nif\0"),
            ],
        );
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Oblivion);
        let header = reader.read_record_header().unwrap();
        let subs = reader.read_sub_records(&header).unwrap();
        assert_eq!(subs.len(), 2);
        assert_eq!(&subs[0].sub_type, b"EDID");
        assert_eq!(subs[0].data, b"TestOblivion\0");
        assert_eq!(&subs[1].sub_type, b"MODL");
    }

    #[test]
    fn oblivion_skip_group_uses_20_byte_header() {
        // Group containing one STAT record. skip_group should land exactly
        // at end-of-buffer — off-by-4 bugs show up here.
        let inner = build_record_for(EsmVariant::Oblivion, b"STAT", 1, &[]);
        let group = build_group_for(EsmVariant::Oblivion, b"STAT", 0, &inner);
        let mut reader = EsmReader::with_variant(&group, EsmVariant::Oblivion);
        let header = reader.read_group_header().unwrap();
        reader.skip_group(&header);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn oblivion_file_header_parses() {
        let tes4 = build_record_for(
            EsmVariant::Oblivion,
            b"TES4",
            0,
            &[
                (b"HEDR", &{
                    let mut d = Vec::new();
                    d.extend_from_slice(&1.0f32.to_le_bytes()); // Oblivion v1.0
                    d.extend_from_slice(&123u32.to_le_bytes()); // record count
                    d.extend_from_slice(&0u32.to_le_bytes());
                    d
                }),
                (b"MAST", b"Oblivion.esm\0"),
            ],
        );
        let mut reader = EsmReader::new(&tes4); // auto-detect
        assert_eq!(reader.variant(), EsmVariant::Oblivion);
        let fh = reader.read_file_header().unwrap();
        assert_eq!(fh.record_count, 123);
        assert_eq!(fh.master_files, vec!["Oblivion.esm"]);
    }
}
