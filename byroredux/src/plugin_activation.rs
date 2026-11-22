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

use std::sync::OnceLock;

use byroredux_plugin::activation::{activate, read_profile_plugin, Activation, ProfilePlugin};

/// What activation did to the session's profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PluginActivationReport {
    /// The profile as given: the `--master` plugins, then the `--esm`.
    pub(crate) profile: Vec<String>,
    /// The activated plugins in load order, the `--esm` last.
    pub(crate) order: Vec<String>,
    /// One line per hoisted master and per disabled plugin; empty when the profile loads as given.
    pub(crate) lines: Vec<String>,
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
        let names: Vec<&str> = report.order.iter().map(|path| path.rsplit(['/', '\\']).next().unwrap_or(path)).collect();
        log::warn!("plugin activation: load order {}", names.join(", "));
    }
}

/// Activate the plugin profile `args` names and return `args` with the activated `--master` list (see the module docs).
/// Args with no `--esm` come back unchanged. Errors only when the `--esm` plugin itself cannot be activated as the
/// last plugin of the order.
pub(crate) fn activate_plugin_args(args: Vec<String>) -> anyhow::Result<Vec<String>> {
    let Some(esm) = crate::cli_args::parse_string_arg(&args, "--esm").filter(|value| !value.starts_with("--")) else {
        return Ok(args);
    };
    let masters = crate::cli_args::parse_all_string_args(&args, "--master");
    let profile: Vec<ProfilePlugin> =
        masters.iter().chain(std::iter::once(&esm)).map(|path| read_profile_plugin(path)).collect();
    let activation = activate(&profile);
    let report = session_report(&profile, &activation)?;
    let activated_masters = report.order[..report.order.len() - 1].to_vec();
    let args = with_masters(args, &activated_masters);
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
        let own = lines.iter().find(|line| line.starts_with(&format!("disabled: {} ", profile[esm].name)));
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
    })
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

    /// The `--esm` plugin must activate and load last: a missing master of its own names the flag to add, and a
    /// `--master` that depends on it is refused.
    #[test]
    fn the_session_plugin_must_activate_and_load_last() {
        let profile = [plugin("Skyrim.esm", &[]), plugin("Dawnguard.esm", &["Skyrim.esm", "Update.esm"])];
        let error = session_report(&profile, &activate(&profile)).unwrap_err().to_string();
        assert!(error.contains("'Dawnguard.esm' (--esm) cannot be activated"), "{error}");
        assert!(error.contains("pass `--master Update.esm` before `--esm`"), "{error}");

        let profile = [plugin("Skyrim.esm", &[]), plugin("Mod.esp", &["Skyrim.esm", "Main.esm"]), plugin("Main.esm", &["Skyrim.esm"])];
        let error = session_report(&profile, &activate(&profile)).unwrap_err().to_string();
        assert!(error.contains("'Main.esm' (--esm) must load last, but Mod.esp declares it as a master"), "{error}");
    }
}
