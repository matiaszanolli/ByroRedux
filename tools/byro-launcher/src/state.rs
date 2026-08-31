//! Launcher state and every decision it makes.
//!
//! All of it is pure over paths and structs, so the interesting behaviour —
//! which games are offered, which are launchable, what a Play click actually
//! requests — is tested without a window or a GPU. `ui.rs` only draws this.

use std::path::{Path, PathBuf};

use byroredux_boot_request::{Action, BootRequest};
use byroredux_core::ecs::{GameProfileEntry, GameProfileRegistry};
use byroredux_game_detect as detect;
use byroredux_game_detect::validate::{Severity, ValidationReport};

/// One game as the Library screen sees it.
pub struct Entry {
    pub candidate: detect::Candidate,
    pub report: ValidationReport,
    /// The registry profile, or `None` when a browsed folder does not match a
    /// known profile — which is a real state, not an error, and the UI has to
    /// say so rather than crash.
    pub profile: Option<GameProfileEntry>,
}

impl Entry {
    pub fn is_launchable(&self) -> bool {
        self.profile.is_some() && self.report.is_launchable()
    }

    /// One-word verdict for the card.
    pub fn verdict(&self) -> &'static str {
        if self.profile.is_none() {
            return "unrecognised";
        }
        match self.report.verdict() {
            Severity::Ok => "ready",
            Severity::Warn => "ready, with warnings",
            Severity::Fail => "not ready",
        }
    }

    /// Ways to start this game, in the order the Play menu shows them.
    ///
    /// Driven entirely by what the profile actually authors: `New game` is
    /// offered only where the profile carries a new-game placement (today,
    /// only Skyrim SE), because everywhere else the flag would resolve to
    /// nothing. Sample cells always work, which is why they are listed.
    pub fn play_options(&self) -> Vec<(String, Action)> {
        let Some(profile) = &self.profile else {
            return Vec::new();
        };
        let mut options = Vec::new();
        if profile.new_game_worldspace.is_some() && profile.new_game_grid.is_some() {
            options.push(("New game".to_owned(), Action::NewGame));
        }
        for cell in &profile.sample_cells {
            options.push((cell.clone(), Action::Cell { edid: cell.clone() }));
        }
        options
    }
}

/// Everything the launcher knows.
pub struct LauncherState {
    pub registry: GameProfileRegistry,
    pub entries: Vec<Entry>,
    pub profiles_path: PathBuf,
    pub selected: Option<usize>,
}

impl LauncherState {
    /// Read the registry, find the games, check them.
    pub fn load(profiles_path: impl Into<PathBuf>) -> Self {
        let profiles_path = profiles_path.into();
        let registry = detect::profiles::load_default();
        let entries = build_entries(&registry, &detect::detect_all(&profiles_path));
        Self {
            registry,
            entries,
            profiles_path,
            selected: None,
        }
    }

    /// Re-run detection and validation, keeping the selection where possible.
    pub fn refresh(&mut self) {
        let selected_profile = self
            .selected
            .and_then(|index| self.entries.get(index))
            .map(|entry| entry.candidate.profile.clone());
        self.registry = detect::profiles::load_default();
        self.entries = build_entries(&self.registry, &detect::detect_all(&self.profiles_path));
        self.selected = selected_profile.and_then(|profile| {
            self.entries
                .iter()
                .position(|entry| entry.candidate.profile == profile)
        });
    }

    /// Add a folder the user picked, validating it against every known profile
    /// and keeping the first that matches.
    ///
    /// Returns whether the folder was recognised. A folder that matches nothing
    /// is still added, so the user can see *why* it was rejected rather than
    /// have the click do nothing.
    pub fn add_manual(&mut self, data_dir: impl Into<PathBuf>) -> bool {
        let data_dir = data_dir.into();
        let matched = identify(&self.registry, &data_dir);
        let profile_key = matched
            .as_ref()
            .map(|(key, _)| key.clone())
            .unwrap_or_else(|| folder_label(&data_dir));

        self.entries
            .retain(|entry| entry.candidate.profile != profile_key);
        let candidate = detect::Candidate {
            display_name: matched
                .as_ref()
                .map(|(_, entry)| entry.name.clone())
                .unwrap_or_else(|| folder_label(&data_dir)),
            profile: profile_key,
            data_dir,
            source: detect::Source::Manual,
        };
        let entry = build_entry(&self.registry, candidate);
        let recognised = entry.profile.is_some();
        self.entries.push(entry);
        recognised
    }

