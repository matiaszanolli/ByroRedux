//! The session's plugin profile, activated once at boot (PRD legacy-mod-compatibility Phase 1, #6670).
//!
//! The profile is every `--master` in argv order followed by the `--esm`, the session's own plugin. Before anything else
//! reads argv, [`activate_plugin_args`] reads each plugin's `TES4` header, activates the profile
//! ([`byroredux_plugin::activation`]: masters hoisted ahead of their dependents, LMC-ID-003; a plugin with a missing
//! master disabled with its dependents, LMC-ID-011) and writes the activated `--master` list back into the effective
//! args, so every reader downstream (the cell loader, the exterior context, the HUD probes, `--list-cells`, the plugin
//! set a save records) sees the same order and the same plugins. What it changed is logged and kept for the
//! `plugins.activation` console command.
//!
//! The `--esm` plugin must activate and must stay last: it is the session's own plugin, and the loaders treat the last
//! plugin as the one whose cells the session opens. A profile that would disable or move it is refused, with the
//! report.
//!
//! Each activated plugin then brings its paired archives (LMC-AST-002, [`byroredux_plugin::archive_pairing`]): the
//! archives in its own directory that the game pairs with it by name, `.esm` and `.esl` included. They are written into
//! the effective args as provider flags, after the archives already listed (the profile's, standing for the game's
//! official archive list) and in load order, so that archive-versus-archive precedence is the official list first,
//! then each plugin's archives, the later plugin's winning; loose files still outrank them all (LMC-AST-001). An
//! archive the args already name keeps its place and its flags.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use byroredux_plugin::activation::{activate, read_profile_plugin, Activation, ProfilePlugin};
use byroredux_plugin::archive_pairing::{
    archives_hold_bgsm_materials, archives_hold_compiled_scripts, paired_archives,
};
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::list_merge::MergePolicy;

/// The profile's record policy (LMC-REC-003, decision Q2): `--merge-policy merge` (the default) merges levelled lists
/// across independent overrides, `--merge-policy legacy` keeps the last plugin's record of every form whole.
static MERGE_POLICY: OnceLock<MergePolicy> = OnceLock::new();

/// The session's record policy, `merge` when boot set none (a test's fold).
pub(crate) fn merge_policy() -> MergePolicy {
    MERGE_POLICY.get().copied().unwrap_or_default()
}

/// `--merge-policy <merge|legacy>` from `args`, the default when absent; an error naming the choices otherwise.
fn parse_merge_policy(args: &[String]) -> anyhow::Result<MergePolicy> {
    let Some(at) = args.iter().position(|arg| arg == "--merge-policy") else {
        return Ok(MergePolicy::default());
    };
    let value = args.get(at + 1).map(String::as_str).unwrap_or("");
    MergePolicy::parse(value).ok_or_else(|| anyhow::anyhow!("--merge-policy takes `merge` or `legacy`, got '{value}'"))
}

/// What activation did to the session's profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PluginActivationReport {
    /// The profile as given: the `--master` plugins, then the `--esm`.
    pub(crate) profile: Vec<String>,
    /// The activated plugins in load order, the `--esm` last.
    pub(crate) order: Vec<String>,
    /// One line per hoisted master and per disabled plugin; empty when the profile loads as given.
    pub(crate) lines: Vec<String>,
    /// The archives each activated plugin brought (LMC-AST-002), in load order; plugins that brought none are left out.
    pub(crate) paired: Vec<PairedArchives>,
}

/// The archives one activated plugin pairs with, beyond those the args already listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PairedArchives {
    /// The plugin, as the profile names it.
    pub(crate) plugin: String,
    /// The archives' paths, in the order they open.
    pub(crate) archives: Vec<String>,
}

impl PluginActivationReport {
    /// One line per plugin that brought archives: `DeadMoney.esm pairs DeadMoney - Main.bsa, DeadMoney - Sounds.bsa`.
    pub(crate) fn paired_lines(&self) -> Vec<String> {
        self.paired
            .iter()
            .map(|entry| {
                let archives: Vec<&str> = entry.archives.iter().map(|path| file_name(path)).collect();
                format!("{} pairs {}", file_name(&entry.plugin), archives.join(", "))
            })
            .collect()
    }
}

/// The file name of a path as the profile or the args spell it.
fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

