//! Skyrim MQ101 ("Unbound") intro conformance probe.
//!
//! This is the data-ingress gate for the carriage/waking-up vertical slice.
//! It proves that the production ESM, BSA, PEX, decompiler, and fragment
//! lowerer paths can all see the vanilla inputs the runtime will eventually
//! orchestrate.  Missing authored data or a parser regression is a hard
//! failure; incomplete effect-lowering coverage is reported as backlog and
//! does not fail the probe.
//!
//! ```bash
//! cargo run --release -p byroredux-scripting --example mq101_conformance -- \
//!     "<Skyrim Special Edition>/Data"
//! ```
//!
//! With no positional argument the probe uses `BYROREDUX_SKYRIM_DATA`, then
//! falls back to the repository's conventional local Steam path.

use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::path::{Path, PathBuf};

use byroredux_bsa::BsaArchive;
use byroredux_papyrus::ast::ScriptItem;
use byroredux_pex::{decompile::decompile_script, parse};
use byroredux_scripting::translate::effects::{lower_fragment, Effect};

const MQ101_FORM_ID: u32 = 0x0003_372b;
const DEFAULT_SKYRIM_DATA: &str =
    "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data";

const CRITICAL_SCRIPTS: &[&str] = &[
    "scripts\\qf_mq101_0003372b.pex",
    "scripts\\mq101questscript.pex",
    "scripts\\mq101playerscript.pex",
    "scripts\\mq101cartriderscript.pex",
    "scripts\\mq101startingcellloadregisterscript.pex",
];

const CRITICAL_ANIMATIONS: &[&str] = &[
    "meshes\\actors\\character\\animations\\carttravelplayeridle.hkx",
    "meshes\\actors\\character\\animations\\carttraveldriveridle.hkx",
    "meshes\\actors\\character\\animations\\cartprisonerbidle.hkx",
    "meshes\\actors\\character\\animations\\cartprisonercidle.hkx",
    "meshes\\actors\\character\\animations\\cartprisonerdidle.hkx",
];

#[derive(Default)]
struct Checks {
    passed: usize,
    failures: Vec<String>,
}

impl Checks {
    fn record(&mut self, name: &str, passed: bool, detail: impl AsRef<str>) {
        let detail = detail.as_ref();
        if passed {
            self.passed += 1;
            println!("PASS  {name:<26} {detail}");
        } else {
            self.failures.push(format!("{name}: {detail}"));
            println!("FAIL  {name:<26} {detail}");
        }
    }
}

fn effect_kind(effect: &Effect) -> &'static str {
    match effect {
        Effect::SetStage { .. } => "SetStage",
        Effect::SetObjectiveDisplayed { .. } => "SetObjectiveDisplayed",
        Effect::SetObjectiveCompleted { .. } => "SetObjectiveCompleted",
        Effect::SetObjectiveFailed { .. } => "SetObjectiveFailed",
        Effect::CompleteAllObjectives { .. } => "CompleteAllObjectives",
        Effect::AddItem { .. } => "AddItem",
        Effect::MoveTo { .. } => "MoveTo",
    }
}

fn is_mq101_pex(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.starts_with("scripts\\") && path.contains("mq101") && path.ends_with(".pex")
}

fn is_scene_fragment_pex(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.starts_with("scripts\\sf_mq101") && path.ends_with(".pex")
}

fn is_package_fragment_pex(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.starts_with("scripts\\pf_mq101") && path.ends_with(".pex")
}

fn is_mq101_voice(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.starts_with("sound\\voice\\") && path.contains("\\mq101") && path.ends_with(".fuz")
}

fn find_voice_archive(data_dir: &Path) -> Option<PathBuf> {
    let preferred = data_dir.join("Skyrim - Voices_en0.bsa");
    if preferred.is_file() {
        return Some(preferred);
    }

    let mut candidates: Vec<PathBuf> = std::fs::read_dir(data_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| {
                    let name = name.to_ascii_lowercase();
                    name.starts_with("skyrim - voices_") && name.ends_with(".bsa")
                })
                .unwrap_or(false)
        })
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

fn preview(items: &[String], limit: usize) -> String {
    let shown = items
        .iter()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > limit {
        format!("{shown}, ... (+{} more)", items.len() - limit)
    } else {
        shown
    }
}

fn data_dir_from_args() -> Result<PathBuf, String> {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    if matches!(first.as_deref(), Some("-h" | "--help")) {
        println!("usage: mq101_conformance [SKYRIM_DATA_DIR]");
        println!("       defaults to BYROREDUX_SKYRIM_DATA, then {DEFAULT_SKYRIM_DATA}");
        std::process::exit(0);
    }
    if let Some(extra) = args.next() {
        return Err(format!("unexpected extra argument: {extra}"));
    }
    Ok(first
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("BYROREDUX_SKYRIM_DATA").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SKYRIM_DATA)))
}

