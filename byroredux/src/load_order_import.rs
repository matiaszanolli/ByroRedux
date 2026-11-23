//! `--import-load-order <source>`: import a legacy load order into a mod profile (PRD legacy-mod-compatibility
//! LMC-ID-006, #6670), then exit. No window, no Vulkan.
//!
//! ```text
//! byroredux --game fnv --import-load-order ~/MO2/profiles/Default [--write ~/.byroredux/boot.toml]
//! byroredux --game fo4 --import-load-order "<Data>/vortex.deployment.json" --import-load-order <Plugins.txt>
//! byroredux --boot ~/.byroredux/boot.toml --cell GSProspectorSaloonInterior
//! ```
//!
//! A source is a `plugins.txt`, a `loadorder.txt`, a Mod Organizer 2 profile directory or a Vortex deployment
//! (`vortex.deployment.json`, or the `Data` directory holding it); the flag repeats, so a Vortex deployment takes the
//! game's own `plugins.txt` beside it. The game is the one the session's base master names (`--game`), and its rules
//! decide the order ([`byroredux_plugin::load_order_import`]). The command prints the active plugins in load order with
//! the root each comes from, the mod roots lowest priority first, and one note per entry it could not resolve.
//! `--write <path>` stores the profile in the launcher's boot request (its `[mods]` table; the rest of an existing
//! request is kept), which `--boot <path>` expands into `--master` / `--esm` / `--mod-root` like any profile.

use std::path::{Path, PathBuf};

use anyhow::Context;
use byroredux_boot_request::{BootRequest, ModsSpec};
use byroredux_plugin::load_order_import::{import, rules_for, ImportSource, ImportedProfile, LoadOrderMethod};

use crate::cli_args::{parse_all_string_args, parse_string_arg};

/// The flag naming a source, repeatable.
pub(crate) const IMPORT_FLAG: &str = "--import-load-order";
/// Where to store the imported profile.
const WRITE_FLAG: &str = "--write";

pub(crate) fn run(args: &[String]) -> anyhow::Result<()> {
    // The game's base master: `--game` appends it as an `--esm`, after any the command line gave.
    let base = parse_all_string_args(args, "--esm")
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .context("--import-load-order needs --game <key> (or --esm <base master path>) to name the game")?;
    let data_dir = base.parent().filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
    let (header, game) = byroredux_plugin::activation::read_plugin_header(&base)?;
    let rules = rules_for(game, header.hedr_version, header.record_version)
        .with_context(|| format!("{}: no load-order import for {game:?}", base.display()))?;
    let given = parse_all_string_args(args, IMPORT_FLAG);
    anyhow::ensure!(!given.is_empty(), "{IMPORT_FLAG} needs a source");
    let sources: Vec<ImportSource> =
        given.iter().map(|source| ImportSource::detect(Path::new(source))).collect::<anyhow::Result<_>>()?;
    let imported = import(&sources, &data_dir, &rules);
    let method = match rules.method {
        LoadOrderMethod::Timestamp => "file-time order",
        LoadOrderMethod::Textfile => "loadorder.txt order",
        LoadOrderMethod::Asterisk => "asterisk plugins.txt order",
    };
    println!("load-order import: {game:?} ({method}), Data {}", data_dir.display());
    report_lines(&imported).iter().for_each(|line| println!("{line}"));
    if let Some(path) = parse_string_arg(args, WRITE_FLAG) {
        write_request(Path::new(&path), &imported, &given)?;
        println!("load-order import: wrote [mods] to {path}; launch with --boot {path}");
    }
    Ok(())
}

