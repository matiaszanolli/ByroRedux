//! Install detection and pre-launch validation.
//!
//! The engine's shipped profile registry defers each game's absolute path to a
//! `--games-root` that defaults to one developer's Steam library. This crate
//! is what replaces that guess with an answer: it finds the installs, checks
//! them, and writes back only what it learned — the path — leaving the curated
//! archive lists alone.
//!
//! Deliberately renderer-free, so the launcher can run every check on a
//! machine where Vulkan initialisation fails. See
//! [`docs/engine/launcher.md`](../../../docs/engine/launcher.md) §3.
//!
//! ```no_run
//! use byroredux_game_detect as detect;
//! for candidate in detect::detect() {
//!     println!("{} at {}", candidate.profile, candidate.data_dir.display());
//! }
//! ```

pub mod catalog;
pub mod overrides;
pub mod profiles;
pub mod steam;
pub mod validate;
pub mod vdf;

use std::path::PathBuf;

pub use catalog::SteamApp;
pub use overrides::RootOverrides;
pub use validate::{Check, Severity, ValidationReport};

/// Where a candidate came from. Surfaced so the launcher can say *why* it
/// believes a game lives somewhere, which is the first question a user asks
/// when it is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Read out of a Steam app manifest.
    Steam,
    /// Already recorded in the user's profiles file.
    Configured,
    /// Picked by the user in a file dialog.
    Manual,
}

/// One install the launcher can offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// `GameProfileEntry` key.
    pub profile: String,
    /// Human-readable title, from the store manifest when available.
    pub display_name: String,
    /// The `Data` directory the engine would load from.
    pub data_dir: PathBuf,
    pub source: Source,
}

/// Every supported title this machine appears to have installed.
///
/// Steam only, today: the Windows registry and GOG probes described in the
/// plan are not implemented, so a non-Steam install still needs Browse or a
/// hand-written override. Ordering follows [`catalog::STEAM_APPS`].
pub fn detect() -> Vec<Candidate> {
    steam::detect()
        .into_iter()
        .map(|install| Candidate {
            profile: install.app.profile.to_owned(),
            display_name: install.name.clone(),
            data_dir: install.data_dir(),
            source: Source::Steam,
        })
        .collect()
}

/// Candidates already recorded in a profiles file's `[roots]` table.
///
/// Read first and treated as authoritative: a user who fixed a path by hand
/// must not have it re-guessed underneath them.
pub fn configured(profiles_path: impl AsRef<std::path::Path>) -> Vec<Candidate> {
    let Ok(overrides) = RootOverrides::load(profiles_path) else {
        return Vec::new();
    };
    overrides
        .roots
        .into_iter()
        .map(|(profile, root)| Candidate {
            display_name: catalog::by_profile(&profile)
                .map(|app| app.install_dir.to_owned())
                .unwrap_or_else(|| profile.clone()),
            profile,
            data_dir: PathBuf::from(root),
            source: Source::Configured,
        })
        .collect()
}

/// Configured candidates, then detected ones for profiles not already
/// configured.
///
/// This is the ordering the launcher's first run wants: honour what the user
/// already set, fill the gaps by detection.
pub fn detect_all(profiles_path: impl AsRef<std::path::Path>) -> Vec<Candidate> {
    let mut all = configured(profiles_path);
    for candidate in detect() {
        if !all.iter().any(|seen| seen.profile == candidate.profile) {
            all.push(candidate);
        }
    }
    all
}

impl Candidate {
    /// Render this candidate as a `[roots]` entry.
    pub fn as_override(&self) -> (String, String) {
        (
            self.profile.clone(),
            self.data_dir.to_string_lossy().into_owned(),
        )
    }
}