static REPORT: OnceLock<PluginActivationReport> = OnceLock::new();

/// The boot activation's report, when the session has a plugin profile.
pub(crate) fn activation_report() -> Option<&'static PluginActivationReport> {
    REPORT.get()
}

/// Log what activation changed, once the logger exists (activation runs before it, so the effective args are final
/// before anything reads them): one warning per hoisted master and per disabled plugin, then the activated order.
pub(crate) fn log_report() {
    let Some(report) = activation_report() else { return };
    for line in &report.lines {
        log::warn!("plugin activation: {line}");
    }
    if !report.lines.is_empty() {
        let names: Vec<&str> = report.order.iter().map(|path| file_name(path)).collect();
        log::warn!("plugin activation: load order {}", names.join(", "));
    }
    for line in report.paired_lines() {
        log::info!("plugin activation: {line}");
    }
}

/// Activate the plugin profile `args` names and return `args` with the activated `--master` list (see the module docs).
/// Args with no `--esm` come back unchanged. Errors only when the `--esm` plugin itself cannot be activated as the
/// last plugin of the order.
pub(crate) fn activate_plugin_args(args: Vec<String>) -> anyhow::Result<Vec<String>> {
    // A second boot path in one process (a test) keeps the first policy, as it keeps the first report.
    let _ = MERGE_POLICY.set(parse_merge_policy(&args)?);
    let Some(esm) = crate::cli_args::parse_string_arg(&args, "--esm").filter(|value| !value.starts_with("--")) else {
        return Ok(args);
    };
    let masters = crate::cli_args::parse_all_string_args(&args, "--master");
    let profile: Vec<ProfilePlugin> =
        masters.iter().chain(std::iter::once(&esm)).map(|path| read_profile_plugin(path)).collect();
    let activation = activate(&profile);
    let mut report = session_report(&profile, &activation)?;
    let activated_masters = report.order[..report.order.len() - 1].to_vec();
    let mut args = with_masters(args, &activated_masters);
    // The base master names the game; every plugin of a profile is written for it.
    if let Some(game) = activation.order.first().and_then(|&position| profile[position].game) {
        report.paired = pair_archives(&report.order, game, &listed_archives(&args), list_archives_in);
        args = with_paired_archives(args, &report.paired, game);
    }
    // A second boot path in one process (a test) keeps the first report; the args it returns are still its own.
    let _ = REPORT.set(report);
    Ok(args)
}

/// The report for `activation` of `profile`, whose last plugin is the session's `--esm`; an error when that plugin is
/// disabled or would not load last.
fn session_report(profile: &[ProfilePlugin], activation: &Activation) -> anyhow::Result<PluginActivationReport> {
    let esm = profile.len() - 1;
    let lines = activation.report(profile);
    if let Some(entry) = activation.disabled_entry(esm) {
        let own = lines.iter().find(|line| line.starts_with(&format!("disabled: {} ", file_name(&profile[esm].name))));
        anyhow::bail!(
            "the session plugin '{}' (--esm) cannot be activated: {}{}\nfull activation report:\n  {}",
            profile[esm].name,
            own.cloned().unwrap_or_else(|| format!("{:?}", entry.cause)),
            match entry.cause {
                byroredux_plugin::activation::DisableCause::MissingMaster { ref master } => {
                    format!(" — pass `--master {master}` before `--esm`")
                }
                _ => String::new(),
            },
            lines.join("\n  ")
        );
    }
    if activation.order.last() != Some(&esm) {
        let dependents: Vec<&str> = activation
            .hoists
            .iter()
            .filter(|hoist| hoist.master == esm)
            .map(|hoist| profile[hoist.ahead_of].name.as_str())
            .collect();
        anyhow::bail!(
            "the session plugin '{}' (--esm) must load last, but {} declare{} it as a master; pass that plugin with --esm \
             instead, or drop it",
            profile[esm].name,
            dependents.join(", "),
            if dependents.len() == 1 { "s" } else { "" }
        );
    }
    Ok(PluginActivationReport {
        profile: profile.iter().map(|plugin| plugin.name.clone()).collect(),
        order: activation.order.iter().map(|&position| profile[position].name.clone()).collect(),
        lines,
        paired: Vec::new(),
    })
}

