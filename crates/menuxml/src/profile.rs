//! Per-game corpus profiles (M48.5).
//!
//! The M48.4 slice hard-coded Oblivion's corpus facts (font table, atlas
//! path, `strings.xml`) in three places. A [`MenuProfile`] collects them
//! so [`crate::MenuRenderer`] and the engine's HUD driver share one table
//! per game. Everything else in the crate (parse/eval/layout/raster) is
//! dialect-level and already game-agnostic.

/// Which of the two archives a game's UI corpus carries fonts in.
///
/// Oblivion ships `.fnt` + `.tex` in the Misc BSA under `fonts\`;
/// FO3/FNV ship them in the *texture* BSA under `textures\fonts\`
/// (verified against the installed corpora — the `.fnt` binary layout
/// itself is identical: 296-byte header, atlas name at offset 12,
/// 256 glyph records of 14 f32s).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontArchive {
    Misc,
    Textures,
}

/// Corpus facts that differ between the legacy-UI games.
#[derive(Debug, Clone, Copy)]
pub struct MenuProfile {
    /// Human label for logs ("Oblivion", "Fallout 3").
    pub label: &'static str,
    /// 1-based `<font>` table: slot *i* → archive path, per the game
    /// ini's `[Fonts]` section (Oblivion.ini / Fallout_default.ini).
    pub font_paths: &'static [&'static str],
    /// Which archive [`Self::font_paths`] and their atlases live in.
    pub font_archive: FontArchive,
    /// Derive glyph-atlas path candidates from the `.fnt`'s embedded
    /// atlas name (Oblivion: `fonts\<name>.tex`; FO3 ships both a
    /// `.tex` and a `.dds` beside each `.fnt`).
    pub font_atlas: fn(&str) -> Vec<String>,
    /// The game's strings document feeding the `strings()` selector,
    /// if it has one. Oblivion: `menus\strings.xml`. FO3/FNV have no
    /// equivalent (their `-sPrefixed` strings are GMST refs, future
    /// work) — `None` reads `strings()` as 0.
    pub strings_path: Option<&'static str>,
}

fn oblivion_atlas(name: &str) -> Vec<String> {
    vec![format!("fonts\\{name}.tex")]
}

fn fallout_atlas(name: &str) -> Vec<String> {
    vec![
        format!("textures\\fonts\\{name}.tex"),
        format!("textures\\fonts\\{name}.dds"),
    ]
}

impl MenuProfile {
    /// Oblivion.ini `[Fonts]` SFontFile_1..5 order.
    pub const fn oblivion() -> Self {
        Self {
            label: "Oblivion",
            font_paths: &[
                "fonts\\Kingthings_Regular.fnt",
                "fonts\\Kingthings_Shadowed.fnt",
                "fonts\\Tahoma_Bold_Small.fnt",
                "fonts\\Daedric_Font.fnt",
                "fonts\\Handwritten.fnt",
            ],
            font_archive: FontArchive::Misc,
            font_atlas: oblivion_atlas,
            strings_path: Some("menus\\strings.xml"),
        }
    }

    /// `Fallout_default.ini` `[Fonts]` sFontFile_1..8 order — the HUD's
    /// text tiles use slots 7 (labels) and 8 (numbers).
    pub const fn fallout3() -> Self {
        Self::fallout_common("Fallout 3", FALLOUT3_FONTS)
    }

    /// FNV adds a ninth slot (`NVFont_Test`) over the FO3 table.
    pub const fn fallout_nv() -> Self {
        Self::fallout_common("Fallout: New Vegas", FALLOUT_NV_FONTS)
    }

    const fn fallout_common(label: &'static str, font_paths: &'static [&'static str]) -> Self {
        Self {
            label,
            font_paths,
            font_archive: FontArchive::Textures,
            font_atlas: fallout_atlas,
            strings_path: None,
        }
    }
}

static FALLOUT3_FONTS: &[&str] = &[
    "textures\\fonts\\Glow_Monofonto_Large.fnt",
    "textures\\fonts\\Monofonto_Large.fnt",
    "textures\\fonts\\Glow_Monofonto_Medium.fnt",
    "textures\\fonts\\Monofonto_VeryLarge02_Dialogs2.fnt",
    "textures\\fonts\\Fixedsys_Comp_uniform_width.fnt",
    "textures\\fonts\\Glow_Monofonto_VL_dialogs.fnt",
    "textures\\fonts\\Baked-in_Monofonto_Large.fnt",
    "textures\\fonts\\Glow_Futura_Caps_Large.fnt",
];

static FALLOUT_NV_FONTS: &[&str] = &[
    "textures\\fonts\\Glow_Monofonto_Large.fnt",
    "textures\\fonts\\Monofonto_Large.fnt",
    "textures\\fonts\\Glow_Monofonto_Medium.fnt",
    "textures\\fonts\\Monofonto_VeryLarge02_Dialogs2.fnt",
    "textures\\fonts\\Fixedsys_Comp_uniform_width.fnt",
    "textures\\fonts\\Glow_Monofonto_VL_dialogs.fnt",
    "textures\\fonts\\Baked-in_Monofonto_Large.fnt",
    "textures\\fonts\\Glow_Futura_Caps_Large.fnt",
    "textures\\fonts\\NVFont_Test.fnt",
];
