//! FNV vanilla-corpus integration tests (M48.5 legacy-UI track).
//!
//! #4665 (PAR-D4-2026-09-21-03) — the FNV MenuXml corpus had zero tests
//! while the HUD profile ships and drives it (`hud.rs`'s Fallout: New
//! Vegas profile). `#[ignore]`d real-data tests: run with `--ignored`
//! against `BYROREDUX_FNV_DATA` (or the default Steam install), resolved
//! through `common` under the #3850 strict-lane contract (#4660).
//! Mirrors `fo3_corpus.rs`; pins the corpus facts the FNV profile
//! depends on (menu XML set, font table, meter prefab).

use std::path::PathBuf;

use byroredux_bsa::BsaArchive;
use byroredux_menuxml::font::Font;
use byroredux_menuxml::menu::MenuAssets;
use byroredux_menuxml::parse::{parse_document, MenuFileSource};
use byroredux_menuxml::profile::MenuProfile;

mod common;

const HUD_MENU: &str = "menus\\main\\hud_main_menu.xml";
const METER_PREFAB: &str = "menus\\prefabs\\meter.xml";

struct NvAssets {
    misc: BsaArchive,
    textures: BsaArchive,
    /// #4665 — FNV's fonts live in the numeric-sibling archive, NOT the
    /// base Textures BSA (measured: all nine .fnt files sit in
    /// `Fallout - Textures2.bsa`).
    textures2: BsaArchive,
    profile: MenuProfile,
}

impl MenuAssets for NvAssets {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
        self.misc.extract(path).ok()
    }
    fn texture(&self, path: &str) -> Option<Vec<u8>> {
        self.textures
            .extract(path)
            .or_else(|_| self.textures2.extract(path))
            .ok()
    }
    fn font(&self, index: u8) -> Option<Vec<u8>> {
        // Fonts live in the *texture* family for FNV, in the profile's
        // `fallout_default.ini` `[Fonts]` slot order (9 slots).
        let path = self.profile.font_paths.get(index as usize - 1)?;
        self.textures2.extract(path).ok()
    }
    fn font_texture(&self, path: &str) -> Option<Vec<u8>> {
        self.textures2
            .extract(path)
            .or_else(|_| self.textures.extract(path))
            .or_else(|_| self.misc.extract(path))
            .ok()
    }
}

fn data_dir() -> Option<PathBuf> {
    common::data_dir(
        "BYROREDUX_FNV_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
    )
}

fn open_assets() -> Option<NvAssets> {
    let dir = data_dir()?;
    let misc = common::open_archive(&dir, "Fallout - Misc.bsa")?;
    let textures = common::open_archive(&dir, "Fallout - Textures.bsa")?;
    let textures2 = common::open_archive(&dir, "Fallout - Textures2.bsa")?;
    Some(NvAssets {
        misc,
        textures,
        textures2,
        profile: MenuProfile::fallout_nv(),
    })
}

struct NoIncludes;

struct ArchiveIncludes<'a>(&'a BsaArchive);

impl MenuFileSource for NoIncludes {
    fn menu_xml(&self, _path: &str) -> Option<Vec<u8>> {
        None
    }
}

impl MenuFileSource for ArchiveIncludes<'_> {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
        self.0.extract(path).ok()
    }
}

/// The corpus facts the FNV profile + HUD driver depend on. The audit
/// measured 121 menu XMLs in `Fallout - Misc.bsa`; the floor below only
/// guards against wholesale corpus loss.
#[test]
#[ignore = "needs FNV game data on disk"]
fn fnv_corpus_facts() {
    let Some(assets) = open_assets() else {
        return;
    };

    let xmls: Vec<String> = assets
        .misc
        .list_files()
        .into_iter()
        .filter(|p| p.to_lowercase().ends_with(".xml"))
        .map(str::to_string)
        .collect();
    assert!(
        xmls.len() >= 110,
        "expected the vanilla FNV menu set (121 measured at audit time), got {}",
        xmls.len()
    );
    assert!(
        xmls
            .iter()
            .any(|p| p.eq_ignore_ascii_case(HUD_MENU)),
        "hud_main_menu.xml present"
    );
    assert!(
        xmls.iter().any(|p| p.eq_ignore_ascii_case(METER_PREFAB)),
        "meter.xml prefab present"
    );

    // Every FNV font slot's .fnt parses with the Oblivion-identical
    // binary layout (14632 B, name@12) and its atlas decodes.
    for index in 1..=assets.profile.font_paths.len() as u8 {
        let fnt = assets.font(index).expect("font slot extractable");
        assert_eq!(fnt.len(), 14632, "slot {index}: Oblivion-layout .fnt");
        let name = String::from_utf8_lossy(&fnt[12..])
            .split('\0')
            .next()
            .unwrap_or("")
            .to_string();
        let atlas = (assets.profile.font_atlas)(&name)
            .iter()
            .find_map(|p| assets.font_texture(p))
            .unwrap_or_else(|| panic!("font {index} ({name}) atlas beside the .fnt"));
        Font::parse(&fnt, &atlas).unwrap_or_else(|e| panic!("font {index} parses: {e}"));
    }
}

