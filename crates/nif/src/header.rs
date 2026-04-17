//! NIF file header parsing.
//!
//! The header contains version info, block type tables, block sizes,
//! and the string table. Everything needed to navigate the block data.

use crate::version::NifVersion;
use std::io::{self, Cursor, Read};
use std::sync::Arc;

/// Parsed NIF file header.
#[derive(Debug, Clone)]
pub struct NifHeader {
    /// File format version (packed u32).
    pub version: NifVersion,
    /// Endianness (true = little-endian, the common case).
    pub little_endian: bool,
    /// Game-specific version tag.
    pub user_version: u32,
    /// Second user version (Bethesda-specific, present in 20.2.0.7+).
    pub user_version_2: u32,
    /// Number of object blocks in the file.
    pub num_blocks: u32,
    /// RTTI class name table (e.g., "NiNode", "NiTriShape").
    pub block_types: Vec<String>,
    /// Maps each block index to its type in `block_types`.
    pub block_type_indices: Vec<u16>,
    /// Byte size of each serialized block.
    pub block_sizes: Vec<u32>,
    /// Global string table (referenced by string-table-indexed fields).
    /// Stored as `Arc<str>` so block parsers can clone references cheaply
    /// (atomic increment) instead of allocating a fresh String per read.
    pub strings: Vec<Arc<str>>,
    /// Maximum string length in the string table.
    pub max_string_length: u32,
    /// Number of object groups (for deferred loading).
    pub num_groups: u32,
}

