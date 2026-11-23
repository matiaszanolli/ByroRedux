//! `--preflight [--preflight-json <file>]`: check a mod profile without a renderer and without running any of its code
//! (PRD legacy-mod-compatibility LMC-DX-001, LMC-SCR-005, LMC-EXT-001, LMC-SET-001; #6671), then exit.
//!
//! ```text
//! byroredux --game fnv --mod-root ~/mods/YUP --esm "YUP - Base Game + All DLC.esm" --preflight
//! byroredux --boot ~/.byroredux/boot.toml --preflight --preflight-json preflight.json
//! ```
//!
//! It prints the plugin report first (`--plugin-report`: activation, the profile's roots, record conflicts and their
//! winners, merged lists, reverts), then, per mod root, every file the root ships classified by the mechanism catalog
//! (`assets/legacy_mod_compat/mechanisms.toml`, read through [`byroredux_plugin::mod_catalog`]) with its disposition:
//! plugins with their activation state, archives with whether a plugin pairs with them, native DLLs (LMC-EXT-001), the
//! keys of a plugin-named INI (LMC-SET-001), and the rest. Every Papyrus script the root ships, loose or in an archive
//! its plugins pair with, and every ObScript SCPT record of its plugins is scanned for extender calls (LMC-SCR-005:
//! eager, not only the scripts a session happens to attach). Then the files beside the game's executable the catalog
//! names (extender loaders, renderer injectors), every asset path two sources ship with the one that wins
//! (LMC-AST-010), and a coverage summary per disposition. A file, INI key or script call the catalog has no entry for
//! is *unclassified*: §7.3 rule 2 fails on it until the catalog says what it is. Nothing a mod ships is executed; DLLs
//! are named, never loaded.
//!
//! The unmanaged root (the game's `Data` directory) is listed but its loose files are not classified: there is no
//! source list of the game's own files to tell them from a mod's (LMC-REC-008). Install a mod as a `--mod-root` to
//! preflight it.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Context;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::mod_catalog::{ini_keys, Classified, ModCatalog};
use byroredux_scripting::compatibility::{CompatibilityDisposition, CompatibilityReport, ExtenderFamily};

use crate::cli_args::parse_string_arg;
use crate::mod_roots::ModRoot;

/// The flag that runs the preflight.
pub(crate) const PREFLIGHT_FLAG: &str = "--preflight";
/// Where to write the preflight as JSON.
const JSON_FLAG: &str = "--preflight-json";
/// Lines of examples a section prints.
const EXAMPLES: usize = 5;
/// How deep a root's directory tree is walked.
const MAX_DEPTH: usize = 32;

/// The catalog's name for the session's game.
fn catalog_game(kind: GameKind, base_master: &Path) -> Option<&'static str> {
    let name = base_master.file_name()?.to_string_lossy().to_lowercase();
    Some(match kind {
        GameKind::Oblivion => "oblivion",
        GameKind::Fallout3NV if name == "fallout3.esm" => "fo3",
        GameKind::Fallout3NV => "fnv",
        GameKind::Skyrim => "skyrim",
        GameKind::Fallout4 => "fo4",
        GameKind::Starfield => "starfield",
        GameKind::Fallout76 => return None,
    })
}

/// One classified item counted under its mechanism.
#[derive(Default)]
struct Tally {
    count: usize,
    examples: Vec<String>,
}

impl Tally {
    fn add(&mut self, example: String) {
        self.count += 1;
        if self.examples.len() < EXAMPLES {
            self.examples.push(example);
        }
    }
}

/// One extender call aggregate: the first spelling seen, occurrences and the scripts that make it.
#[derive(Default)]
struct CallTally {
    spelling: String,
    occurrences: usize,
    scripts: BTreeSet<String>,
}

