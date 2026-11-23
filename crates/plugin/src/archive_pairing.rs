//! Which archives a legacy plugin pairs with by name (PRD legacy-mod-compatibility LMC-AST-002).
//!
//! Every game this engine targets opens, beside the archives its INI lists, the archives that share an enabled plugin's
//! name. The rule is per game, and it applies to `.esm`, `.esp` and `.esl` plugins alike (owner ruling on #6670,
//! 2026-11-22). The shipped data shows why: every New Vegas and Fallout 3 DLC `.esm` sits beside its
//! `<Stem> - Main.bsa` / `<Stem> - Sounds.bsa`, every Skyrim SE Creation Club `.esm` / `.esl` beside its `<stem>.bsa`,
//! every Fallout 4 DLC `.esm` beside its `- Main` / `- Textures` / `- Voices_en` `.ba2`, and the Starfield DLC
//! `SFBGS003.esm` beside its `- Main` / `- Textures` / `- Voices_<lang>` `.ba2`. No shipped INI lists any of these.
//!
//! | game | the archive pairs with `<stem>.<ext>` when its name is | source |
//! |---|---|---|
//! | Oblivion | `<stem><rest>.bsa`, `<rest>` empty or starting with a space, `-` or `_` | UESP, *BSAs and Archive Invalidation* |
//! | Fallout 3, New Vegas | `<stem>.bsa` or `<stem> - <anything>.bsa` | GECK wiki, *BSA Files* |
//! | Skyrim LE / SE | `<stem>.bsa` | CK wiki, *File Menu* ("name your bsa the same name as your .esp file") |
//! | Fallout 4 | `<stem> - Main.ba2`, `<stem> - Textures.ba2`; `<stem> - Voices_en.ba2` | FO4 CK wiki, *Archive File*; the voice suffix from the shipped DLC |
//! | Starfield | `<stem> - Main.ba2`, `- Textures`, `- Localization`, `- Voices_en` | Archive2 docs; `- Localization` and `- Voices_en` from the shipped DLC |
//!
//! Names compare case-insensitively, as the games' file systems do. Not paired, for want of a source: Skyrim's
//! community-described `<stem> - Textures.bsa`, the `_Xbox` console archives, the Fallout 4 high-resolution pack's
//! `- Textures01`..`16`, and Fallout 76 (no plugin-paired content). Voice archives pair in English only: the engine has no
//! language setting, and the profiles open the `_en` voice archives (see [`VOICE_LANGUAGE`]).

use byroredux_core::form_id::plugin_key;

use crate::esm::reader::GameKind;

/// The `sLanguage` the voice-archive suffix is built from. The engine has no language setting; this is the language the
/// game profiles' voice archives (`Skyrim - Voices_en0.bsa`) are already in.
pub const VOICE_LANGUAGE: &str = "en";

/// Whether an archive named `archive` (a file name or path) pairs with the plugin `plugin` (a file name or path) in
/// `game`. See the module docs for the rules.
pub fn pairs_with(game: GameKind, plugin: &str, archive: &str) -> bool {
    let plugin = plugin_key(plugin);
    let Some(stem) = [".esm", ".esp", ".esl"].iter().find_map(|ext| plugin.strip_suffix(ext)) else {
        return false;
    };
    let archive = plugin_key(archive);
    let (extension, suffixes): (&str, &[&str]) = match game {
        GameKind::Fallout4 => (".ba2", &[" - main", " - textures", " - voices_en"]),
        GameKind::Starfield => (".ba2", &[" - main", " - textures", " - localization", " - voices_en"]),
        GameKind::Oblivion | GameKind::Fallout3NV | GameKind::Skyrim => (".bsa", &[]),
        GameKind::Fallout76 => return false,
    };
    let Some(rest) = archive.strip_suffix(extension).and_then(|name| name.strip_prefix(stem)) else {
        return false;
    };
    match game {
        GameKind::Oblivion => rest.is_empty() || rest.starts_with([' ', '-', '_']),
        GameKind::Fallout3NV => rest.is_empty() || rest.starts_with(" - "),
        GameKind::Skyrim => rest.is_empty(),
        // The ` - voices_en` entries spell `VOICE_LANGUAGE` out; `the_voice_suffix_is_the_voice_language` keeps them so.
        _ => suffixes.contains(&rest),
    }
}

/// The names among `archives` (file names in the plugin's directory) that pair with `plugin` in `game`, ordered by
/// case-folded name so a plugin's archives open in the same order on every file system. One plugin's question; a
/// profile asks it for every plugin of a directory through one [`ArchiveDirectory`].
pub fn paired_archives<'a>(game: GameKind, plugin: &str, archives: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    ArchiveDirectory::new(archives).paired(game, plugin).into_iter().copied().collect()
}

