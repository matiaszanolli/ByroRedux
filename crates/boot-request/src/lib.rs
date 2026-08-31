//! The launcher → engine handoff contract.
//!
//! A [`BootRequest`] describes **what the user wants to play**, not which
//! engine flags express it. That distinction is the whole point of the type:
//! the argv surface is developer-shaped and grows a flag every time a bench or
//! diagnostic knob lands, so a launcher UI built over argv would have to grow
//! with it. A launcher built over *intent* does not — `--cornell-oracle` is not
//! an intent a player can hold.
//!
//! Both [`byro-launcher`] and the engine link this crate so the two cannot
//! drift, and it stays free of engine/GPU dependencies so the launcher can be
//! built and tested without a Vulkan device.
//!
//! ```no_run
//! use byroredux_boot_request::BootRequest;
//! let request = BootRequest::load("~/.byroredux/boot.toml")?;
//! let expansion = request.to_args(&std::env::args().collect::<Vec<_>>());
//! # Ok::<(), byroredux_boot_request::BootRequestError>(())
//! ```
//!
//! See [`docs/engine/launcher.md`](../../../docs/engine/launcher.md) §2 for the
//! design rationale, and [`args`] for the intent → argv translation itself.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

mod args;

pub use args::Expansion;

/// Contract version written into every request and checked strictly on read.
///
/// A mismatch is a launcher/engine **install** problem — the two binaries came
/// from different builds — not a recoverable parse warning, so
/// [`BootRequest::from_toml_str`] refuses rather than half-loading a request
/// whose field meanings it cannot vouch for.
pub const CONTRACT_VERSION: u32 = 1;

/// Default location of the request file, relative to the user's config dir.
pub const DEFAULT_FILE_NAME: &str = "boot.toml";

/// Everything the launcher decided, in one file.
///
/// Field order matters for TOML output: scalars must precede tables, so
/// `version` stays first and the four sub-tables follow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BootRequest {
    /// Always [`CONTRACT_VERSION`] on write; checked strictly on read.
    pub version: u32,
    pub game: GameSpec,
    /// Absent means "the engine decides" — it falls through to the
    /// `[defaults]` table in `profiles.toml`, or to the default scene.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<Action>,
    #[serde(default, skip_serializing_if = "SettingsSpec::is_empty")]
    pub settings: SettingsSpec,
    /// Reserved for P5. Present in v1 so adding load-order support later is
    /// not a `version` bump.
    #[serde(default, skip_serializing_if = "ModsSpec::is_empty")]
    pub mods: ModsSpec,
}

/// Which game, and where its data lives.
///
/// The two content-source modes are distinguished by [`Self::data_dir`]:
///
/// - **Profile-driven** (`data_dir` empty): only [`Self::profile`] is
///   meaningful. Expansion emits `--game <key>` and lets the engine's existing,
///   already-tested `expand_game_profile_args` resolve the ESM and all five
///   archive categories from the registry. This is the common case.
/// - **Self-contained** (`data_dir` set): the launcher detected an install the
///   shipped profile does not describe, so it writes resolved paths and
///   expansion emits explicit `--esm` / `--bsa` / … flags joined against
///   `data_dir`. `profile` is still recorded, for display and for matching
///   save-slot metadata, but does not drive argv.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GameSpec {
    /// `GameProfileEntry` key — `skyrim_se`, `fo4`, `fnv`, …
    #[serde(default)]
    pub profile: String,
    /// Absolute path to the game's `Data` directory. Empty selects
    /// profile-driven mode.
    #[serde(default)]
    pub data_dir: String,
    /// Main plugin filename inside `data_dir`. Self-contained mode only.
    #[serde(default)]
    pub esm: String,
    /// Ordered master plugin filenames, base game first. Self-contained mode
    /// only; each is joined against `data_dir`, since the cell loader takes
    /// master *paths*.
    #[serde(default)]
    pub masters: Vec<String>,
    #[serde(default, skip_serializing_if = "Archives::is_empty")]
    pub archives: Archives,
}

/// Archive filenames by category, matching `GameProfileEntry`'s five
/// `default_*_bsas` lists one-for-one. Self-contained mode only.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Archives {
    #[serde(default)]
    pub meshes: Vec<String>,
    #[serde(default)]
    pub textures: Vec<String>,
    #[serde(default)]
    pub scripts: Vec<String>,
    #[serde(default)]
    pub sounds: Vec<String>,
    /// BGSM/BGEM containers — FO4 / FO76 / Starfield only.
    #[serde(default)]
    pub materials: Vec<String>,
}

impl Archives {
    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty()
            && self.textures.is_empty()
            && self.scripts.is_empty()
            && self.sounds.is_empty()
            && self.materials.is_empty()
    }
}

