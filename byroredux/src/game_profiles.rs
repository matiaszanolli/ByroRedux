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

#[derive(Debug, Deserialize)]
struct ProfilesFile {
    #[serde(default)]
    profiles: BTreeMap<String, ProfileEntryDe>,
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