/// The identity of an archive path: its canonical path when it exists, else its spelling with `\` as `/`, both
/// case-folded as the games' file systems are.
fn archive_identity(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|canonical| canonical.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.replace('\\', "/"))
        .to_lowercase()
}

/// Every archive the args name with an archive flag, by [`archive_identity`].
fn listed_archives(args: &[String]) -> HashSet<String> {
    args.windows(2)
        .filter(|pair| crate::cli_args::ARCHIVE_FLAGS.contains(&pair[0].as_str()) && !pair[1].starts_with("--"))
        .map(|pair| archive_identity(&pair[1]))
        .collect()
}

/// The `.bsa` / `.ba2` file names in `dir`, unordered; empty when it cannot be read.
fn list_archives_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file() || kind.is_symlink()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| {
            let lower = name.to_lowercase();
            lower.ends_with(".bsa") || lower.ends_with(".ba2")
        })
        .collect()
}

/// The archives each plugin of `order` pairs with in `game`, found in the plugin's own directory by `list_dir` (read
/// once per directory), leaving out those `listed` already names. An archive two plugins pair with (a Fallout
/// `Foo - Bar.bsa` beside both `Foo.esp` and `Foo - Bar.esp`) is the first one's.
fn pair_archives(
    order: &[String],
    game: GameKind,
    listed: &HashSet<String>,
    mut list_dir: impl FnMut(&Path) -> Vec<String>,
) -> Vec<PairedArchives> {
    let mut listings: HashMap<PathBuf, Vec<String>> = HashMap::new();
    let mut taken: HashSet<String> = listed.clone();
    let mut paired = Vec::new();
    for plugin in order {
        let dir = match Path::new(plugin).parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let names = listings.entry(dir.clone()).or_insert_with(|| list_dir(&dir));
        let archives: Vec<String> = paired_archives(game, plugin, names.iter().map(String::as_str))
            .into_iter()
            .map(|name| dir.join(name).to_string_lossy().into_owned())
            .filter(|path| taken.insert(archive_identity(path)))
            .collect();
        if !archives.is_empty() {
            paired.push(PairedArchives { plugin: plugin.clone(), archives });
        }
    }
    paired
}

/// `args` with each paired archive given to every provider that can read it. A paired archive is a mod bundle, not a
/// category archive, so it feeds the mesh, texture and sound providers, the script provider where the game archives
/// its compiled scripts, and the material provider where its materials are `.bgsm` files. The last-wins providers get
/// the archives appended in load order; the script provider, whose first-listed archive wins, gets them ahead of the
/// archives already listed, in reverse, so the same archive wins in every provider.
fn with_paired_archives(mut args: Vec<String>, paired: &[PairedArchives], game: GameKind) -> Vec<String> {
    let archives: Vec<&String> = paired.iter().flat_map(|entry| &entry.archives).collect();
    let mut flags = vec!["--bsa", "--textures-bsa", "--sounds-bsa"];
    if archives_hold_bgsm_materials(game) {
        flags.push("--materials-ba2");
    }
    for archive in &archives {
        for flag in &flags {
            args.extend([(*flag).to_owned(), (*archive).clone()]);
        }
    }
    if archives_hold_compiled_scripts(game) {
        let at = args.iter().position(|arg| arg == "--scripts-bsa").unwrap_or(args.len());
        let scripts = archives.iter().rev().flat_map(|archive| ["--scripts-bsa".to_owned(), (*archive).clone()]);
        args.splice(at..at, scripts);
    }
    args
}