fn run() -> Result<Checks, Box<dyn Error>> {
    let data_dir = data_dir_from_args().map_err(std::io::Error::other)?;
    let esm_path = data_dir.join("Skyrim.esm");
    let scripts_path = data_dir.join("Skyrim - Misc.bsa");
    let animations_path = data_dir.join("Skyrim - Animations.bsa");

    println!("== Skyrim MQ101 conformance ==");
    println!("data: {}", data_dir.display());
    println!();

    let esm_bytes = std::fs::read(&esm_path)?;
    let index = byroredux_plugin::esm::parse_esm(&esm_bytes).map_err(std::io::Error::other)?;
    let mut checks = Checks::default();

    let Some(quest) = index.quests.get(&MQ101_FORM_ID) else {
        checks.record(
            "MQ101 record",
            false,
            format!(
                "QUST {MQ101_FORM_ID:08X} was not indexed from {}",
                esm_path.display()
            ),
        );
        return Ok(checks);
    };

    checks.record(
        "MQ101 record",
        quest.editor_id.eq_ignore_ascii_case("MQ101"),
        format!(
            "QUST {:08X} EDID={} stages={} objectives={}",
            quest.form_id,
            quest.editor_id,
            quest.stages.len(),
            quest.objectives.len()
        ),
    );
    checks.record(
        "quest aliases",
        !quest.aliases.is_empty(),
        format!("{} aliases decoded", quest.aliases.len()),
    );
    checks.record(
        "stage fragments",
        !quest.fragments.is_empty(),
        format!(
            "{} stage-to-function bindings decoded",
            quest.fragments.len()
        ),
    );

    let (attached_scripts, attached_properties) = quest
        .script_instance
        .as_ref()
        .map(|instance| {
            (
                instance.scripts.len(),
                instance
                    .scripts
                    .iter()
                    .map(|script| script.properties.len())
                    .sum::<usize>(),
            )
        })
        .unwrap_or_default();
    checks.record(
        "quest VMAD",
        attached_scripts > 0 && attached_properties > 0,
        format!("{attached_scripts} attached script(s), {attached_properties} properties"),
    );

    let scripts = BsaArchive::open(&scripts_path)?;
    let mut mq101_pex: Vec<String> = scripts
        .list_files()
        .into_iter()
        .filter(|path| is_mq101_pex(path))
        .map(str::to_owned)
        .collect();
    mq101_pex.sort();
    let scene_count = mq101_pex
        .iter()
        .filter(|path| is_scene_fragment_pex(path))
        .count();
    let package_count = mq101_pex
        .iter()
        .filter(|path| is_package_fragment_pex(path))
        .count();
    checks.record(
        "MQ101 PEX corpus",
        !mq101_pex.is_empty(),
        format!(
            "{} scripts ({scene_count} scene fragments, {package_count} package fragments)",
            mq101_pex.len()
        ),
    );
    checks.record(
        "scene fragment assets",
        scene_count > 0,
        format!("{scene_count} SF_MQ101*.pex files"),
    );
    checks.record(
        "package fragment assets",
        package_count > 0,
        format!("{package_count} PF_MQ101*.pex files"),
    );

    let missing_scripts: Vec<String> = CRITICAL_SCRIPTS
        .iter()
        .filter(|path| !scripts.contains(path))
        .map(|path| (*path).to_owned())
        .collect();
    checks.record(
        "critical scripts",
        missing_scripts.is_empty(),
        if missing_scripts.is_empty() {
            format!("all {} present", CRITICAL_SCRIPTS.len())
        } else {
            format!("missing: {}", preview(&missing_scripts, 5))
        },
    );

    let qf_path = CRITICAL_SCRIPTS[0];
    match scripts.extract(qf_path) {
        Err(error) => checks.record("QF decompile", false, format!("extract failed: {error}")),
        Ok(bytes) => match parse(&bytes) {
            Err(error) => checks.record(
                "QF decompile",
                false,
                format!("PEX parse failed: {error:?}"),
            ),
            Ok(pex) => {
                let decompiled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    decompile_script(&pex)
                }));
                match decompiled {
                    Err(_) => checks.record("QF decompile", false, "decompiler panicked"),
                    Ok(Err(error)) => checks.record(
                        "QF decompile",
                        false,
                        format!("AST lowering failed: {error:?}"),
                    ),
                    Ok(Ok(script)) => {
                        let functions: HashMap<String, _> = script
                            .body
                            .iter()
                            .filter_map(|item| match &item.node {
                                ScriptItem::Function(function) => {
                                    Some((function.name.node.0.to_ascii_lowercase(), function))
                                }
                                _ => None,
                            })
                            .collect();

                        let mut missing_bindings = Vec::new();
                        let mut no_op = 0usize;
                        let mut behavioral = 0usize;
                        let mut lowered = 0usize;
                        let mut effects = BTreeMap::<&'static str, usize>::new();

                        for binding in &quest.fragments {
                            let key = binding.fragment_name.to_ascii_lowercase();
                            let Some(function) = functions.get(&key) else {
                                missing_bindings.push(binding.fragment_name.clone());
                                continue;
                            };
                            if function.body.is_empty() {
                                no_op += 1;
                                continue;
                            }
                            behavioral += 1;
                            if let Some(fragment_effects) = lower_fragment(&function.body) {
                                lowered += 1;
                                for effect in &fragment_effects {
                                    *effects.entry(effect_kind(effect)).or_default() += 1;
                                }
                            }
                        }

                        checks.record(
                            "QF decompile",
                            true,
                            format!("{} callable functions recovered", functions.len()),
                        );
                        checks.record(
                            "fragment linkage",
                            missing_bindings.is_empty(),
                            if missing_bindings.is_empty() {
                                format!(
                                    "all {} QUST bindings resolve in the QF PEX",
                                    quest.fragments.len()
                                )
                            } else {
                                format!(
                                    "{} missing: {}",
                                    missing_bindings.len(),
                                    preview(&missing_bindings, 8)
                                )
                            },
                        );

                        let pct = if behavioral == 0 {
                            0.0
                        } else {
                            100.0 * lowered as f64 / behavioral as f64
                        };
                        println!();
                        println!("-- current MQ101 quest-fragment coverage (informational) --");
                        println!("bound no-op fragments:       {no_op}");
                        println!("bound behavioral fragments: {behavioral}");
                        println!("fully lowered today:         {lowered} ({pct:.1}%)");
                        println!("declined/backlog:            {}", behavioral - lowered);
                        if !effects.is_empty() {
                            println!("effects emitted:");
                            for (kind, count) in effects {
                                println!("  {kind:<24} {count}");
                            }
                        }
                    }
                }
            }
        },
    }

    let animations = BsaArchive::open(&animations_path)?;
    let cart_animation_count = animations
        .list_files()
        .into_iter()
        .filter(|path| {
            let path = path.to_ascii_lowercase();
            path.contains("cart") && path.ends_with(".hkx")
        })
        .count();
    let missing_animations: Vec<String> = CRITICAL_ANIMATIONS
        .iter()
        .filter(|path| !animations.contains(path))
        .map(|path| (*path).to_owned())
        .collect();
    checks.record(
        "critical cart HKX",
        missing_animations.is_empty(),
        if missing_animations.is_empty() {
            format!(
                "all {} present ({cart_animation_count} cart-related HKX files total)",
                CRITICAL_ANIMATIONS.len()
            )
        } else {
            format!("missing: {}", preview(&missing_animations, 5))
        },
    );

    match find_voice_archive(&data_dir) {
        None => checks.record(
            "MQ101 voice assets",
            false,
            "no Skyrim - Voices_*.bsa found",
        ),
        Some(path) => {
            let voices = BsaArchive::open(&path)?;
            let voice_count = voices
                .list_files()
                .into_iter()
                .filter(|voice| is_mq101_voice(voice))
                .count();
            checks.record(
                "MQ101 voice assets",
                voice_count > 0,
                format!(
                    "{voice_count} FUZ files in {}",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("voice archive")
                ),
            );
        }
    }

    println!();
    println!("-- authored workload inventory --");
    println!("quest aliases:          {}", quest.aliases.len());
    println!("quest stage bindings:   {}", quest.fragments.len());
    println!("MQ101 compiled scripts: {}", mq101_pex.len());
    println!("scene fragment scripts: {scene_count}");
    println!("package fragment scripts: {package_count}");

    Ok(checks)
}

