//! Scaleform translation tables (`interface\translate_<language>.txt`): the `$Key → text` map GFx
//! applies to a text field created with or set to a `$` key (M48 GUI-FONT-004). Ruffle has no such
//! pass; the vendored `ruffle_core` carries one (`ruffle_core::byro_translation`,
//! `third_party/ruffle/PATCHES.md`), and [`install_translations`] gives it the active language's
//! table.
//!
//! The shipped files are UTF-16LE with a byte-order mark, one `key<TAB>text` entry per CRLF line.
//! The file is game-archive input: a malformed line is skipped, never an error.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

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

fn translation_plugins() -> &'static RwLock<Vec<String>> {
    static PLUGINS: OnceLock<RwLock<Vec<String>>> = OnceLock::new();
    PLUGINS.get_or_init(Default::default)
}

/// The session's enabled plugins in load order (file names), whose
/// `interface\translations\<plugin stem>_<language>.txt` tables [`install_translations`] merges, as
/// SKSE and F4SE do for every enabled plugin (catalog U4). Process-wide, like the text language.
pub fn set_translation_plugins(plugins: Vec<String>) {
    if let Ok(mut current) = translation_plugins().write() {
        *current = plugins;
    }
}

/// Parse a plugin's translation file by the script extenders' rules (SKSE64 / F4SE
/// `Translation.cpp`, `ParseTranslation`): UCS-2 little-endian with its byte-order mark, else the
/// file is refused; a line of at least four characters that starts with `$`; the key ends at the
/// line's *last* tab. Entries in file order.
pub fn parse_extender_translation(bytes: &[u8]) -> Option<Vec<(String, String)>> {
    let body = bytes.strip_prefix(&[0xFF, 0xFE])?;
    let text = utf16(body, u16::from_le_bytes);
    let mut entries = Vec::new();
    for raw in text.split('\n') {
        // The length test is on the line as read, carriage return included (`len < 4`).
        if raw.chars().count() < 4 || !raw.starts_with('$') {
            continue;
        }
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let Some((key, value)) = line.rsplit_once('\t') else { continue };
        if key.chars().count() < 2 || entries.len() >= MAX_ENTRIES {
            continue;
        }
        entries.push((key.to_owned(), value.to_owned()));
    }
    Some(entries)
}

/// The active language's merged table and the files it came from: `interface\translate_<spelling>.txt`
/// for the first spelling `provider` carries (GUI-FONT-002's language), then each of `plugins`'
/// `interface\translations\<plugin stem>_<spelling>.txt` in load order. A key the table already
/// holds keeps its first definition, as the extenders' hash-set insert does (`tHashSet::Insert`
/// returns `kInsert_Duplicate`): a mod adds keys but cannot replace the game's, and an earlier
/// plugin's entry beats a later one's. `None` when no file is found at all.
pub(crate) fn merged_translation_table(
    provider: &dyn ScaleformResourceProvider,
    spellings: &[String],
    plugins: &[String],
) -> Option<(Vec<String>, HashMap<String, String>)> {
    let vanilla = spellings.iter().find_map(|spelling| {
        let path = format!("interface\\translate_{}.txt", spelling.to_ascii_lowercase());
        provider.load(&path).ok().flatten().map(|bytes| (path, bytes))
    });
    let mut sources: Vec<String> = vanilla.as_ref().map(|(path, _)| path.clone()).into_iter().collect();
    let mut table = vanilla.map(|(_, bytes)| parse_translation_table(&bytes)).unwrap_or_default();
    for plugin in plugins {
        let stem = plugin.rsplit_once('.').map_or(plugin.as_str(), |(stem, _)| stem).to_ascii_lowercase();
        let Some((path, bytes)) = spellings.iter().find_map(|spelling| {
            let path = format!("interface\\translations\\{stem}_{}.txt", spelling.to_ascii_lowercase());
            provider.load(&path).ok().flatten().map(|bytes| (path, bytes))
        }) else {
            continue;
        };
        let Some(entries) = parse_extender_translation(&bytes) else {
            log::warn!("Scaleform translation {path:?}: not UCS-2 LE with a byte-order mark, refused as the extenders refuse it");
            continue;
        };
        for (key, value) in entries {
            if table.len() < MAX_ENTRIES || table.contains_key(&key) {
                table.entry(key).or_insert(value);
            }
        }
        sources.push(path);
    }
    (!sources.is_empty()).then_some((sources, table))
}