#[derive(Default)]
struct RootScan {
    label: String,
    disabled: bool,
    unmanaged: bool,
    files: usize,
    /// mechanism id → items.
    mechanisms: BTreeMap<String, Tally>,
    unclassified: Vec<String>,
    not_game_data: usize,
    notes: Vec<String>,
    papyrus_scripts: usize,
    obscript_scripts: usize,
    /// Calls to functions the base game's own scripts declare, which the SDK adapts but no extender provides.
    base_game_calls: usize,
    /// (mechanism, family, function case-folded (Papyrus names are case-insensitive), disposition) → calls.
    calls: BTreeMap<(String, String, String, String), CallTally>,
}

impl RootScan {
    fn classify(&mut self, classified: Classified<'_>, example: String) {
        match classified {
            Classified::Mechanism(mechanism) => self.mechanisms.entry(mechanism.id.clone()).or_default().add(example),
            Classified::NotGameData => self.not_game_data += 1,
            Classified::Unclassified => self.unclassified.push(example),
        }
    }

    /// Fold one script's extender findings in, each call under the mechanism the catalog files it as: a script-library
    /// DLL's API (PapyrusUtil, JContainers) is S5, a call the engine has no contract for S4.uncovered, the rest
    /// S4.covered. A base-game function (`Game.GetPlayer`, and per game `Game.IsPluginInstalled` on Fallout 4) is no
    /// extender call and is only counted. Malformed call metadata is unclassified: the scan could not say what the call
    /// is.
    fn add_report(&mut self, game: &str, script: &str, report: &CompatibilityReport) {
        for finding in &report.findings {
            let compatibility = &finding.compatibility;
            let base_game = compatibility.family == ExtenderFamily::BaseGame
                || matches!(&finding.call.target, byroredux_pex::CallTarget::StaticType(provider)
                    if byroredux_sdk::compatibility::is_base_game_papyrus_function(game, provider, &finding.call.function));
            if base_game {
                self.base_game_calls += 1;
                continue;
            }
            let mechanism = match (compatibility.family, compatibility.disposition) {
                (ExtenderFamily::PapyrusUtil | ExtenderFamily::JContainers, _) => "S5",
                (_, CompatibilityDisposition::Unsupported) => "S4.uncovered",
                _ => "S4.covered",
            };
            let key = (
                mechanism.to_owned(),
                format!("{:?}", compatibility.family),
                finding.call.function.to_lowercase(),
                format!("{:?}", compatibility.disposition),
            );
            let tally = self.calls.entry(key).or_default();
            if tally.spelling.is_empty() {
                tally.spelling.clone_from(&finding.call.function);
            }
            tally.occurrences += 1;
            tally.scripts.insert(script.to_owned());
        }
        for malformed in &report.malformed_calls {
            self.unclassified.push(format!("{script}: unreadable call metadata: {malformed:?}"));
        }
    }
}

/// Every file under `dir`, as (path relative to `dir` with `/`, absolute path), sorted.
fn walk(dir: &Path) -> Vec<(String, PathBuf)> {
    fn visit(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<(String, PathBuf)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
            if path.is_dir() {
                if depth < MAX_DEPTH {
                    visit(&path, &relative, depth + 1, out);
                }
            } else if path.is_file() {
                out.push((relative, path));
            }
        }
    }
    let mut out = Vec::new();
    visit(dir, "", 0, &mut out);
    out.sort();
    out
}

fn is_papyrus(path: &str) -> bool {
    path.starts_with("scripts/") && path.ends_with(".pex")
}

/// One asset path's sources, for the conflict section: (root position, `None` for a loose file or the archive's name,
/// the archive's position in load order).
type Sources = Vec<(usize, Option<(String, usize)>)>;