/// The import as the command prints it.
fn report_lines(imported: &ImportedProfile) -> Vec<String> {
    let root_label = |index: Option<usize>| match index {
        None => "Data".to_owned(),
        Some(i) => imported.roots[i].path.file_name().map_or("?".to_owned(), |n| n.to_string_lossy().to_lowercase()),
    };
    let mut lines = vec![format!("plugins ({}), load order:", imported.plugins.len())];
    lines.extend(
        imported
            .plugins
            .iter()
            .enumerate()
            .map(|(position, plugin)| format!("  {position:3}  {}  [{}]", plugin.name, root_label(plugin.root))),
    );
    lines.push(format!("mod roots ({}), lowest priority first:", imported.roots.len()));
    lines.extend(imported.roots.iter().enumerate().map(|(position, root)| {
        format!("  {position:3}  {}  {}  {}", root_label(Some(position)), root.kind, root.path.display())
    }));
    lines.push(format!("notes ({}):", imported.notes.len()));
    lines.extend(imported.notes.iter().map(|note| format!("  {note}")));
    lines
}

/// Store the profile in the boot request at `path`: its `[mods]` replaced, the rest of an existing request kept. A new
/// request names the game by the `--game` key the command line gave.
fn write_request(path: &Path, imported: &ImportedProfile, sources: &[String]) -> anyhow::Result<()> {
    let mut request = if path.exists() {
        BootRequest::load(path).with_context(|| format!("{WRITE_FLAG} {}: read the existing request", path.display()))?
    } else {
        // `--game` was consumed by the profile expander; read it from the command line as typed.
        let typed: Vec<String> = std::env::args().collect();
        let key = parse_string_arg(&typed, "--game")
            .with_context(|| format!("{WRITE_FLAG} needs --game <key> to name the game of a new boot request"))?;
        BootRequest::for_profile(key)
    };
    request.mods = mods_spec(imported, sources);
    request.save(path).with_context(|| format!("{WRITE_FLAG} {}", path.display()))?;
    Ok(())
}

fn mods_spec(imported: &ImportedProfile, sources: &[String]) -> ModsSpec {
    ModsSpec {
        load_order: imported.plugins.iter().map(|plugin| plugin.name.clone()).collect(),
        roots: imported.roots.iter().map(|root| root.path.to_string_lossy().into_owned()).collect(),
        imported_from: sources.join("; "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::load_order_import::{ImportedPlugin, ImportedRoot};

    fn imported() -> ImportedProfile {
        ImportedProfile {
            plugins: vec![
                ImportedPlugin { name: "FalloutNV.esm".into(), root: None },
                ImportedPlugin { name: "WMX.esp".into(), root: Some(0) },
            ],
            roots: vec![ImportedRoot { path: PathBuf::from("/mo2/mods/WMX"), kind: "MO2 mod" }],
            notes: vec!["MO2: 1 disabled mod(s) left out".into()],
        }
    }

    #[test]
    fn the_report_names_each_plugin_with_its_root() {
        let lines = report_lines(&imported());
        assert_eq!(lines[1], "    0  FalloutNV.esm  [Data]");
        assert_eq!(lines[2], "    1  WMX.esp  [wmx]");
        assert!(lines.contains(&"    0  wmx  MO2 mod  /mo2/mods/WMX".to_owned()), "{lines:?}");
        assert_eq!(lines.last().unwrap(), "  MO2: 1 disabled mod(s) left out");
    }

    /// Writing into an existing request replaces its `[mods]` and keeps everything else.
    #[test]
    fn writing_keeps_the_rest_of_an_existing_request() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("boot.toml");
        let existing = BootRequest::for_profile("fnv")
            .with_action(byroredux_boot_request::Action::Cell { edid: "GSProspectorSaloonInterior".into() });
        existing.save(&path).unwrap();
        write_request(&path, &imported(), &["/mo2/profiles/Default".to_owned()]).unwrap();
        let written = BootRequest::load(&path).unwrap();
        assert_eq!(written.action, existing.action);
        assert_eq!(written.mods.load_order, vec!["FalloutNV.esm", "WMX.esp"]);
        assert_eq!(written.mods.roots, vec!["/mo2/mods/WMX"]);
        assert_eq!(written.mods.imported_from, "/mo2/profiles/Default");
    }
}
