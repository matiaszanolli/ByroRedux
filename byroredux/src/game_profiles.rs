//! TOML loader for the [`GameProfileRegistry`] (Phase 5 of the
//! debug-UI plan).
//!
//! The registry resource itself lives in core (so the debug-server
//! can read it without depending on this crate); this module just
//! parses `assets/debug_profiles.toml` + the per-user override
//! `~/.byroredux/profiles.toml` into core's `GameProfileEntry`
//! values. Both files missing = empty registry, never an error.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use byroredux_core::ecs::{GameProfileEntry, GameProfileRegistry};
use serde::Deserialize;

/// Path inside the engine source tree where the default profile
/// file lives. The loader tries the per-user override after the
/// shipped file so user-edited entries replace defaults with the
/// same key.
pub const DEFAULT_PROFILES_PATH: &str = "assets/debug_profiles.toml";

/// Default shared games root when neither `--games-root` CLI nor
/// `BYROREDUX_GAMES_ROOT` env var is set. Tuned for Linux Steam
/// installs — the most common dev box layout. Phase 20.
pub const DEFAULT_GAMES_ROOT: &str = "/mnt/data/SteamLibrary/steamapps/common";

/// Resolve the shared games root, in priority order:
///   1. `--games-root <path>` CLI argument (caller supplies via `cli_arg`)
///   2. `BYROREDUX_GAMES_ROOT` environment variable
///   3. [`DEFAULT_GAMES_ROOT`]
///
/// Used by `--game <key>` CLI expansion to turn a profile's
/// `subdir` into an absolute path. Phase 20.
pub fn resolve_games_root(cli_arg: Option<&str>) -> PathBuf {
    if let Some(path) = cli_arg {
        return PathBuf::from(path);
    }
    if let Ok(env) = std::env::var("BYROREDUX_GAMES_ROOT") {
        if !env.is_empty() {
            return PathBuf::from(env);
        }
    }
    PathBuf::from(DEFAULT_GAMES_ROOT)
}

/// Compose a profile's data directory from `games_root` + the
/// profile's `subdir` field. Used at `--game <key>` expansion
/// time when the profile's `root` is empty. Returns the explicit
/// `entry.root` unchanged when set (per-user override path). Phase 20.
pub fn resolve_profile_root(entry: &GameProfileEntry, games_root: &Path) -> PathBuf {
    if !entry.root.is_empty() {
        return PathBuf::from(&entry.root);
    }
    if !entry.subdir.is_empty() {
        return games_root.join(&entry.subdir);
    }
    PathBuf::new()
}

/// Serde landing type for one profile block — same shape as
/// [`GameProfileEntry`] but with serde derives. The loader maps
/// this onto the core type so the protocol crate's wire format
/// and the in-engine resource share one source of truth (the
/// core type), without dragging serde into core.
#[derive(Debug, Deserialize)]
struct ProfileEntryDe {
    name: String,
    #[serde(default)]
    root: String,
    /// Game folder under the shared games root (e.g. `Fallout 4/Data`).
    /// Combined with `BYROREDUX_GAMES_ROOT` / `--games-root` at
    /// load time when `root` is empty. Phase 20.
    #[serde(default)]
    subdir: String,
    esm: String,
    #[serde(default)]
    default_bsas: Vec<String>,
    #[serde(default)]
    default_textures_bsas: Vec<String>,
    #[serde(default)]
    default_materials_bsas: Vec<String>,
    #[serde(default)]
    sample_cells: Vec<String>,
}

impl From<ProfileEntryDe> for GameProfileEntry {
    fn from(de: ProfileEntryDe) -> Self {
        Self {
            name: de.name,
            root: de.root,
            subdir: de.subdir,
            esm: de.esm,
            default_bsas: de.default_bsas,
            default_textures_bsas: de.default_textures_bsas,
            default_materials_bsas: de.default_materials_bsas,
            sample_cells: de.sample_cells,
        }
    }
}

/// Serde landing type for the optional `[defaults]` table — the
/// launch defaults that let `cargo run` (and `--bench-hold`) boot
/// straight into a game/cell with no CLI flags. Every field is
/// optional; an absent table or field falls back to the engine's
/// built-in behaviour (the spinning-cube demo when no game is set).
#[derive(Debug, Default, Deserialize)]
struct DefaultsDe {
    /// Profile key (a `[profiles.<key>]` block) loaded when no
    /// `--game` / `--esm` / `--mesh` / `--cmd` flag is given.
    #[serde(default)]
    game: Option<String>,
    /// Cell editor ID loaded when a profile is resolved and no
    /// `--cell` / `--grid` / `--wrld` flag is given.
    #[serde(default)]
    cell: Option<String>,
    /// Shared games root override (same role as `--games-root` /
    /// `BYROREDUX_GAMES_ROOT`). Lets a non-Steam install point the
    /// whole registry at one folder from the config file.
    #[serde(default)]
    games_root: Option<String>,
    /// REND-#1451 — initial point/spot attenuation knee fraction for
    /// the `LightTuning` resource, so a benched value persists across
    /// runs without a `light.atten` command each session.
    #[serde(default)]
    light_atten_knee: Option<f32>,
    /// REND-#1451 — start with the legacy window-only attenuation.
    #[serde(default)]
    light_atten_legacy: Option<bool>,
}