pub(crate) fn run(plugin_paths: &[String], args: &[String]) -> anyhow::Result<()> {
    crate::commands::plugins::run_report(plugin_paths, &[])?;
    let report = crate::plugin_activation::activation_report()
        .context("--preflight needs a plugin profile: --game <key>, --esm <plugin> or --boot <request>")?;
    let base = PathBuf::from(report.order.first().context("--preflight: the profile has no plugin")?);
    let (_, kind) = byroredux_plugin::activation::read_plugin_header(&base)?;
    let game = catalog_game(kind, &base).with_context(|| format!("--preflight: no mechanism catalog for {kind:?}"))?;
    let catalog = ModCatalog::embedded();

    // The archives that open, by file name, in load order (LMC-AST-002).
    let paired: Vec<String> = report
        .paired
        .iter()
        .flat_map(|entry| entry.archives.iter())
        .map(|path| Path::new(path).file_name().map_or(String::new(), |n| n.to_string_lossy().to_lowercase()))
        .collect();
    let paired_position: HashMap<&str, usize> = paired.iter().enumerate().map(|(i, name)| (name.as_str(), i)).collect();

    let mut scans = Vec::new();
    let mut sources: BTreeMap<String, Sources> = BTreeMap::new();
    for (position, root) in report.roots.iter().enumerate() {
        let mut scan = RootScan {
            label: root.label(),
            disabled: root.disabled,
            unmanaged: root.unmanaged,
            ..RootScan::default()
        };
        if root.unmanaged {
            scan.notes.push(format!(
                "{} plugin(s), {} archive(s); loose files are not classified: no source list of the game's own files \
                 tells them from a mod's (LMC-REC-008). Install a mod as a --mod-root to preflight it.",
                root.plugins.len(),
                root.archives.len()
            ));
        } else {
            scan_root(root, position, game, catalog, &paired_position, &mut scan, &mut sources);
        }
        scans.push(scan);
    }
    let game_dir = scan_game_dir(&report.roots, game, catalog);
    let conflicts = conflict_lines(&sources, &report.roots);

    let lines = report_lines(game, catalog, &scans, &game_dir, &conflicts);
    lines.iter().for_each(|line| println!("{line}"));
    if let Some(path) = parse_string_arg(args, JSON_FLAG) {
        let json = report_json(game, catalog, &scans, &game_dir, &conflicts);
        std::fs::write(&path, serde_json::to_string_pretty(&json)?).with_context(|| format!("{JSON_FLAG} {path}"))?;
        println!("preflight: wrote {path}");
    }
    Ok(())
}

