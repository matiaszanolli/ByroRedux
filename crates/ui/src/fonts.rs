//! M48 GUI-FONT-001…003 — the fonts a Bethesda Scaleform movie asks for.
//!
//! **Font classes (GUI-FONT-001).** A Skyrim movie imports its fonts by class name from
//! `gfxfontlib.swf` (`hudmenu.swf`: `$EverywhereMediumFont`, `$DragonFont`). The shipped
//! `interface\gfxfontlib.swf` is a placeholder that exports only some of those names; GFx serves
//! the import from the game's font configuration instead (`interface\fontconfig.txt`):
//! `fontlib "Interface\fonts_en.swf"` lines name the libraries, `map "$EverywhereMediumFont" =
//! "Futura Condensed" Normal` lines map a class to a face and a style. [`font_library`] builds that
//! library: every mapped class exported under its name, each pointing at the matching `DefineFont`
//! copied out of the font libraries. The navigator serves it for a `gfxfontlib.swf` request.
//!
//! **Language (GUI-FONT-002).** The configuration is the active language's:
//! `fontconfig_<language>.txt` when the archive ships one for a [`set_text_languages`] spelling
//! (`fontconfig_pl.txt`), else `fontconfig.txt`; its `fontlib` lines name that language's library.
//!
//! **Fallback (GUI-FONT-003).** Noto Sans Regular and Bold (SIL OFL 1.1, `assets/fonts/noto-sans`,
//! PRD Q5) are registered with every player as its device font and its default sans, serif and
//! typewriter fonts. Ruffle lays a span whose embedded face it cannot find out in the default sans
//! (`html/layout.rs` `resolve_font`), so an unmapped class, a face no library carries, or a glyph a
//! face lacks falls back to Noto Sans; each unresolved class is reported once.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock, RwLock};

use swf::{ExportedAsset, Font, FontFlag, SwfStr, Tag};

use crate::navigator::ScaleformResourceProvider;

/// The bundled fallback face.
pub const FALLBACK_FONT_NAME: &str = "Noto Sans";
const FALLBACK_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/noto-sans/NotoSans-Regular.ttf");
const FALLBACK_BOLD: &[u8] = include_bytes!("../../../assets/fonts/noto-sans/NotoSans-Bold.ttf");

/// Register the fallback face with `player` and make it the default sans, serif and typewriter
/// font (GUI-FONT-003).
pub(crate) fn install_fallback_fonts(player: &mut ruffle_core::Player) {
    use ruffle_core::backend::ui::FontDefinition;
    use ruffle_core::font::{DefaultFont, FontFileData};
    for (data, is_bold) in [(FALLBACK_REGULAR, false), (FALLBACK_BOLD, true)] {
        player.register_device_font(FontDefinition::FontFile {
            name: FALLBACK_FONT_NAME.to_owned(),
            is_bold,
            is_italic: false,
            data: FontFileData::new(data),
            index: 0,
        });
    }
    for font in [DefaultFont::Sans, DefaultFont::Serif, DefaultFont::Typewriter] {
        player.set_default_font(font, vec![FALLBACK_FONT_NAME.to_owned()]);
    }
}

fn languages() -> &'static RwLock<Vec<String>> {
    static LANGUAGES: OnceLock<RwLock<Vec<String>>> = OnceLock::new();
    LANGUAGES.get_or_init(|| RwLock::new(vec!["english".to_owned(), "en".to_owned()]))
}

/// The active language's file-name spellings, in the order to try (`["english", "en"]`, the
/// default): the game-text language the `.STRINGS` loader uses, which picks the font
/// configuration and the translation table. Process-wide, like that language.
pub fn set_text_languages(spellings: Vec<String>) {
    if let Ok(mut languages) = languages().write() {
        *languages = spellings;
    }
}

/// The spellings [`set_text_languages`] set.
pub fn text_languages() -> Vec<String> {
    languages().read().map(|languages| languages.clone()).unwrap_or_default()
}

/// One `map` line: a font class, the face it names and the style it asks for (`None`: any).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FontMap {
    pub class: String,
    pub face: String,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
}

/// A parsed font configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FontConfig {
    /// The `fontlib` paths, lowercased with backslashes, as the archive names them.
    pub libraries: Vec<String>,
    pub maps: Vec<FontMap>,
}

