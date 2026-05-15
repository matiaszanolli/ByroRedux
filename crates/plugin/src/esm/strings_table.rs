//! Bethesda `.STRINGS` / `.DLSTRINGS` / `.ILSTRINGS` companion-file loader.
//!
//! Skyrim (and later games) separate localizable text from the ESM/ESP
//! records. A localized plugin (`TES4.flags & 0x80`) stores FULL / DESC /
//! etc. sub-records as 4-byte lstring-table indices rather than inline
//! z-strings. The actual strings live in one of three sibling files under
//! `<esm_dir>/Strings/<plugin_stem>_<lang>.<EXT>`:
//!
//! | Extension    | Content                       | Notes                              |
//! |--------------|-------------------------------|------------------------------------|
//! | `.STRINGS`   | FULL, RNAM, CNAM, SHRT, …     | bare null-terminated strings       |
//! | `.DLSTRINGS` | DESC (description/lore text)  | length-prefixed + null-terminated  |
//! | `.ILSTRINGS` | INFO (dialogue responses)     | length-prefixed + null-terminated  |
//!
//! ## Binary layout
//!
//! Every file starts with an 8-byte header:
//!
//! ```text
//! [count:     u32 LE]   number of directory entries
//! [data_size: u32 LE]   byte length of the string data blob
//! ```
//!
//! Followed by `count × 8` directory bytes:
//!
//! ```text
//! [id:     u32 LE]   lstring-table ID (matches the u32 in the sub-record)
//! [offset: u32 LE]   byte offset into the string data blob (0-based)
//! ```
//!
//! Then `data_size` bytes of string data:
//!
//! - **`.STRINGS`** — each entry is a raw null-terminated string starting at
//!   `offset`.
//! - **`.DLSTRINGS` / `.ILSTRINGS`** — each entry starts with a 4-byte LE
//!   length count, then the string bytes, then a null terminator.
//!   The length includes the null byte on some versions and excludes it on
//!   others; we always stop at the first `\0` regardless.
//!
//! ## Usage
//!
//! ```ignore
//! use crate::esm::strings_table::{StringTableSet, StringsTableGuard};
//!
//! let tables = StringTableSet::load(plugin_path, "english");
//! let _guard = StringsTableGuard::new(tables);
//! let index = parse_esm_with_load_order(bytes, remap)?;
//! // lstring placeholders are now resolved inside parse_esm
//! ```
//!
//! [`StringsTableGuard`]: crate::esm::records::common::StringsTableGuard
//! [`parse_esm_with_load_order`]: crate::esm::records::parse_esm_with_load_order

use std::collections::HashMap;
use std::io;
use std::path::Path;

/// A single loaded Bethesda companion string file.
///
/// Parses the on-disk format into an in-memory `id → String` map for O(1)
/// lookup at record-parse time. Corrupt or out-of-bounds entries are skipped
/// with a warning rather than propagating an error so a single bad entry
/// doesn't abort the whole parse.
pub struct StringsTable {
    map: HashMap<u32, String>,
}

impl StringsTable {
    /// Parse a raw companion-file byte buffer.
    ///
    /// `has_length_prefix` — `true` for `.DLSTRINGS` and `.ILSTRINGS`
    /// (each string is preceded by a 4-byte LE length), `false` for plain
    /// `.STRINGS` (strings are stored as bare null-terminated bytes).
    pub fn parse(data: &[u8], has_length_prefix: bool) -> io::Result<Self> {
        if data.len() < 8 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "strings file header too short (need 8 bytes)",
            ));
        }

        let count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        let data_size = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;

        let dir_bytes = count.checked_mul(8).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "strings directory count overflow")
        })?;
        let dir_end = 8usize.checked_add(dir_bytes).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "strings directory offset overflow")
        })?;
        let blob_end = dir_end.checked_add(data_size).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "strings blob offset overflow")
        })?;

        if data.len() < blob_end {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "strings file truncated: need {} bytes, have {}",
                    blob_end,
                    data.len()
                ),
            ));
        }

        let dir = &data[8..dir_end];
        let blob = &data[dir_end..blob_end];

        let mut map = HashMap::with_capacity(count);
        for i in 0..count {
            let entry = &dir[i * 8..(i + 1) * 8];
            let id = u32::from_le_bytes(entry[0..4].try_into().unwrap());
            let offset = u32::from_le_bytes(entry[4..8].try_into().unwrap()) as usize;

            if offset >= blob.len() {
                log::warn!("strings entry 0x{:08X}: offset {} out of blob ({})", id, offset, blob.len());
                continue;
            }

            let s = if has_length_prefix {
                if offset + 4 > blob.len() {
                    log::warn!("strings entry 0x{:08X}: truncated length prefix at {}", id, offset);
                    continue;
                }
                // 4-byte LE length prefix (may or may not include the null).
                // We ignore it and just scan for the null terminator.
                let str_start = offset + 4;
                if str_start > blob.len() {
                    continue;
                }
                let nul_pos = blob[str_start..]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(blob.len() - str_start);
                String::from_utf8_lossy(&blob[str_start..str_start + nul_pos]).into_owned()
            } else {
                let nul_pos = blob[offset..]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(blob.len() - offset);
                String::from_utf8_lossy(&blob[offset..offset + nul_pos]).into_owned()
            };

            map.insert(id, s);
        }

        Ok(Self { map })
    }

    /// Look up an lstring ID. Returns `None` if the ID is not present.
    #[inline]
    pub fn get(&self, id: u32) -> Option<&str> {
        self.map.get(&id).map(|s| s.as_str())
    }

    /// Number of entries in this table.
    #[inline]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if the table has no entries.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// All three companion-file variants for one localized plugin.