/// Resolved launch defaults (the `[defaults]` table), merged across
/// the shipped file and the per-user override (user fields win when
/// present). All fields optional — callers apply only what is set.
#[derive(Debug, Default, Clone)]
pub struct LaunchDefaults {
    pub game: Option<String>,
    pub cell: Option<String>,
    pub games_root: Option<String>,
    pub light_atten_knee: Option<f32>,
    pub light_atten_legacy: Option<bool>,
}

impl LaunchDefaults {
    /// Overlay `other` onto `self`: every `Some` field in `other`
    /// replaces `self`'s value. Used to let the per-user override's
    /// `[defaults]` win over the shipped file's, field by field.
    fn overlay(&mut self, other: DefaultsDe) {
        if other.game.is_some() {
            self.game = other.game;
        }
        if other.cell.is_some() {
            self.cell = other.cell;
        }
        if other.games_root.is_some() {
            self.games_root = other.games_root;
        }
        if other.light_atten_knee.is_some() {
            self.light_atten_knee = other.light_atten_knee;
        }
        if other.light_atten_legacy.is_some() {
            self.light_atten_legacy = other.light_atten_legacy;
        }
    }
}

#[derive(Debug, Deserialize)]
struct ProfilesFile {
    #[serde(default)]
    profiles: BTreeMap<String, ProfileEntryDe>,
    #[serde(default)]
    defaults: DefaultsDe,
}

/// Load profiles using the engine-default + user-override merge
/// rule. User-override entries replace shipped entries with the
/// same key; keys appearing in only one file pass through.
pub fn load_default() -> GameProfileRegistry {
    let mut out: BTreeMap<String, GameProfileEntry> = BTreeMap::new();

    // Shipped defaults: try CWD-relative first (cargo run from
    // repo root), then binary-parent-relative (release ships).
    for shipped in [
        PathBuf::from(DEFAULT_PROFILES_PATH),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join(DEFAULT_PROFILES_PATH)))
            .unwrap_or_default(),
    ] {
        if shipped.exists() {
            merge_from(&shipped, &mut out);
            break;
        }
    }

    // Per-user override.
    if let Some(home) = home_dir() {
        let user_path = home.join(".byroredux").join("profiles.toml");
        if user_path.exists() {
            merge_from(&user_path, &mut out);
        }
    }

    // Expand `~/...` prefixes on roots so the registry's consumers
    // get a ready-to-use absolute path.
    for entry in out.values_mut() {
        if let Some(stripped) = entry.root.strip_prefix("~/") {
            if let Some(home) = home_dir() {
                entry.root = home.join(stripped).to_string_lossy().into_owned();
            }
        }
    }

    GameProfileRegistry::new(out)
}

fn merge_from(path: &Path, out: &mut BTreeMap<String, GameProfileEntry>) {
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("debug profiles: failed to read {}: {}", path.display(), e);
            return;
        }
    };
    let parsed: ProfilesFile = match toml::from_str(&contents) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("debug profiles: failed to parse {}: {}", path.display(), e);
            return;
        }
    };
    let added = parsed.profiles.len();
    for (k, v) in parsed.profiles {
        out.insert(k, GameProfileEntry::from(v));
    }
    log::info!(
        "debug profiles: loaded {} entries from {}",
        added,
        path.display(),
    );
}

/// The ordered config files to read, shipped-first then per-user
/// override: `[assets/debug_profiles.toml (CWD or exe-parent),
/// ~/.byroredux/profiles.toml]`, filtered to those that exist. Both
/// `load_default` (profiles) and [`load_launch_defaults`] consume this
/// so the two stay in lockstep on which files contribute.
fn ordered_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for shipped in [
        PathBuf::from(DEFAULT_PROFILES_PATH),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join(DEFAULT_PROFILES_PATH)))
            .unwrap_or_default(),
    ] {
        if shipped.exists() {
            paths.push(shipped);
            break;
        }
    }
    if let Some(home) = home_dir() {
        let user_path = home.join(".byroredux").join("profiles.toml");
        if user_path.exists() {
            paths.push(user_path);
        }
    }
    paths
}

