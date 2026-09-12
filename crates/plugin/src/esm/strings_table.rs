//! Bethesda `.STRINGS` / `.DLSTRINGS` / `.ILSTRINGS` companion-file loader.
//!
//! Skyrim (and later games) separate localizable text from the ESM/ESP
//! records. A localized plugin (`TES4.flags & 0x80`) stores FULL / DESC /
//! etc. sub-records as 4-byte lstring-table indices rather than inline
//! z-strings. The actual strings live in one of three files under
//! `Strings/<plugin_stem>_<lang>.<EXT>`, either loose beside the plugin or
//! packed in a game archive. `<lang>` is spelled out in full on Skyrim
//! (`_english`) and abbreviated from Fallout 4 onward (`_en`) — see
//! [`language_candidates`], which resolves both from one token (#4168):
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
//! `text`, not `ignore` (#3348): this sketch uses `crate::` paths (doctests
//! compile as an *external* crate) and undefined locals, so it can never
//! build — but `ignore` still built it under `cargo test -- --ignored`,
//! reddening that sweep.
//! ```text
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

/// Windows-1252 `0x80..=0x9F` → Unicode. Every other byte is either ASCII
/// (`0x00..=0x7F`) or maps to the identically-numbered code point
/// (`0xA0..=0xFF`), so only this 32-entry window needs a table. The five
/// bytes cp1252 leaves undefined (`0x81`, `0x8D`, `0x8F`, `0x90`, `0x9D`)
/// map to their C1 control code points, matching the WHATWG encoding
/// standard.
const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