///
/// Constructed by [`StringTableSet::load`] from a plugin path + language tag.
/// Passed into a parse scope via [`StringsTableGuard`] in `records::common`.
///
/// [`StringsTableGuard`]: crate::esm::records::common::StringsTableGuard
#[derive(Default)]
pub struct StringTableSet {
    /// `.STRINGS` — FULL, RNAM, CNAM, SHRT, etc.
    pub strings: Option<StringsTable>,
    /// `.DLSTRINGS` — DESC (description / lore text).
    pub dlstrings: Option<StringsTable>,
    /// `.ILSTRINGS` — INFO dialogue response text.
    pub ilstrings: Option<StringsTable>,
}

impl StringTableSet {
    /// Load the three companion files for `plugin_path` using `language`
    /// (e.g. `"english"`, `"french"`).
    ///
    /// The files are expected at:
    /// ```text
    /// <plugin_dir>/Strings/<plugin_stem>_<language>.STRINGS
    /// <plugin_dir>/Strings/<plugin_stem>_<language>.DLSTRINGS
    /// <plugin_dir>/Strings/<plugin_stem>_<language>.ILSTRINGS
    /// ```
    ///
    /// Missing files are silently skipped — a mod that ships only `.STRINGS`
    /// still resolves FULL entries correctly. Parse errors are logged as
    /// warnings and that table is omitted.
    pub fn load(plugin_path: &Path, language: &str) -> Self {
        let stem = plugin_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let strings_dir = plugin_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("Strings");

        let load_file = |ext: &str, has_prefix: bool| -> Option<StringsTable> {
            let name = format!("{stem}_{language}.{ext}");
            let path = strings_dir.join(&name);
            let data = std::fs::read(&path).ok()?;
            match StringsTable::parse(&data, has_prefix) {
                Ok(t) => {
                    log::debug!(
                        "loaded {} ({} entries)",
                        path.display(),
                        t.len()
                    );
                    Some(t)
                }
                Err(e) => {
                    log::warn!("failed to parse {}: {e}", path.display());
                    None
                }
            }
        };

        Self {
            strings: load_file("STRINGS", false),
            dlstrings: load_file("DLSTRINGS", true),
            ilstrings: load_file("ILSTRINGS", true),
        }
    }

