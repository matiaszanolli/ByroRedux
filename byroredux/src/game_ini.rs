//! The install's shipped default INI: the file each game reads its settings' defaults from before
//! the user's own INI, and the keys the engine takes from it (the start sequence,
//! `docs/engine/video-playback.md` §5; the music level and crossfade, `docs/engine/music.md` §7).
//! A key the INI does not set falls back to the executable's compiled-in default, which each caller
//! states beside its key.

use byroredux_plugin::esm::reader::GameKind;

/// The default INI each game ships beside its executable. Fallout 3 and New Vegas share a name.
pub(crate) fn default_ini_name(game: GameKind) -> Option<&'static str> {
    Some(match game {
        GameKind::Oblivion => "Oblivion_default.ini",
        GameKind::Fallout3NV => "Fallout_default.ini",
        GameKind::Skyrim => "Skyrim_Default.ini",
        GameKind::Fallout4 => "Fallout4_Default.ini",
        GameKind::Starfield => "Starfield.ini",
        // Fallout 76 is out of every PRD that reads one.
        _ => return None,
    })
}

/// The text of `game`'s default INI in the install `args` load from (the parent of the `--esm`
/// plugin's `Data` folder), matched case-insensitively. `None` without an `--esm` or an INI.
pub(crate) fn read_default_ini(args: &[String], game: GameKind) -> Option<String> {
    let (_, data) = crate::cli_args::esm_discovery_root(args).ok()?;
    let install = data.parent()?.to_path_buf();
    let path = crate::systems::script_requests::find_case_insensitive(&install, default_ini_name(game)?)?;
    std::fs::read(path).ok().map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// `[section] key` of an INI file's text, both matched case-insensitively: `Some("")` for a key set
/// to nothing, `None` for one the section does not set. Lines starting with `;` are comments.
pub(crate) fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            in_section = name.trim().eq_ignore_ascii_case(section);
            continue;
        }
        if !in_section || line.starts_with(';') {
            continue;
        }
        if let Some((name, value)) = line.split_once('=') {
            if name.trim().eq_ignore_ascii_case(key) {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

/// [`ini_value`] read as a number; `None` when unset or not a number.
pub(crate) fn ini_f32(text: &str, section: &str, key: &str) -> Option<f32> {
    ini_value(text, section, key)?.parse().ok()
}

/// The game a plugin belongs to, from its `TES4` header alone (the first megabyte is far more than
/// the header needs), without reading the whole file. `None` when it cannot be read.
pub(crate) fn plugin_game(path: &std::path::Path) -> Option<GameKind> {
    use std::io::Read;
    let mut prefix = Vec::new();
    std::fs::File::open(path).ok()?.take(1 << 20).read_to_end(&mut prefix).ok()?;
    let mut reader = byroredux_plugin::esm::reader::EsmReader::new(&prefix);
    let header = reader.read_file_header().ok()?;
    Some(GameKind::from_header(reader.variant(), header.hedr_version, header.record_version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_read_from_its_section_only_and_comments_are_skipped() {
        let ini = "[General]\n; sIntroSequence=x\nsIntroSequence=a.bik\n[Audio]\nfDefaultMusicVolume=0.6000\nsEmpty=\n";
        assert_eq!(ini_value(ini, "general", "SINTROSEQUENCE").as_deref(), Some("a.bik"));
        assert_eq!(ini_value(ini, "General", "fDefaultMusicVolume"), None, "another section's key");
        assert_eq!(ini_f32(ini, "Audio", "fDefaultMusicVolume"), Some(0.6));
        assert_eq!(ini_value(ini, "Audio", "sEmpty").as_deref(), Some(""), "set to nothing");
        assert_eq!(ini_f32(ini, "Audio", "sEmpty"), None);
    }
}