    /// Everything currently known, as a writable `[roots]` set.
    pub fn overrides(&self) -> detect::RootOverrides {
        detect::overrides_for(
            &self
                .entries
                .iter()
                .filter(|entry| entry.profile.is_some())
                .map(|entry| entry.candidate.clone())
                .collect::<Vec<_>>(),
        )
    }

    /// Compose the request for a Play click.
    ///
    /// Profile-driven, never self-contained: [`Self::remember`] records the
    /// path in `[roots]` first, so the engine's own profile expander resolves
    /// the ESM and all five archive categories. That keeps one implementation
    /// of "which archives does this game need" rather than teaching the
    /// launcher a second one.
    pub fn boot_request(&self, index: usize, action: Action) -> Option<BootRequest> {
        let entry = self.entries.get(index)?;
        entry.profile.as_ref()?;
        Some(BootRequest::for_profile(entry.candidate.profile.clone()).with_action(action))
    }

    /// Persist the known paths so the engine resolves them the same way.
    pub fn remember(&self) -> Result<(), detect::overrides::OverrideError> {
        self.overrides().merge_into_file(&self.profiles_path)
    }
}

fn build_entries(registry: &GameProfileRegistry, candidates: &[detect::Candidate]) -> Vec<Entry> {
    candidates
        .iter()
        .cloned()
        .map(|candidate| build_entry(registry, candidate))
        .collect()
}

fn build_entry(registry: &GameProfileRegistry, candidate: detect::Candidate) -> Entry {
    let profile = registry.get(&candidate.profile).cloned();
    let report = match &profile {
        Some(entry) => detect::validate::validate(entry, &candidate.data_dir),
        // No profile means nothing to validate against; say that in the report
        // rather than leaving an empty one that reads as "ready".
        None => ValidationReport {
            profile: candidate.display_name.clone(),
            data_dir: candidate.data_dir.clone(),
            checks: vec![detect::Check {
                severity: Severity::Fail,
                label: "Profile".to_owned(),
                detail: format!(
                    "no profile named {:?}; this folder is not a game the engine knows",
                    candidate.profile
                ),
            }],
        },
    };
    Entry {
        candidate,
        report,
        profile,
    }
}

/// Which profile, if any, describes the game in this folder.
///
/// Decided by the main plugin's presence, since that is the one file every
/// profile names and no two vanilla games share.
pub fn identify(
    registry: &GameProfileRegistry,
    data_dir: &Path,
) -> Option<(String, GameProfileEntry)> {
    registry
        .iter()
        .find(|(_, entry)| !entry.esm.is_empty() && data_dir.join(&entry.esm).is_file())
        .map(|(key, entry)| (key.to_string(), entry.clone()))
}

