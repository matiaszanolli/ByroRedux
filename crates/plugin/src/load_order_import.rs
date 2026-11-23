//! Import a legacy load order into a mod profile (PRD legacy-mod-compatibility LMC-ID-006, Phase 1 #6670).
//!
//! One shot: a game's `plugins.txt` / `loadorder.txt`, a Mod Organizer 2 profile, or a Vortex deployment becomes the
//! two orders a profile carries — the active plugins in load order, and the mod roots lowest priority first
//! (LMC-AST-010) with the plugins and files each owns (LMC-ID-012) — plus one diagnostic per entry the import could
//! not resolve. Nothing here loads a plugin; activation (LMC-ID-003, -011) runs on the result as on any profile.
//!
//! The rules follow the tools that write these files, never this engine's guesses:
//!
//! - **Plugin lists** — LOOT's libloadorder (`Ortham/libloadorder` at `c9ea87f9ce`, `src/game_settings.rs` and
//!   `src/load_order/*.rs`): Oblivion, Fallout 3 and New Vegas order plugins by file time, masters first
//!   (`timestamp_based.rs` `plugin_sorter`), and `plugins.txt` names the active ones; Skyrim LE orders by
//!   `loadorder.txt` with `plugins.txt` the active set; Skyrim SE, Fallout 4 and Starfield list the order in
//!   `plugins.txt`, a leading `*` marking an active plugin (`asterisk_based.rs` `plugin_line_mapper`). Lines starting
//!   with `#` and empty lines are skipped. The hard-coded early-loading plugins and the `.ccc` Creation Club list load
//!   first and are always active (`early_loading_plugins`); on New Vegas a `.nam` file activates the `.esm`/`.esp` of
//!   its basename (`find_nam_plugins`); on Skyrim LE `Update.esm` is always active.
//! - **Mod Organizer 2** — `ModOrganizer2/modorganizer` at `4ba22f0d8b` (`src/profile.cpp`, `src/settings.cpp`,
//!   `src/modinfoseparator.cpp`) and `modorganizer-game_bethesda` at `a6a2340313`: `modlist.txt` is written highest
//!   priority first (`writeModlistNow` walks the priorities in reverse), `+` enabled, `-` disabled, `*` a foreign mod
//!   (the game's own content); a mod named `*_separator` is a separator; `overwrite` sits above every mod. The mods,
//!   profiles and overwrite directories default to `%BASE_DIR%/mods`, `/profiles` and `/overwrite`, the base being
//!   `[Settings] base_directory` or the directory of `ModOrganizer.ini`. A profile carries `plugins.txt` and, for the
//!   Gamebryo-era games, `loadorder.txt` (`GamebryoGamePlugins::writePluginLists`).
//! - **Vortex** — `vortex.deployment.json` in the game's `Data` (read from a live Fallout 4 deployment, format
//!   `version` 1): `stagingPath`, `targetPath`, and per deployed file its `relPath` and the staging mod (`source`) it
//!   came from. A mod's staging folder is its root; the roots are ordered so that every deployed file's mod outranks
//!   each other staged mod carrying that path.
//!
//! Not modelled, and said so: Fallout 4 / Starfield `sTestFile` INI entries, Starfield's `My Games` `.ccc` and
//! `BlueprintShips-` activation, ghosted (`.ghost`) plugins, and MO2's archive list (`archives.txt`), whose ordering
//! of archives by mod order the engine does not reproduce (LMC-ID-006).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::esm::reader::GameKind;
use crate::esm::strings_table::decode_entry;

/// How a game decides its plugin order (libloadorder `LoadOrderMethod`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOrderMethod {
    /// Oblivion, Fallout 3, New Vegas: file modification time, masters first; `plugins.txt` lists the active plugins.
    Timestamp,
    /// Skyrim LE: `loadorder.txt` is the order, `plugins.txt` the active set.
    Textfile,
    /// Skyrim SE, Fallout 4, Starfield: `plugins.txt` is the order, `*` marks an active plugin.
    Asterisk,
}

/// One game's load-order rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameRules {
    pub method: LoadOrderMethod,
    /// Plugins the game loads first and activates whether or not a list names them, in order.
    pub early: &'static [&'static str],
    /// The Creation Club list, in the game's root directory, whose lines load after `early` and are active.
    pub ccc: Option<&'static str>,
    /// Always active, wherever the order puts them: the base master of the Gamebryo-era games, Skyrim LE's Update.esm.
    pub always_active: &'static [&'static str],
    /// New Vegas: a `.nam` file in `Data` activates the `.esm` / `.esp` of its basename.
    pub nam_activation: bool,
}

