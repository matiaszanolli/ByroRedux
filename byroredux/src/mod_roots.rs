//! The session's mod roots (PRD legacy-mod-compatibility LMC-ID-012, LMC-AST-010, LMC-ID-011; #6670).
//!
//! A *mod root* is one mod's directory as a mod manager deploys it: the plugins and archives at its top level and the
//! loose files under it (`textures\`, `meshes\`, ...). The profile names its roots with `--mod-root <dir>`, repeated,
//! lowest priority first. The directory the base master lives in, the game's `Data` directory, is the implicit
//! *unmanaged* root and sorts first. A root's identity is its directory name, case-folded (no root carries a manifest
//! yet). A directory listed twice is one root, kept at its first place; two different directories with one identity
//! are an identity collision, reported, and both load (LMC-ID-010).
//!
//! - **Plugins.** A `--master` or `--esm` given as a bare file name that names no file is looked up in the roots, the
//!   highest-priority root first and the unmanaged root last, as a mod manager's virtual `Data` resolves it.
//! - **Loose files (LMC-AST-010).** The roots' loose files are layered in root order, a later root's file winning the
//!   path, and the whole loose tier stays above every archive (the asset providers read the order back through
//!   [`crate::asset_provider::loose_dirs`]).
//! - **Disabled roots (LMC-ID-011).** A root whose every plugin in the profile activation disabled is disabled with
//!   them: its loose files leave the session, since they usually reference the missing forms. A root that keeps a
//!   loadable plugin keeps its assets, with a warning; a root that ships no plugin of the profile is never affected.
//!
//! What each root owns is in the activation report ([`crate::plugin_activation`]); its loose files are counted only when
//! a report is printed, from the same cached index the providers open.

use std::path::{Path, PathBuf};

use byroredux_core::form_id::plugin_key;

/// The flag that names a mod root, repeated in priority order.
pub(crate) const MOD_ROOT_FLAG: &str = "--mod-root";
/// The flag activation writes for each root whose loose files the session layers, in order, the unmanaged root first.
pub(crate) const LOOSE_ROOT_FLAG: &str = "--loose-root";

/// One root of the profile and what activation decided about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModRoot {
    /// The directory name, case-folded; `unmanaged` for the game's `Data` directory.
    pub(crate) identity: String,
    pub(crate) path: PathBuf,
    /// The game's `Data` directory, which sorts first and is never disabled.
    pub(crate) unmanaged: bool,
    /// The plugin file names at the root's top level, sorted.
    pub(crate) plugins: Vec<String>,
    /// The archive file names at the root's top level, sorted.
    pub(crate) archives: Vec<String>,
    /// The profile's plugins this root ships, as the profile names them, with whether activation kept each.
    pub(crate) profile_plugins: Vec<(String, bool)>,
    /// Activation disabled every plugin of the profile this root ships (LMC-ID-011).
    pub(crate) disabled: bool,
}

impl ModRoot {
    /// What the report calls the root: its identity, or `unmanaged (<path>)`.
    pub(crate) fn label(&self) -> String {
        if self.unmanaged { format!("unmanaged ({})", self.path.display()) } else { self.identity.clone() }
    }
}

/// The `--mod-root` directories `args` names, in order.
pub(crate) fn mod_root_args(args: &[String]) -> Vec<PathBuf> {
    crate::cli_args::parse_all_string_args(args, MOD_ROOT_FLAG).into_iter().map(PathBuf::from).collect()
}

/// The file names at the top level of `dir` whose extension is one of `extensions` (lowercase, without the dot),
/// sorted; empty when it cannot be read.
fn top_level_files(dir: &Path, extensions: &[&str]) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| {
            let lower = name.to_lowercase();
            extensions.iter().any(|extension| lower.rsplit_once('.').is_some_and(|(_, ext)| ext == *extension))
        })
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names
}

fn root(path: PathBuf, unmanaged: bool) -> ModRoot {
    ModRoot {
        identity: if unmanaged { "unmanaged".to_owned() } else { plugin_key(&path.to_string_lossy()) },
        plugins: top_level_files(&path, &["esm", "esp", "esl"]),
        archives: top_level_files(&path, &["bsa", "ba2"]),
        path,
        unmanaged,
        profile_plugins: Vec::new(),
        disabled: false,
    }
}

