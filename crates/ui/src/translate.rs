//! Scaleform translation tables (`interface\translate_<language>.txt`, #7036): the `$Key → text`
//! map GFx applies to a text field set to a `$`-prefixed string. Ruffle has no such pass, so the
//! engine resolves a key itself before writing the text (the Fallout 4 compass's
//! `$Compass_North` → `N`).
//!
//! The shipped files are UTF-16LE with a byte-order mark, one `key<TAB>text` entry per CRLF line.
//! The file is game-archive input: a malformed line is skipped, never an error.

use std::collections::HashMap;

/// Entries kept at most; a vanilla table holds a few thousand.
const MAX_ENTRIES: usize = 65_536;

/// Parse a translation table. UTF-16 by its byte-order mark (LE or BE), UTF-8 otherwise. Only
/// `$`-prefixed keys are kept; a later duplicate replaces an earlier one, as GFx loads them.
pub fn parse_translation_table(bytes: &[u8]) -> HashMap<String, String> {
    let text = match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        _ => String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).into_owned(),
    };
    let mut table = HashMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('\t') else {
            continue;
        };
        if !key.starts_with('$') || (table.len() >= MAX_ENTRIES && !table.contains_key(key)) {
            continue;
        }
        table.insert(key.to_owned(), value.trim_end_matches('\r').to_owned());
    }
    table
}

fn utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
    let units = bytes.as_chunks::<2>().0.iter().map(|&pair| unit(pair));
    char::decode_utf16(units).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16le(text: &str) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        bytes
    }

    #[test]
    fn a_utf16_table_maps_dollar_keys_to_their_text() {
        let table = parse_translation_table(&utf16le("$Compass_North\tN\r\n$Compass_East\tE\r\nnot a key\tx\r\n$NoTab\r\n$Dup\ta\r\n$Dup\tb"));
        assert_eq!(table.get("$Compass_North").map(String::as_str), Some("N"));
        assert_eq!(table.get("$Compass_East").map(String::as_str), Some("E"));
        assert_eq!(table.get("$Dup").map(String::as_str), Some("b"));
        assert_eq!(table.len(), 3, "the keyless and tabless lines are skipped");
    }

    #[test]
    fn utf8_and_big_endian_tables_parse_too_and_garbage_is_empty() {
        assert_eq!(parse_translation_table(b"$A\tx\n").get("$A").map(String::as_str), Some("x"));
        let mut be = vec![0xFE, 0xFF];
        be.extend("$B\ty".encode_utf16().flat_map(u16::to_be_bytes));
        assert_eq!(parse_translation_table(&be).get("$B").map(String::as_str), Some("y"));
        assert!(parse_translation_table(&[0xFF, 0xFE, 0x00]).is_empty());
    }
}