const SKYRIM_SE_EARLY: &[&str] = &["Skyrim.esm", "Update.esm", "Dawnguard.esm", "HearthFires.esm", "Dragonborn.esm"];
const FALLOUT4_EARLY: &[&str] = &[
    "Fallout4.esm",
    "DLCRobot.esm",
    "DLCworkshop01.esm",
    "DLCCoast.esm",
    "DLCworkshop02.esm",
    "DLCworkshop03.esm",
    "DLCNukaWorld.esm",
    "DLCUltraHighResolution.esm",
];
const STARFIELD_EARLY: &[&str] = &[
    "Starfield.esm",
    "Constellation.esm",
    "OldMars.esm",
    "ShatteredSpace.esm",
    "SFBGS003.esm",
    "SFBGS004.esm",
    "SFBGS006.esm",
    "SFBGS007.esm",
    "SFBGS008.esm",
    "SFBGS00D.esm",
    "SFBGS047.esm",
    "SFBGS050.esm",
];

/// The rules for the game a base master's header names. `hedr_version` tells Fallout 3 (0.94 / 0.85) from New Vegas
/// (1.34) and `record_version` Skyrim LE (40) from SE (44), as [`GameKind::from_header`] does. `None` for a game whose
/// load order is not imported (Fallout 76).
pub fn rules_for(game: GameKind, hedr_version: f32, record_version: u16) -> Option<GameRules> {
    let timestamp = |base: &'static [&'static str], nam: bool| GameRules {
        method: LoadOrderMethod::Timestamp,
        early: &[],
        ccc: None,
        always_active: base,
        nam_activation: nam,
    };
    Some(match game {
        GameKind::Oblivion => timestamp(&["Oblivion.esm"], false),
        GameKind::Fallout3NV if hedr_version > 1.0 => timestamp(&["FalloutNV.esm"], true),
        GameKind::Fallout3NV => timestamp(&["Fallout3.esm"], false),
        GameKind::Skyrim if record_version < 44 => GameRules {
            method: LoadOrderMethod::Textfile,
            early: &["Skyrim.esm"],
            ccc: None,
            always_active: &["Update.esm"],
            nam_activation: false,
        },
        GameKind::Skyrim => GameRules {
            method: LoadOrderMethod::Asterisk,
            early: SKYRIM_SE_EARLY,
            ccc: Some("Skyrim.ccc"),
            always_active: &[],
            nam_activation: false,
        },
        GameKind::Fallout4 => GameRules {
            method: LoadOrderMethod::Asterisk,
            early: FALLOUT4_EARLY,
            ccc: Some("Fallout4.ccc"),
            always_active: &[],
            nam_activation: false,
        },
        GameKind::Starfield => GameRules {
            method: LoadOrderMethod::Asterisk,
            early: STARFIELD_EARLY,
            ccc: Some("Starfield.ccc"),
            always_active: &[],
            nam_activation: false,
        },
        _ => return None,
    })
}

// ── list files ──────────────────────────────────────────────────────────────

/// One line of a `plugins.txt` / `loadorder.txt`: the plugin name, and whether it carried the `*` of an asterisk-format
/// list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEntry {
    pub name: String,
    pub starred: bool,
}

/// The entries of a plugin list, in file order: `#` lines and empty lines skipped, a leading `*` taken off and kept as
/// [`ListEntry::starred`]. Decoded as UTF-8, else Windows-1252 (the games write the latter; ASCII is both).
pub fn parse_plugin_list(bytes: &[u8]) -> Vec<ListEntry> {
    decode_entry(bytes)
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| match line.strip_prefix('*') {
            Some(name) => ListEntry { name: name.to_owned(), starred: true },
            None => ListEntry { name: line.to_owned(), starred: false },
        })
        .collect()
}

/// A Mod Organizer 2 mod's state in `modlist.txt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModState {
    Enabled,
    Disabled,
    /// `*`: content MO2 does not manage (the game's own DLC and Creation Club files in `Data`).
    Foreign,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModlistEntry {
    pub name: String,
    pub state: ModState,
}