impl NifHeader {
    /// Parse a NIF header from raw file bytes.
    /// Returns the header and the byte offset where block data begins.
    pub fn parse(data: &[u8]) -> io::Result<(Self, usize)> {
        // Phase 1: Parse ASCII header line
        let header_line_end = data
            .iter()
            .position(|&b| b == b'\n')
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no header line found"))?;
        let header_line = std::str::from_utf8(&data[..header_line_end])
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "non-UTF8 header line"))?;

        // Validate it's a Gamebryo/NetImmerse file
        if !header_line.contains("Gamebryo File Format")
            && !header_line.contains("NetImmerse File Format")
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unrecognized NIF header: {header_line}"),
            ));
        }

        let mut cursor = Cursor::new(data);
        cursor.set_position((header_line_end + 1) as u64);

        // Phase 2: Binary header fields
        let version = NifVersion(read_u32_le(&mut cursor)?);

        // Endianness byte (present in version >= 20.0.0.4)
        let little_endian = if version >= NifVersion(0x14000004) {
            let e = read_u8(&mut cursor)?;
            e != 0
        } else {
            true // older files are always little-endian
        };

        // User version (present in version >= 10.0.1.8 per nif.xml).
        // Older NetImmerse files (10.0.1.0–10.0.1.7) — including a chunk of
        // Oblivion's leftover content like meshes/creatures/minotaur/horn*.nif
        // — go directly from `version` to `num_blocks` with no `user_version`
        // field, so reading one would corrupt num_blocks and blow up the
        // block parser with "failed to fill whole buffer".
        let user_version = if version >= NifVersion(0x0A000108) {
            read_u32_le(&mut cursor)?
        } else {
            0
        };

        let num_blocks = read_u32_le(&mut cursor)?;

        // BSStreamHeader presence — per nif.xml `#BSSTREAMHEADER#`:
        //   (VER == 10.0.1.2)
        //   OR ((VER in {20.2.0.7, 20.0.0.5}
        //        OR (10.1.0.0 <= VER <= 20.0.0.4 AND USER <= 11))
        //       AND USER >= 3)
        //
        // The struct itself reads:
        //   BS Version    u32                                 (== `user_version_2`)
        //   Author        ExportString
        //   Unknown Int   u32,           only if BS Version > 130   (FO76, Starfield)
        //   Process Script ExportString, only if BS Version < 131  (≤ FO4)
        //   Export Script ExportString
        //   Max Filepath  ExportString, only if BS Version >= 103  (FO4+)
        //
        // Previously `user_version >= 3` alone (no version guard) — see #170.
        let has_bs_stream_header = version == NifVersion(0x0A000102)
            || (user_version >= 3
                && (version == NifVersion::V20_2_0_7
                    || version == NifVersion(0x14000005) // 20.0.0.5
                    || (version >= NifVersion(0x0A010000) // 10.1.0.0
                        && version <= NifVersion(0x14000004) // 20.0.0.4
                        && user_version <= 11)));
        let user_version_2 = if has_bs_stream_header {
            read_u32_le(&mut cursor)?
        } else {
            0
        };
        if has_bs_stream_header {
            let _author = read_short_string(&mut cursor)?;
            if user_version_2 > 130 {
                let _unknown_int = read_u32_le(&mut cursor)?;
            }
            if user_version_2 < 131 {
                let _process_script = read_short_string(&mut cursor)?;
            }
            let _export_script = read_short_string(&mut cursor)?;
            if user_version_2 >= 103 {
                let _max_filepath = read_short_string(&mut cursor)?;
            }
        }

        // Block types table — nif.xml: since 5.0.0.1. Previously gated at
        // 10.0.1.0 which missed any 5.x–10.0.0.x file. See #171.
        //
        // #388: bound `num_blocks` against the byte budget for the
        // following block-index / block-size arrays so a corrupt header
        // u32 (e.g. drifted from a CRC) can't OOM the parser.
        let total_bytes = cursor.get_ref().len();
        let pos = cursor.position() as usize;
        let remaining = total_bytes.saturating_sub(pos);
        let (block_types, block_type_indices) = if version >= NifVersion(0x05000001) {
            let num_block_types = read_u16_le(&mut cursor)? as usize;
            let mut types = Vec::with_capacity(num_block_types);
            for _ in 0..num_block_types {
                types.push(read_sized_string(&mut cursor)?);
            }
            // Each block_type_index entry is a u16; the indices array
            // must fit in what's left of the file.
            if (num_blocks as usize)
                .checked_mul(2)
                .map_or(true, |n| n > remaining)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "NIF header claims {num_blocks} blocks but only {remaining} bytes remain",
                    ),
                ));
            }
            let mut indices = Vec::with_capacity(num_blocks as usize);
            for _ in 0..num_blocks {
                indices.push(read_u16_le(&mut cursor)?);
            }
            (types, indices)
        } else {
            (Vec::new(), Vec::new())
        };

        // Block sizes — nif.xml: since 20.2.0.5. Previously gated at
        // 20.2.0.7 which missed 20.2.0.5 and 20.2.0.6 files. See #171.
        let block_sizes = if version >= NifVersion(0x14020005) {
            // Per #388 — same byte-budget guard as the indices table.
            let pos = cursor.position() as usize;
            let remaining = total_bytes.saturating_sub(pos);
            if (num_blocks as usize)
                .checked_mul(4)
                .map_or(true, |n| n > remaining)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "NIF header claims {num_blocks} block sizes but only {remaining} bytes remain",
                    ),
                ));
            }
            let mut sizes = Vec::with_capacity(num_blocks as usize);
            for _ in 0..num_blocks {
                sizes.push(read_u32_le(&mut cursor)?);
            }
            sizes
        } else {
            Vec::new()
        };

        // String table — since 20.1.0.1 per nif.xml. Must stay in sync with
        // the same threshold in NifStream::read_string (stream.rs); a mismatch
        // would corrupt reads on 20.1.0.1/20.1.0.2 files.
        let (strings, max_string_length) = if version >= NifVersion(0x14010001) {
            let num_strings = read_u32_le(&mut cursor)? as usize;
            let max_len = read_u32_le(&mut cursor)?;
            let mut strs: Vec<Arc<str>> = Vec::with_capacity(num_strings);
            for _ in 0..num_strings {
                strs.push(Arc::from(read_sized_string(&mut cursor)?));
            }
            (strs, max_len)
        } else {
            (Vec::new(), 0)
        };

        // Number of groups — nif.xml: since 5.0.0.6. Previously gated at
        // 10.0.1.0 which missed any 5.x–10.0.0.x file. See #171.
        let num_groups = if version >= NifVersion(0x05000006) {
            read_u32_le(&mut cursor)?
        } else {
            0
        };

        // Skip group sizes if present
        if num_groups > 0 {
            for _ in 0..num_groups {
                let _ = read_u32_le(&mut cursor)?;
            }
        }

        let offset = cursor.position() as usize;

        Ok((
            NifHeader {
                version,
                little_endian,
                user_version,
                user_version_2,
                num_blocks,
                block_types,
                block_type_indices,
                block_sizes,
                strings,
                max_string_length,
                num_groups,
            },
            offset,
        ))
    }

    /// Get the type name of a block by its index.
    pub fn block_type_name(&self, block_index: usize) -> Option<&str> {
        let type_idx = *self.block_type_indices.get(block_index)? as usize;
        self.block_types.get(type_idx).map(|s| s.as_str())
    }
}