/// The quoted strings on a line, in order.
fn quoted(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

/// Parse `fontconfig.txt`. Lines other than `fontlib` and `map` (`validNameChars`) are skipped;
/// so is a malformed one (game-archive input).
pub(crate) fn parse_fontconfig(text: &str) -> FontConfig {
    let mut config = FontConfig::default();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("fontlib") {
            if let Some(path) = quoted(line).first() {
                config.libraries.push(path.replace('/', "\\").to_ascii_lowercase());
            }
        } else if line.starts_with("map") {
            let parts = quoted(line);
            let (Some(class), Some(face)) = (parts.first(), parts.get(1)) else {
                continue;
            };
            let style = line.rsplit('"').next().unwrap_or("").trim();
            let (bold, italic) = match style.to_ascii_lowercase().as_str() {
                "normal" => (Some(false), Some(false)),
                "bold" => (Some(true), Some(false)),
                "italic" => (Some(false), Some(true)),
                "bolditalic" => (Some(true), Some(true)),
                // `Roman`, `Demi` (the Creation Club font): no style GFx's map syntax names.
                _ => (None, None),
            };
            config.maps.push(FontMap { class: (*class).to_owned(), face: (*face).to_owned(), bold, italic });
        }
    }
    config
}

/// The language's font configuration in `provider`: `interface\fontconfig_<spelling>.txt` for the
/// first spelling the archive ships, else `interface\fontconfig.txt` (GUI-FONT-002).
pub(crate) fn load_fontconfig(provider: &dyn ScaleformResourceProvider, spellings: &[String]) -> Option<FontConfig> {
    let candidates = spellings
        .iter()
        .map(|spelling| format!("interface\\fontconfig_{}.txt", spelling.to_ascii_lowercase()))
        .chain(std::iter::once("interface\\fontconfig.txt".to_owned()));
    for path in candidates {
        if let Ok(Some(bytes)) = provider.load(&path) {
            return Some(parse_fontconfig(&crate::translate::decode_text(&bytes)));
        }
    }
    None
}

/// Report an unresolved font class once per process.
fn report_unresolved(class: &str, why: &str) {
    static REPORTED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let mut reported = REPORTED.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if reported.len() < crate::MAX_DISTINCT_HOST_METHOD_NAMES && reported.insert(class.to_owned()) {
        log::warn!("Scaleform font class '{class}' {why}; its text falls back to {FALLBACK_FONT_NAME}");
    }
}

/// What [`font_library`] built.
#[derive(Debug)]
pub(crate) struct FontLibrary {
    /// The library movie: one `DefineFont` per face used, exported under every class mapped to it.
    pub swf: Vec<u8>,
    /// The classes it exports.
    pub classes: Vec<String>,
    /// The mapped classes no library face matched.
    pub unresolved: Vec<String>,
}