/// The mods of a `modlist.txt`, **lowest priority first** (the file is written highest first). Separators
/// (`*_separator`), `overwrite`, comments and empty lines are left out.
pub fn parse_modlist(text: &str) -> Vec<ModlistEntry> {
    let mut mods: Vec<ModlistEntry> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (state, name) = match line.as_bytes()[0] {
                b'+' => (ModState::Enabled, line[1..].trim()),
                b'-' => (ModState::Disabled, line[1..].trim()),
                b'*' => (ModState::Foreign, line[1..].trim()),
                _ => (ModState::Enabled, line),
            };
            let lower = name.to_lowercase();
            (!name.is_empty() && lower != "overwrite" && !lower.ends_with("_separator"))
                .then(|| ModlistEntry { name: name.to_owned(), state })
        })
        .collect();
    mods.reverse();
    mods
}

/// The directories an MO2 instance keeps its mods and overwrite in, from its `ModOrganizer.ini` (`ini_dir` is the
/// directory the file is in): `[Settings] base_directory`, else `ini_dir`; `mod_directory` and `overwrite_directory`,
/// with `%BASE_DIR%`, else `<base>/mods` and `<base>/overwrite`. Windows paths are mapped through [`wine_path`].
pub fn mo2_directories(ini: &str, ini_dir: &Path) -> (PathBuf, PathBuf, Vec<String>) {
    let mut settings: HashMap<String, String> = HashMap::new();
    let mut section = String::new();
    for line in ini.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            section = name.to_owned();
        } else if section == "Settings"
            && let Some((key, value)) = line.split_once('=')
        {
            settings.insert(key.trim().to_owned(), value.trim().trim_matches('"').replace("\\\\", "\\"));
        }
    }
    let mut notes = Vec::new();
    let mut native = |value: &str| -> PathBuf {
        wine_path(value).unwrap_or_else(|| {
            notes.push(format!("MO2 path {value} has no Linux equivalent (only Z: maps to /); left as written"));
            PathBuf::from(value)
        })
    };
    let base = settings.get("base_directory").map(|value| native(value)).unwrap_or_else(|| ini_dir.to_path_buf());
    let mut configured = |key: &str, default: &str| -> PathBuf {
        match settings.get(key) {
            Some(value) => native(&value.replace("%BASE_DIR%", &base.to_string_lossy())),
            None => base.join(default),
        }
    };
    let mods = configured("mod_directory", "mods");
    let overwrite = configured("overwrite_directory", "overwrite");
    (mods, overwrite, notes)
}

/// A Windows path as Wine / Proton presents it on Linux: `Z:` is the Linux root; a path that is already absolute
/// stays. `None` for another drive letter, whose mapping lives in a Wine prefix this import does not know.
pub fn wine_path(value: &str) -> Option<PathBuf> {
    if value.starts_with('/') {
        return Some(PathBuf::from(value));
    }
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].eq_ignore_ascii_case(&b'z') {
        let rest = value[2..].replace('\\', "/");
        return Some(PathBuf::from(if rest.starts_with('/') { rest } else { format!("/{rest}") }));
    }
    None
}

/// The parts of a `vortex.deployment.json` the import reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VortexDeployment {
    pub staging_path: String,
    pub target_path: String,
    pub deployment_method: String,
    /// `(relPath, source)`: each deployed file, `\`-separated as written, and the staging mod it came from.
    pub files: Vec<(String, String)>,
}

pub fn parse_vortex_deployment(text: &str) -> Result<VortexDeployment> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct File {
        rel_path: String,
        source: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Manifest {
        version: u32,
        staging_path: String,
        #[serde(default)]
        target_path: String,
        #[serde(default)]
        deployment_method: String,
        files: Vec<File>,
    }
    let manifest: Manifest = serde_json::from_str(text).context("parse vortex.deployment.json")?;
    anyhow::ensure!(manifest.version == 1, "vortex.deployment.json version {} (this import reads version 1)", manifest.version);
    Ok(VortexDeployment {
        staging_path: manifest.staging_path,
        target_path: manifest.target_path,
        deployment_method: manifest.deployment_method,
        files: manifest.files.into_iter().map(|file| (file.rel_path, file.source)).collect(),
    })
}