/// The profile's roots: `unmanaged` (the base master's directory) first, then each of `given` that is a directory and
/// not already a root, in order; and one report line per root left out and per identity two directories share.
pub(crate) fn discover(unmanaged: Option<&Path>, given: &[PathBuf]) -> (Vec<ModRoot>, Vec<String>) {
    let mut roots: Vec<ModRoot> = unmanaged.map(|path| root(path.to_path_buf(), true)).into_iter().collect();
    let mut lines = Vec::new();
    for path in given {
        // A trailing separator would leave the identity empty.
        let path: PathBuf = path.components().collect();
        if !path.is_dir() {
            lines.push(format!("mod root {}: not a directory, left out", path.display()));
            continue;
        }
        let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some(earlier) = roots.iter().find(|earlier| canonical(&earlier.path) == canonical(&path)) {
            lines.push(format!(
                "mod root {}: the same directory as the earlier root {}, left out (it is one root)",
                path.display(),
                earlier.label()
            ));
            continue;
        }
        let added = root(path, false);
        if let Some(earlier) = roots.iter().find(|earlier| !earlier.unmanaged && earlier.identity == added.identity) {
            lines.push(format!(
                "identity collision (LMC-ID-010): mod roots {} and {} are different directories with the identity '{}'; \
                 both load, the later winning their loose files",
                earlier.path.display(),
                added.path.display(),
                added.identity
            ));
        }
        roots.push(added);
    }
    (roots, lines)
}

