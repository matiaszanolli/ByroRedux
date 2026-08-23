//! FormID → archive-relative audio path resolution.
//!
//! EX-16 item 1 (#2372) named this the missing half of REGN ambient-sound
//! consumption: `RegionDataKind::Sound` carries a `sound_form: u32`
//! pointing at a `SOUN` record, but until `soun::parse_soun` there was no
//! decoded file path to resolve it to, and no resolver connecting the two.
//! This module is deliberately just the lookup + archive-key
//! normalization — mirroring `script::pex_archive_path`'s shape for
//! Papyrus `.pex` names — not a playback pipeline. Wiring a REGN-keyed
//! `AudioEmitter` (item 5) is separate, downstream work: it needs the
//! generic crossfade/prune machinery already in `crates/audio`, gated on
//! this resolver existing at all.

use byroredux_plugin::esm::records::SounRecord;
use std::collections::HashMap;

/// Look up a `SOUN` FormID's decoded file path. Returns `None` when the
/// FormID isn't a known SOUN (bad data) or the record omitted `FNAM`
/// (rare placeholder records — see `soun::parse_soun`'s doc).
#[allow(dead_code)] // landed ahead of its consumer — EX-16 item 5 (REGN-keyed AudioEmitter, #2372) is the pending caller
pub(crate) fn resolve_sound_path(sounds: &HashMap<u32, SounRecord>, form_id: u32) -> Option<&str> {
    sounds
        .get(&form_id)
        .map(|s| s.sound_path.as_str())
        .filter(|p| !p.is_empty())
}

/// Normalise a `SOUN.FNAM` value to its archive key: lowercase,
/// backslash-separated, under the `sound\` folder. `FNAM` is authored
/// relative to `Data\Sound\` without that prefix (the same convention as
/// `MODL` being relative to `Meshes\` and `ICON` to `Textures\`); a path
/// that already carries the folder, or uses forward slashes, is accepted
/// unchanged in meaning. Mirrors `script::pex_archive_path`.
#[allow(dead_code)] // landed ahead of its consumer — EX-16 item 5 (REGN-keyed AudioEmitter, #2372) is the pending caller
pub(crate) fn sound_archive_path(sound_path: &str) -> String {
    let mut path = sound_path.replace('/', "\\").to_ascii_lowercase();
    if !path.starts_with("sound\\") {
        path = format!("sound\\{path}");
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn soun(form_id: u32, path: &str) -> SounRecord {
        SounRecord {
            form_id,
            editor_id: String::new(),
            sound_path: path.to_string(),
        }
    }

    #[test]
    fn resolve_sound_path_finds_known_form_id() {
        let mut sounds = HashMap::new();
        sounds.insert(0x1234, soun(0x1234, "fx\\explosion01.wav"));
        assert_eq!(
            resolve_sound_path(&sounds, 0x1234),
            Some("fx\\explosion01.wav")
        );
    }

    #[test]
    fn resolve_sound_path_missing_form_id_returns_none() {
        let sounds = HashMap::new();
        assert_eq!(resolve_sound_path(&sounds, 0xDEAD_BEEF), None);
    }

    #[test]
    fn resolve_sound_path_empty_fnam_returns_none() {
        // Placeholder SOUN with no FNAM decodes to an empty path — treat
        // it the same as "not found" rather than handing callers an
        // archive lookup that can never hit.
        let mut sounds = HashMap::new();
        sounds.insert(0x5, soun(0x5, ""));
        assert_eq!(resolve_sound_path(&sounds, 0x5), None);
    }

    #[test]
    fn sound_archive_path_prepends_folder_and_lowercases() {
        assert_eq!(
            sound_archive_path("FX\\Explosion01.wav"),
            "sound\\fx\\explosion01.wav"
        );
    }

    #[test]
    fn sound_archive_path_normalizes_forward_slashes() {
        assert_eq!(
            sound_archive_path("amb/wind_loop.wav"),
            "sound\\amb\\wind_loop.wav"
        );
    }

    #[test]
    fn sound_archive_path_does_not_double_prefix() {
        // Defensive: a path already authored with the `sound\` folder
        // (non-vanilla authoring) must not become `sound\sound\...`.
        assert_eq!(
            sound_archive_path("sound\\fx\\explosion01.wav"),
            "sound\\fx\\explosion01.wav"
        );
    }
}