/// Order `mods` (sorted names) lowest priority first so that, for every deployed `(path, winner)`, the winner outranks
/// every other mod that `carries(mod, path)`. Ties keep name order. Returns the order and a note per conflict that
/// cannot be honoured (a cycle; its mods then follow in name order).
pub fn order_by_winners(
    mods: &[String],
    winners: &[(String, String)],
    carries: impl Fn(&str, &str) -> bool,
) -> (Vec<String>, Vec<String>) {
    let index: HashMap<&str, usize> = mods.iter().enumerate().map(|(i, name)| (name.as_str(), i)).collect();
    // below[w] = the mods that must sort below w.
    let mut edges: BTreeMap<usize, HashSet<usize>> = BTreeMap::new();
    for (path, winner) in winners {
        let Some(&w) = index.get(winner.as_str()) else { continue };
        for (loser, name) in mods.iter().enumerate() {
            if loser != w && carries(name, path) {
                edges.entry(w).or_default().insert(loser);
            }
        }
    }
    let mut indegree = vec![0usize; mods.len()]; // count of mods that must sort below
    for (&w, below) in &edges {
        indegree[w] += below.len();
    }
    let mut placed = vec![false; mods.len()];
    let mut order = Vec::with_capacity(mods.len());
    while order.len() < mods.len() {
        let next = (0..mods.len()).find(|&i| !placed[i] && indegree[i] == 0);
        let Some(next) = next else { break };
        placed[next] = true;
        order.push(next);
        for (&w, below) in &edges {
            if below.contains(&next) {
                indegree[w] -= 1;
            }
        }
    }
    let mut notes = Vec::new();
    let stuck: Vec<usize> = (0..mods.len()).filter(|&i| !placed[i]).collect();
    if !stuck.is_empty() {
        notes.push(format!(
            "Vortex: the deployed files of {} disagree on which mod wins (a cycle); they follow in name order",
            stuck.iter().map(|&i| mods[i].as_str()).collect::<Vec<_>>().join(", ")
        ));
        order.extend(stuck);
    }
    (order.into_iter().map(|i| mods[i].clone()).collect(), notes)
}

// ── the import ──────────────────────────────────────────────────────────────

/// What one `--import-load-order` argument is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportSource {
    /// A `plugins.txt`: the active plugins (and, asterisk-format, the order).
    PluginsTxt(PathBuf),
    /// A `loadorder.txt`: the order.
    LoadorderTxt(PathBuf),
    /// An MO2 profile directory (`modlist.txt`, `plugins.txt`, maybe `loadorder.txt`).
    Mo2Profile(PathBuf),
    /// A `vortex.deployment.json`.
    Vortex(PathBuf),
}

impl ImportSource {
    /// Classify `path`: a directory holding `modlist.txt` is an MO2 profile, one holding `vortex.deployment.json` a
    /// Vortex deployment; a file by its name.
    pub fn detect(path: &Path) -> Result<Self> {
        if path.is_dir() {
            if let Some(modlist) = find_ci(path, "modlist.txt") {
                return Ok(Self::Mo2Profile(modlist.parent().unwrap_or(path).to_path_buf()));
            }
            if let Some(manifest) = find_ci(path, "vortex.deployment.json") {
                return Ok(Self::Vortex(manifest));
            }
            anyhow::bail!("{}: a directory with neither modlist.txt (MO2 profile) nor vortex.deployment.json", path.display());
        }
        anyhow::ensure!(path.is_file(), "{}: no such file or directory", path.display());
        let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        Ok(match name.as_str() {
            "plugins.txt" => Self::PluginsTxt(path.to_path_buf()),
            "loadorder.txt" => Self::LoadorderTxt(path.to_path_buf()),
            "vortex.deployment.json" => Self::Vortex(path.to_path_buf()),
            "modlist.txt" => Self::Mo2Profile(path.parent().unwrap_or(Path::new(".")).to_path_buf()),
            _ => anyhow::bail!(
                "{}: not plugins.txt, loadorder.txt, modlist.txt or vortex.deployment.json",
                path.display()
            ),
        })
    }
}

/// The file named `name` in `dir`, any case.
fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|entry| entry.path()).find(|path| {
        path.is_file() && path.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
    })
}

/// A mod root the import found, in priority order (lowest first).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedRoot {
    pub path: PathBuf,
    /// Where it came from: `MO2 mod`, `MO2 overwrite`, `Vortex mod`.
    pub kind: &'static str,
}

