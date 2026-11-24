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
    fn keybinds(&self, mod_name: &str) -> Option<Vec<u8>> {
        self.read(format!("mcm\\config\\{mod_name}\\keybinds.json"))
    }
    fn legacy_registrations(&self) -> Option<Vec<u8>> {
        self.read("mcm\\settings\\keybinds.json".to_owned())
    }
    fn profile_registrations(&self) -> Option<Vec<u8>> {
        std::fs::read(self.profile_dir.join(BINDINGS_FILE)).ok()
    }
    fn write_profile_registrations(&self, text: &str) -> Result<(), String> {
        let path = self.profile_dir.join(BINDINGS_FILE);
        write_atomically(&path, text).map_err(|error| format!("{}: {error}", path.display()))
    }
}

/// The profile's hotkey bindings (slice 4b), beside its mods' `<mod>.ini` files.
const BINDINGS_FILE: &str = "keybinds.json";

/// Write `text` beside `path` and rename it over, so a crash never leaves half a file.
fn write_atomically(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let extension = path.extension().map_or(String::new(), |ext| ext.to_string_lossy().into_owned());
    let partial = path.with_extension(format!("{extension}.partial"));
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

/// The profile's directory for `area` (`mcm`, `moddata`) under `settings_dir`, for the game whose base master is
/// `base_master`.
fn profile_dir(settings_dir: &Path, area: &str, base_master: &str, profile: Option<&str>) -> PathBuf {
    let game = base_master.rsplit_once('.').map_or(base_master, |(stem, _)| stem).to_ascii_lowercase();
    let profile = profile.map(str::trim).filter(|name| !name.is_empty()).unwrap_or("default");
    settings_dir.join(area).join(path_safe(&game)).join(path_safe(profile))
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
        profile_dir: profile_dir(&settings_dir, "mcm", plugins.first().map_or("", String::as_str), profile.as_deref()),
    };
    log::debug!("MCM: {} loose tier(s); settings in {}", files.sources.tier_count(), files.profile_dir.display());
    // Fallout 4 MCM's menus are folders, not scripts (`docs/engine/mcm-host.md` §5.2): the configs under the
    // mod roots, by folder name.
    let fallout4 = plugins.first().is_some_and(|base| base.eq_ignore_ascii_case("Fallout4.esm"));
    let configs = if fallout4 { config_folders(&files.sources.list("mcm\\config\\")) } else { Vec::new() };
    let store = McmHelperStore::new(Arc::new(files), &plugins);
    let store = if fallout4 {
        log::info!("FO4 MCM: {} config(s) under the mod roots", configs.len());
        store.for_fallout4(configs)
    } else {
        store
    };
    world.insert_resource(store);
}

/// The mod data files PapyrusUtil's `JsonUtil` and JContainers read and write (LMC PRD Q5,
/// [`byroredux_scripting::json_util`]): the profile's own copy under `<settings.toml's directory>/moddata/<base
/// master>/<profile>/`, by its path under `Data/`, over a mod root's shipped file (loose files, as both read them).
struct JsonFiles {
    sources: LayeredSources,
    data_dir: PathBuf,
}

impl JsonFiles {
    /// The profile's copy of a resolved path (lowercase, `/`-separated, never above `Data/`).
    fn profile_path(&self, path: &str) -> PathBuf {
        path.split('/').fold(self.data_dir.clone(), |dir, part| dir.join(part))
    }
}

impl byroredux_scripting::json_util::JsonUtilFiles for JsonFiles {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        std::fs::read(self.profile_path(path)).ok().or_else(|| self.sources.extract(&path.replace('/', "\\")))
    }
    fn write(&self, path: &str, text: &str) -> Result<(), String> {
        let target = self.profile_path(path);
        write_atomically(&target, text).map_err(|error| format!("{}: {error}", target.display()))
    }
    fn list(&self, folder: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.profile_path(folder))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .chain(
                self.sources
                    .list(&format!("{}\\", folder.replace('/', "\\")))
                    .iter()
                    .filter_map(|path| path.rsplit_once('\\').map(|(dir, name)| (dir.to_owned(), name.to_owned())))
                    .filter(|(dir, _)| dir.eq_ignore_ascii_case(&folder.replace('/', "\\")))
                    .map(|(_, name)| name),
            )
            .filter(|name| name.to_ascii_lowercase().ends_with(".json"))
            .map(|name| name.to_ascii_lowercase())
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }
}

