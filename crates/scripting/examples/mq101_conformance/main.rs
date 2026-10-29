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
//! With no positional argument the probe uses `BYROREDUX_SKYRIMSE_DATA`, then
//! falls back to the repository's conventional local Steam path.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::path::{Path, PathBuf};

use byroredux_bsa::BsaArchive;
use byroredux_core::ecs::World;
use byroredux_papyrus::ast::{CallArg, Event, Expr, Function, Script, ScriptItem, Stmt};
use byroredux_papyrus::span::Spanned;
use byroredux_pex::{decompile::decompile_script, parse};
use byroredux_plugin::esm::records::condition::RunOn;
use byroredux_plugin::esm::records::script_instance::SceneFragmentEvent;
use byroredux_plugin::esm::records::{
    NpcRecord, PackDataInput, PackDataTargetKind, PackDataValue, PackLocationTarget,
    PackProcedure, PackRecord, QustRecord, SceneAction, SceneActionType, ScenRecord,
    QUEST_FLAG_START_GAME_ENABLED, SCENE_BEGIN_ON_QUEST_START, EsmIndex,
};
use byroredux_plugin::esm::CellData;
use byroredux_scripting::fragment::{populate_quest_fragments_from_script, quest_property_names};
use byroredux_scripting::papyrus_demo::quest_advance::{ActivatorGate, QuestAdvanceOnActivate};
use byroredux_scripting::papyrus_demo::PapyrusPlayerEntity;
use byroredux_scripting::quest_stages::QuestStageState;
use byroredux_scripting::translate::compose::QuestRef;
use byroredux_scripting::translate::effects::{
    lower_fragment_with_quest_properties, ActorRef, Effect, FragmentGuard, ValueExpr,
};
use byroredux_scripting::{
    dispatch_player_cinematic_animation_event, image_space_modifier_system,
    install_engine_start_quest, install_image_space_modifiers, install_scene_quest_aliases,
    install_scene_records, quest_startup_system, refresh_scene_actor_bindings,
    scene_playback_system, translate_pex, CinematicAnimationEvent, CinematicPresentationState,
    ConditionFunction, DialogueRegistry, ImageSpaceModifierApplication, QuestFormId,
    QuestStageFragments, SceneAliasCandidate, ScenePlaybackState, ScenePlayer, SceneRegistry,
};

mod ast_probe;
mod fragments;
mod packages;
mod report;
mod scenes;
mod scripts;

use ast_probe::*;
use fragments::*;
use packages::*;
use report::*;
use scenes::*;
use scripts::*;

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
    "meshes\\actors\\character\\animations\\cartdriveridlesway.hkx",
    "meshes\\actors\\character\\animations\\cartprisoneraidle.hkx",
    "meshes\\actors\\character\\animations\\cartprisonerbidle.hkx",
    "meshes\\actors\\character\\animations\\cartprisonercidle.hkx",
    "meshes\\actors\\character\\animations\\cartprisonerdidle.hkx",
    "meshes\\actors\\character\\animations\\cartprisonerasway.hkx",
    "meshes\\actors\\character\\animations\\cartprisonerbsway.hkx",
    "meshes\\actors\\character\\animations\\cartprisonercsway.hkx",
    "meshes\\actors\\character\\animations\\cartprisonerdsway.hkx",
];