/// Build the `gfxfontlib.swf` the language's font configuration describes (GUI-FONT-001), or
/// `None` when `provider` carries no configuration.
pub(crate) fn font_library(provider: &dyn ScaleformResourceProvider, spellings: &[String]) -> Option<FontLibrary> {
    let config = load_fontconfig(provider, spellings)?;
    // Each library, decompressed; the parsed fonts borrow from these buffers.
    let mut buffers = Vec::new();
    for path in &config.libraries {
        match provider.load(path) {
            // #5966 — archive input: refuse a declared length past the ceiling before inflating.
            Ok(Some(bytes)) if let Err(error) = crate::prepare::ensure_declared_len_within_ceiling(&bytes) => {
                log::warn!("Scaleform font library {path:?} refused: {error}");
            }
            Ok(Some(bytes)) => match swf::decompress_swf(&bytes[..]) {
                Ok(buffer) => buffers.push(buffer),
                Err(error) => log::warn!("Scaleform font library {path:?} does not decode: {error}"),
            },
            _ => log::warn!("Scaleform font library {path:?} is not in the interface archive"),
        }
    }
    let movies: Vec<swf::Swf<'_>> = buffers.iter().filter_map(|buffer| swf::parse_swf(buffer).ok()).collect();
    let mut faces: Vec<&Font<'_>> = Vec::new();
    for movie in &movies {
        for tag in &movie.tags {
            if let Tag::DefineFont2(font) = tag {
                faces.push(font);
            }
        }
    }

    let mut chosen: HashMap<usize, u16> = HashMap::new();
    let mut fonts: Vec<Font<'_>> = Vec::new();
    let mut exports: Vec<(u16, String)> = Vec::new();
    let mut unresolved = Vec::new();
    for map in &config.maps {
        let style_matches = |font: &&Font<'_>| {
            map.bold.is_none_or(|bold| font.flags.contains(FontFlag::IS_BOLD) == bold)
                && map.italic.is_none_or(|italic| font.flags.contains(FontFlag::IS_ITALIC) == italic)
        };
        let named = |font: &&Font<'_>| font.name.to_str_lossy(swf::UTF_8).eq_ignore_ascii_case(&map.face);
        let found = faces
            .iter()
            .position(|font| named(font) && style_matches(font))
            .or_else(|| faces.iter().position(named));
        let Some(index) = found else {
            report_unresolved(&map.class, &format!("maps to {:?}, which no font library carries", map.face));
            unresolved.push(map.class.clone());
            continue;
        };
        let id = *chosen.entry(index).or_insert_with(|| {
            let id = fonts.len() as u16 + 1;
            let mut font = faces[index].clone();
            font.id = id;
            fonts.push(font);
            id
        });
        exports.push((id, map.class.clone()));
    }

    let version = movies.first().map_or(10, |movie| movie.header.version()).max(8);
    let names: Vec<&SwfStr> = exports.iter().map(|(_, class)| SwfStr::from_utf8_str(class)).collect();
    let mut tags: Vec<Tag<'_>> = fonts.into_iter().map(|font| Tag::DefineFont2(Box::new(font))).collect();
    tags.push(Tag::ExportAssets(
        exports.iter().zip(&names).map(|((id, _), name)| ExportedAsset { id: *id, name }).collect(),
    ));
    tags.push(Tag::ShowFrame);
    let mut header = swf::Header::default_with_swf_version(version);
    header.num_frames = 1;
    let mut swf = Vec::new();
    if let Err(error) = swf::write_swf(&header, &tags, &mut swf) {
        log::warn!("Scaleform font library could not be written: {error}");
        return None;
    }
    Some(FontLibrary { swf, classes: exports.into_iter().map(|(_, class)| class).collect(), unresolved })
}