/// What the player pressed.
///
/// Internally tagged so the TOML reads as `kind = "cell"` plus that variant's
/// own fields, which keeps one `[action]` table rather than a nested one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// Start at the profile's authored new-game placement.
    ///
    /// Only meaningful in profile-driven mode: the target lives in the
    /// profile's `new_game_worldspace` / `_grid` / `_radius`. A self-contained
    /// request should resolve it to [`Action::Grid`] instead; expansion emits a
    /// note when it cannot.
    NewGame,
    /// Resume a numbered save slot (`save_<slot>.ess`).
    Continue { slot: u32 },
    /// Load one cell by editor ID.
    Cell { edid: String },
    /// Load an exterior grid.
    Grid {
        worldspace: String,
        x: i32,
        y: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radius: Option<u32>,
    },
}

/// Where the shared settings registry file lives.
///
/// The engine reads this file before `VulkanContext::new`, so a launcher that
/// writes it steers renderer setup without the engine knowing a launcher
/// exists. Carried here rather than as an argv flag because the engine resolves
/// it through `BYROREDUX_SETTINGS_PATH`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SettingsSpec {
    #[serde(default)]
    pub path: String,
}

impl SettingsSpec {
    pub fn is_empty(&self) -> bool {
        self.path.trim().is_empty()
    }
}

/// Reserved for P5 (see `docs/engine/launcher.md` §7).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModsSpec {
    #[serde(default)]
    pub load_order: Vec<String>,
}

impl ModsSpec {
    pub fn is_empty(&self) -> bool {
        self.load_order.is_empty()
    }
}

/// Failure modes of reading or writing a request.
#[derive(Debug, thiserror::Error)]
pub enum BootRequestError {
    #[error("could not read boot request {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not write boot request {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("boot request is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("boot request could not be serialised: {0}")]
    Serialize(#[from] toml::ser::Error),
    /// The launcher and engine are from different builds. Named explicitly
    /// rather than folded into `Parse` because the fix is "reinstall", not
    /// "edit the file".
    #[error(
        "boot request declares contract version {found}, but this engine speaks {supported}; \
         the launcher and engine are from different builds"
    )]
    UnsupportedVersion { found: u32, supported: u32 },
}

impl Default for BootRequest {
    fn default() -> Self {
        Self {
            version: CONTRACT_VERSION,
            game: GameSpec::default(),
            action: None,
            settings: SettingsSpec::default(),
            mods: ModsSpec::default(),
        }
    }
}

impl BootRequest {
    /// Profile-driven request for one profile key — the common launcher case.
    pub fn for_profile(profile: impl Into<String>) -> Self {
        Self {
            game: GameSpec {
                profile: profile.into(),
                ..GameSpec::default()
            },
            ..Self::default()
        }
    }

    /// Builder-style action setter, so the launcher can compose in one
    /// expression.
    pub fn with_action(mut self, action: Action) -> Self {
        self.action = Some(action);
        self
    }

    /// True when the request carries its own resolved paths and must not be
    /// re-resolved through the profile registry. See [`GameSpec`].
    pub fn is_self_contained(&self) -> bool {
        !self.game.data_dir.trim().is_empty()
    }

    /// Settings-registry path, if the request names one.
    pub fn settings_path(&self) -> Option<&str> {
        let path = self.settings.path.trim();
        (!path.is_empty()).then_some(path)
    }

    /// Parse from TOML text, refusing an unknown contract version.
    ///
    /// The version is read from a permissive first pass so that a *future*
    /// request — whose other fields this build cannot model — still produces
    /// the version-mismatch error rather than a confusing field-level one.
    pub fn from_toml_str(text: &str) -> Result<Self, BootRequestError> {
        #[derive(Deserialize)]
        struct VersionProbe {
            #[serde(default)]
            version: u32,
        }
        let probe: VersionProbe = toml::from_str(text)?;
        if probe.version != CONTRACT_VERSION {
            return Err(BootRequestError::UnsupportedVersion {
                found: probe.version,
                supported: CONTRACT_VERSION,
            });
        }
        Ok(toml::from_str(text)?)
    }

    /// Serialise to TOML text, always stamping the current contract version.
    pub fn to_toml_string(&self) -> Result<String, BootRequestError> {
        let mut stamped = self.clone();
        stamped.version = CONTRACT_VERSION;
        Ok(toml::to_string_pretty(&stamped)?)
    }

