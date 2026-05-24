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
    pub root: String,
    pub esm: String,
    pub default_bsas: Vec<String>,
    pub default_textures_bsas: Vec<String>,
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