/// One plugin of the imported order: its file name as found on disk, and the root that ships it (`None`: `Data`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedPlugin {
    pub name: String,
    pub root: Option<usize>,
}

/// The imported profile: the active plugins in load order, the roots lowest priority first, and one note per entry
/// the import could not resolve or chose to leave out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportedProfile {
    pub plugins: Vec<ImportedPlugin>,
    pub roots: Vec<ImportedRoot>,
    pub notes: Vec<String>,
}

/// Import `sources` for the game whose `Data` directory is `data_dir` (its parent is the game's root, where the
/// `.ccc` list lives). Later sources override earlier ones of the same kind, with a note.
pub fn import(sources: &[ImportSource], data_dir: &Path, rules: &GameRules) -> ImportedProfile {
    let mut out = ImportedProfile::default();
    let mut plugin_list: Option<(PathBuf, Vec<ListEntry>)> = None;
    let mut order_list: Option<(PathBuf, Vec<ListEntry>)> = None;
    let set_list = |slot: &mut Option<(PathBuf, Vec<ListEntry>)>, path: &Path, notes: &mut Vec<String>| {
        match std::fs::read(path) {
            Ok(bytes) => {
                if let Some((earlier, _)) = slot.as_ref() {
                    notes.push(format!("{} replaces {}", path.display(), earlier.display()));
                }
                *slot = Some((path.to_path_buf(), parse_plugin_list(&bytes)));
            }
            Err(error) => notes.push(format!("{}: {error}", path.display())),
        }
    };
    for source in sources {
        match source {
            ImportSource::PluginsTxt(path) => set_list(&mut plugin_list, path, &mut out.notes),
            ImportSource::LoadorderTxt(path) => set_list(&mut order_list, path, &mut out.notes),
            ImportSource::Mo2Profile(dir) => {
                import_mo2(dir, &mut out);
                if let Some(path) = find_ci(dir, "plugins.txt") {
                    set_list(&mut plugin_list, &path, &mut out.notes);
                } else {
                    out.notes.push(format!("MO2 profile {}: no plugins.txt", dir.display()));
                }
                if let Some(path) = find_ci(dir, "loadorder.txt") {
                    set_list(&mut order_list, &path, &mut out.notes);
                }
            }
            ImportSource::Vortex(path) => import_vortex(path, data_dir, &mut out),
        }
    }
    order_plugins(&mut out, data_dir, rules, plugin_list.map(|(_, list)| list), order_list.map(|(_, list)| list));
    out
}

fn import_mo2(profile: &Path, out: &mut ImportedProfile) {
    let modlist = match find_ci(profile, "modlist.txt").map(std::fs::read) {
        Some(Ok(bytes)) => parse_modlist(&decode_entry(&bytes)),
        _ => {
            out.notes.push(format!("MO2 profile {}: modlist.txt cannot be read", profile.display()));
            return;
        }
    };
    // profiles/<name> → the instance base, where ModOrganizer.ini lives in a portable instance.
    let base = profile.parent().and_then(Path::parent).unwrap_or(profile);
    let (mods_dir, overwrite, notes) = match find_ci(base, "ModOrganizer.ini").map(std::fs::read_to_string) {
        Some(Ok(ini)) => mo2_directories(&ini, base),
        _ => {
            out.notes.push(format!(
                "MO2 profile {}: no ModOrganizer.ini at {}; using its default layout (mods/ and overwrite/ beside profiles/)",
                profile.display(),
                base.display()
            ));
            (base.join("mods"), base.join("overwrite"), Vec::new())
        }
    };
    out.notes.extend(notes);
    let (mut disabled, mut foreign) = (0, 0);
    for entry in modlist {
        match entry.state {
            ModState::Disabled => disabled += 1,
            ModState::Foreign => foreign += 1,
            ModState::Enabled => {
                let path = mods_dir.join(&entry.name);
                if path.is_dir() {
                    out.roots.push(ImportedRoot { path, kind: "MO2 mod" });
                } else {
                    out.notes.push(format!("MO2 mod '{}': no directory {}; left out", entry.name, path.display()));
                }
            }
        }
    }
    if disabled > 0 {
        out.notes.push(format!("MO2: {disabled} disabled mod(s) left out"));
    }
    if foreign > 0 {
        out.notes.push(format!("MO2: {foreign} foreign entr(y/ies) (the game's own content) stay in Data"));
    }
    if std::fs::read_dir(&overwrite).is_ok_and(|mut entries| entries.next().is_some()) {
        out.roots.push(ImportedRoot { path: overwrite, kind: "MO2 overwrite" });
    }
    if let Some(Ok(text)) = find_ci(profile, "archives.txt").map(std::fs::read)
        && !parse_plugin_list(&text).is_empty()
    {
        out.notes.push(
            "MO2 archives.txt lists an archive order: archives here load by plugin order and root order, not by \
             that list (LMC-ID-006)"
                .to_owned(),
        );
    }
}