fn scan_root(
    root: &ModRoot,
    position: usize,
    game: &str,
    catalog: &ModCatalog,
    paired_position: &HashMap<&str, usize>,
    scan: &mut RootScan,
    sources: &mut BTreeMap<String, Sources>,
) {
    let kept: HashMap<String, bool> =
        root.profile_plugins.iter().map(|(name, kept)| (plugin_file_name(name), *kept)).collect();
    let plugin_stems: HashSet<String> = root
        .plugins
        .iter()
        .filter_map(|name| name.rsplit_once('.').map(|(stem, _)| stem.to_lowercase()))
        .collect();

    for (relative, path) in walk(&root.path) {
        scan.files += 1;
        let lower = relative.to_lowercase();
        let top_level = !lower.contains('/');
        let extension = lower.rsplit_once('.').map_or("", |(_, ext)| ext);
        match extension {
            "esm" | "esp" | "esl" | "esh" if top_level => {
                let state = match kept.get(&lower) {
                    Some(true) => "loaded",
                    Some(false) => "disabled by activation",
                    None => "not in the profile",
                };
                scan.classify(catalog.classify_path(game, &lower), format!("{relative} ({state})"));
                if state != "loaded" {
                    scan.notes.push(format!("{relative}: {state}"));
                }
            }
            "bsa" | "ba2" if top_level => {
                let opens = paired_position.contains_key(lower.as_str());
                scan.classify(catalog.classify_path(game, &lower), relative.clone());
                if !opens {
                    scan.notes.push(format!("{relative}: no enabled plugin pairs with it, so it does not open"));
                    continue;
                }
                scan_archive(game, &path, &relative, position, paired_position[lower.as_str()], scan, sources);
            }
            "ini" if top_level && lower.rsplit_once('.').is_some_and(|(stem, _)| plugin_stems.contains(stem)) => {
                // A plugin-named INI sets game INI values (LMC-SET-001): each key is its own item.
                let text = std::fs::read(&path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default();
                for key in ini_keys(&text) {
                    scan.classify(catalog.classify_ini_key(game, &key), format!("{relative} [{key}]"));
                }
            }
            _ => {
                let classified = catalog.classify_path(game, &lower);
                scan.classify(classified, relative.clone());
                if is_papyrus(&lower)
                    && let Ok(bytes) = std::fs::read(&path)
                {
                    scan_papyrus(game, &relative, &bytes, scan);
                }
                if matches!(classified, Classified::Mechanism(_)) && !lower.ends_with(".dll") && !lower.ends_with(".ini") {
                    sources.entry(lower).or_default().push((position, None));
                }
            }
        }
    }

    // ObScript SCPT records of the root's loaded plugins (Oblivion, FO3, FNV).
    if matches!(game, "oblivion" | "fo3" | "fnv") {
        for (name, _) in root.profile_plugins.iter().filter(|(_, kept)| *kept) {
            let path = root.path.join(plugin_file_name(name));
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let Ok(index) = byroredux_plugin::esm::records::parse_esm(&bytes) else {
                scan.notes.push(format!("{name}: its records could not be parsed for the script scan"));
                continue;
            };
            let dialect = crate::cell_loader::references::obscript_dialect_for(index.game, index.character_rules);
            for script in index.scripts.values() {
                scan.obscript_scripts += 1;
                let label = format!("{name} SCPT {}", script.editor_id);
                let report = match (script.source.as_deref(), dialect) {
                    (Some(source), _) => byroredux_scripting::compatibility::analyze_obscript_compatibility(&script.editor_id, source),
                    (None, Some(dialect)) => byroredux_scripting::compatibility::analyze_obscript_bytecode_compatibility(
                        &script.editor_id,
                        &script.compiled,
                        dialect,
                    ),
                    (None, None) => continue,
                };
                scan.add_report(game, &label, &report);
            }
        }
    }
}

/// The plugin file name a profile entry names, case-folded.
fn plugin_file_name(name: &str) -> String {
    name.rsplit(['/', '\\']).next().unwrap_or(name).to_lowercase()
}

fn scan_archive(
    game: &str,
    path: &Path,
    relative: &str,
    root_position: usize,
    order: usize,
    scan: &mut RootScan,
    sources: &mut BTreeMap<String, Sources>,
) {
    // The provider layer owns the archive readers (#5009): BSA/BA2 dispatch, entry extraction.
    let archive = match crate::asset_provider::Archive::open(&path.to_string_lossy()) {
        Ok(archive) => archive,
        Err(error) => {
            scan.notes.push(format!("{relative}: cannot be read: {error}"));
            return;
        }
    };
    let name = relative.to_lowercase();
    let files: Vec<(String, String)> =
        archive.all_paths().into_iter().map(|entry| (entry.replace('\\', "/").to_lowercase(), entry.to_owned())).collect();
    for (file, entry) in files {
        if is_papyrus(&file) {
            match archive.extract(&entry) {
                Ok(bytes) => scan_papyrus(game, &format!("{relative}:{file}"), &bytes, scan),
                Err(error) => scan.notes.push(format!("{relative}:{file}: cannot be extracted: {error}")),
            }
        }
        sources.entry(file).or_default().push((root_position, Some((name.clone(), order))));
    }
}

fn scan_papyrus(game: &str, label: &str, bytes: &[u8], scan: &mut RootScan) {
    scan.papyrus_scripts += 1;
    match byroredux_pex::parse(bytes) {
        Ok(pex) => scan.add_report(game, label, &byroredux_scripting::compatibility::analyze_pex_compatibility(&pex)),
        Err(error) => scan.unclassified.push(format!("{label}: not a readable compiled script: {error}")),
    }
}

/// Files beside the game's executable the catalog names (extender loaders, renderer injectors), as
/// `(relative path, mechanism id)`. Everything else there is the game's own and is not listed.
fn scan_game_dir(roots: &[ModRoot], game: &str, catalog: &ModCatalog) -> Vec<(String, String)> {
    let Some(game_dir) = roots.iter().find(|root| root.unmanaged).and_then(|root| root.path.parent()) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(game_dir) else { return found };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case("data") {
            continue;
        }
        let files = if entry.path().is_dir() {
            walk(&entry.path()).into_iter().map(|(relative, _)| format!("{name}/{relative}")).collect()
        } else {
            vec![name]
        };
        for file in files {
            if let Classified::Mechanism(mechanism) = catalog.classify_game_dir(game, &file) {
                found.push((file, mechanism.id.clone()));
            }
        }
    }
    found.sort();
    found
}