    /// Read a request from disk.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, BootRequestError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| BootRequestError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml_str(&text)
    }

    /// Write a request to disk, creating the parent directory if needed.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), BootRequestError> {
        let path = path.as_ref();
        let text = self.to_toml_string()?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|source| BootRequestError::Write {
                    path: path.to_path_buf(),
                    source,
                })?;
            }
        }
        fs::write(path, text).map_err(|source| BootRequestError::Write {
            path: path.to_path_buf(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> BootRequest {
        BootRequest {
            version: CONTRACT_VERSION,
            game: GameSpec {
                profile: "skyrim_se".into(),
                data_dir: "/games/Skyrim Special Edition/Data".into(),
                esm: "Dawnguard.esm".into(),
                masters: vec!["Skyrim.esm".into(), "Update.esm".into()],
                archives: Archives {
                    meshes: vec!["Skyrim - Meshes0.bsa".into()],
                    textures: vec!["Skyrim - Textures0.bsa".into()],
                    scripts: vec!["Skyrim - Misc.bsa".into()],
                    sounds: vec![],
                    materials: vec![],
                },
            },
            action: Some(Action::Cell {
                edid: "WhiterunBanneredMare".into(),
            }),
            settings: SettingsSpec {
                path: "/home/u/.byroredux/settings.toml".into(),
            },
            mods: ModsSpec::default(),
        }
    }

    #[test]
    fn a_full_request_round_trips_through_toml() {
        let text = sample().to_toml_string().unwrap();
        assert_eq!(BootRequest::from_toml_str(&text).unwrap(), sample());
    }

    /// Every action variant must survive the internally-tagged TOML
    /// representation, including the unit variant and the optional radius —
    /// a variant that silently fails to round-trip would send the player to
    /// the wrong place with no error.
    #[test]
    fn every_action_variant_round_trips() {
        let actions = [
            Action::NewGame,
            Action::Continue { slot: 3 },
            Action::Cell {
                edid: "MegatonPlayerHouse".into(),
            },
            Action::Grid {
                worldspace: "Tamriel".into(),
                x: 5,
                y: -24,
                radius: Some(1),
            },
            Action::Grid {
                worldspace: "WastelandNV".into(),
                x: 0,
                y: 0,
                radius: None,
            },
        ];
        for action in actions {
            let request = BootRequest::for_profile("skyrim_se").with_action(action.clone());
            let text = request.to_toml_string().unwrap();
            let parsed = BootRequest::from_toml_str(&text).unwrap();
            assert_eq!(
                parsed.action,
                Some(action),
                "round trip failed for:\n{text}"
            );
        }
    }

    /// The minimum useful request: one profile key, nothing else.
    #[test]
    fn a_minimal_profile_request_round_trips() {
        let request = BootRequest::for_profile("fnv");
        let text = request.to_toml_string().unwrap();
        assert_eq!(BootRequest::from_toml_str(&text).unwrap(), request);
        assert!(!request.is_self_contained());
        assert_eq!(request.settings_path(), None);
    }

    #[test]
    fn an_unknown_contract_version_is_refused_by_version_not_by_field() {
        // Carries a field this build does not model; the version check must
        // still be what fires, so the user is told to reinstall rather than
        // shown a field-level parse error.
        let text = "version = 99\n[game]\nprofile = \"fnv\"\nunknown_future_field = 7\n";
        match BootRequest::from_toml_str(text) {
            Err(BootRequestError::UnsupportedVersion { found, supported }) => {
                assert_eq!((found, supported), (99, CONTRACT_VERSION));
            }
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
    }

    /// A request with no `version` at all reads as 0, which is still a
    /// mismatch — an unstamped file is not silently treated as current.
    #[test]
    fn a_missing_version_is_a_mismatch_not_a_default() {
        assert!(matches!(
            BootRequest::from_toml_str("[game]\nprofile = \"fnv\"\n"),
            Err(BootRequestError::UnsupportedVersion { found: 0, .. })
        ));
    }

    /// `to_toml_string` stamps the current version regardless of what the
    /// in-memory value held, so a request built by hand cannot write a file
    /// this build would then refuse to read.
    #[test]
    fn serialising_always_stamps_the_current_version() {
        let mut request = BootRequest::for_profile("fo4");
        request.version = 42;
        let text = request.to_toml_string().unwrap();
        assert_eq!(
            BootRequest::from_toml_str(&text).unwrap().version,
            CONTRACT_VERSION
        );
    }

    #[test]
    fn save_then_load_round_trips_through_a_created_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/boot.toml");
        sample().save(&path).unwrap();
        assert_eq!(BootRequest::load(&path).unwrap(), sample());
    }

    #[test]
    fn a_missing_file_reports_the_path() {
        let error = BootRequest::load("/nonexistent/boot.toml").unwrap_err();
        assert!(
            error.to_string().contains("/nonexistent/boot.toml"),
            "error should name the path: {error}"
        );
    }
}