/// Decode one companion-file entry: UTF-8 first, Windows-1252 fallback.
///
/// #4170. Both decode arms used to call `String::from_utf8_lossy`, which
/// turns every non-UTF-8 byte into U+FFFD. Bethesda's English tables are
/// cp1252 and do carry such bytes — measured over the shipped archives,
/// **80 of 172 806** `_en` entries on Fallout 4 and **228 of 187 563** on
/// Starfield (curly apostrophes, ellipses, and the RobCo glitch-art `¤`
/// runs like `W¤lcom¤ ¤o R¤¤Co Industries`). Skyrim's 67 414 are pure
/// ASCII and unaffected either way.
///
/// The order is load-bearing and the reason this is not a flat cp1252
/// decode. The *localized* tables are genuinely UTF-8: `Fallout4_fr`,
/// `_ja` and `_ru` all decode as valid UTF-8 end to end with zero
/// replacement characters. Decoding those as cp1252 would mojibake every
/// multi-byte sequence in them (`é` → `Ã©`), trading 80 corrupt English
/// entries for ~1.8 M corrupt Russian bytes. So: try UTF-8, and fall back
/// to cp1252 only for the byte runs UTF-8 rejects.
fn decode_entry(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes
            .iter()
            .map(|&b| match b {
                0x80..=0x9F => CP1252_HIGH[(b - 0x80) as usize],
                _ => b as char,
            })
            .collect(),
    }
}

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
            io::Error::new(
                io::ErrorKind::InvalidData,
                "strings directory count overflow",
            )
        })?;
        let dir_end = 8usize.checked_add(dir_bytes).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "strings directory offset overflow",
            )
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
                log::warn!(
                    "strings entry 0x{:08X}: offset {} out of blob ({})",
                    id,
                    offset,
                    blob.len()
                );
                continue;
            }

            let s = if has_length_prefix {
                if offset + 4 > blob.len() {
                    log::warn!(
                        "strings entry 0x{:08X}: truncated length prefix at {}",
                        id,
                        offset
                    );
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
                decode_entry(&blob[str_start..str_start + nul_pos])
            } else {
                let nul_pos = blob[offset..]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(blob.len() - offset);
                decode_entry(&blob[offset..offset + nul_pos])
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

/// Long ⇄ short language-tag aliases, measured from the shipped archives.
///
/// #4168. Skyrim spells the companion-file language segment out in full
/// (`Skyrim - Interface.bsa` holds `dawnguard_english.strings`,
/// `_french`, `_german`, `_italian`, `_japanese`, `_polish`, `_russian`,
/// `_spanish`, `_chinese`); every Creation-Engine title from Fallout 4
/// onward uses a short ISO-ish tag instead (`Fallout4 - Interface.ba2`
/// holds `Fallout4_en.STRINGS`, and zero file in it contains the string
/// "english"). Measured tag set across the Fallout 4 and Starfield
/// archives: `cn de en es esmx fr it ja pl ptbr ru zhhans zhhant`.
///
/// Only the nine pairs below have both spellings in shipped content.
/// `esmx` (Latin-American Spanish), `ptbr`, `zhhans` and `zhhant` are
/// short-only — they have no long-form equivalent and need no row.
const LANGUAGE_ALIASES: [(&str, &str); 9] = [
    ("english", "en"),
    ("french", "fr"),
    ("german", "de"),
    ("italian", "it"),
    ("japanese", "ja"),
    ("polish", "pl"),
    ("russian", "ru"),
    ("spanish", "es"),
    ("chinese", "cn"),
];

/// Expand a language token into the file-name segments to try, in order.
///
/// The caller's own token always comes first, so an explicit
/// `BYRO_STRINGS_LANG=ptbr` is honoured verbatim and Skyrim's default
/// `"english"` still hits on the first probe (no behaviour change for the
/// one game that worked before). The alias, when there is one, follows —
/// which is what lets the same default resolve `Fallout4_en.STRINGS` on
/// Fallout 4 / Fallout 76 / Starfield.
///
/// Matching is case-insensitive; the returned candidates preserve the
/// spelling that will actually be used in the file name.
pub fn language_candidates(language: &str) -> Vec<String> {
    let lower = language.to_ascii_lowercase();
    let mut out = vec![language.to_string()];
    for (long, short) in LANGUAGE_ALIASES {
        let alias = if lower == long {
            short
        } else if lower == short {
            long
        } else {
            continue;
        };
        if !out.iter().any(|c| c.eq_ignore_ascii_case(alias)) {
            out.push(alias.to_string());
        }
        break;
    }
    out
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
    /// (e.g. `"english"`, `"en"`, `"french"`).
    ///
    /// The files are expected at:
    /// ```text
    /// <plugin_dir>/Strings/<plugin_stem>_<language>.STRINGS
    /// <plugin_dir>/Strings/<plugin_stem>_<language>.DLSTRINGS
    /// <plugin_dir>/Strings/<plugin_stem>_<language>.ILSTRINGS
    /// ```
    ///
    /// `language` is expanded through [`language_candidates`], so the
    /// Skyrim spelling (`_english`) and the Fallout 4+ one (`_en`) both
    /// resolve from a single token (#4168).
    ///
    /// Missing files are silently skipped — a mod that ships only `.STRINGS`
    /// still resolves FULL entries correctly. Parse errors are logged as
    /// warnings and that table is omitted.
    pub fn load(plugin_path: &Path, language: &str) -> Self {
        Self::load_with_archive(plugin_path, language, |_| None)
    }

    /// Load companion tables with an archive fallback.
    ///
    /// `read_archive` receives a canonical, backslash-separated path such as
    /// `strings\Skyrim_english.STRINGS`. A loose file always wins, matching
    /// Bethesda's override order; the callback is consulted only when that
    /// loose file is absent.
    pub fn load_with_archive<F>(plugin_path: &Path, language: &str, mut read_archive: F) -> Self
    where
        F: FnMut(&str) -> Option<Vec<u8>>,
    {
        let stem = plugin_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let strings_dir = plugin_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("Strings");

        // #4168 — try every spelling of the language segment, not just
        // the caller's. A loose file still beats an archive entry for the
        // *same* candidate, matching Bethesda's override order; the
        // candidate loop sits outside that so a fully-present `_english`
        // install is never shadowed by a stray loose `_en` file.
        let candidates = language_candidates(language);
        let mut load_file = |ext: &str, has_prefix: bool| -> Option<StringsTable> {
            for candidate in &candidates {
                let name = format!("{stem}_{candidate}.{ext}");
                let path = strings_dir.join(&name);
                let (data, source) = match std::fs::read(&path) {
                    Ok(data) => (data, path.display().to_string()),
                    Err(_) => {
                        let archive_path = format!(r"strings\{name}");
                        match read_archive(&archive_path) {
                            Some(data) => (data, archive_path),
                            None => continue,
                        }
                    }
                };
                match StringsTable::parse(&data, has_prefix) {
                    Ok(t) => {
                        log::debug!("loaded {} ({} entries)", source, t.len());
                        return Some(t);
                    }
                    Err(e) => {
                        log::warn!("failed to parse {}: {e}", source);
                        return None;
                    }
                }
            }
            None
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
        let raw: Vec<(u32, &[u8])> = entries.iter().map(|(id, s)| (*id, s.as_bytes())).collect();
        build_strings_file_raw(&raw, has_prefix)
    }

    /// Byte-level variant — the companion files are *not* UTF-8 (#4170),
    /// so the cp1252 fixtures have to bypass `&str` entirely.
    fn build_strings_file_raw(entries: &[(u32, &[u8])], has_prefix: bool) -> Vec<u8> {
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
            blob.extend_from_slice(s);
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
            &[
                (0x0010u32, "A fine blade, worthy of a Companion."),
                (0x0020, ""),
            ],
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
        let strings_data = build_strings_file(&[(0x0001u32, "Iron Sword")], false);
        let dlstrings_data = build_strings_file(&[(0x0010u32, "A fine blade.")], true);

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
    fn archive_fallback_uses_canonical_path() {
        let plugin = Path::new("/definitely-not-a-real-data-dir/Skyrim.esm");
        let data = build_strings_file(&[(0x0001, "Iron Sword")], false);
        let mut requested = Vec::new();

        let set = StringTableSet::load_with_archive(plugin, "english", |path| {
            requested.push(path.to_owned());
            (path == r"strings\Skyrim_english.STRINGS").then(|| data.clone())
        });

        assert_eq!(set.resolve(0x0001), Some("Iron Sword"));
        assert!(requested.contains(&r"strings\Skyrim_english.STRINGS".to_owned()));
    }

    #[test]
    fn loose_table_overrides_archive_fallback() {
        let dir = std::env::temp_dir().join(format!(
            "byroredux-plugin-loose-strings-{}",
            std::process::id()
        ));
        let plugin = dir.join("Skyrim.esm");
        let strings_dir = dir.join("Strings");
        std::fs::create_dir_all(&strings_dir).unwrap();
        std::fs::write(
            strings_dir.join("Skyrim_english.STRINGS"),
            build_strings_file(&[(0x0001, "Loose Sword")], false),
        )
        .unwrap();

        let set = StringTableSet::load_with_archive(&plugin, "english", |_| {
            Some(build_strings_file(&[(0x0001, "Packed Sword")], false))
        });

        assert_eq!(set.resolve(0x0001), Some("Loose Sword"));
        std::fs::remove_dir_all(&dir).unwrap();
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
        data.extend_from_slice(b"Hi"); // only 2 bytes of blob (need 10)
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

    /// Regression: #4170. Real bytes lifted from `Fallout4_en.STRINGS`
    /// (`Fallout4 - Interface.ba2`). `0xA4` is cp1252 `¤`, the RobCo
    /// glitch-art character Bethesda sprinkles through damaged-terminal
    /// text. `String::from_utf8_lossy` turned every one into U+FFFD.
    #[test]
    fn cp1252_glitch_art_decodes_instead_of_becoming_replacement_chars() {
        let raw: &[u8] = b"W\xa4lcom\xa4 \xa4o R\xa4\xa4Co Industries";
        let data = build_strings_file_raw(&[(0x0001u32, raw)], false);
        let table = StringsTable::parse(&data, false).unwrap();
        let got = table.get(0x0001).unwrap();
        assert_eq!(got, "W¤lcom¤ ¤o R¤¤Co Industries");
        assert!(
            !got.contains('\u{FFFD}'),
            "cp1252 bytes must not degrade to U+FFFD: {got:?}"
        );
    }

    /// Regression: #4170. The two cp1252 bytes that actually dominate the
    /// corpus — `0x92` (curly apostrophe) and `0x85` (ellipsis) — sit in
    /// the `0x80..=0x9F` window where cp1252 diverges from Latin-1, so a
    /// plain `b as char` fallback would get them wrong too.
    #[test]
    fn cp1252_high_window_is_not_decoded_as_latin1() {
        let data = build_strings_file_raw(
            &[
                (0x0001u32, b"Vault\x92s Finest"),
                (0x0002u32, b"Loading\x85"),
                (0x0003u32, b"\x80 100"),
            ],
            false,
        );
        let table = StringsTable::parse(&data, false).unwrap();
        assert_eq!(table.get(0x0001), Some("Vault\u{2019}s Finest"));
        assert_eq!(table.get(0x0002), Some("Loading\u{2026}"));
        // 0x80 is € in cp1252 but U+0080 (a C1 control) in Latin-1.
        assert_eq!(table.get(0x0003), Some("\u{20AC} 100"));
    }

    /// Regression: #4170, and the reason the fix is UTF-8-*first* rather
    /// than a flat cp1252 decode. `Fallout4_fr/_ja/_ru.STRINGS` are
    /// genuinely UTF-8 end to end (measured: zero replacement characters,
    /// 1.8 M non-ASCII bytes on `_ru` alone). Decoding those as cp1252
    /// would mojibake every multi-byte sequence, which is a far larger
    /// regression than the 80 English entries the fix is for.
    #[test]
    fn genuine_utf8_tables_survive_the_cp1252_fallback() {
        let data = build_strings_file_raw(
            &[
                (0x0001u32, "Fusil à plasma".as_bytes()),
                (0x0002u32, "Пистолет".as_bytes()),
                (0x0003u32, "プラズマ".as_bytes()),
            ],
            false,
        );
        let table = StringsTable::parse(&data, false).unwrap();
        assert_eq!(table.get(0x0001), Some("Fusil à plasma"));
        assert_eq!(table.get(0x0002), Some("Пистолет"));
        assert_eq!(table.get(0x0003), Some("プラズマ"));
    }

    /// Regression: #4168. Skyrim's `_english` and Fallout 4's `_en` are
    /// the same language; one token has to reach both spellings.
    #[test]
    fn language_candidates_pair_the_long_and_short_spellings() {
        assert_eq!(language_candidates("english"), ["english", "en"]);
        assert_eq!(language_candidates("en"), ["en", "english"]);
        assert_eq!(language_candidates("russian"), ["russian", "ru"]);
        assert_eq!(language_candidates("ja"), ["ja", "japanese"]);
        // Case-insensitive match, but the caller's spelling is preserved
        // verbatim as the first candidate.
        assert_eq!(language_candidates("English"), ["English", "en"]);
    }

    /// Regression: #4168. `esmx` / `ptbr` / `zhhans` / `zhhant` ship only
    /// in the short form — inventing a long spelling for them would add a
    /// guaranteed-miss probe on every load.
    #[test]
    fn language_candidates_leave_short_only_tags_alone() {
        for tag in ["ptbr", "esmx", "zhhans", "zhhant"] {
            assert_eq!(
                language_candidates(tag),
                [tag],
                "{tag} has no long-form spelling in any shipped archive"
            );
        }
    }

    /// Regression: #4168 end to end. An archive that holds only the
    /// Fallout 4 spelling must still resolve under the default
    /// `"english"` token — this is the exact shape of the live bug, where
    /// every lstring on FO4 / FO76 / Starfield stayed unresolved.
    #[test]
    fn fallout4_style_en_tables_resolve_under_the_english_default() {
        let strings = build_strings_file(&[(0x0001u32, "Dogmeat")], false);
        let seen = std::cell::RefCell::new(Vec::new());
        let set = StringTableSet::load_with_archive(
            Path::new("/nonexistent/Data/Fallout4.esm"),
            "english",
            |path| {
                seen.borrow_mut().push(path.to_string());
                (path == r"strings\Fallout4_en.STRINGS").then(|| strings.clone())
            },
        );
        assert_eq!(set.resolve(0x0001), Some("Dogmeat"));
        // The long spelling is still probed first, so a Skyrim install is
        // untouched by the new candidate.
        let probes = seen.borrow();
        assert_eq!(probes[0], r"strings\Fallout4_english.STRINGS");
        assert_eq!(probes[1], r"strings\Fallout4_en.STRINGS");
    }

    /// Regression: #4168. The Skyrim spelling must still win when both
    /// are present, so adding the candidate cannot change behaviour on
    /// the one game that already worked.
    #[test]
    fn skyrim_style_english_tables_still_win_over_the_short_alias() {
        let long = build_strings_file(&[(0x0001u32, "Whiterun")], false);
        let short = build_strings_file(&[(0x0001u32, "WRONG")], false);
        let set = StringTableSet::load_with_archive(
            Path::new("/nonexistent/Data/Skyrim.esm"),
            "english",
            |path| match path {
                r"strings\Skyrim_english.STRINGS" => Some(long.clone()),
                r"strings\Skyrim_en.STRINGS" => Some(short.clone()),
                _ => None,
            },
        );
        assert_eq!(set.resolve(0x0001), Some("Whiterun"));
    }
}