/// Every asset path more than one source ships, with the one that wins (LMC-AST-010: the later root's loose file over
/// an earlier root's, any loose file over any archive, a later archive over an earlier one) and the ones it shadows.
fn conflict_lines(sources: &BTreeMap<String, Sources>, roots: &[ModRoot]) -> Vec<String> {
    let label = |(root, archive): &(usize, Option<(String, usize)>)| match archive {
        None => format!("{} (loose)", roots[*root].label()),
        Some((name, _)) => format!("{} ({name})", roots[*root].label()),
    };
    sources
        .iter()
        .filter(|(_, from)| from.len() > 1)
        .map(|(path, from)| {
            let winner = from
                .iter()
                .max_by_key(|(root, archive)| match archive {
                    None => (1, *root, 0),
                    Some((_, order)) => (0, 0, *order),
                })
                .expect("more than one source");
            let losers: Vec<String> = from.iter().filter(|source| *source != winner).map(&label).collect();
            format!("{path}: {} over {}", label(winner), losers.join(", "))
        })
        .collect()
}

fn disposition_of<'a>(catalog: &'a ModCatalog, game: &str, id: &str) -> (&'a str, &'a str) {
    catalog.get(id).map_or(("D?", "?"), |m| (m.disposition_for(game), m.name.as_str()))
}