// ── Helper functions for raw cursor reading ────────────────────────────

fn read_u8(cursor: &mut Cursor<&[u8]>) -> io::Result<u8> {
    let mut buf = [0u8; 1];
    cursor.read_exact(&mut buf)?;
    Ok(buf[0])
}

fn read_u16_le(cursor: &mut Cursor<&[u8]>) -> io::Result<u16> {
    let mut buf = [0u8; 2];
    cursor.read_exact(&mut buf)?;
    Ok(u16::from_le_bytes(buf))
}

fn read_u32_le(cursor: &mut Cursor<&[u8]>) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    cursor.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_sized_string(cursor: &mut Cursor<&[u8]>) -> io::Result<String> {
    let len = read_u32_le(cursor)? as usize;
    let mut buf = vec![0u8; len];
    cursor.read_exact(&mut buf)?;
    match String::from_utf8(buf) {
        Ok(s) => Ok(s),
        Err(e) => Ok(String::from_utf8_lossy(e.as_bytes()).into_owned()),
    }
}

fn read_short_string(cursor: &mut Cursor<&[u8]>) -> io::Result<String> {
    let len = read_u8(cursor)? as usize;
    let mut buf = vec![0u8; len];
    cursor.read_exact(&mut buf)?;
    // Short strings may include null terminator
    if buf.last() == Some(&0) {
        buf.pop();
    }
    match String::from_utf8(buf) {
        Ok(s) => Ok(s),
        Err(e) => Ok(String::from_utf8_lossy(e.as_bytes()).into_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::NifVersion;

    /// Build a minimal valid NIF header for version 20.2.0.7 (Skyrim).
    /// num_blocks=0, user_version=12, user_version_2=83.
    fn build_minimal_skyrim_header() -> Vec<u8> {
        let mut buf = Vec::new();

        // ASCII header line
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");

        // u32: version
        buf.extend_from_slice(&0x14020007u32.to_le_bytes());

        // u8: little-endian flag (version >= 20.0.0.4)
        buf.push(1);

        // u32: user_version (version >= 10.0.1.0)
        buf.extend_from_slice(&12u32.to_le_bytes());

        // u32: num_blocks
        buf.extend_from_slice(&0u32.to_le_bytes());

        // u32: user_version_2 (version >= 10.0.1.0 && user_version >= 10)
        buf.extend_from_slice(&83u32.to_le_bytes());

        // short strings: author, process_script, export_script
        // (version >= 10.0.1.0 && user_version >= 10, user_version_2 > 0)
        buf.push(1);
        buf.push(0); // author: 1 byte, null terminator
        buf.push(1);
        buf.push(0); // process_script
        buf.push(1);
        buf.push(0); // export_script

        // u16: num_block_types = 0 (version >= 10.0.1.0)
        buf.extend_from_slice(&0u16.to_le_bytes());

        // (no block type indices — num_blocks is 0)
        // (no block sizes — num_blocks is 0)

        // u32: num_strings = 0 (version >= 20.1.0.3)
        buf.extend_from_slice(&0u32.to_le_bytes());
        // u32: max_string_length = 0
        buf.extend_from_slice(&0u32.to_le_bytes());

        // u32: num_groups = 0 (version >= 10.0.1.0)
        buf.extend_from_slice(&0u32.to_le_bytes());

        buf
    }

    #[test]
    fn parse_minimal_skyrim_header() {
        let data = build_minimal_skyrim_header();
        let (header, offset) = NifHeader::parse(&data).unwrap();

        assert_eq!(header.version, NifVersion::V20_2_0_7);
        assert!(header.little_endian);
        assert_eq!(header.user_version, 12);
        assert_eq!(header.user_version_2, 83);
        assert_eq!(header.num_blocks, 0);
        assert!(header.block_types.is_empty());
        assert!(header.block_sizes.is_empty());
        assert!(header.strings.is_empty());
        assert_eq!(header.num_groups, 0);
        assert_eq!(offset, data.len());
    }

    #[test]
    fn parse_header_with_blocks_and_strings() {
        let mut buf = Vec::new();

        // Header line
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
        buf.extend_from_slice(&0x14020007u32.to_le_bytes()); // version
        buf.push(1); // little-endian
        buf.extend_from_slice(&12u32.to_le_bytes()); // user_version
        buf.extend_from_slice(&2u32.to_le_bytes()); // num_blocks = 2
        buf.extend_from_slice(&83u32.to_le_bytes()); // user_version_2

        // Author/process/export short strings
        buf.push(1);
        buf.push(0);
        buf.push(1);
        buf.push(0);
        buf.push(1);
        buf.push(0);

        // Block types: 2 types
        buf.extend_from_slice(&2u16.to_le_bytes());
        // "NiNode" (sized string)
        buf.extend_from_slice(&6u32.to_le_bytes());
        buf.extend_from_slice(b"NiNode");
        // "NiTriShape" (sized string)
        buf.extend_from_slice(&10u32.to_le_bytes());
        buf.extend_from_slice(b"NiTriShape");

        // Block type indices: block 0 → type 0, block 1 → type 1
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());

        // Block sizes: 100 bytes each (version >= 20.2.0.7)
        buf.extend_from_slice(&100u32.to_le_bytes());
        buf.extend_from_slice(&200u32.to_le_bytes());

        // String table: 2 strings
        buf.extend_from_slice(&2u32.to_le_bytes()); // num_strings
        buf.extend_from_slice(&6u32.to_le_bytes()); // max_string_length
                                                    // "Scene" (sized string)
        buf.extend_from_slice(&5u32.to_le_bytes());
        buf.extend_from_slice(b"Scene");
        // "Mesh01" (sized string)
        buf.extend_from_slice(&6u32.to_le_bytes());
        buf.extend_from_slice(b"Mesh01");

        // num_groups = 0
        buf.extend_from_slice(&0u32.to_le_bytes());

        let (header, _offset) = NifHeader::parse(&buf).unwrap();

        assert_eq!(header.num_blocks, 2);
        assert_eq!(header.block_types, vec!["NiNode", "NiTriShape"]);
        assert_eq!(header.block_type_indices, vec![0, 1]);
        assert_eq!(header.block_sizes, vec![100, 200]);
        assert_eq!(header.strings.len(), 2);
        assert_eq!(&*header.strings[0], "Scene");
        assert_eq!(&*header.strings[1], "Mesh01");
        assert_eq!(header.max_string_length, 6);

        assert_eq!(header.block_type_name(0), Some("NiNode"));
        assert_eq!(header.block_type_name(1), Some("NiTriShape"));
        assert_eq!(header.block_type_name(2), None);
    }

    #[test]
    fn reject_invalid_header_line() {
        let data = b"Not a NIF file\n\x00\x00\x00\x00";
        let result = NifHeader::parse(data);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unrecognized NIF header"));
    }

    #[test]
    fn accept_netimmerse_header() {
        // Old NIF files use "NetImmerse File Format" instead of "Gamebryo"
        let mut buf = Vec::new();
        buf.extend_from_slice(b"NetImmerse File Format, Version 4.0.0.2\n");
        buf.extend_from_slice(&0x04000002u32.to_le_bytes()); // version 4.0.0.2
                                                             // num_blocks (no user_version for this old version)
        buf.extend_from_slice(&0u32.to_le_bytes());

        let (header, _) = NifHeader::parse(&buf).unwrap();
        assert_eq!(header.version, NifVersion::V4_0_0_2);
        assert_eq!(header.num_blocks, 0);
        // Old versions don't have user_version, block types, etc.
        assert_eq!(header.user_version, 0);
        assert!(header.block_types.is_empty());
    }

    /// Regression for #170: BSStreamHeader must NOT be read for a
    /// non-Bethesda file with user_version >= 3 at a version outside the
    /// nif.xml-specified BSStreamHeader range. v20.1.0.0 is NOT 10.0.1.2,
    /// NOT 20.0.0.5, NOT 20.2.0.7, and NOT in 10.1.0.0–20.0.0.4.
    /// Previously `user_version >= 3` alone triggered the read, which
    /// would consume bytes from the block-type table as a bogus
    /// BSStreamHeader.
    #[test]
    fn bs_stream_header_not_read_for_off_spec_version() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.1.0.0\n");
        buf.extend_from_slice(&0x14010000u32.to_le_bytes()); // v20.1.0.0
        buf.push(1); // little-endian (>= 20.0.0.4)
        buf.extend_from_slice(&4u32.to_le_bytes()); // user_version = 4
        buf.extend_from_slice(&0u32.to_le_bytes()); // num_blocks = 0
                                                    // No BSStreamHeader should follow. Next: block_types (since 5.0.0.1).
        buf.extend_from_slice(&0u16.to_le_bytes()); // num_block_types = 0
                                                    // No block_sizes (version < 20.2.0.5).
                                                    // No string table (version < 20.1.0.1).
                                                    // num_groups:
        buf.extend_from_slice(&0u32.to_le_bytes());

        let (header, offset) = NifHeader::parse(&buf).unwrap();
        assert_eq!(header.version, NifVersion(0x14010000));
        assert_eq!(header.user_version, 4);
        assert_eq!(header.user_version_2, 0);
        assert_eq!(header.num_blocks, 0);
        assert_eq!(offset, buf.len());
    }

    /// Regression for #171: block_sizes should be present at v20.2.0.5,
    /// not just v20.2.0.7.
    #[test]
    fn block_sizes_present_at_20_2_0_5() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.5\n");
        buf.extend_from_slice(&0x14020005u32.to_le_bytes()); // 20.2.0.5
        buf.push(1); // little-endian
        buf.extend_from_slice(&0u32.to_le_bytes()); // user_version = 0 (non-Bethesda)
        buf.extend_from_slice(&1u32.to_le_bytes()); // num_blocks = 1
                                                    // No BSStreamHeader (user_version < 3 and version != 10.0.1.2).
                                                    // Block types since >= 5.0.0.1:
        buf.extend_from_slice(&1u16.to_le_bytes()); // num_block_types
        buf.extend_from_slice(&6u32.to_le_bytes()); // "NiNode"
        buf.extend_from_slice(b"NiNode");
        buf.extend_from_slice(&0u16.to_le_bytes()); // block 0 → type 0
                                                    // Block sizes since >= 20.2.0.5:
        buf.extend_from_slice(&100u32.to_le_bytes()); // block 0 size
                                                      // String table since >= 20.1.0.1:
        buf.extend_from_slice(&0u32.to_le_bytes()); // num_strings
        buf.extend_from_slice(&0u32.to_le_bytes()); // max_string_length
                                                    // num_groups:
        buf.extend_from_slice(&0u32.to_le_bytes());

        let (header, offset) = NifHeader::parse(&buf).unwrap();
        assert_eq!(header.block_sizes, vec![100]);
        assert_eq!(header.block_types, vec!["NiNode"]);
        assert_eq!(offset, buf.len());
    }
}