/// Collect candidates into a writable override set.
pub fn overrides_for(candidates: &[Candidate]) -> RootOverrides {
    RootOverrides {
        roots: candidates.iter().map(Candidate::as_override).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The catalog's `install_dir` and the shipped profile's `subdir` must
    /// stay in lockstep: detection builds a data dir from the former, while a
    /// `--game <key>` launch without an override builds it from the latter, so
    /// a divergence would send the two routes to different folders.
    ///
    /// Reads the real shipped file, and is skipped when run from outside the
    /// repo (an installed binary's test run has no `assets/`).
    #[test]
    fn catalog_matches_shipped_profiles() {
        let path = std::path::Path::new("../../assets/debug_profiles.toml");
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let document: toml::Table = toml::from_str(&text).unwrap();
        let profiles = document["profiles"].as_table().unwrap();

        for app in catalog::STEAM_APPS {
            let profile = profiles
                .get(app.profile)
                .unwrap_or_else(|| panic!("shipped profiles have no `{}` block", app.profile));
            let subdir = profile["subdir"].as_str().unwrap();
            assert_eq!(
                subdir,
                format!("{}/Data", app.install_dir),
                "catalog install_dir for `{}` disagrees with its shipped subdir",
                app.profile
            );
        }
    }

    /// #3790 / #3896 — a patch archive must be listed AFTER the base archive
    /// it overrides, in every shipped profile that has such a pair: archive
    /// resolution is last-listed-wins (`TextureProvider::extract_mesh` /
    /// `extract` / `extract_via_facegen_tool_path_fallback` in
    /// `byroredux/src/asset_provider/texture.rs` all walk their archive list
    /// in REVERSE push order and return the first hit), so listing the patch
    /// archive first makes it lose priority to the base archive it's meant to
    /// override — the opposite of retail's own archive priority.
    ///
    /// #3896 — this test previously asserted the exact opposite, and passed,
    /// because #3637 (`3562401b`) inverted resolution to last-wins without
    /// updating either the profiles or this test. It was pinning the broken
    /// order with the falsified premise stated in its own assertion message.
    /// It now covers all three profiles that ship a patch/base pair, so one
    /// being accidentally right (fo4 was) can no longer hide the others.
    ///
    /// Reads the real shipped file, and is skipped when run from outside the
    /// repo, same as `catalog_matches_shipped_profiles` above.
    #[test]
    fn profiles_list_patch_archives_after_the_base_archives_they_override() {
        let path = std::path::Path::new("../../assets/debug_profiles.toml");
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let document: toml::Table = toml::from_str(&text).unwrap();

        // (profile, list key, patch archive, base archive it must override).
        // fo4 is included because it was the one profile that happened to be
        // ordered correctly under last-wins — pinning it stops a future edit
        // from "fixing" it into consistency with the broken ones (#3896).
        for (profile, key, patch, base) in [
            ("fnv", "default_bsas", "Update.bsa", "Fallout - Meshes.bsa"),
            (
                "starfield",
                "default_bsas",
                "Starfield - MeshesPatch.ba2",
                "Starfield - Meshes01.ba2",
            ),
            (
                "starfield",
                "default_textures_bsas",
                "Starfield - TexturesPatch01.ba2",
                "Starfield - Textures01.ba2",
            ),
            (
                "fo4",
                "default_textures_bsas",
                "Fallout4 - TexturesPatch.ba2",
                "Fallout4 - Textures1.ba2",
            ),
        ] {
            let block = document["profiles"][profile]
                .as_table()
                .unwrap_or_else(|| panic!("shipped profiles have no `{profile}` block"));
            let archives = block[key]
                .as_array()
                .unwrap_or_else(|| panic!("{profile}.{key} must be an array"))
                .iter()
                .map(|v| v.as_str().expect("archive entries must be strings"))
                .collect::<Vec<_>>();

            let patch_pos = archives
                .iter()
                .position(|&b| b == patch)
                .unwrap_or_else(|| panic!("{profile}.{key} must list {patch}"));
            let base_pos = archives
                .iter()
                .position(|&b| b == base)
                .unwrap_or_else(|| panic!("{profile}.{key} must list {base}"));
            assert!(
                base_pos < patch_pos,
                "{patch} must FOLLOW {base} in {profile}.{key} {archives:?} — \
                 archive resolution is last-listed-wins (#3637), so this order \
                 is what makes the patch archive actually win",
            );
        }
    }

    /// #3788 — `--game fnv` must supply a `--sounds-bsa`, or the three M44
    /// audio consumers (footstep, water splash, REGN ambient) silently
    /// early-return with nothing to distinguish "no archive" from "not
    /// implemented". Reads the real shipped file, same pattern as
    /// `fnv_profile_lists_update_bsa_before_the_base_meshes_archive` above.
    #[test]
    fn fnv_profile_declares_a_sounds_archive() {
        let path = std::path::Path::new("../../assets/debug_profiles.toml");
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let document: toml::Table = toml::from_str(&text).unwrap();
        let fnv = document["profiles"]["fnv"]
            .as_table()
            .expect("shipped profiles have no `fnv` block");
        let default_sounds_bsas = fnv
            .get("default_sounds_bsas")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(|v| v.as_str().unwrap_or("")).collect::<Vec<_>>())
            .unwrap_or_default();
        assert!(
            !default_sounds_bsas.is_empty(),
            "fnv.default_sounds_bsas must not be empty — footstep audio, water-splash \
             acoustics, and REGN ambient all silently early-return with no --sounds-bsa \
             supplied at all (#3788)",
        );
        assert!(
            default_sounds_bsas.contains(&"Fallout - Sound.bsa"),
            "fnv.default_sounds_bsas {default_sounds_bsas:?} must list Fallout - Sound.bsa \
             (the vanilla FNV sound archive; the default footstep and water-splash keys \
             resolve inside it byte-for-byte — #3913)",
        );
    }

    #[test]
    fn configured_roots_are_read_back_as_candidates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");
        RootOverrides {
            roots: [("fnv".to_owned(), "/games/FNV/Data".to_owned())]
                .into_iter()
                .collect(),
        }
        .merge_into_file(&path)
        .unwrap();

        let candidates = configured(&path);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].profile, "fnv");
        assert_eq!(candidates[0].source, Source::Configured);
        assert_eq!(candidates[0].data_dir, PathBuf::from("/games/FNV/Data"));
    }

    /// A hand-fixed path must not be re-guessed by detection underneath the
    /// user, so a configured profile suppresses its detected twin.
    #[test]
    fn configured_candidates_take_precedence_over_detection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");
        RootOverrides {
            roots: [("fnv".to_owned(), "/hand/edited/Data".to_owned())]
                .into_iter()
                .collect(),
        }
        .merge_into_file(&path)
        .unwrap();

        let all = detect_all(&path);
        let fnv: Vec<&Candidate> = all.iter().filter(|c| c.profile == "fnv").collect();
        assert_eq!(fnv.len(), 1, "one entry per profile");
        assert_eq!(fnv[0].data_dir, PathBuf::from("/hand/edited/Data"));
        assert_eq!(fnv[0].source, Source::Configured);
    }

    #[test]
    fn candidates_round_trip_through_the_override_set() {
        let candidates = vec![Candidate {
            profile: "fo4".into(),
            display_name: "Fallout 4".into(),
            data_dir: PathBuf::from("/games/FO4/Data"),
            source: Source::Steam,
        }];
        let overrides = overrides_for(&candidates);
        assert_eq!(overrides.roots["fo4"], "/games/FO4/Data");
    }
}