/// The archive names of one directory, sorted by their folded key, so the archives a plugin pairs with are found by
/// its stem instead of by testing every name (LMC-PERF-001: a 2,000-plugin profile in one `Data` directory made four
/// million [`pairs_with`] calls). Every rule pairs an archive whose folded name starts with the plugin's folded stem,
/// so the names outside that range are ones [`pairs_with`] rejects; the names inside it are still decided by it.
pub struct ArchiveDirectory<S> {
    /// `(plugin_key(name), name)`, sorted by the key, then the name.
    names: Vec<(String, S)>,
}

impl<S: AsRef<str>> ArchiveDirectory<S> {
    pub fn new(archives: impl IntoIterator<Item = S>) -> Self {
        let mut names: Vec<(String, S)> = archives.into_iter().map(|name| (plugin_key(name.as_ref()), name)).collect();
        names.sort_unstable_by(|(a, a_name), (b, b_name)| a.cmp(b).then_with(|| a_name.as_ref().cmp(b_name.as_ref())));
        Self { names }
    }

    /// [`paired_archives`] of `plugin` over this directory's names.
    pub fn paired(&self, game: GameKind, plugin: &str) -> Vec<&S> {
        let key = plugin_key(plugin);
        let Some(stem) = [".esm", ".esp", ".esl"].iter().find_map(|ext| key.strip_suffix(ext)) else {
            return Vec::new();
        };
        let start = self.names.partition_point(|(name, _)| name.as_str() < stem);
        let mut paired: Vec<&S> = self.names[start..]
            .iter()
            .take_while(|(name, _)| name.starts_with(stem))
            .map(|(_, name)| name)
            .filter(|archive| pairs_with(game, plugin, archive.as_ref()))
            .collect();
        paired.sort_by_cached_key(|archive| archive.as_ref().to_lowercase());
        paired
    }
}

/// Whether `game` ships compiled scripts (Papyrus `.pex`) inside its archives, so a paired archive also feeds the
/// script provider. Oblivion, Fallout 3 and New Vegas compile their scripts into the plugin.
pub fn archives_hold_compiled_scripts(game: GameKind) -> bool {
    matches!(game, GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield)
}