/// #5902 — every FNV menu XML is parsed both with and without include
/// splicing, with the vanilla corpus and tile totals pinned.
#[test]
#[ignore = "needs FNV game data on disk"]
fn fnv_menu_xml_corpus_parses_with_exact_totals() {
    let Some(assets) = open_assets() else {
        return;
    };
    let xmls: Vec<String> = assets
        .misc
        .list_files()
        .into_iter()
        .filter(|path| path.to_ascii_lowercase().ends_with(".xml"))
        .map(str::to_string)
        .collect();
    assert_eq!(xmls.len(), 121, "FNV vanilla menu XML census changed");

    let mut tiles = 0usize;
    let mut spliced_tiles = 0usize;
    let mut largest = 0usize;
    for path in &xmls {
        let bytes = assets.misc.extract(path).expect("extract FNV menu XML");
        let text = String::from_utf8(bytes).expect("FNV menu XML is UTF-8");
        let mut no_includes = NoIncludes;
        let document = parse_document(&text, &mut no_includes);
        assert!(!document.tiles.is_empty(), "{path} parsed empty");
        tiles += document.tiles.len();
        let included = parse_document(&text, &mut ArchiveIncludes(&assets.misc));
        assert!(!included.tiles.is_empty(), "{path} parsed empty with includes");
        largest = largest.max(included.tiles.len());
        spliced_tiles += included.tiles.len();
    }

    assert_eq!(tiles, 1_060, "FNV un-spliced tile total changed");
    assert_eq!(spliced_tiles, 2_206, "FNV include-spliced tile total changed");
    assert_eq!(largest, 195, "FNV largest menu tile count changed");
}

/// #5669 — header census of every DDS under `textures\interface\`: the
/// HUD driver's art (24-bpp `hud_comp_direction_vertical`, 32-bpp
/// `target_*` and `shared\line\*`) must all decode. The vanilla counts
/// are pinned so the census can neither pass vacuously nor shrink.
#[test]
#[ignore = "needs FNV game data on disk"]
fn interface_texture_header_census_decodes() {
    let Some(assets) = open_assets() else {
        return;
    };
    // FNV's interface art sits in either texture archive (Textures2
    // carries the base set here); walk both listings, dedupe, and
    // extract through the same textures→textures2 fallback
    // `NvAssets::texture` serves.
    let paths: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        assets
            .textures
            .list_files()
            .into_iter()
            .chain(assets.textures2.list_files())
            .filter(|p| {
                let p = p.to_lowercase();
                p.starts_with("textures\\interface\\") && p.ends_with(".dds")
            })
            .filter(|p| seen.insert(p.to_string()))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        paths.len(),
        1767,
        "the vanilla interface texture count moved — re-measure, don't re-scope"
    );

    let mut failures: Vec<String> = Vec::new();
    for path in &paths {
        let bytes = assets
            .textures
            .extract(path)
            .or_else(|_| assets.textures2.extract(path))
            .expect("extract dds");
        if byroredux_menuxml::Rgba8::decode_dds(&bytes).is_none() {
            let detail = if bytes.len() >= 128 {
                let h = |off: usize| {
                    u32::from_le_bytes(bytes[4 + off..4 + off + 4].try_into().unwrap())
                };
                format!(
                    "fourcc={:?} flags={:#x} bpp={} masks={:#x}/{:#x}/{:#x}/{:#x}",
                    &bytes[84..88],
                    h(76),
                    h(84),
                    h(88),
                    h(92),
                    h(96),
                    h(100),
                )
            } else {
                format!("truncated header ({} B)", bytes.len())
            };
            failures.push(format!("{path}: {detail}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} interface DDS headers undecodable:\n{}",
        failures.len(),
        paths.len(),
        failures.join("\n")
    );
}