    /// Resolve an lstring ID against all three tables.
    ///
    /// Checks `.STRINGS` first (most FULL/name records land here), then
    /// `.DLSTRINGS`, then `.ILSTRINGS`. Returns `None` when the ID is not
    /// present in any table.
    pub fn resolve(&self, id: u32) -> Option<&str> {
        self.strings
            .as_ref()
            .and_then(|t| t.get(id))
            .or_else(|| self.dlstrings.as_ref().and_then(|t| t.get(id)))
            .or_else(|| self.ilstrings.as_ref().and_then(|t| t.get(id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic `.STRINGS` file with `entries` mappings.
    fn build_strings_file(entries: &[(u32, &str)], has_prefix: bool) -> Vec<u8> {
        let count = entries.len() as u32;

        // Build the string blob first so we know each offset.
        let mut blob = Vec::new();
        let mut offsets = Vec::new();
        for (_, s) in entries {
            offsets.push(blob.len() as u32);
            if has_prefix {
                // 4-byte length prefix (byte count including null)
                let len = s.len() as u32 + 1;
                blob.extend_from_slice(&len.to_le_bytes());
            }
            blob.extend_from_slice(s.as_bytes());
            blob.push(0); // null terminator
        }

        let data_size = blob.len() as u32;
        let mut out = Vec::new();
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&data_size.to_le_bytes());
        for (i, (id, _)) in entries.iter().enumerate() {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&offsets[i].to_le_bytes());
        }
        out.extend_from_slice(&blob);
        out
    }

    #[test]
    fn strings_round_trip() {
        let data = build_strings_file(
            &[(0x0001u32, "Iron Sword"), (0x0002, "Dragonscale Armor")],
            false,
        );
        let table = StringsTable::parse(&data, false).unwrap();
        assert_eq!(table.get(0x0001), Some("Iron Sword"));
        assert_eq!(table.get(0x0002), Some("Dragonscale Armor"));
        assert_eq!(table.get(0x9999), None);
    }

    #[test]
    fn dlstrings_round_trip() {
        let data = build_strings_file(
            &[(0x0010u32, "A fine blade, worthy of a Companion."), (0x0020, "")],
            true,
        );
        let table = StringsTable::parse(&data, true).unwrap();
        assert_eq!(
            table.get(0x0010),
            Some("A fine blade, worthy of a Companion.")
        );
        // Empty string: length prefix = 1 (null only), string content = ""
        assert_eq!(table.get(0x0020), Some(""));
    }

    #[test]
    fn string_table_set_resolve_priority() {
        // ID 0x0001 exists only in .STRINGS; ID 0x0010 only in .DLSTRINGS.
        let strings_data =
            build_strings_file(&[(0x0001u32, "Iron Sword")], false);
        let dlstrings_data =
            build_strings_file(&[(0x0010u32, "A fine blade.")], true);

        let set = StringTableSet {
            strings: Some(StringsTable::parse(&strings_data, false).unwrap()),
            dlstrings: Some(StringsTable::parse(&dlstrings_data, true).unwrap()),
            ilstrings: None,
        };

        assert_eq!(set.resolve(0x0001), Some("Iron Sword"));
        assert_eq!(set.resolve(0x0010), Some("A fine blade."));
        assert_eq!(set.resolve(0xDEAD), None);
    }

    #[test]
    fn parse_rejects_truncated_header() {
        let data = [0u8; 4]; // only 4 bytes — header needs 8
        assert!(StringsTable::parse(&data, false).is_err());
    }

    #[test]
    fn parse_rejects_truncated_blob() {
        // Directory says 1 entry with data_size=10, but blob is only 2 bytes.
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_le_bytes()); // count
        data.extend_from_slice(&10u32.to_le_bytes()); // data_size (claimed)
        data.extend_from_slice(&0x0001u32.to_le_bytes()); // id
        data.extend_from_slice(&0u32.to_le_bytes()); // offset
        data.extend_from_slice(&[b'H', b'i']); // only 2 bytes of blob (need 10)
        assert!(StringsTable::parse(&data, false).is_err());
    }

    #[test]
    fn oob_offset_skipped_gracefully() {
        // Entry with offset pointing past the blob end — must not panic, just skip.
        let mut data = build_strings_file(&[(0x0001u32, "Valid")], false);
        // Corrupt the directory offset of entry 0 to point way past the blob.
        let offset_pos = 8 + 4; // skip header + id field of first entry
        let bad_offset = 0xFFFF_FFFFu32;
        data[offset_pos..offset_pos + 4].copy_from_slice(&bad_offset.to_le_bytes());

        // Also add a valid entry AFTER the corrupt one.
        let data2 = build_strings_file(&[(0x0001u32, "Corrupt"), (0x0002, "Good")], false);
        // Corrupt only entry 0's offset.
        let mut data2 = data2;
        let offset_pos = 8 + 4; // first entry id=4 bytes, then offset
        data2[offset_pos..offset_pos + 4].copy_from_slice(&bad_offset.to_le_bytes());

        let table = StringsTable::parse(&data2, false).unwrap();
        // The corrupt entry is skipped; the valid one must survive.
        assert_eq!(table.get(0x0001), None);
        assert_eq!(table.get(0x0002), Some("Good"));
    }
}