fn import_vortex(manifest_path: &Path, data_dir: &Path, out: &mut ImportedProfile) {
    let deployment = match std::fs::read_to_string(manifest_path)
        .map_err(anyhow::Error::from)
        .and_then(|text| parse_vortex_deployment(&text))
    {
        Ok(deployment) => deployment,
        Err(error) => {
            out.notes.push(format!("{}: {error:#}", manifest_path.display()));
            return;
        }
    };
    if let Some(target) = wine_path(&deployment.target_path)
        && !same_dir(&target, data_dir)
    {
        out.notes.push(format!(
            "Vortex deployed into {}, not this game's Data {}",
            target.display(),
            data_dir.display()
        ));
    }
    let Some(staging) = wine_path(&deployment.staging_path) else {
        out.notes.push(format!(
            "Vortex staging folder {} has no Linux equivalent (only Z: maps to /); its mods stay in Data, unowned",
            deployment.staging_path
        ));
        return;
    };
    let mut sources: Vec<String> = deployment.files.iter().map(|(_, source)| source.clone()).collect();
    sources.sort_by_key(|name| name.to_lowercase());
    sources.dedup();
    let (present, missing): (Vec<String>, Vec<String>) = sources.into_iter().partition(|name| staging.join(name).is_dir());
    for name in &missing {
        let count = deployment.files.iter().filter(|(_, source)| source == name).count();
        out.notes.push(format!(
            "Vortex mod '{name}': no staging folder under {}; its {count} deployed file(s) stay in Data, unowned",
            staging.display()
        ));
    }
    let carries = |name: &str, rel: &str| staging.join(name).join(rel.replace('\\', "/")).is_file();
    let (order, notes) = order_by_winners(&present, &deployment.files, carries);
    out.notes.extend(notes);
    out.roots.extend(order.into_iter().map(|name| ImportedRoot { path: staging.join(name), kind: "Vortex mod" }));
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canonical(a) == canonical(b)
}

/// A plugin file the profile can load: where it is (`None`: `Data`), its name on disk, whether its header carries the
/// master flag, and its modification time.
struct Available {
    root: Option<usize>,
    name: String,
    master: bool,
    mtime: std::time::SystemTime,
}

fn plugin_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ["esm", "esp", "esl"].iter().any(|e| ext.eq_ignore_ascii_case(e)))
        })
        .collect()
}

/// The TES4 record's master flag (`0x1`), read from the first 12 bytes.
fn has_master_flag(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 12];
    std::fs::File::open(path).and_then(|mut file| file.read_exact(&mut head)).is_ok()
        && &head[..4] == b"TES4"
        && u32::from_le_bytes([head[8], head[9], head[10], head[11]]) & 1 == 1
}