/// Whether `game`'s materials are `.bgsm` / `.bgem` files under `materials\`, so a paired archive also feeds the
/// material provider. Starfield's component databases are found through the mesh archives instead.
pub fn archives_hold_bgsm_materials(game: GameKind) -> bool {
    matches!(game, GameKind::Fallout4 | GameKind::Fallout76)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The indexed lookup finds what testing every name finds, for each game's rule, over names that share and extend
    /// stems: `Foo` and `Foo - Bar` plugins, a `Foo_` and a `Foobar` archive, case and extension variants.
    #[test]
    fn the_directory_index_pairs_what_testing_every_name_pairs() {
        let names = [
            "Foo.bsa", "foo - main.bsa", "Foo - Bar.bsa", "Foo - Bar - Main.bsa", "Foo_Extra.bsa", "Foo Extra.bsa", "Foobar.bsa",
            "FOO - MAIN.BA2", "Foo - Textures.ba2", "Foo - Voices_en.ba2", "Foo - Localization.ba2", "Foo - Bar - Main.ba2",
            "Bar.bsa", "Bar - Main.ba2", "A.bsa", "Zeta - Main.bsa", "foo.BSA", "Foo.ba2", "Fo.bsa", "Foo-x.bsa",
        ];
        let plugins = ["Foo.esp", "foo.ESM", "Foo - Bar.esp", "Foobar.esl", "Bar.esm", "Fo.esp", "Zeta.esm", "Missing.esp", "Foo.txt"];
        let directory = ArchiveDirectory::new(names);
        for game in [GameKind::Oblivion, GameKind::Fallout3NV, GameKind::Skyrim, GameKind::Fallout4, GameKind::Starfield, GameKind::Fallout76] {
            for plugin in plugins {
                let mut scanned: Vec<&str> = names.iter().copied().filter(|archive| pairs_with(game, plugin, archive)).collect();
                scanned.sort_by_cached_key(|archive| archive.to_lowercase());
                let indexed: Vec<&str> = directory.paired(game, plugin).into_iter().copied().collect();
                assert_eq!(indexed, scanned, "{game:?} {plugin}");
            }
        }
        assert_eq!(paired_archives(GameKind::Fallout3NV, "Foo.esp", names), ["Foo - Bar - Main.bsa", "Foo - Bar.bsa", "foo - main.bsa", "Foo.bsa", "foo.BSA"]);
    }

    /// The rule each game applies, over archive names shipped with the installed games (and the documented examples).
    #[test]
    fn each_game_pairs_its_documented_names() {
        let cases: &[(GameKind, &str, &str, bool)] = &[
            // Oblivion: `<stem>` then nothing or a space, hyphen or underscore.
            (GameKind::Oblivion, "Knights.esp", "Knights.bsa", true),
            (GameKind::Oblivion, "DLCShiveringIsles.esp", "DLCShiveringIsles - Meshes.bsa", true),
            (GameKind::Oblivion, "Mod.esp", "Mod_Extra.bsa", true),
            (GameKind::Oblivion, "Mod.esp", "ModExtra.bsa", false),
            (GameKind::Oblivion, "Oblivion.esm", "Oblivion - Textures - Compressed.bsa", true),
            // Fallout 3 / New Vegas: `<stem>.bsa` or `<stem> - <anything>.bsa`, an `.esm` included (#6670 ruling).
            (GameKind::Fallout3NV, "DeadMoney.esm", "DeadMoney - Main.bsa", true),
            (GameKind::Fallout3NV, "DeadMoney.esm", "DeadMoney - Sounds.bsa", true),
            (GameKind::Fallout3NV, "JunkyMod.esp", "JunkyMod.bsa", true),
            (GameKind::Fallout3NV, "JunkyMod.esp", "JunkyMod - Assets.bsa", true),
            (GameKind::Fallout3NV, "JunkyMod.esp", "JunkyMod_Assets.bsa", false),
            (GameKind::Fallout3NV, "FalloutNV.esm", "Fallout - Meshes.bsa", false),
            (GameKind::Fallout3NV, "DeadMoney.esm", "DeadMoney - Main.ba2", false),
            // Skyrim: `<stem>.bsa` alone, `.esl` included.
            (GameKind::Skyrim, "ccBGSSSE037-Curios.esl", "ccBGSSSE037-Curios.bsa", true),
            (GameKind::Skyrim, "ccBGSSSE001-Fish.esm", "ccbgssse001-fish.BSA", true),
            (GameKind::Skyrim, "Skyrim.esm", "Skyrim - Meshes0.bsa", false),
            (GameKind::Skyrim, "Mod.esp", "Mod - Textures.bsa", false),
            // Fallout 4: the fixed suffixes, the English voice archive among them.
            (GameKind::Fallout4, "DLCCoast.esm", "DLCCoast - Main.ba2", true),
            (GameKind::Fallout4, "DLCCoast.esm", "DLCCoast - Textures.ba2", true),
            (GameKind::Fallout4, "DLCCoast.esm", "DLCCoast - Voices_en.ba2", true),
            (GameKind::Fallout4, "DLCCoast.esm", "DLCCoast - Voices_fr.ba2", false),
            (GameKind::Fallout4, "DLCCoast.esm", "DLCCoast - Geometry.csg", false),
            (GameKind::Fallout4, "ccbgsfo4021-powerarmorskinblue.esl", "ccbgsfo4021-powerarmorskinblue - main.ba2", true),
            (GameKind::Fallout4, "Fallout4.esm", "Fallout4 - Textures1.ba2", false),
            (GameKind::Fallout4, "DLCUltraHighResolution.esm", "DLCUltraHighResolution - Textures01.ba2", false),
            // Starfield: the fixed suffixes, localisation included, console archives not.
            (GameKind::Starfield, "SFBGS003.esm", "SFBGS003 - Main.ba2", true),
            (GameKind::Starfield, "SFBGS003.esm", "SFBGS003 - Voices_en.ba2", true),
            (GameKind::Starfield, "SFBGS003.esm", "SFBGS003 - Voices_ja.ba2", false),
            (GameKind::Starfield, "Constellation.esm", "Constellation - Localization.ba2", true),
            (GameKind::Starfield, "Mod.esm", "Mod - Main_Xbox.ba2", false),
            // Not a plugin: nothing pairs.
            (GameKind::Fallout3NV, "Readme.txt", "Readme.bsa", false),
            (GameKind::Fallout76, "SeventySix.esm", "SeventySix - Main.ba2", false),
        ];
        for &(game, plugin, archive, expected) in cases {
            assert_eq!(pairs_with(game, plugin, archive), expected, "{game:?}: {plugin} ~ {archive}");
        }
    }

    /// The voice suffix in the rule table is the voice language, for both games that pair voice archives.
    #[test]
    fn the_voice_suffix_is_the_voice_language() {
        for game in [GameKind::Fallout4, GameKind::Starfield] {
            assert!(pairs_with(game, "Mod.esm", &format!("Mod - Voices_{VOICE_LANGUAGE}.ba2")), "{game:?}");
        }
    }

    /// A plugin given by its path pairs by its file name; the archives come back in case-folded name order.
    #[test]
    fn paired_archives_match_by_file_name_in_name_order() {
        let names = ["DeadMoney - Sounds.bsa", "Fallout - Meshes.bsa", "deadmoney - main.bsa", "DeadMoney.esm"];
        assert_eq!(
            paired_archives(GameKind::Fallout3NV, "/games/FNV/Data/DeadMoney.esm", names),
            ["deadmoney - main.bsa", "DeadMoney - Sounds.bsa"]
        );
    }
}