/// Install the session's PapyrusUtil `JsonUtil` store (LMC PRD Q5) for the active load order's game and the
/// session's profile.
pub(crate) fn install_json_util(world: &mut byroredux_core::ecs::world::World) {
    let Some(base_master) = world
        .try_resource::<crate::cell_loader::load_order::ActiveLoadOrder>()
        .and_then(|order| order.0.first().map(|name| Path::new(name).file_name().map_or_else(|| name.clone(), |file| file.to_string_lossy().into_owned())))
    else {
        return;
    };
    let args = crate::cli_args::effective_args();
    let settings = crate::settings_io::SettingsPersistence::discover();
    let settings_dir = settings.path().parent().map(Path::to_path_buf).unwrap_or_default();
    let profile = crate::cli_args::parse_string_arg(&args, PROFILE_NAME_FLAG);
    let files = JsonFiles {
        // Any loose tier: JContainers reads files anywhere under `Data/`.
        sources: LayeredSources::over(Vec::new(), &args, &[""], &[]),
        data_dir: profile_dir(&settings_dir, "moddata", &base_master, profile.as_deref()),
    };
    log::debug!("JsonUtil: {} loose tier(s); files in {}", files.sources.tier_count(), files.data_dir.display());
    world.insert_resource(byroredux_scripting::json_util::JsonUtilStore::new(Arc::new(files)));
}

/// The folders under `mcm\config\` that hold a `config.json`, from the listed paths.
fn config_folders(paths: &[String]) -> Vec<String> {
    let mut folders: Vec<String> = paths
        .iter()
        .filter_map(|path| path.strip_prefix("mcm\\config\\")?.strip_suffix("\\config.json"))
        .filter(|folder| !folder.is_empty() && !folder.contains('\\'))
        .map(str::to_owned)
        .collect();
    folders.sort_unstable();
    folders.dedup();
    folders
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_directory_is_per_game_and_named_profile() {
        let root = Path::new("/home/u/.config/byroredux");
        assert_eq!(profile_dir(root, "mcm", "Skyrim.esm", None), root.join("mcm/skyrim/default"));
        assert_eq!(profile_dir(root, "mcm", "FalloutNV.esm", Some("Survival")), root.join("mcm/falloutnv/Survival"));
        assert_eq!(profile_dir(root, "mcm", "Fallout3.esm", Some("  ")), root.join("mcm/fallout3/default"));
        assert_eq!(profile_dir(root, "mcm", "Skyrim.esm", Some("../evil")), root.join("mcm/skyrim/.._evil"), "no escape from the directory");
    }

    /// LMC Q5 — JsonUtil's files live in the profile's own `moddata` directory, under `skse/plugins/`, by the
    /// resolved path; a written file reads back and lists with its folder.
    #[test]
    fn json_util_files_write_and_list_in_the_profile_directory() {
        use byroredux_scripting::json_util::{resolve, resolve_folder, JsonUtilFiles};
        let root = std::env::temp_dir().join(format!("byro-jsonutil-{}", std::process::id()));
        let data_dir = profile_dir(&root, "moddata", "Skyrim.esm", Some("Main"));
        assert_eq!(data_dir, root.join("moddata/skyrim/Main"));
        let plugins_dir = data_dir.join("skse").join("plugins");
        let files = JsonFiles { sources: LayeredSources::over(Vec::new(), &[], &[""], &[]), data_dir };
        let path = resolve("MyMod/Settings").unwrap();
        assert!(files.read(&path).is_none());
        files.write(&path, "{\"int\":{\"a\":1}}").unwrap();
        assert_eq!(files.read(&path).as_deref(), Some(&b"{\"int\":{\"a\":1}}"[..]));
        assert!(plugins_dir.join("storageutildata/mymod/settings.json").is_file());
        assert_eq!(files.list(&resolve_folder("MyMod").unwrap()), vec!["settings.json".to_owned()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn config_folders_are_the_ones_holding_a_config() {
        let paths = [
            "mcm\\config\\frost\\config.json",
            "mcm\\config\\frost\\settings.ini",
            "mcm\\config\\onlyini\\settings.ini",
            "mcm\\config\\deep\\nested\\config.json",
            "mcm\\config\\config.json",
            "mcm\\config\\ash\\config.json",
        ]
        .map(str::to_owned);
        assert_eq!(config_folders(&paths), ["ash", "frost"]);
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
        assert!(files.profile_registrations().is_none());
        files.write_profile_registrations("{}\n").unwrap();
        assert_eq!(files.profile_registrations().as_deref(), Some(&b"{}\n"[..]));
        assert!(dir.path().join("mcm/skyrim/default/keybinds.json").is_file());
    }
}