fn report_lines(
    game: &str,
    catalog: &ModCatalog,
    scans: &[RootScan],
    game_dir: &[(String, String)],
    conflicts: &[String],
) -> Vec<String> {
    let mut lines = vec![format!("preflight: {game}, mechanism catalog v{}", catalog.version)];
    let mut coverage: BTreeMap<String, usize> = BTreeMap::new();
    let mut unclassified = 0;
    for scan in scans {
        let state = if scan.disabled { ", disabled" } else { "" };
        lines.push(format!("root {}{state}: {} file(s)", scan.label, scan.files));
        for (id, tally) in &scan.mechanisms {
            let (disposition, name) = disposition_of(catalog, game, id);
            *coverage.entry(disposition.to_owned()).or_default() += tally.count;
            lines.push(format!("  {id} {disposition} {name}: {} — {}", tally.count, tally.examples.join(", ")));
        }
        if scan.papyrus_scripts + scan.obscript_scripts > 0 {
            lines.push(format!(
                "  scripts: {} Papyrus, {} ObScript; {} call(s) to base-game functions the engine adapts; extender calls:",
                scan.papyrus_scripts, scan.obscript_scripts, scan.base_game_calls
            ));
        }
        for ((id, family, _, compatibility), tally) in &scan.calls {
            let (disposition, _) = disposition_of(catalog, game, id);
            *coverage.entry(disposition.to_owned()).or_default() += tally.occurrences;
            lines.push(format!(
                "    {id} {disposition} {family} {} ({compatibility}): {} call(s) in {} script(s)",
                tally.spelling,
                tally.occurrences,
                tally.scripts.len()
            ));
        }
        if scan.not_game_data > 0 {
            lines.push(format!("  not game data (documentation, installer files): {}", scan.not_game_data));
        }
        unclassified += scan.unclassified.len();
        if !scan.unclassified.is_empty() {
            lines.push(format!("  UNCLASSIFIED: {}", scan.unclassified.len()));
            lines.extend(scan.unclassified.iter().take(EXAMPLES * 4).map(|item| format!("    {item}")));
        }
        lines.extend(scan.notes.iter().map(|note| format!("  note: {note}")));
    }
    if !game_dir.is_empty() {
        lines.push("game directory:".to_owned());
        for (file, id) in game_dir {
            let (disposition, name) = disposition_of(catalog, game, id);
            *coverage.entry(disposition.to_owned()).or_default() += 1;
            lines.push(format!("  {file}: {id} {disposition} {name}"));
        }
    }
    lines.push(format!("asset paths more than one source ships: {}", conflicts.len()));
    lines.extend(conflicts.iter().take(EXAMPLES * 2).map(|line| format!("  {line}")));
    let summary: Vec<String> = coverage.iter().map(|(d, n)| format!("{d} {n}")).collect();
    lines.push(format!("coverage: {}; unclassified {unclassified}", summary.join(", ")));
    lines.push(if unclassified == 0 {
        "preflight: every item is classified".to_owned()
    } else {
        format!("preflight: {unclassified} unclassified item(s) — the catalog has no entry for them (§7.3 rule 2)")
    });
    lines
}

fn report_json(
    game: &str,
    catalog: &ModCatalog,
    scans: &[RootScan],
    game_dir: &[(String, String)],
    conflicts: &[String],
) -> serde_json::Value {
    let roots: Vec<serde_json::Value> = scans
        .iter()
        .map(|scan| {
            let mechanisms: serde_json::Map<String, serde_json::Value> = scan
                .mechanisms
                .iter()
                .map(|(id, tally)| {
                    let (disposition, _) = disposition_of(catalog, game, id);
                    (id.clone(), serde_json::json!({"disposition": disposition, "count": tally.count, "examples": tally.examples}))
                })
                .collect();
            let calls: Vec<serde_json::Value> = scan
                .calls
                .iter()
                .map(|((id, family, _, compatibility), tally)| {
                    serde_json::json!({
                        "mechanism": id, "disposition": disposition_of(catalog, game, id).0, "family": family,
                        "function": tally.spelling, "compatibility": compatibility, "occurrences": tally.occurrences,
                        "scripts": tally.scripts.len(),
                    })
                })
                .collect();
            serde_json::json!({
                "root": scan.label, "unmanaged": scan.unmanaged, "disabled": scan.disabled, "files": scan.files,
                "mechanisms": mechanisms, "calls": calls, "papyrus_scripts": scan.papyrus_scripts,
                "obscript_scripts": scan.obscript_scripts, "base_game_calls": scan.base_game_calls,
                "not_game_data": scan.not_game_data,
                "unclassified": scan.unclassified, "notes": scan.notes,
            })
        })
        .collect();
    let game_dir: Vec<serde_json::Value> = game_dir
        .iter()
        .map(|(file, id)| serde_json::json!({"file": file, "mechanism": id, "disposition": disposition_of(catalog, game, id).0}))
        .collect();
    serde_json::json!({
        "game": game, "catalog_version": catalog.version, "roots": roots, "game_dir": game_dir,
        "asset_conflicts": conflicts.len(), "asset_conflict_examples": conflicts.iter().take(50).collect::<Vec<_>>(),
        "unclassified": scans.iter().map(|scan| scan.unclassified.len()).sum::<usize>(),
    })
}

#[cfg(test)]
#[path = "preflight_tests.rs"]
mod tests;