/// The path a profile plugin named `value` resolves to: `value` itself when it names a file or a path, else the plugin
/// of that name in the highest-priority root that ships one (the unmanaged root last), else `value` unchanged, for
/// activation to report as unreadable.
pub(crate) fn resolve_plugin(value: &str, roots: &[ModRoot]) -> String {
    if Path::new(value).is_file() || value.contains(['/', '\\']) {
        return value.to_owned();
    }
    let key = plugin_key(value);
    roots
        .iter()
        .rev()
        .find_map(|root| {
            let name = root.plugins.iter().find(|name| plugin_key(name) == key)?;
            Some(root.path.join(name).to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| value.to_owned())
}

/// The root `plugin` (a path) lies at the top level of, by canonical directory; `None` for a plugin outside every root.
fn owner_of(plugin: &str, roots: &[ModRoot]) -> Option<usize> {
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = match Path::new(plugin).parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let dir = canonical(&dir);
    roots.iter().position(|root| canonical(&root.path) == dir)
}

/// Record which root ships each plugin of `profile` (its paths, with whether activation kept each) and disable the
/// roots whose every such plugin activation disabled (LMC-ID-011); one report line per disabled root and per root that
/// keeps its assets beside a disabled plugin.
pub(crate) fn apply_activation(roots: &mut [ModRoot], profile: &[(String, bool)]) -> Vec<String> {
    for (plugin, kept) in profile {
        if let Some(owner) = owner_of(plugin, roots) {
            roots[owner].profile_plugins.push((plugin.clone(), *kept));
        }
    }
    let name = |path: &String| path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned();
    let mut lines = Vec::new();
    for root in roots.iter_mut().filter(|root| !root.unmanaged) {
        let lost: Vec<String> = root.profile_plugins.iter().filter(|(_, kept)| !kept).map(|(path, _)| name(path)).collect();
        if lost.is_empty() {
            continue;
        }
        if lost.len() == root.profile_plugins.len() {
            root.disabled = true;
            lines.push(format!(
                "disabled root: {} — activation disabled every plugin of the profile it ships ({}), so its loose files \
                 leave the session (LMC-ID-011)",
                root.identity,
                lost.join(", ")
            ));
        } else {
            lines.push(format!(
                "root {} keeps its assets beside its disabled {}: they may reference {} forms",
                root.identity,
                lost.join(", "),
                if lost.len() == 1 { "its" } else { "their" }
            ));
        }
    }
    lines
}

/// The roots whose loose files the session layers, in order: every root activation left enabled.
pub(crate) fn loose_root_paths(roots: &[ModRoot]) -> Vec<&Path> {
    roots.iter().filter(|root| !root.disabled).map(|root| root.path.as_path()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn roots_list_their_plugins_and_archives_and_skip_what_is_not_a_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("Data");
        touch(&data.join("Skyrim.esm"));
        touch(&data.join("Skyrim - Meshes0.bsa"));
        let mod_a = tmp.path().join("Better Swords");
        touch(&mod_a.join("BetterSwords.esp"));
        touch(&mod_a.join("BetterSwords.BSA"));
        touch(&mod_a.join("textures/sword_d.dds"));
        touch(&mod_a.join("readme.txt"));
        let (roots, lines) = discover(Some(&data), &[mod_a.clone(), tmp.path().join("absent")]);
        assert_eq!(roots.len(), 2);
        assert!(roots[0].unmanaged);
        assert_eq!(roots[0].plugins, ["Skyrim.esm"]);
        assert_eq!(roots[1].identity, "better swords", "the directory name, case-folded");
        assert_eq!(roots[1].plugins, ["BetterSwords.esp"]);
        assert_eq!(roots[1].archives, ["BetterSwords.BSA"]);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with("not a directory, left out"), "{lines:?}");
    }

    #[test]
    fn a_bare_plugin_name_resolves_through_the_highest_priority_root() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("Data");
        touch(&data.join("Skyrim.esm"));
        touch(&data.join("Shared.esp"));
        let low = tmp.path().join("low");
        touch(&low.join("shared.ESP"));
        touch(&low.join("Low.esp"));
        let high = tmp.path().join("high");
        touch(&high.join("Shared.esp"));
        let (roots, _) = discover(Some(&data), &[low.clone(), high.clone()]);
        let resolved = |name: &str| PathBuf::from(resolve_plugin(name, &roots));
        assert_eq!(resolved("Shared.esp"), high.join("Shared.esp"), "the later root wins");
        assert_eq!(resolved("low.esp"), low.join("Low.esp"), "names match case-insensitively");
        assert_eq!(resolved("Skyrim.esm"), data.join("Skyrim.esm"), "the unmanaged root is searched last");
        assert_eq!(resolve_plugin("Absent.esp", &roots), "Absent.esp");
        let path = data.join("Skyrim.esm").to_string_lossy().into_owned();
        assert_eq!(resolve_plugin(&path, &roots), path, "a path is used as given");
    }

    #[test]
    fn a_root_whose_every_plugin_is_disabled_is_disabled_and_one_that_keeps_a_plugin_warns() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("Data");
        touch(&data.join("Skyrim.esm"));
        let gone = tmp.path().join("NeedsDLC");
        touch(&gone.join("NeedsDLC.esp"));
        let partial = tmp.path().join("Weapons");
        touch(&partial.join("Weapons.esp"));
        touch(&partial.join("Weapons - DLC Patch.esp"));
        let assets = tmp.path().join("Textures Only");
        touch(&assets.join("textures/a.dds"));
        let (mut roots, _) = discover(Some(&data), &[gone.clone(), partial.clone(), assets.clone()]);
        let path = |dir: &Path, name: &str| dir.join(name).to_string_lossy().into_owned();
        let profile = vec![
            (path(&data, "Skyrim.esm"), true),
            (path(&gone, "NeedsDLC.esp"), false),
            (path(&partial, "Weapons.esp"), true),
            (path(&partial, "Weapons - DLC Patch.esp"), false),
        ];
        let lines = apply_activation(&mut roots, &profile);
        assert!(roots[1].disabled);
        assert!(!roots[2].disabled && !roots[3].disabled);
        assert_eq!(roots[2].profile_plugins.len(), 2);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].starts_with("disabled root: needsdlc"), "{lines:?}");
        assert!(lines[1].starts_with("root weapons keeps its assets beside its disabled Weapons - DLC Patch.esp"), "{lines:?}");
        let layered: Vec<&Path> = loose_root_paths(&roots);
        assert_eq!(layered, [data.as_path(), partial.as_path(), assets.as_path()]);
    }

    #[test]
    fn two_roots_with_one_identity_are_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let first = tmp.path().join("a/Patch");
        let second = tmp.path().join("b/patch/");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let (roots, lines) = discover(None, &[first, second]);
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[1].identity, "patch", "a trailing separator still names the directory");
        assert!(lines[0].starts_with("identity collision (LMC-ID-010)"), "{lines:?}");
        assert!(lines[0].contains("are different directories with the identity 'patch'"), "{lines:?}");
    }

    /// LMC-ID-010 — the same directory listed twice, under any spelling, is one root, not a collision.
    #[test]
    fn a_directory_listed_twice_is_one_root() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("Data");
        touch(&data.join("Skyrim.esm"));
        let patch = tmp.path().join("Patch");
        touch(&patch.join("Patch.esp"));
        let (roots, lines) = discover(Some(&data), &[patch.clone(), tmp.path().join("Patch/../Patch"), data.clone()]);
        assert_eq!(roots.len(), 2, "{roots:?}");
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].ends_with("the same directory as the earlier root patch, left out (it is one root)"), "{lines:?}");
        assert!(lines[1].contains("the same directory as the earlier root unmanaged ("), "{lines:?}");
    }
}
