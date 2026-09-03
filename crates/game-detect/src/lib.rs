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

    /// #3790 — `Update.bsa` (FNV's own base-game patch archive) must be
    /// listed BEFORE `Fallout - Meshes.bsa` in the shipped `fnv` profile's
    /// `default_bsas`: archive resolution is first-listed-wins
    /// (`TextureProvider::extract_mesh` / `extract`/`extract_via_facegen_
    /// tool_path_fallback` in `byroredux/src/asset_provider/texture.rs`
    /// all walk their archive list in push order and return the first
    /// hit), so listing the patch archive second would make it lose
    /// priority to the base archive it's meant to override — the opposite
    /// of retail's own archive priority. Reads the real shipped file, and
    /// is skipped when run from outside the repo, same as
    /// `catalog_matches_shipped_profiles` above.
    #[test]
    fn fnv_profile_lists_update_bsa_before_the_base_meshes_archive() {
        let path = std::path::Path::new("../../assets/debug_profiles.toml");
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let document: toml::Table = toml::from_str(&text).unwrap();
        let fnv = document["profiles"]["fnv"]
            .as_table()
            .expect("shipped profiles have no `fnv` block");
        let default_bsas = fnv["default_bsas"]
            .as_array()
            .expect("fnv.default_bsas must be an array")
            .iter()
            .map(|v| v.as_str().expect("default_bsas entries must be strings"))
            .collect::<Vec<_>>();

        let update_pos = default_bsas
            .iter()
            .position(|&b| b == "Update.bsa")
            .expect("fnv.default_bsas must list Update.bsa (#3790)");
        let base_pos = default_bsas
            .iter()
            .position(|&b| b == "Fallout - Meshes.bsa")
            .expect("fnv.default_bsas must list Fallout - Meshes.bsa");
        assert!(
            update_pos < base_pos,
            "Update.bsa must precede Fallout - Meshes.bsa in fnv.default_bsas \
             {default_bsas:?} — archive resolution is first-listed-wins, so this \
             order is what makes the patch archive actually win",
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