/// Load the merged `[defaults]` launch table from the shipped file +
/// per-user override (user fields win, field by field). Missing files
/// / missing table → all-`None` defaults (engine keeps its built-in
/// behaviour). Parse failures log a warning and are skipped — never an
/// error, so a typo in the config can't brick the launch.
pub fn load_launch_defaults() -> LaunchDefaults {
    let mut out = LaunchDefaults::default();
    for path in ordered_config_paths() {
        let contents = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("launch defaults: failed to read {}: {}", path.display(), e);
                continue;
            }
        };
        match toml::from_str::<ProfilesFile>(&contents) {
            Ok(parsed) => out.overlay(parsed.defaults),
            Err(e) => {
                log::warn!("launch defaults: failed to parse {}: {}", path.display(), e);
            }
        }
    }
    // Expand a leading `~/` on games_root so callers get an absolute
    // path (mirrors the profile-root expansion in `load_default`).
    if let Some(root) = out.games_root.as_ref().and_then(|r| r.strip_prefix("~/")) {
        if let Some(home) = home_dir() {
            out.games_root = Some(home.join(root).to_string_lossy().into_owned());
        }
    }
    out
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_in_memory_toml() {
        let toml = r#"
[profiles.fnv]
name = "FNV"
root = "/tmp/nonexistent-path-for-test"
esm = "FalloutNV.esm"
default_bsas = ["Fallout - Meshes.bsa"]
default_textures_bsas = []
sample_cells = ["GSDocMitchellHouse"]
        "#;
        let parsed: ProfilesFile = toml::from_str(toml).expect("parse");
        assert_eq!(parsed.profiles.len(), 1);
        let p = &parsed.profiles["fnv"];
        assert_eq!(p.name, "FNV");
        assert_eq!(p.default_bsas, vec!["Fallout - Meshes.bsa"]);
        // is_usable lives on core's GameProfileEntry — round-
        // trip the converted entry to exercise it.
        let entry: GameProfileEntry = ProfileEntryDe {
            name: p.name.clone(),
            root: p.root.clone(),
            subdir: p.subdir.clone(),
            esm: p.esm.clone(),
            default_bsas: p.default_bsas.clone(),
            default_textures_bsas: p.default_textures_bsas.clone(),
            default_materials_bsas: p.default_materials_bsas.clone(),
            sample_cells: p.sample_cells.clone(),
        }
        .into();
        assert!(!entry.is_usable());
    }

    #[test]
    fn parses_defaults_table() {
        let toml = r#"
[defaults]
game = "fnv"
cell = "GSProspectorSaloonInterior"
light_atten_knee = 0.4

[profiles.fnv]
name = "FNV"
esm = "FalloutNV.esm"
        "#;
        let parsed: ProfilesFile = toml::from_str(toml).expect("parse");
        assert_eq!(parsed.defaults.game.as_deref(), Some("fnv"));
        assert_eq!(
            parsed.defaults.cell.as_deref(),
            Some("GSProspectorSaloonInterior")
        );
        assert_eq!(parsed.defaults.light_atten_knee, Some(0.4));
        assert_eq!(parsed.defaults.games_root, None);
    }

    #[test]
    fn missing_defaults_table_is_all_none() {
        let toml = "[profiles.fnv]\nname = \"FNV\"\nesm = \"FalloutNV.esm\"\n";
        let parsed: ProfilesFile = toml::from_str(toml).expect("parse");
        assert!(parsed.defaults.game.is_none());
        assert!(parsed.defaults.cell.is_none());
        assert!(parsed.defaults.light_atten_knee.is_none());
    }

    #[test]
    fn launch_defaults_overlay_user_wins_field_by_field() {
        // Shipped sets game+cell; user overrides only cell + adds knee.
        let mut acc = LaunchDefaults::default();
        let shipped: ProfilesFile =
            toml::from_str("[defaults]\ngame = \"fnv\"\ncell = \"ShippedCell\"\n").unwrap();
        acc.overlay(shipped.defaults);
        let user: ProfilesFile =
            toml::from_str("[defaults]\ncell = \"UserCell\"\nlight_atten_knee = 0.35\n").unwrap();
        acc.overlay(user.defaults);

        assert_eq!(
            acc.game.as_deref(),
            Some("fnv"),
            "shipped game survives (user didn't set it)"
        );
        assert_eq!(acc.cell.as_deref(), Some("UserCell"), "user cell wins");
        assert_eq!(acc.light_atten_knee, Some(0.35), "user-only field applies");
    }

    #[test]
    fn user_override_replaces_shipped_entry() {
        let mut out = BTreeMap::new();
        let mut shipped = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            shipped,
            "[profiles.fnv]\nname = \"shipped FNV\"\nroot = \"\"\nesm = \"FalloutNV.esm\"\n"
        )
        .unwrap();
        merge_from(shipped.path(), &mut out);
        assert_eq!(out["fnv"].name, "shipped FNV");

        let mut user = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            user,
            "[profiles.fnv]\nname = \"user FNV\"\nroot = \"/games/fnv\"\nesm = \"FalloutNV.esm\"\n"
        )
        .unwrap();
        merge_from(user.path(), &mut out);
        assert_eq!(out["fnv"].name, "user FNV");
        assert_eq!(out["fnv"].root, "/games/fnv");
    }
}