/// Give Ruffle's `$`-key pass the active language's merged table ([`merged_translation_table`]
/// over the session's [`set_translation_plugins`]). Loaded once per language and plugin set; a
/// provider with no table at all leaves the installed one in place.
pub(crate) fn install_translations(provider: &dyn ScaleformResourceProvider, spellings: &[String]) {
    static INSTALLED: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    let installed = INSTALLED.get_or_init(Default::default);
    let plugins = translation_plugins().read().map(|plugins| plugins.clone()).unwrap_or_default();
    let Some((sources, table)) = merged_translation_table(provider, spellings, &plugins) else {
        return;
    };
    let key = sources.join("|");
    let mut current = installed.lock().unwrap_or_else(|e| e.into_inner());
    if current.as_deref() == Some(key.as_str()) {
        return;
    }
    log::info!("Scaleform translation table {}: {} key(s) from {} file(s)", sources[0], table.len(), sources.len());
    let table = Arc::new(table);
    ruffle_core::byro_translation::set_text_translator(Some(Arc::new(move |key: &str| table.get(key).cloned())));
    *current = Some(key);
}

/// Install the active language's table from `provider` for a surface with no movie of its own (the
/// native Mod Configuration page), which [`translate_text`] then reads. The same table a movie
/// load installs; installing it again for the same files is free.
pub fn load_translations(provider: &dyn ScaleformResourceProvider) {
    install_translations(provider, &crate::fonts::text_languages());
}

/// `text` with its `$` key translated through the installed table, else unchanged: an unknown key
/// stays as written, as the game draws it. Text with no `$`, and any text before a table is
/// installed, passes through.
pub fn translate_text(text: &str) -> String {
    ruffle_core::byro_translation::translate(&ruffle_core::string::WString::from_utf8(text))
        .map_or_else(|| text.to_owned(), |translated| translated.to_utf8_lossy().into_owned())
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
        // The native pages read the same table (the Mod Configuration page): a known key is
        // replaced, an unknown one stays as written, plain text passes through. Asserted here, with
        // the process-wide table, so no parallel test sees it installed.
        assert_eq!(translate_text("$Quit?"), "Quit to main menu?");
        assert_eq!(translate_text("$Unknown"), "$Unknown");
        assert_eq!(translate_text("Quit"), "Quit");
        set_text_translator(None);
        assert_eq!(translate_text("$Quit?"), "$Quit?", "before a table is installed text passes through");
        assert_eq!(t("$Quit?"), None);
    }

    /// U4 — a plugin translation file follows the script extenders' parser (SKSE64 / F4SE
    /// `ParseTranslation`): UCS-2 LE with its byte-order mark or nothing, `$`-led lines of at least
    /// four characters as read, the key ending at the last tab.
    #[test]
    fn a_plugin_translation_file_parses_by_the_extenders_rules() {
        let entries = parse_extender_translation(&utf16le("$SKI_Title\tSkyUI\r\n$A\tb\tc\r\n$x\t\nnot\tkey\r\n$NoTab\r\n"))
            .expect("a UCS-2 LE file with its mark");
        assert_eq!(
            entries,
            vec![("$SKI_Title".to_owned(), "SkyUI".to_owned()), ("$A\tb".to_owned(), "c".to_owned())],
            "the last tab splits; `$x<TAB>` is three characters as read and is skipped"
        );
        assert_eq!(parse_extender_translation(b"$A\tb\n"), None, "UTF-8 without the mark is refused");
    }

    /// U4 — the merged table keeps a key's first definition: the game's table, then each enabled
    /// plugin's file in load order, as the extenders' hash-set insert keeps the existing entry.
    #[test]
    fn plugin_tables_add_keys_and_the_first_definition_wins() {
        struct Files(Vec<(&'static str, Vec<u8>)>);
        impl ScaleformResourceProvider for Files {
            fn load(&self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
                Ok(self.0.iter().find(|(p, _)| *p == path).map(|(_, b)| b.clone()))
            }
        }
        let files = Files(vec![
            ("interface\\translate_english.txt", utf16le("$Map\tMap\r\n")),
            ("interface\\translations\\skyui_se_english.txt", utf16le("$Map\tSkyUI map\r\n$SKI\tSkyUI\r\n")),
            ("interface\\translations\\later_english.txt", utf16le("$SKI\tLater\r\n$Later\tL\r\n")),
        ]);
        let plugins = ["Skyrim.esm".to_owned(), "SkyUI_SE.esp".to_owned(), "Later.esp".to_owned()];
        let (sources, table) =
            merged_translation_table(&files, &["english".to_owned(), "en".to_owned()], &plugins).expect("three tables");
        assert_eq!(sources.len(), 3, "the game's table and two plugin tables; Skyrim.esm ships none");
        assert_eq!(table["$Map"], "Map", "a mod cannot replace the game's key");
        assert_eq!(table["$SKI"], "SkyUI", "the earlier plugin's entry wins");
        assert_eq!(table["$Later"], "L");
    }
}