const TWO_STATE_ACTIVATOR_SCRIPT: &str = "scripts\\default2stateactivator.pex";
const MQ101_GATE_1: u32 = 0x0009_0A05;
const OPENING_REFERENCES: &[(u32, &str)] = &[
    (0x0004_678E, "PlayerStartMarker"),
    (0x0004_6790, "RalofStartMarker"),
    (0x0004_6795, "HadvarStartMarker"),
    (0x000B_9DF3, "Cart1"),
    (0x000B_B970, "Cart2"),
    (0x000B_9DF2, "CartHorse1"),
    (0x000B_B971, "CartHorse2"),
    (0x0001_98BA, "GeneralTullius"),
    (0x0001_98BC, "Elenwen"),
    (0x0001_B131, "Ulfric"),
    (0x0006_54FB, "Prisoner01"),
    (0x0006_54ED, "StormcloakPrisoner01"),
    (0x0006_54F0, "StormcloakPrisoner02"),
    (0x0006_54EF, "StormcloakPrisoner03"),
    (0x0006_54EE, "StormcloakPrisoner04"),
    (0x0006_54F6, "ImperialSoldier01"),
    (0x0006_54F5, "ImperialSoldier02"),
    (0x0006_54FA, "ImperialSoldierHelgen02"),
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
        println!("       defaults to BYROREDUX_SKYRIMSE_DATA, then {DEFAULT_SKYRIM_DATA}");
        std::process::exit(0);
    }
    if let Some(extra) = args.next() {
        return Err(format!("unexpected extra argument: {extra}"));
    }
    Ok(first
        .map(PathBuf::from)
        .or_else(|| std::env::var_os(byroredux_plugin::esm::test_paths::SKYRIM_SE_ENV).map(PathBuf::from))
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

    // Stage order mirrors the authored data flow: QUST record -> SCEN
    // records and runtime plans -> per-scene structural analysis ->
    // alias/linkage resolution -> BSA script ingress -> fragment and
    // callback contracts -> IMAD runtime -> quest-fragment lowering ->
    // animation/voice archives -> workload inventory.
    check_quest_record(&mut checks, &index, quest);
    let scenes = check_scene_runtime(&mut checks, &index, quest);
    let mut inventory = check_scene_structure(&mut checks, &index, quest, &scenes);
    check_scene_linkage(&mut checks, &index, quest, &scenes, &mut inventory);
    let corpus = check_scripts(&mut checks, &index, &scripts_path, &inventory)?;
    check_fragments(&mut checks, &scenes, &corpus);
    check_imad_runtime(&mut checks, &index, quest);
    check_two_state_gate(&mut checks, &index, &corpus.scripts);
    check_quest_fragments(&mut checks, quest, &corpus.scripts);
    check_animations(&mut checks, &animations_path)?;
    check_voice(&mut checks, &data_dir)?;
    print_inventory(quest, &scenes, &inventory, &corpus);
    Ok(checks)
}

/// Stage 1 — the indexed QUST record itself: EDID, flags, aliases, stage
/// bindings, and the attached VMAD script/property counts.
fn check_quest_record(checks: &mut Checks, index: &EsmIndex, quest: &QustRecord) {
    let opening_cells: Vec<String> = OPENING_REFERENCES
        .iter()
        .map(|(form_id, name)| {
            format!(
                "{name}=0x{form_id:08X}@{:?}",
                index.cells.cell_for_refr_form_id(*form_id)
            )
        })
        .collect();
    checks.record(
        "opening reference cells",
        OPENING_REFERENCES
            .iter()
            .all(|(form_id, _)| index.cells.cell_for_refr_form_id(*form_id).is_some()),
        opening_cells.join(", "),
    );

    checks.record(
        "MQ101 record",
        quest.editor_id.eq_ignore_ascii_case("MQ101"),
        format!(
            "QUST {:08X} EDID={} flags={:02X} startup={:?} stages={} objectives={}",
            quest.form_id,
            quest.editor_id,
            quest.quest_flags,
            quest.start_up_stage,
            quest.stages.len(),
            quest.objectives.len()
        ),
    );
    checks.record(
        "quest startup contract",
        quest.quest_flags & QUEST_FLAG_START_GAME_ENABLED == 0 && quest.start_up_stage == Some(0),
        format!(
            "engine-root quest (Start Game Enabled={}), authored startup stage {:?}",
            quest.quest_flags & QUEST_FLAG_START_GAME_ENABLED != 0,
            quest.start_up_stage
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
}

/// Stage 10 — the critical cart HKX animations in the animations archive.
fn check_animations(checks: &mut Checks, animations_path: &Path) -> Result<(), Box<dyn Error>> {
    let animations = BsaArchive::open(animations_path)?;
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
    Ok(())
}

/// Stage 11 — MQ101 FUZ voice lines in the voices archive.
fn check_voice(checks: &mut Checks, data_dir: &Path) -> Result<(), Box<dyn Error>> {
    match find_voice_archive(data_dir) {
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
    Ok(())
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
