//! MCM Helper's files for the session (LMC-UI-002 slice 4, `docs/engine/mcm-host.md` §5.1): the
//! scripting crate's [`McmHelperStore`] reads a mod's `MCM/Config/<mod>/config.json` and
//! `settings.ini`, and any `MCM/Settings/<mod>.ini` an earlier MCM Helper install left, through the
//! profile's mod roots (MCM Helper reads loose files only), and writes the player's values to the
//! profile's own directory.
//!
//! The profile directory is `<settings.toml's directory>/mcm/<base master>/<profile>/` (owner's decision
//! §6.4): `<profile>` is `--profile-name` (a boot request's `[mods] name`), `default` without one. The
//! base master's stem names the game, so New Vegas and Fallout 3 keep their settings apart.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use byroredux_scripting::mcm::helper::settings::HelperFiles;
use byroredux_scripting::mcm::helper::McmHelperStore;

use crate::asset_provider::LayeredSources;

/// Names the profile whose settings a session reads and writes.
pub(crate) const PROFILE_NAME_FLAG: &str = "--profile-name";

/// The session's MCM Helper files.
struct McmFiles {
    sources: LayeredSources,
    profile_dir: PathBuf,
}

impl McmFiles {
    fn read(&self, path: String) -> Option<Vec<u8>> {
        self.sources.extract(&path.to_ascii_lowercase())
    }

    fn profile_file(&self, mod_name: &str) -> PathBuf {
        self.profile_dir.join(format!("{}.ini", path_safe(mod_name)))
    }
}

impl HelperFiles for McmFiles {
    fn config(&self, mod_name: &str) -> Option<Vec<u8>> {
        self.read(format!("mcm\\config\\{mod_name}\\config.json"))
    }
    fn defaults(&self, mod_name: &str) -> Option<Vec<u8>> {
        self.read(format!("mcm\\config\\{mod_name}\\settings.ini"))
    }
    fn legacy_settings(&self, mod_name: &str) -> Option<Vec<u8>> {
        self.read(format!("mcm\\settings\\{mod_name}.ini"))
    }
    fn profile_settings(&self, mod_name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.profile_file(mod_name)).ok()
    }
    fn write_profile_settings(&self, mod_name: &str, text: &str) -> Result<(), String> {
        let path = self.profile_file(mod_name);
        write_atomically(&path, text).map_err(|error| format!("{}: {error}", path.display()))
    }
}

/// Write `text` beside `path` and rename it over, so a crash never leaves half a file.
fn write_atomically(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension("ini.partial");
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, path)
}

/// A name usable as one path component: separators and characters Windows refuses become `_`.
fn path_safe(name: &str) -> String {
    let safe: String = name
        .trim()
        .chars()
        .map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();
    match safe.as_str() {
        "" | "." | ".." => "_".to_owned(),
        _ => safe,
    }
}

/// The profile's settings directory under `settings_dir`, for the game whose base master is `base_master`.
fn profile_dir(settings_dir: &Path, base_master: &str, profile: Option<&str>) -> PathBuf {
    let game = base_master.rsplit_once('.').map_or(base_master, |(stem, _)| stem).to_ascii_lowercase();
    let profile = profile.map(str::trim).filter(|name| !name.is_empty()).unwrap_or("default");
    settings_dir.join("mcm").join(path_safe(&game)).join(path_safe(profile))
}

/// Install the session's [`McmHelperStore`] over the active load order: its plugins name each config
/// quest's mod, and its base master names the game's settings directory.
pub(crate) fn install(world: &mut byroredux_core::ecs::world::World) {
    let Some(plugins) = world
        .try_resource::<crate::cell_loader::load_order::ActiveLoadOrder>()
        // By file name: a plugin the load order holds by path is keyed by its name, as the identity is.
        .map(|order| {
            order.0.iter().map(|name| Path::new(name).file_name().map_or_else(|| name.clone(), |file| file.to_string_lossy().into_owned())).collect::<Vec<String>>()
        })
    else {
        return;
    };
    let args = crate::cli_args::effective_args();
    let settings = crate::settings_io::SettingsPersistence::discover();
    let settings_dir = settings.path().parent().map(Path::to_path_buf).unwrap_or_default();
    let profile = crate::cli_args::parse_string_arg(&args, PROFILE_NAME_FLAG);
    let files = McmFiles {
        sources: LayeredSources::over(Vec::new(), &args, &["mcm\\"], &[]),
        profile_dir: profile_dir(&settings_dir, plugins.first().map_or("", String::as_str), profile.as_deref()),
    };
    log::debug!("MCM Helper: {} loose tier(s); settings in {}", files.sources.tier_count(), files.profile_dir.display());
    world.insert_resource(McmHelperStore::new(Arc::new(files), &plugins));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_directory_is_per_game_and_named_profile() {
        let root = Path::new("/home/u/.config/byroredux");
        assert_eq!(profile_dir(root, "Skyrim.esm", None), root.join("mcm/skyrim/default"));
        assert_eq!(profile_dir(root, "FalloutNV.esm", Some("Survival")), root.join("mcm/falloutnv/Survival"));
        assert_eq!(profile_dir(root, "Fallout3.esm", Some("  ")), root.join("mcm/fallout3/default"));
        assert_eq!(profile_dir(root, "Skyrim.esm", Some("../evil")), root.join("mcm/skyrim/.._evil"), "no escape from the directory");
    }

    #[test]
    fn path_components_are_made_safe() {
        assert_eq!(path_safe("Frost Fall"), "Frost Fall");
        assert_eq!(path_safe("a/b\\c:d"), "a_b_c_d");
        assert_eq!(path_safe(".."), "_");
        assert_eq!(path_safe(""), "_");
    }

    #[test]
    fn profile_settings_round_trip_through_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let files = McmFiles { sources: LayeredSources::over(Vec::new(), &[], &["mcm\\"], &[]), profile_dir: dir.path().join("mcm/skyrim/default") };
        assert!(files.profile_settings("Frost").is_none());
        files.write_profile_settings("Frost", "[Main]\niLevel=3\n").unwrap();
        assert_eq!(files.profile_settings("Frost").as_deref(), Some(&b"[Main]\niLevel=3\n"[..]));
        assert!(!dir.path().join("mcm/skyrim/default/Frost.ini.partial").exists(), "the partial file was renamed over");
    }
}