fn main() {
    match run() {
        Err(error) => {
            eprintln!("mq101-conformance: input/probe error: {error}");
            std::process::exit(2);
        }
        Ok(checks) if checks.failures.is_empty() => {
            println!();
            println!("RESULT: PASS ({} checks)", checks.passed);
        }
        Ok(checks) => {
            println!();
            println!(
                "RESULT: FAIL ({} passed, {} failed)",
                checks.passed,
                checks.failures.len()
            );
            for failure in &checks.failures {
                println!("  - {failure}");
            }
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_classification_is_case_insensitive_and_scoped() {
        assert!(is_mq101_pex("Scripts\\QF_MQ101_0003372B.PEX"));
        assert!(is_scene_fragment_pex(
            "scripts\\SF_MQ101Scene1_0004679B.pex"
        ));
        assert!(is_package_fragment_pex(
            "scripts\\PF_MQ101PlayerToChoppingBlock_00065C94.pex"
        ));
        assert!(is_mq101_voice(
            "Sound\\Voice\\Skyrim.esm\\MaleNord\\MQ101__00046789_1.FUZ"
        ));
        assert!(!is_mq101_pex("scripts\\mq102questscript.pex"));
        assert!(!is_mq101_voice("sound\\fx\\mq101_cart.wav"));
    }

    #[test]
    fn preview_marks_truncation() {
        let items = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        assert_eq!(preview(&items, 2), "a, b, ... (+1 more)");
        assert_eq!(preview(&items, 3), "a, b, c");
    }
}