fn order_plugins(
    out: &mut ImportedProfile,
    data_dir: &Path,
    rules: &GameRules,
    plugin_list: Option<Vec<ListEntry>>,
    order_list: Option<Vec<ListEntry>>,
) {
    // Every loadable plugin by case-folded name; a higher root shadows a lower one and Data, as a virtual Data would.
    let mut available: HashMap<String, Available> = HashMap::new();
    let dirs = std::iter::once((None, data_dir.to_path_buf()))
        .chain(out.roots.iter().enumerate().map(|(i, root)| (Some(i), root.path.clone())));
    for (root, dir) in dirs {
        for path in plugin_files(&dir) {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            available.insert(name.to_lowercase(), Available { root, name, master: has_master_flag(&path), mtime });
        }
    }
    let key = |name: &str| name.to_lowercase();
    let mut active: HashSet<String> = HashSet::new();
    let mut sequence: Vec<String> = Vec::new();
    let early_present = |names: &mut Vec<String>| {
        let mut early: Vec<String> = rules.early.iter().map(|s| (*s).to_owned()).collect();
        if let Some(ccc) = rules.ccc
            && let Some(root) = data_dir.parent()
            && let Ok(bytes) = std::fs::read(root.join(ccc))
        {
            early.extend(parse_plugin_list(&bytes).into_iter().map(|entry| entry.name));
        }
        names.extend(early.into_iter().filter(|name| available.contains_key(&key(name))));
    };
    match rules.method {
        LoadOrderMethod::Asterisk => {
            early_present(&mut sequence);
            active.extend(sequence.iter().map(|name| key(name)));
            match &plugin_list {
                Some(list) => {
                    for entry in list {
                        sequence.push(entry.name.clone());
                        if entry.starred {
                            active.insert(key(&entry.name));
                        }
                    }
                }
                None => out.notes.push(
                    "no plugins.txt: only the game's early-loading plugins are active (pass the game's or the mod \
                     manager's plugins.txt)"
                        .to_owned(),
                ),
            }
        }
        LoadOrderMethod::Textfile | LoadOrderMethod::Timestamp => {
            early_present(&mut sequence);
            active.extend(sequence.iter().map(|name| key(name)));
            match (&order_list, &plugin_list) {
                (Some(order), _) => sequence.extend(order.iter().map(|entry| entry.name.clone())),
                (None, Some(list)) if rules.method == LoadOrderMethod::Textfile => {
                    out.notes.push("no loadorder.txt: plugins.txt order used".to_owned());
                    sequence.extend(list.iter().map(|entry| entry.name.clone()));
                }
                _ if rules.method == LoadOrderMethod::Timestamp => {
                    // libloadorder `plugin_sorter`: masters first, then file time, a tie broken by name, descending.
                    let mut by_time: Vec<&Available> = available.values().collect();
                    by_time.sort_by(|a, b| {
                        b.master
                            .cmp(&a.master)
                            .then(a.mtime.cmp(&b.mtime))
                            .then_with(|| b.name.to_uppercase().cmp(&a.name.to_uppercase()))
                    });
                    sequence.extend(by_time.into_iter().map(|plugin| plugin.name.clone()));
                }
                _ => out.notes.push("no loadorder.txt or plugins.txt: only the always-active plugins load".to_owned()),
            }
            match &plugin_list {
                Some(list) => active.extend(list.iter().map(|entry| key(&entry.name))),
                None => out.notes.push(
                    "no plugins.txt: only the always-active plugins are active (pass the game's or the mod manager's \
                     plugins.txt)"
                        .to_owned(),
                ),
            }
        }
    }
    for name in rules.always_active {
        active.insert(key(name));
        if !sequence.iter().any(|entry| key(entry) == key(name)) {
            sequence.push((*name).to_owned());
        }
    }
    if rules.nam_activation {
        for path in std::fs::read_dir(data_dir).into_iter().flatten().flatten().map(|entry| entry.path()) {
            if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("nam"))
                && let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned())
            {
                for ext in ["esm", "esp"] {
                    let name = format!("{stem}.{ext}");
                    if available.contains_key(&key(&name)) {
                        active.insert(key(&name));
                        if !sequence.iter().any(|entry| key(entry) == key(&name)) {
                            sequence.push(name);
                        }
                    }
                }
            }
        }
    }
    // An active plugin the order list leaves out still loads (a stale loadorder.txt): after the listed ones.
    if let Some(list) = &plugin_list {
        for entry in list {
            if !sequence.iter().any(|name| key(name) == key(&entry.name)) {
                if available.contains_key(&key(&entry.name)) {
                    out.notes.push(format!("{}: active but not in the load order list; loaded last", entry.name));
                }
                sequence.push(entry.name.clone());
            }
        }
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut inactive = 0usize;
    for name in sequence {
        let k = key(&name);
        if !seen.insert(k.clone()) {
            continue;
        }
        if !active.contains(&k) {
            inactive += 1;
            continue;
        }
        match available.get(&k) {
            Some(plugin) => out.plugins.push(ImportedPlugin { name: plugin.name.clone(), root: plugin.root }),
            None => out.notes.push(format!("{name}: active, but in neither Data nor any root; left out")),
        }
    }
    if inactive > 0 {
        out.notes.push(format!("{inactive} inactive plugin(s) left out"));
    }
}

#[cfg(test)]
#[path = "load_order_import_tests.rs"]
mod tests;