fn folder_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::collections::BTreeMap as Map;

    fn registry() -> GameProfileRegistry {
        GameProfileRegistry::new(Map::from([
            (
                "fnv".to_owned(),
                GameProfileEntry {
                    name: "Fallout New Vegas".into(),
                    esm: "FalloutNV.esm".into(),
                    sample_cells: vec!["GSDocMitchellHouse".into()],
                    ..GameProfileEntry::default()
                },
            ),
            (
                "skyrim_se".to_owned(),
                GameProfileEntry {
                    name: "Skyrim Special Edition".into(),
                    esm: "Skyrim.esm".into(),
                    new_game_worldspace: Some("Tamriel".into()),
                    new_game_grid: Some("5,-24".into()),
                    sample_cells: vec!["WhiterunBanneredMare".into()],
                    ..GameProfileEntry::default()
                },
            ),
        ]))
    }

    fn entry_for(key: &str, data_dir: PathBuf) -> Entry {
        build_entry(
            &registry(),
            detect::Candidate {
                profile: key.to_owned(),
                display_name: key.to_owned(),
                data_dir,
                source: detect::Source::Manual,
            },
        )
    }

    /// New game is offered only where the profile authors a placement;
    /// offering it everywhere would give five of six games a button that
    /// silently does nothing.
    #[test]
    fn new_game_is_offered_only_where_the_profile_has_a_target() {
        let skyrim = entry_for("skyrim_se", PathBuf::from("/nope"));
        assert_eq!(skyrim.play_options()[0].0, "New game");
        assert_eq!(
            skyrim.play_options()[1].1,
            Action::Cell {
                edid: "WhiterunBanneredMare".into()
            }
        );

        let fnv = entry_for("fnv", PathBuf::from("/nope"));
        assert!(fnv
            .play_options()
            .iter()
            .all(|(label, _)| label != "New game"));
        assert_eq!(fnv.play_options().len(), 1);
    }

    /// A folder that matches no profile is a real state the UI must render, not
    /// an empty report that would read as "ready".
    #[test]
    fn an_unrecognised_folder_reports_why_rather_than_looking_ready() {
        let entry = entry_for("not-a-game", PathBuf::from("/tmp"));
        assert!(entry.profile.is_none());
        assert!(!entry.is_launchable());
        assert_eq!(entry.verdict(), "unrecognised");
        assert_eq!(entry.report.verdict(), Severity::Fail);
        assert!(entry.play_options().is_empty());
    }

    #[test]
    fn a_folder_is_identified_by_its_main_plugin() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Skyrim.esm"), b"TES4").unwrap();
        let (key, entry) = identify(&registry(), dir.path()).unwrap();
        assert_eq!(key, "skyrim_se");
        assert_eq!(entry.name, "Skyrim Special Edition");

        let empty = tempfile::tempdir().unwrap();
        assert!(identify(&registry(), empty.path()).is_none());
    }

    /// Play emits a profile-driven request, so the engine's own expander does
    /// the archive fan-out and the launcher never learns a second answer.
    #[test]
    fn a_play_click_requests_a_profile_not_a_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("FalloutNV.esm"), b"TES4").unwrap();
        let mut state = LauncherState {
            registry: registry(),
            entries: Vec::new(),
            profiles_path: dir.path().join("profiles.toml"),
            selected: None,
        };
        assert!(state.add_manual(dir.path()));

        let request = state
            .boot_request(
                0,
                Action::Cell {
                    edid: "GSDocMitchellHouse".into(),
                },
            )
            .unwrap();
        assert!(
            !request.is_self_contained(),
            "must defer to the profile expander"
        );
        assert_eq!(request.game.profile, "fnv");
        assert_eq!(
            request.action,
            Some(Action::Cell {
                edid: "GSDocMitchellHouse".into()
            })
        );
    }

    /// Browsing to a folder must not create a second card for a game already
    /// listed — the user is correcting a path, not adding a game.
    #[test]
    fn browsing_to_a_known_game_replaces_its_entry() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("FalloutNV.esm"), b"TES4").unwrap();
        let mut state = LauncherState {
            registry: registry(),
            entries: Vec::new(),
            profiles_path: dir.path().join("profiles.toml"),
            selected: None,
        };
        state.add_manual(dir.path());
        state.add_manual(dir.path());
        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.entries[0].candidate.profile, "fnv");
    }

    /// An unrecognised folder is still listed, so the click does something the
    /// user can act on.
    #[test]
    fn browsing_to_a_folder_with_no_game_still_lists_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = LauncherState {
            registry: registry(),
            entries: Vec::new(),
            profiles_path: dir.path().join("profiles.toml"),
            selected: None,
        };
        assert!(!state.add_manual(dir.path()));
        assert_eq!(state.entries.len(), 1);
        assert!(state.boot_request(0, Action::NewGame).is_none());
    }

    /// Only recognised games are written back: an unrecognised folder has no
    /// profile key to be a `[roots]` entry for.
    #[test]
    fn only_recognised_games_are_remembered() {
        let games = tempfile::tempdir().unwrap();
        std::fs::write(games.path().join("Skyrim.esm"), b"TES4").unwrap();
        let junk = tempfile::tempdir().unwrap();

        let mut state = LauncherState {
            registry: registry(),
            entries: Vec::new(),
            profiles_path: games.path().join("profiles.toml"),
            selected: None,
        };
        state.add_manual(games.path());
        state.add_manual(junk.path());

        let overrides = state.overrides();
        assert_eq!(overrides.roots.len(), 1);
        assert!(overrides.roots.contains_key("skyrim_se"));

        state.remember().unwrap();
        let read = detect::RootOverrides::load(&state.profiles_path).unwrap();
        assert_eq!(
            read.roots["skyrim_se"],
            games.path().to_string_lossy().to_string()
        );
    }

    #[test]
    fn a_refresh_keeps_the_selection_on_the_same_game() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");
        detect::RootOverrides {
            roots: BTreeMap::from([("fnv".to_owned(), "/games/FNV/Data".to_owned())]),
        }
        .merge_into_file(&path)
        .unwrap();

        let mut state = LauncherState::load(&path);
        let Some(index) = state
            .entries
            .iter()
            .position(|entry| entry.candidate.profile == "fnv")
        else {
            return; // no shipped registry available in this test environment
        };
        state.selected = Some(index);
        state.refresh();
        assert_eq!(
            state
                .selected
                .map(|index| state.entries[index].candidate.profile.clone()),
            Some("fnv".to_owned())
        );
    }
}
