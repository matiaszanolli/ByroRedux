//! Game profile registry resource (Phase 5 of the debug-UI plan).
//!
//! Defines the in-engine shape of one configured game install plus
//! the registry resource the debug-server reads when handling
//! `ListGameProfiles`. The TOML loader that populates this lives
//! in the binary (`byroredux/src/game_profiles.rs`) — core
//! deliberately avoids the `serde` + `toml` deps that file pulls
//! in. Each binary-side `GameProfileEntry` maps 1:1 onto the wire-
//! format `byroredux_debug_protocol::GameProfile`; the debug-
//! server's handler does the conversion.

use std::collections::BTreeMap;
use std::path::Path;

use super::resource::Resource;

/// One configured game install.
#[derive(Debug, Clone, Default)]
pub struct GameProfileEntry {
    pub name: String,
    /// Absolute path to the game's data directory. Empty when
    /// shipped-but-unconfigured (loader hasn't filled it from
    /// either the engine-default file or the per-user override).
    ///
    /// Resolution priority (computed at load time by the binary):
    ///   1. Per-user TOML override sets an absolute path → used as-is.
    ///   2. Shipped TOML carries [`Self::subdir`] (e.g.
    ///      `"Fallout 4/Data"`); root is joined as
    ///      `<games-root>/<subdir>` where `<games-root>` comes
    ///      from `--games-root` CLI / `BYROREDUX_GAMES_ROOT` env
    ///      / `/mnt/data/SteamLibrary/steamapps/common` default.
    ///   3. Neither set → empty, profile is "unconfigured" per
    ///      [`Self::is_usable`].
    pub root: String,
    /// Game-folder subdirectory under the shared games root, e.g.
    /// `"Fallout 4/Data"` for the Steam install. Combined with the
    /// shared `--games-root` at CLI-expansion time to produce
    /// [`Self::root`]. Empty when the profile ships an explicit
    /// absolute `root` instead (per-user override path). Phase 20.
    pub subdir: String,
    pub esm: String,
    pub default_bsas: Vec<String>,
    pub default_textures_bsas: Vec<String>,
    /// Materials archive (BGSM/BGEM container — FO4 / FO76 / SF
    /// only). The binary expands this into `--materials-ba2 <name>`
    /// args when `--game <key>` is used. Empty Vec for Skyrim+ and
    /// older. Phase 20.
    pub default_materials_bsas: Vec<String>,
    pub sample_cells: Vec<String>,
}

impl GameProfileEntry {
    /// True when the profile carries a non-empty root that
    /// actually exists on disk — the UI uses this gate to grey-
    /// out load actions against unconfigured profiles.
    pub fn is_usable(&self) -> bool {
        !self.root.is_empty() && Path::new(&self.root).exists()
    }
}

/// World resource — flat map of `(key, entry)` pairs sorted by key.
/// Inserted at boot by the binary; always present even when the
/// loader produces an empty set (no profile files on disk).
#[derive(Debug, Default)]
pub struct GameProfileRegistry {
    profiles: BTreeMap<String, GameProfileEntry>,
}

impl Resource for GameProfileRegistry {}

impl GameProfileRegistry {
    pub fn new(profiles: BTreeMap<String, GameProfileEntry>) -> Self {
        Self { profiles }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &GameProfileEntry)> {
        self.profiles.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&GameProfileEntry> {
        self.profiles.get(key)
    }
}
