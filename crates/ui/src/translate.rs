//! Scaleform translation tables (`interface\translate_<language>.txt`): the `$Key → text` map GFx
//! applies to a text field created with or set to a `$` key (M48 GUI-FONT-004). Ruffle has no such
//! pass; the vendored `ruffle_core` carries one (`ruffle_core::byro_translation`,
//! `third_party/ruffle/PATCHES.md`), and [`install_translations`] gives it the active language's
//! table.
//!
//! The shipped files are UTF-16LE with a byte-order mark, one `key<TAB>text` entry per CRLF line.
//! The file is game-archive input: a malformed line is skipped, never an error.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::navigator::ScaleformResourceProvider;

/// Entries kept at most; a vanilla table holds a few thousand.
const MAX_ENTRIES: usize = 65_536;

/// Parse a translation table. UTF-16 by its byte-order mark (LE or BE), UTF-8 otherwise. Only
/// `$`-prefixed keys are kept; a later duplicate replaces an earlier one, as GFx loads them.
pub fn parse_translation_table(bytes: &[u8]) -> HashMap<String, String> {
    let text = decode_text(bytes);
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

/// Give Ruffle's `$`-key pass the active language's table: `interface\translate_<spelling>.txt`
/// for the first spelling `provider` carries (GUI-FONT-002's language). Loaded once per language;
/// a provider with no table leaves the installed one in place.
pub(crate) fn install_translations(provider: &dyn ScaleformResourceProvider, spellings: &[String]) {
    static INSTALLED: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    let installed = INSTALLED.get_or_init(Default::default);
    let Some((path, bytes)) = spellings.iter().find_map(|spelling| {
        let path = format!("interface\\translate_{}.txt", spelling.to_ascii_lowercase());
        provider.load(&path).ok().flatten().map(|bytes| (path, bytes))
    }) else {
        return;
    };
    let mut current = installed.lock().unwrap_or_else(|e| e.into_inner());
    if current.as_deref() == Some(path.as_str()) {
        return;
    }
    let table = Arc::new(parse_translation_table(&bytes));
    log::info!("Scaleform translation table {path:?}: {} key(s)", table.len());
    ruffle_core::byro_translation::set_text_translator(Some(Arc::new(move |key: &str| table.get(key).cloned())));
    *current = Some(path);
}

/// A game interface text file's contents: UTF-16 by its byte-order mark (LE or BE), UTF-8
/// (with or without a mark) otherwise. Shared with the font configuration reader.
pub(crate) fn decode_text(bytes: &[u8]) -> String {
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        _ => String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).into_owned(),
    }
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

    /// GUI-FONT-004 — with a table installed, Ruffle's patched text pass replaces a `$` key, whole
    /// or per HTML run, and leaves everything else alone.
    #[test]
    fn the_ruffle_text_pass_translates_installed_keys() {
        use ruffle_core::byro_translation::{set_text_translator, translate};
        use ruffle_core::string::WString;
        let table = Arc::new(parse_translation_table(&utf16le("$Compass_North\tN\r\n$Quit?\tQuit to main menu?")));
        set_text_translator(Some(Arc::new(move |key: &str| table.get(key).cloned())));
        let t = |s: &str| translate(&WString::from_utf8(s)).map(|w| w.to_utf8_lossy().into_owned());
        assert_eq!(t("$Quit?").as_deref(), Some("Quit to main menu?"));
        assert_eq!(t("<p align=\"center\"><b>$Compass_North</b></p>").as_deref(), Some("<p align=\"center\"><b>N</b></p>"));
        assert_eq!(t("$Unknown"), None);
        assert_eq!(t("costs $5"), None, "only a run that starts with $ is a key");
        set_text_translator(None);
        assert_eq!(t("$Quit?"), None);
    }
}
