//! Low-level binary reader for the TES4 record format.
//!
//! ESM/ESP files are sequences of records and groups. Each record has a
//! 4-char type code, data size, flags, and form ID. Records contain
//! sub-records (type + size + data). Groups contain other records/groups.

use anyhow::{ensure, Context, Result};
use flate2::read::ZlibDecoder;
use std::io::Read;

/// Size of a record header in bytes (type + data_size + flags + form_id + vc_info + version).
const RECORD_HEADER_SIZE: usize = 24;
/// Size of a group header in bytes (type + size + label + group_type + vc_info + version).
const GROUP_HEADER_SIZE: usize = 24;
/// Record flag: data is zlib-compressed.
const FLAG_COMPRESSED: u32 = 0x00040000;

/// Binary reader for ESM/ESP files.
pub struct EsmReader<'a> {
    data: &'a [u8],
    pos: usize,
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
}

impl<'a> EsmReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
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

    /// Read a record header (24 bytes).
    pub fn read_record_header(&mut self) -> Result<RecordHeader> {
        ensure!(self.remaining() >= RECORD_HEADER_SIZE, "Truncated record header");
        let record_type = self.read_bytes_4();
        let data_size = self.read_u32();
        let flags = self.read_u32();
        let form_id = self.read_u32();
        // Skip vc_info (u16) + vc_unknown (u16) + version (u16) + unknown (u16)
        self.skip(8);
        Ok(RecordHeader { record_type, data_size, flags, form_id })
    }

    /// Read a group header (24 bytes). Caller must verify peek_type == "GRUP" first.
    pub fn read_group_header(&mut self) -> Result<GroupHeader> {
        ensure!(self.remaining() >= GROUP_HEADER_SIZE, "Truncated group header");
        let typ = self.read_bytes_4();
        ensure!(&typ == b"GRUP", "Expected GRUP, got {:?}", std::str::from_utf8(&typ));
        let total_size = self.read_u32();
        let label = self.read_bytes_4();
        let group_type = self.read_u32();
        // Skip vc_info (u16) + vc_unknown (u16) + version (u16) + unknown (u16)
        self.skip(8);
        Ok(GroupHeader { label, group_type, total_size })
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
            ensure!(self.remaining() >= compressed_len, "Truncated compressed data");
            let compressed = &self.data[self.pos..self.pos + compressed_len];
            self.pos += compressed_len;

            let mut decoder = ZlibDecoder::new(compressed);
            let mut decompressed = Vec::with_capacity(decompressed_size);
            decoder.read_to_end(&mut decompressed)
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
            let sub_size = u16::from_le_bytes([raw_data[sub_pos + 4], raw_data[sub_pos + 5]]) as usize;
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

    /// Skip a group's remaining content (total_size includes the 24-byte header already read).
    pub fn skip_group(&mut self, header: &GroupHeader) {
        let remaining = header.total_size as usize - GROUP_HEADER_SIZE;
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

        for sub in &subs {
            match &sub.sub_type {
                b"HEDR" if sub.data.len() >= 12 => {
                    record_count = u32::from_le_bytes([
                        sub.data[4], sub.data[5], sub.data[6], sub.data[7],
                    ]);
                }
                b"MAST" => {
                    // Null-terminated string.
                    let name = sub.data.split(|&b| b == 0).next().unwrap_or(&sub.data);
                    masters.push(String::from_utf8_lossy(name).to_string());
                }
                _ => {}
            }
        }

        Ok(FileHeader { master_files: masters, record_count })
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

    fn build_record(typ: &[u8; 4], form_id: u32, sub_records: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
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
        buf.extend_from_slice(&[0u8; 8]); // vc_info + version padding
        buf.extend_from_slice(&sub_data);
        buf
    }

    fn build_group(label: &[u8; 4], group_type: u32, content: &[u8]) -> Vec<u8> {
        let total_size = GROUP_HEADER_SIZE + content.len();
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GRUP");
        buf.extend_from_slice(&(total_size as u32).to_le_bytes());
        buf.extend_from_slice(label);
        buf.extend_from_slice(&group_type.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]); // vc_info + version padding
        buf.extend_from_slice(content);
        buf
    }

    #[test]
    fn read_record_header_basic() {
        let data = build_record(b"STAT", 0x12345, &[]);
        let mut reader = EsmReader::new(&data);
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
            &[
                (b"EDID", b"TestStatic\0"),
                (b"MODL", b"meshes\\test.nif\0"),
            ],
        );
        let mut reader = EsmReader::new(&data);
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
        let mut reader = EsmReader::new(&group);
        let header = reader.read_group_header().unwrap();
        assert_eq!(&header.label, b"CELL");
        assert_eq!(header.group_type, 0);
        assert_eq!(header.total_size, GROUP_HEADER_SIZE as u32);
    }

    #[test]
    fn is_group_detects_grup() {
        let group = build_group(b"CELL", 0, &[]);
        let reader = EsmReader::new(&group);
        assert!(reader.is_group());

        let record = build_record(b"STAT", 0, &[]);
        let reader = EsmReader::new(&record);
        assert!(!reader.is_group());
    }

    #[test]
    fn skip_record_advances_position() {
        let data = build_record(b"STAT", 0, &[(b"EDID", b"Test\0")]);
        let mut reader = EsmReader::new(&data);
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
                    d.extend_from_slice(&42u32.to_le_bytes());  // record count
                    d.extend_from_slice(&0u32.to_le_bytes());   // next object id
                    d
                }),
                (b"MAST", b"FalloutNV.esm\0"),
                (b"DATA", &0u64.to_le_bytes()),
            ],
        );
        let mut reader = EsmReader::new(&tes4);
        let fh = reader.read_file_header().unwrap();
        assert_eq!(fh.record_count, 42);
        assert_eq!(fh.master_files, vec!["FalloutNV.esm"]);
    }
}