/// Whether `archive_path` is the font library movies import their classes from.
pub(crate) fn is_font_library_request(archive_path: &str) -> bool {
    let name = archive_path.rsplit(['\\', '/']).next().unwrap_or(archive_path);
    name.eq_ignore_ascii_case("gfxfontlib.swf")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    struct Memory(HashMap<String, Vec<u8>>);

    impl ScaleformResourceProvider for Memory {
        fn load(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
            Ok(self.0.get(path).cloned())
        }
    }

    fn font<'a>(id: u16, name: &'a str, bold: bool) -> Font<'a> {
        Font {
            version: 3,
            id,
            name: SwfStr::from_utf8_str(name),
            language: swf::Language::Unknown,
            layout: None,
            glyphs: Vec::new(),
            flags: if bold { FontFlag::IS_BOLD } else { FontFlag::empty() },
        }
    }

    fn library(fonts: Vec<Font<'_>>) -> Vec<u8> {
        let mut tags: Vec<Tag<'_>> = fonts.into_iter().map(|font| Tag::DefineFont2(Box::new(font))).collect();
        tags.push(Tag::ShowFrame);
        let mut bytes = Vec::new();
        swf::write_swf(&swf::Header::default_with_swf_version(8), &tags, &mut bytes).unwrap();
        bytes
    }

    const CONFIG: &str = "fontlib \"Interface\\fonts_en.swf\"\r\n\
        map \"$EverywhereMediumFont\" = \"Futura Condensed\" Normal\r\n\
        map \"$EverywhereBoldFont\" = \"Futura Condensed\" Bold\r\n\
        map \"$StartMenuFont\" = \"Futura Condensed\" Normal\r\n\
        map \"$CClub_Font\" = \"Eurostile LT Cyr Std\" Roman\r\n\
        validNameChars \"abc\"\r\n";

    #[test]
    fn fontconfig_lines_parse_to_libraries_and_styled_maps() {
        let config = parse_fontconfig(CONFIG);
        assert_eq!(config.libraries, ["interface\\fonts_en.swf"]);
        assert_eq!(config.maps.len(), 4);
        assert_eq!(config.maps[1], FontMap { class: "$EverywhereBoldFont".into(), face: "Futura Condensed".into(), bold: Some(true), italic: Some(false) });
        assert_eq!((config.maps[3].bold, config.maps[3].italic), (None, None), "Roman names no GFx style");
    }

    /// GUI-FONT-001/002 — the library exports every mapped class under its name, pointing at the
    /// face of the right style (one copy per face), from the language's configuration when the
    /// archive ships one; an unmatched face is reported and left out.
    #[test]
    fn the_font_library_exports_each_mapped_class_from_the_language_configuration() {
        let mut files = HashMap::new();
        files.insert("interface\\fontconfig.txt".to_owned(), CONFIG.as_bytes().to_vec());
        files.insert(
            "interface\\fonts_en.swf".to_owned(),
            library(vec![font(7, "Futura Condensed", false), font(9, "Futura Condensed", true)]),
        );
        let provider = Memory(files);
        let built = font_library(&provider, &["english".into(), "en".into()]).unwrap();
        assert_eq!(built.classes, ["$EverywhereMediumFont", "$EverywhereBoldFont", "$StartMenuFont"]);
        assert_eq!(built.unresolved, ["$CClub_Font"]);
        let swf = swf::decompress_swf(&built.swf[..]).unwrap();
        let movie = swf::parse_swf(&swf).unwrap();
        let mut fonts = HashMap::new();
        let mut exports = Vec::new();
        for tag in &movie.tags {
            match tag {
                Tag::DefineFont2(font) => {
                    fonts.insert(font.id, font.flags.contains(FontFlag::IS_BOLD));
                }
                Tag::ExportAssets(assets) => exports.extend(assets.iter().map(|a| (a.name.to_str_lossy(swf::UTF_8).into_owned(), a.id))),
                _ => {}
            }
        }
        assert_eq!(fonts.len(), 2, "one copy per face");
        let bold = |class: &str| fonts[&exports.iter().find(|(name, _)| name == class).unwrap().1];
        assert!(!bold("$EverywhereMediumFont") && bold("$EverywhereBoldFont") && !bold("$StartMenuFont"));

        // A language with its own configuration uses it.
        let mut files = provider.0.clone();
        files.insert("interface\\fontconfig_pl.txt".to_owned(), b"fontlib \"Interface\\fonts_pl.swf\"\nmap \"$X\" = \"Polish Face\" Normal\n".to_vec());
        files.insert("interface\\fonts_pl.swf".to_owned(), library(vec![font(1, "Polish Face", false)]));
        let polish = font_library(&Memory(files), &["polish".into(), "pl".into()]).unwrap();
        assert_eq!(polish.classes, ["$X"]);
        assert!(font_library(&Memory(HashMap::new()), &[]).is_none(), "no configuration, no library");
        assert!(is_font_library_request("interface\\gfxfontlib.swf") && !is_font_library_request("interface\\fonts_en.swf"));
    }

    /// GUI-FONT-001 on the installed Skyrim SE archive: the library exports the classes
    /// `hudmenu.swf` imports, from `fonts_en.swf`, and every vanilla map resolves.
    #[test]
    #[ignore = "needs Skyrim SE game data on disk"]
    fn installed_skyrim_font_configuration_builds_the_classes_the_hud_imports() {
        use byroredux_plugin::esm::test_paths::{data_dir, SKYRIM_SE_DEFAULT, SKYRIM_SE_ENV};
        let path = data_dir(SKYRIM_SE_ENV, SKYRIM_SE_DEFAULT).join("Skyrim - Interface.bsa");
        let archive = byroredux_bsa::BsaArchive::open(&path).unwrap_or_else(|e| panic!("open {path:?}: {e}"));
        let built = font_library(&archive, &["english".into(), "en".into()]).expect("fontconfig.txt");
        for class in ["$EverywhereMediumFont", "$DragonFont", "$EverywhereFont", "$EverywhereBoldFont", "$ConsoleFont"] {
            assert!(built.classes.iter().any(|c| c == class), "{class} missing from {:?}", built.classes);
        }
        assert!(built.unresolved.is_empty(), "unresolved vanilla classes: {:?}", built.unresolved);
    }
}
