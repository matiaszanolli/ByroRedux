//! FNV vanilla-corpus integration tests (M48.5 legacy-UI track).
//!
//! #4665 (PAR-D4-2026-09-21-03) — the FNV MenuXml corpus had zero tests
//! while the HUD profile ships and drives it (`hud.rs`'s Fallout: New
//! Vegas profile). Gated on `BYROREDUX_FNV_DATA` pointing at a real
//! Fallout: New Vegas `Data/` directory (falls back to the default Steam
//! install when present), so `cargo test` stays hermetic elsewhere.
//! Mirrors `fo3_corpus.rs`; pins the corpus facts the FNV profile
//! depends on (menu XML set, font table, meter prefab).

use std::path::PathBuf;

use byroredux_bsa::BsaArchive;
use byroredux_menuxml::font::Font;
use byroredux_menuxml::menu::MenuAssets;
use byroredux_menuxml::profile::MenuProfile;

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
    if let Some(dir) = std::env::var("BYROREDUX_FNV_DATA").ok() {
        return Some(PathBuf::from(dir));
    }
    let default = PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data");
    default.is_dir().then_some(default)
}

fn open_assets() -> Option<NvAssets> {
    let dir = data_dir()?;
    let misc = BsaArchive::open(dir.join("Fallout - Misc.bsa")).ok()?;
    let textures = BsaArchive::open(dir.join("Fallout - Textures.bsa")).ok()?;
    let textures2 = BsaArchive::open(dir.join("Fallout - Textures2.bsa")).ok()?;
    Some(NvAssets {
        misc,
        textures,
        textures2,
        profile: MenuProfile::fallout_nv(),
    })
}

/// The corpus facts the FNV profile + HUD driver depend on. The audit
/// measured 121 menu XMLs in `Fallout - Misc.bsa`; the floor below only
/// guards against wholesale corpus loss.
#[test]
fn fnv_corpus_facts() {
    let Some(assets) = open_assets() else {
        eprintln!("skipping: FNV data not found");
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