/// `args` with every `--master <value>` pair replaced by `masters`, at the place the first one stood (or before the
/// `--esm` when there was none). A `--master` followed by another flag is left alone, as the readers skip it.
fn with_masters(args: Vec<String>, masters: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len() + 2 * masters.len());
    let mut insert_at: Option<usize> = None;
    let mut iter = args.into_iter().peekable();
    while let Some(arg) = iter.next() {
        if arg == "--master" && iter.peek().is_some_and(|value| !value.starts_with("--")) {
            iter.next();
            insert_at.get_or_insert(out.len());
            continue;
        }
        out.push(arg);
    }
    let at = insert_at.or_else(|| out.iter().position(|arg| arg == "--esm")).unwrap_or(out.len());
    let pairs = masters.iter().flat_map(|master| ["--master".to_owned(), master.clone()]);
    out.splice(at..at, pairs);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    fn plugin(name: &str, masters: &[&str]) -> ProfilePlugin {
        ProfilePlugin::new(name, masters.iter().copied())
    }

    /// The activated masters replace the given ones where the first stood; every other arg keeps its place.
    #[test]
    fn the_activated_masters_replace_the_given_ones_in_place() {
        let given = args(&["byroredux", "--bsa", "a.bsa", "--master", "B.esm", "--cell", "X", "--master", "A.esm", "--esm", "C.esp"]);
        let out = with_masters(given, &["A.esm".to_owned(), "B.esm".to_owned()]);
        assert_eq!(out, args(&["byroredux", "--bsa", "a.bsa", "--master", "A.esm", "--master", "B.esm", "--cell", "X", "--esm", "C.esp"]));
        // A disabled master is gone; with none left, no `--master` remains.
        let out = with_masters(args(&["byroredux", "--master", "Gone.esp", "--esm", "C.esp"]), &[]);
        assert_eq!(out, args(&["byroredux", "--esm", "C.esp"]));
        // The readers skip a `--master` that is followed by a flag, and so does the rewrite.
        let out = with_masters(args(&["byroredux", "--master", "--esm", "C.esp"]), &["A.esm".to_owned()]);
        assert_eq!(out, args(&["byroredux", "--master", "--master", "A.esm", "--esm", "C.esp"]));
    }

    /// LMC-REC-003 — the policy defaults to `merge` (decision Q2), takes `legacy`, and refuses anything else.
    #[test]
    fn the_merge_policy_flag_defaults_to_merge() {
        assert_eq!(parse_merge_policy(&args(&["byroredux"])).unwrap(), MergePolicy::Merge);
        assert_eq!(parse_merge_policy(&args(&["byroredux", "--merge-policy", "legacy"])).unwrap(), MergePolicy::Legacy);
        let error = parse_merge_policy(&args(&["byroredux", "--merge-policy", "smash"])).unwrap_err().to_string();
        assert!(error.contains("takes `merge` or `legacy`, got 'smash'"), "{error}");
        assert!(parse_merge_policy(&args(&["byroredux", "--merge-policy"])).is_err());
    }

    /// Args without an `--esm` have no profile and pass through untouched.
    #[test]
    fn args_without_an_esm_pass_through() {
        let given = args(&["byroredux", "path/to/mesh.nif", "--master", "A.esm"]);
        assert_eq!(activate_plugin_args(given.clone()).unwrap(), given);
    }

    /// A profile that loads as given reports nothing; a hoist and a disabled patch are reported and the `--esm` stays last.
    #[test]
    fn the_session_report_names_what_moved_and_what_was_left_out() {
        let profile = [plugin("FalloutNV.esm", &[]), plugin("MyMod.esm", &["FalloutNV.esm"])];
        let report = session_report(&profile, &activate(&profile)).unwrap();
        assert!(report.lines.is_empty());
        assert_eq!(report.order, ["FalloutNV.esm", "MyMod.esm"]);

        let profile = [
            plugin("Patch.esp", &["FalloutNV.esm", "LonesomeRoad.esm"]),
            plugin("FalloutNV.esm", &[]),
            plugin("Main.esp", &["FalloutNV.esm"]),
        ];
        let report = session_report(&profile, &activate(&profile)).unwrap();
        assert_eq!(report.order, ["FalloutNV.esm", "Main.esp"]);
        assert_eq!(report.lines.len(), 1, "{:?}", report.lines);
        assert!(report.lines[0].contains("Patch.esp — its master LonesomeRoad.esm is not in the profile"));
    }

    /// LMC-AST-002 — each plugin brings the archives in its own directory that pair with it, `.esm` included, in load
    /// order; an archive the args already list stays theirs, and one listing is read per directory.
    #[test]
    fn each_plugin_brings_its_paired_archives_in_load_order() {
        let order = args(&["/fnv/Data/FalloutNV.esm", "/fnv/Data/DeadMoney.esm", "/mods/Junky.esp"]);
        let listed: HashSet<String> = [archive_identity("/fnv/Data/DeadMoney - Sounds.bsa")].into();
        let mut reads = Vec::new();
        let paired = pair_archives(&order, GameKind::Fallout3NV, &listed, |dir| {
            reads.push(dir.to_path_buf());
            match dir.to_str() {
                Some("/fnv/Data") => args(&["Fallout - Meshes.bsa", "DeadMoney - Sounds.bsa", "DeadMoney - Main.bsa"]),
                _ => args(&["Junky.bsa", "Junky - Assets.bsa"]),
            }
        });
        assert_eq!(reads, [PathBuf::from("/fnv/Data"), PathBuf::from("/mods")]);
        assert_eq!(
            paired,
            [
                PairedArchives { plugin: order[1].clone(), archives: args(&["/fnv/Data/DeadMoney - Main.bsa"]) },
                PairedArchives { plugin: order[2].clone(), archives: args(&["/mods/Junky - Assets.bsa", "/mods/Junky.bsa"]) },
            ]
        );
        let report = PluginActivationReport { profile: order.clone(), order, lines: Vec::new(), paired };
        assert_eq!(report.paired_lines(), ["DeadMoney.esm pairs DeadMoney - Main.bsa", "Junky.esp pairs Junky - Assets.bsa, Junky.bsa"]);
    }

    /// A paired archive feeds every provider that can read it: after the listed archives where the last wins, ahead of
    /// them in reverse for the first-wins script provider; scripts and materials only where the game archives them.
    #[test]
    fn paired_archives_feed_every_provider_with_the_later_plugin_winning() {
        let paired = [
            PairedArchives { plugin: "A.esm".into(), archives: args(&["a.ba2"]) },
            PairedArchives { plugin: "B.esp".into(), archives: args(&["b.ba2"]) },
        ];
        let given = args(&["byroredux", "--bsa", "v.ba2", "--scripts-bsa", "vs.ba2", "--esm", "B.esp"]);
        let out = with_paired_archives(given.clone(), &paired, GameKind::Fallout4);
        let mut expected = args(&["byroredux", "--bsa", "v.ba2", "--scripts-bsa", "b.ba2", "--scripts-bsa", "a.ba2"]);
        expected.extend(args(&["--scripts-bsa", "vs.ba2", "--esm", "B.esp"]));
        for archive in ["a.ba2", "b.ba2"] {
            for flag in ["--bsa", "--textures-bsa", "--sounds-bsa", "--materials-ba2"] {
                expected.extend(args(&[flag, archive]));
            }
        }
        assert_eq!(out, expected);

        let out = with_paired_archives(given.clone(), &paired, GameKind::Fallout3NV);
        assert_eq!(out.iter().filter(|arg| *arg == "--scripts-bsa").count(), 1, "New Vegas compiles its scripts into the plugin");
        assert!(!out.iter().any(|arg| arg == "--materials-ba2"), "New Vegas has no .bgsm materials");
        assert_eq!(&out[..given.len()], &given[..], "the listed archives keep their place");
    }

    /// The `--esm` plugin must activate and load last: a missing master of its own names the flag to add, and a
    /// `--master` that depends on it is refused.
    #[test]
    fn the_session_plugin_must_activate_and_load_last() {
        let profile = [plugin("Skyrim.esm", &[]), plugin("Dawnguard.esm", &["Skyrim.esm", "Update.esm"])];
        let error = session_report(&profile, &activate(&profile)).unwrap_err().to_string();
        assert!(error.contains("'Dawnguard.esm' (--esm) cannot be activated"), "{error}");
        assert!(error.contains("pass `--master Update.esm` before `--esm`"), "{error}");
        // Given by path, as every real launch does, it still quotes its own report line.
        let profile = [plugin("/d/Skyrim.esm", &[]), plugin("/d/Dawnguard.esm", &["Skyrim.esm", "Update.esm"])];
        let error = session_report(&profile, &activate(&profile)).unwrap_err().to_string();
        assert!(error.contains("cannot be activated: disabled: Dawnguard.esm — its master Update.esm is not in the profile"), "{error}");

        let profile = [plugin("Skyrim.esm", &[]), plugin("Mod.esp", &["Skyrim.esm", "Main.esm"]), plugin("Main.esm", &["Skyrim.esm"])];
        let error = session_report(&profile, &activate(&profile)).unwrap_err().to_string();
        assert!(error.contains("'Main.esm' (--esm) must load last, but Mod.esp declares it as a master"), "{error}");
    }
}
