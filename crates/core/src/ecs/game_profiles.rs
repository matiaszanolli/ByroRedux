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
    /// Compiled Papyrus archives expanded as repeatable `--scripts-bsa`
    /// arguments. Empty for games/launches that do not use the PEX runtime.
    pub default_scripts_bsas: Vec<String>,
    /// Voice and general sound archives expanded as repeatable
    /// `--sounds-bsa` arguments.
    pub default_sounds_bsas: Vec<String>,
    /// Materials archive (BGSM/BGEM container — FO4 / FO76 / SF
    /// only). The binary expands this into `--materials-ba2 <name>`
    /// args when `--game <key>` is used. Empty Vec for Skyrim+ and
    /// older. Phase 20.
    pub default_materials_bsas: Vec<String>,
    /// Whole-mod archives that ship with some *editions* of the game but are
    /// not on every install — Anniversary Edition's `_ResourcePack.bsa` and
    /// the per-Creation-Club `cc*.bsa` set. Expanded present-only: an entry
    /// that is not on disk is skipped silently, because absence is the
    /// normal case rather than a misconfiguration.
    ///
    /// #3924 — separate from [`Self::default_bsas`] for two reasons. Absence
    /// must not warn (a `default_bsas` miss is a broken install and says so),
    /// and these are not category archives: unlike the vanilla
    /// `Skyrim - Meshes0` / `Textures0` split, one `cc*.bsa` carries that
    /// mod's meshes, textures and sounds together, so each entry is expanded
    /// into every content flag rather than just `--bsa`. Measured on a stock
    /// AE install, 2026-09-07: all five carry `meshes\` + `textures\`, and
    /// three also carry `sound\`.
    ///
    /// Appended after the required lists so #3637's last-wins archive
    /// precedence puts add-on content on top of vanilla, which is the
    /// override order the game itself uses.
    ///
    /// The NIF corpus gate models the same tier as
    /// `Game::optional_mesh_archives` (`crates/nif/tests/common/mod.rs`);
    /// `skyrim_optional_archives_match_the_corpus_gate_tier` in
    /// `crates/nif/tests/parse_real_nifs.rs` keeps the two from drifting, so
    /// the gate cannot go on measuring content the engine cannot open.
    pub optional_bsas: Vec<String>,
    /// Optional worldspace/grid boot target used by `--game <key>
    /// --new-game`. Kept in the profile because intro placements differ by
    /// title and edition.
    pub new_game_worldspace: Option<String>,
    pub new_game_grid: Option<String>,
    pub new_game_radius: Option<u32>,
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
