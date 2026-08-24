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

use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::path::{Path, PathBuf};

use byroredux_bsa::BsaArchive;
use byroredux_core::ecs::World;
use byroredux_papyrus::ast::{CallArg, Event, Expr, Script, ScriptItem, Stmt};
use byroredux_papyrus::span::Spanned;
use byroredux_pex::{decompile::decompile_script, parse};
use byroredux_plugin::esm::records::condition::RunOn;
use byroredux_plugin::esm::records::script_instance::SceneFragmentEvent;
use byroredux_plugin::esm::records::{
    SceneActionType, QUEST_FLAG_START_GAME_ENABLED, SCENE_BEGIN_ON_QUEST_START,
};
use byroredux_scripting::fragment::quest_property_names;
use byroredux_scripting::papyrus_demo::quest_advance::{ActivatorGate, QuestAdvanceOnActivate};
use byroredux_scripting::papyrus_demo::PlayerEntity;
use byroredux_scripting::quest_stages::QuestStageState;
use byroredux_scripting::translate::compose::QuestRef;
use byroredux_scripting::translate::effects::{lower_fragment_with_quest_properties, Effect};
use byroredux_scripting::{
    dispatch_player_cinematic_animation_event, image_space_modifier_system,
    install_engine_start_quest, install_image_space_modifiers, install_scene_quest_aliases,
    install_scene_records, quest_startup_system, refresh_scene_actor_bindings,
    scene_playback_system, translate_pex, CinematicAnimationEvent, CinematicPresentationState,
    ConditionFunction, DialogueRegistry, ImageSpaceModifierApplication, QuestFormId,
    SceneAliasCandidate, ScenePlaybackState, ScenePlayer, SceneRegistry,
};

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

fn effect_kind(effect: &Effect) -> &'static str {
    match effect {
        Effect::SetStage { .. } => "SetStage",
        Effect::StartQuest { .. } => "StartQuest",
        Effect::StopQuest { .. } => "StopQuest",
        Effect::CompleteQuest { .. } => "CompleteQuest",
        Effect::ResetQuest { .. } => "ResetQuest",
        Effect::SetQuestActive { .. } => "SetQuestActive",
        Effect::SetObjectiveDisplayed { .. } => "SetObjectiveDisplayed",
        Effect::SetObjectiveCompleted { .. } => "SetObjectiveCompleted",
        Effect::SetObjectiveFailed { .. } => "SetObjectiveFailed",
        Effect::CompleteAllObjectives { .. } => "CompleteAllObjectives",
        Effect::FailAllObjectives { .. } => "FailAllObjectives",
        Effect::AddItem { .. } => "AddItem",
        Effect::EquipItem { .. } => "EquipItem",
        Effect::MoveTo { .. } => "MoveTo",
        Effect::Disable { .. } => "Disable",
        Effect::StartScene { .. } => "StartScene",
        Effect::StopScene { .. } => "StopScene",
        Effect::Activate { .. } => "Activate",
        Effect::SetOpen { .. } => "SetOpen",
        Effect::SetPlayerRestrained { .. } => "SetPlayerRestrained",
        Effect::SetPlayerControls { .. } => "SetPlayerControls",
        Effect::SetPlayerAiDriven { .. } => "SetPlayerAiDriven",
        Effect::SetHudCartMode { .. } => "SetHudCartMode",
        Effect::PlayIdle { .. } => "PlayIdle",
        Effect::SetVehicle { .. } => "SetVehicle",
        Effect::TetherToHorse { .. } => "TetherToHorse",
        Effect::SetMotionType { .. } => "SetMotionType",
        Effect::SetSittingRotation { .. } => "SetSittingRotation",
        Effect::ExitCart { .. } => "ExitCart",
        Effect::RegisterPlayerAnimationEvent { .. } => "RegisterPlayerAnimationEvent",
        Effect::EvaluatePackage { .. } => "EvaluatePackage",
        Effect::Wait { .. } => "Wait",
        Effect::WaitForActors3DLoaded { .. } => "WaitForActors3DLoaded",
    }
}

fn expression_call_name(expression: &Expr) -> Option<String> {
    let Expr::Call { callee, .. } = expression else {
        return None;
    };
    match &callee.node {
        Expr::MemberAccess { member, .. } => Some(member.node.0.to_ascii_lowercase()),
        Expr::Ident(name) => Some(name.0.to_ascii_lowercase()),
        _ => Some("<dynamic-call>".to_owned()),
    }
}

fn statement_shape(statement: &Stmt) -> String {
    match statement {
        Stmt::ExprStmt(expression) => expression_call_name(&expression.node)
            .map_or_else(|| "expr".to_owned(), |name| format!("call:{name}")),
        Stmt::VarDecl(variable) => variable
            .initial_value
            .as_ref()
            .and_then(|value| expression_call_name(&value.node))
            .map_or_else(|| "let".to_owned(), |name| format!("let:{name}")),
        Stmt::Assign { value, .. } => expression_call_name(&value.node)
            .map_or_else(|| "assign".to_owned(), |name| format!("assign:{name}")),
        Stmt::If { .. } => "if".to_owned(),
        Stmt::While { .. } => "while".to_owned(),
        Stmt::Return(_) => "return".to_owned(),
    }
}

fn expression_contains_string(expression: &Expr, needle: &str) -> bool {
    match expression {
        Expr::StringLit(value) => value.eq_ignore_ascii_case(needle),
        Expr::MemberAccess { object, .. } => expression_contains_string(&object.node, needle),
        Expr::Index { object, index } => {
            expression_contains_string(&object.node, needle)
                || expression_contains_string(&index.node, needle)
        }
        Expr::Call { callee, args } => {
            expression_contains_string(&callee.node, needle)
                || args
                    .iter()
                    .any(|arg| expression_contains_string(&arg.value.node, needle))
        }
        Expr::UnaryOp { operand, .. } => expression_contains_string(&operand.node, needle),
        Expr::BinaryOp { left, right, .. } => {
            expression_contains_string(&left.node, needle)
                || expression_contains_string(&right.node, needle)
        }
        Expr::Cast { expr, .. } => expression_contains_string(&expr.node, needle),
        Expr::New { size, .. } => expression_contains_string(&size.node, needle),
        Expr::ArrayLit(values) => values
            .iter()
            .any(|value| expression_contains_string(&value.node, needle)),
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::BoolLit(_)
        | Expr::NoneLit
        | Expr::Ident(_)
        | Expr::ParentAccess => false,
    }
}

fn expression_contains_call(expression: &Expr, receiver: &str, method: &str) -> bool {
    let is_match = match expression {
        Expr::Call { callee, .. } => match &callee.node {
            Expr::MemberAccess { object, member } => {
                matches!(&object.node, Expr::Ident(name) if name.0.eq_ignore_ascii_case(receiver))
                    && member.node.0.eq_ignore_ascii_case(method)
            }
            _ => false,
        },
        _ => false,
    };
    if is_match {
        return true;
    }
    match expression {
        Expr::MemberAccess { object, .. } => {
            expression_contains_call(&object.node, receiver, method)
        }
        Expr::Index { object, index } => {
            expression_contains_call(&object.node, receiver, method)
                || expression_contains_call(&index.node, receiver, method)
        }
        Expr::Call { callee, args } => {
            expression_contains_call(&callee.node, receiver, method)
                || args
                    .iter()
                    .any(|arg| expression_contains_call(&arg.value.node, receiver, method))
        }
        Expr::UnaryOp { operand, .. } => expression_contains_call(&operand.node, receiver, method),
        Expr::BinaryOp { left, right, .. } => {
            expression_contains_call(&left.node, receiver, method)
                || expression_contains_call(&right.node, receiver, method)
        }
        Expr::Cast { expr, .. } => expression_contains_call(&expr.node, receiver, method),
        Expr::New { size, .. } => expression_contains_call(&size.node, receiver, method),
        Expr::ArrayLit(values) => values
            .iter()
            .any(|value| expression_contains_call(&value.node, receiver, method)),
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::BoolLit(_)
        | Expr::StringLit(_)
        | Expr::NoneLit
        | Expr::Ident(_)
        | Expr::ParentAccess => false,
    }
}

fn statement_call(statement: &Stmt) -> Option<(&Expr, &str, &[CallArg])> {
    let Stmt::ExprStmt(expression) = statement else {
        return None;
    };
    let Expr::Call { callee, args } = &expression.node else {
        return None;
    };
    let Expr::MemberAccess { object, member } = &callee.node else {
        return None;
    };
    Some((&object.node, member.node.0.as_str(), args))
}

fn ident_is(expression: &Expr, expected: &str) -> bool {
    matches!(expression, Expr::Ident(name) if name.0.eq_ignore_ascii_case(expected))
}

fn single_float_arg(args: &[CallArg], expected: f64) -> bool {
    matches!(args, [arg] if matches!(arg.value.node, Expr::FloatLit(value) if value == expected))
}

fn single_int_arg(args: &[CallArg], expected: i64) -> bool {
    matches!(args, [arg] if matches!(arg.value.node, Expr::IntLit(value) if value == expected))
}

fn unregister_call(statement: &Stmt, event_name: &str) -> bool {
    let Some((receiver, method, args)) = statement_call(statement) else {
        return false;
    };
    ident_is(receiver, "self")
        && method.eq_ignore_ascii_case("UnregisterForAnimationEvent")
        && matches!(args, [source, event]
            if expression_contains_call(&source.value.node, "game", "GetPlayer")
                && matches!(&event.value.node, Expr::StringLit(value)
                    if value.eq_ignore_ascii_case(event_name)))
}

fn callback_branch<'a>(event: &'a Event, event_name: &str) -> Option<&'a [Spanned<Stmt>]> {
    event.body.iter().find_map(|statement| {
        let Stmt::If {
            condition, body, ..
        } = &statement.node
        else {
            return None;
        };
        (expression_contains_string(&condition.node, event_name)
            && expression_contains_call(&condition.node, "game", "GetPlayer"))
        .then_some(body.as_slice())
    })
}

fn mq101_player_callback_contract(script: &Script) -> bool {
    let Some(event) = script.body.iter().find_map(|item| match &item.node {
        ScriptItem::Event(event) if event.name.node.0.eq_ignore_ascii_case("OnAnimationEvent") => {
            Some(event)
        }
        _ => None,
    }) else {
        return false;
    };
    let Some(play_imod) = callback_branch(event, "PlayImod") else {
        return false;
    };
    let Some(furniture_exit) = callback_branch(event, "IdleFurnitureExit") else {
        return false;
    };

    let play_imod_ok = matches!(play_imod, [player_imod, blur, stage, unregister]
        if matches!(statement_call(&player_imod.node), Some((receiver, method, args))
            if ident_is(receiver, "::PlayerAlduinIMOD_var")
                && method.eq_ignore_ascii_case("Apply")
                && single_float_arg(args, 1.0))
        && matches!(statement_call(&blur.node), Some((receiver, method, args))
            if ident_is(receiver, "::CGDragonAttackBlurLong_var")
                && method.eq_ignore_ascii_case("Apply")
                && single_float_arg(args, 1.0))
        && matches!(statement_call(&stage.node), Some((receiver, method, args))
            if ident_is(receiver, "self")
                && method.eq_ignore_ascii_case("SetStage")
                && single_int_arg(args, 145))
        && unregister_call(&unregister.node, "PlayImod"));
    let furniture_exit_ok = matches!(furniture_exit, [stage, unregister]
        if matches!(statement_call(&stage.node), Some((receiver, method, args))
            if ident_is(receiver, "self")
                && method.eq_ignore_ascii_case("SetStage")
                && single_int_arg(args, 160))
        && unregister_call(&unregister.node, "IdleFurnitureExit"));
    play_imod_ok && furniture_exit_ok
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

    let mut mq101_scenes: Vec<_> = index
        .scenes
        .values()
        .filter(|scene| scene.quest_form_id == Some(MQ101_FORM_ID))
        .collect();
    mq101_scenes.sort_by(|left, right| left.editor_id.cmp(&right.editor_id));
    let primary_scene = mq101_scenes
        .iter()
        .copied()
        .find(|scene| scene.editor_id.eq_ignore_ascii_case("MQ101Scene1"));
    checks.record(
        "MQ101 scene records",
        !mq101_scenes.is_empty() && primary_scene.is_some(),
        format!(
            "{} quest-owned SCEN records; MQ101Scene1 {}",
            mq101_scenes.len(),
            primary_scene
                .map(|scene| format!(
                    "{:08X}: {} phases, {} actors, {} actions",
                    scene.form_id,
                    scene.phases.len(),
                    scene.actors.len(),
                    scene.actions.len()
                ))
                .unwrap_or_else(|| "missing".to_owned())
        ),
    );
    checks.record(
        "primary scene auto-start",
        primary_scene.is_some_and(|scene| scene.flags & SCENE_BEGIN_ON_QUEST_START != 0),
        primary_scene
            .map(|scene| {
                format!(
                    "MQ101Scene1 flags={:08X}, Begin On Quest Start={}",
                    scene.flags,
                    scene.flags & SCENE_BEGIN_ON_QUEST_START != 0
                )
            })
            .unwrap_or_else(|| "MQ101Scene1 missing".to_owned()),
    );

    // Construct the same immutable registry + per-scene player shape the
    // live cell loader installs. This catches parser/runtime type drift even
    // before actor alias and dialogue executors are available in a smoke run.
    let runtime_registry =
        SceneRegistry::from_records(mq101_scenes.iter().map(|scene| (**scene).clone()));
    let runtime_primary = primary_scene.and_then(|scene| {
        runtime_registry
            .definition(scene.form_id)
            .map(|definition| (scene, definition, ScenePlayer::new(scene.form_id)))
    });
    checks.record(
        "scene runtime plan",
        runtime_registry.len() == mq101_scenes.len()
            && runtime_primary
                .as_ref()
                .is_some_and(|(source, definition, player)| {
                    definition.phases.len() == source.phases.len()
                        && definition.actions.len() == source.actions.len()
                        && player.scene_form_id == source.form_id
                        && player.current_phase == 0
                        && player.state == ScenePlaybackState::Dormant
                }),
        format!(
            "{} definitions construct SceneRegistry/ScenePlayer state; MQ101Scene1 {}",
            runtime_registry.len(),
            if runtime_primary.is_some() {
                "ready"
            } else {
                "missing"
            }
        ),
    );

    let mq101_topic_ids: HashSet<u32> = mq101_scenes
        .iter()
        .flat_map(|scene| &scene.actions)
        .filter(|action| action.action_type == SceneActionType::Dialogue)
        .filter_map(|action| action.topic_form_id)
        .collect();
    let runtime_dialogue_registry = DialogueRegistry::from_records(
        mq101_topic_ids
            .iter()
            .filter_map(|topic| index.dialogues.get(topic).cloned()),
    );
    let empty_runtime_topics: Vec<String> = mq101_topic_ids
        .iter()
        .filter_map(|topic| {
            runtime_dialogue_registry
                .topic(*topic)
                .filter(|record| record.infos.is_empty())
                .map(|record| format!("{:08X} {}", record.form_id, record.editor_id))
        })
        .collect();
    let mq101_info_count: usize = mq101_topic_ids
        .iter()
        .filter_map(|topic| runtime_dialogue_registry.topic(*topic))
        .map(|topic| topic.infos.len())
        .sum();
    checks.record(
        "dialogue runtime plan",
        runtime_dialogue_registry.len() == mq101_topic_ids.len()
            && mq101_info_count > 0,
        format!(
            "{} unique scene DIAL topics expose {mq101_info_count} authored INFO responses; {} empty placeholder(s) use fail-safe completion{}",
            runtime_dialogue_registry.len(),
            empty_runtime_topics.len(),
            if empty_runtime_topics.is_empty() {
                String::new()
            } else {
                format!(": {}", preview(&empty_runtime_topics, 8))
            }
        ),
    );

    let mut bootstrap_world = World::new();
    byroredux_scripting::register(&mut bootstrap_world);
    let bootstrap_player = bootstrap_world.spawn();
    bootstrap_world.insert_resource(PlayerEntity(bootstrap_player));
    bootstrap_world.insert_resource(QuestStageState::default());
    install_scene_records(
        &mut bootstrap_world,
        mq101_scenes.iter().map(|scene| (**scene).clone()),
    );
    install_engine_start_quest(
        &mut bootstrap_world,
        QuestFormId(quest.form_id),
        quest.start_up_stage,
    );
    quest_startup_system(&bootstrap_world, 0.0);
    scene_playback_system(&bootstrap_world, 0.0);
    let bootstrapped_primary = primary_scene.and_then(|scene| {
        let entity = bootstrap_world
            .resource::<SceneRegistry>()
            .scene_entity(scene.form_id)?;
        bootstrap_world
            .get::<ScenePlayer>(entity)
            .map(|player| player.clone())
    });
    checks.record(
        "new-game scene bootstrap",
        bootstrap_world
            .resource::<QuestStageState>()
            .is_started(QuestFormId(MQ101_FORM_ID))
            && bootstrapped_primary
                .as_ref()
                .is_some_and(|player| player.state != ScenePlaybackState::Dormant),
        format!(
            "MQ101 stage={}, MQ101Scene1 state={:?}",
            bootstrap_world
                .resource::<QuestStageState>()
                .get_stage(QuestFormId(MQ101_FORM_ID)),
            bootstrapped_primary.as_ref().map(|player| &player.state)
        ),
    );

    let quest_alias_ids: HashSet<i32> = quest.aliases.iter().map(|alias| alias.alias_id).collect();
    let mut dialogue_actions = 0usize;
    let mut package_actions = 0usize;
    let mut package_action_stacks = BTreeMap::<usize, usize>::new();
    let mut package_procedures = BTreeMap::<u32, usize>::new();
    let mut package_procedure_samples = BTreeMap::<u32, Vec<String>>::new();
    let mut package_templates = BTreeMap::<String, usize>::new();
    let mut package_tree_procedures = BTreeMap::<String, usize>::new();
    let mut package_tree_procedure_samples = BTreeMap::<String, Vec<String>>::new();
    let mut unresolved_package_templates = Vec::new();
    let mut package_execution_kinds = BTreeMap::<String, usize>::new();
    let mut unresolved_movement_targets = Vec::new();
    let mut package_actions_with_phase_completion_conditions = 0usize;
    let mut timer_actions = 0usize;
    let mut unknown_actions = Vec::new();
    let mut invalid_ranges = Vec::new();
    let mut unresolved_aliases = Vec::new();
    let mut unresolved_action_actors = Vec::new();
    let mut unresolved_topics = Vec::new();
    let mut unresolved_packages = Vec::new();
    let mut invalid_fragment_phases = Vec::new();
    let mut scene_fragment_scripts = HashSet::new();
    let mut total_scene_phases = 0usize;
    let mut total_scene_actions = 0usize;
    let mut total_scene_fragments = 0usize;
    let mut scene_condition_functions = BTreeMap::<u32, usize>::new();
    let mut unsupported_scene_condition_functions = BTreeMap::<u32, usize>::new();
    let mut unsupported_scene_condition_sites = BTreeMap::<u32, Vec<String>>::new();
    let mut quest_alias_conditions = 0usize;

    for scene in &mq101_scenes {
        total_scene_phases += scene.phases.len();
        total_scene_actions += scene.actions.len();
        total_scene_fragments += scene.fragments.len();

        let mut record_condition =
            |scope: String, condition: &byroredux_plugin::esm::records::condition::Condition| {
                *scene_condition_functions
                    .entry(condition.function_index)
                    .or_default() += 1;
                if matches!(
                    ConditionFunction::from_index(condition.function_index),
                    ConditionFunction::Unknown(_)
                ) {
                    *unsupported_scene_condition_functions
                        .entry(condition.function_index)
                        .or_default() += 1;
                    unsupported_scene_condition_sites
                        .entry(condition.function_index)
                        .or_default()
                        .push(format!(
                            "{} {scope} run={:?} p1={:08X} p2={} extra={}",
                            scene.editor_id,
                            condition.run_on,
                            condition.param_1,
                            condition.param_2,
                            condition.extra_data_id,
                        ));
                }
                if condition.run_on == RunOn::QuestAlias {
                    quest_alias_conditions += 1;
                }
            };
        for condition in &scene.conditions {
            record_condition("scene-start".to_owned(), condition);
        }
        for (phase_index, phase) in scene.phases.iter().enumerate() {
            for condition in &phase.start_conditions {
                record_condition(format!("phase-{phase_index}-start"), condition);
            }
            for condition in &phase.completion_conditions {
                record_condition(format!("phase-{phase_index}-completion"), condition);
            }
        }

        let actor_ids: HashSet<i32> = scene
            .actors
            .iter()
            .map(|actor| actor.actor_id as i32)
            .collect();
        for actor in &scene.actors {
            if !quest_alias_ids.contains(&(actor.actor_id as i32)) {
                unresolved_aliases.push(format!("{}:alias{}", scene.editor_id, actor.actor_id));
            }
        }

        for action in &scene.actions {
            match action.action_type {
                SceneActionType::Dialogue => dialogue_actions += 1,
                SceneActionType::Package => {
                    package_actions += 1;
                    *package_action_stacks
                        .entry(action.packages.len())
                        .or_default() += 1;
                    if scene
                        .phases
                        .get(action.end_phase as usize)
                        .is_some_and(|phase| !phase.completion_conditions.is_empty())
                    {
                        package_actions_with_phase_completion_conditions += 1;
                    }
                    for package_form_id in &action.packages {
                        if let Some(package) = index.packages.get(package_form_id) {
                            *package_procedures
                                .entry(package.procedure_type)
                                .or_default() += 1;
                            let samples = package_procedure_samples
                                .entry(package.procedure_type)
                                .or_default();
                            if samples.len() < 5 {
                                samples.push(format!(
                                    "{}:{} {}",
                                    scene.editor_id, action.index, package.editor_id
                                ));
                            }
                            let procedure_tree = match package.package_template_form_id {
                                Some(template_form_id) => {
                                    match index.packages.get(&template_form_id) {
                                        Some(template) => {
                                            *package_templates
                                                .entry(format!(
                                                    "{:08X} {}",
                                                    template.form_id, template.editor_id
                                                ))
                                                .or_default() += 1;
                                            Some(template)
                                        }
                                        None => {
                                            unresolved_package_templates.push(format!(
                                                "{}:{} {} -> {template_form_id:08X}",
                                                scene.editor_id, action.index, package.editor_id
                                            ));
                                            None
                                        }
                                    }
                                }
                                None if !package.procedures.is_empty() => Some(package),
                                None => {
                                    unresolved_package_templates.push(format!(
                                        "{}:{} {} has no template/tree",
                                        scene.editor_id, action.index, package.editor_id
                                    ));
                                    None
                                }
                            };
                            if let Some(tree) = procedure_tree {
                                for procedure in &tree.procedures {
                                    *package_tree_procedures
                                        .entry(procedure.procedure_type.clone())
                                        .or_default() += 1;
                                    let samples = package_tree_procedure_samples
                                        .entry(procedure.procedure_type.clone())
                                        .or_default();
                                    if samples.len() < 5 {
                                        samples.push(format!(
                                            "{}:{} {} via {}{}",
                                            scene.editor_id,
                                            action.index,
                                            package.editor_id,
                                            tree.editor_id,
                                            if procedure.success_completes_package() {
                                                " [completes]"
                                            } else {
                                                ""
                                            }
                                        ));
                                    }
                                }
                                if let Some(procedure) = tree
                                    .procedures
                                    .iter()
                                    .find(|procedure| procedure.success_completes_package())
                                    .or_else(|| tree.procedures.first())
                                {
                                    let kind = match procedure.procedure_type.as_str() {
                                        "Travel" | "Patrol" | "Escort" | "FollowTo" => {
                                            let target = procedure
                                                .data_input_indexes
                                                .iter()
                                                .filter_map(|index| {
                                                    package
                                                        .data_inputs
                                                        .iter()
                                                        .find(|input| input.index == *index)
                                                })
                                                .find_map(|input| match input.value {
                                                    byroredux_plugin::esm::records::PackDataValue::Location(location) => match location.target {
                                                        byroredux_plugin::esm::records::PackLocationTarget::NearReference(form_id) => Some(form_id),
                                                        _ => None,
                                                    },
                                                    byroredux_plugin::esm::records::PackDataValue::Target(target) => match target.target {
                                                        byroredux_plugin::esm::records::PackDataTargetKind::SpecificReference(form_id)
                                                        | byroredux_plugin::esm::records::PackDataTargetKind::LinkedReference(form_id) => Some(form_id),
                                                        _ => None,
                                                    },
                                                    _ => None,
                                                });
                                            if let Some(target) = target {
                                                if target == 0x14 {
                                                    "move-dynamic"
                                                } else {
                                                    let resolved =
                                                        index.cells.cells.values().any(|cell| {
                                                            cell.references.iter().any(|placed| {
                                                                placed.form_id == target
                                                            })
                                                        }) || index
                                                            .cells
                                                            .exterior_cells
                                                            .values()
                                                            .any(|grids| {
                                                                grids.values().any(|cell| {
                                                                    cell.references.iter().any(
                                                                        |placed| {
                                                                            placed.form_id == target
                                                                        },
                                                                    )
                                                                })
                                                            })
                                                            || index
                                                                .cells
                                                                .worldspace_persistent_cells
                                                                .values()
                                                                .any(|cell| {
                                                                    cell.references.iter().any(
                                                                        |placed| {
                                                                            placed.form_id == target
                                                                        },
                                                                    )
                                                                });
                                                    if resolved {
                                                        "move-to"
                                                    } else {
                                                        unresolved_movement_targets.push(format!(
                                                            "{}:{} {} {} -> {target:08X}",
                                                            scene.editor_id,
                                                            action.index,
                                                            package.editor_id,
                                                            procedure.procedure_type,
                                                        ));
                                                        "await-target"
                                                    }
                                                }
                                            } else {
                                                let dynamic = procedure
                                                    .data_input_indexes
                                                    .iter()
                                                    .filter_map(|index| {
                                                        package.data_inputs.iter().find(|input| input.index == *index)
                                                    })
                                                    .any(|input| match input.value {
                                                        byroredux_plugin::esm::records::PackDataValue::Location(location) => matches!(location.location_type, 2 | 3 | 8 | 9),
                                                        byroredux_plugin::esm::records::PackDataValue::Target(target) => matches!(
                                                            target.target,
                                                            byroredux_plugin::esm::records::PackDataTargetKind::ReferenceAlias(_)
                                                                | byroredux_plugin::esm::records::PackDataTargetKind::SelfTarget
                                                        ),
                                                        _ => false,
                                                    });
                                                if dynamic {
                                                    "move-dynamic"
                                                } else {
                                                    unresolved_movement_targets.push(format!(
                                                        "{}:{} {} {} has no resolvable target input",
                                                        scene.editor_id,
                                                        action.index,
                                                        package.editor_id,
                                                        procedure.procedure_type,
                                                    ));
                                                    "await-target"
                                                }
                                            }
                                        }
                                        "Acquire" | "Activate" | "Shout" | "Sit"
                                        | "UseIdleMarker" | "UseWeapon" => "interaction",
                                        _ => "await-external",
                                    };
                                    *package_execution_kinds.entry(kind.to_owned()).or_default() +=
                                        1;
                                }
                            }
                        }
                    }
                }
                SceneActionType::Timer => timer_actions += 1,
                SceneActionType::Unknown(raw) => {
                    unknown_actions
                        .push(format!("{}:{} type {raw}", scene.editor_id, action.index));
                }
            }
            if action.start_phase > action.end_phase
                || action.end_phase as usize >= scene.phases.len()
            {
                invalid_ranges.push(format!(
                    "{}:{} {}..{} of {}",
                    scene.editor_id,
                    action.index,
                    action.start_phase,
                    action.end_phase,
                    scene.phases.len()
                ));
            }
            if action.actor_id >= 0 && !actor_ids.contains(&action.actor_id) {
                unresolved_action_actors.push(format!(
                    "{}:{} actor {}",
                    scene.editor_id, action.index, action.actor_id
                ));
            }
            if let Some(topic) = action.topic_form_id {
                if !index.dialogues.contains_key(&topic) {
                    unresolved_topics.push(format!(
                        "{}:{} DIAL {topic:08X}",
                        scene.editor_id, action.index
                    ));
                }
            }
            for package in &action.packages {
                if !index.packages.contains_key(package) {
                    unresolved_packages.push(format!(
                        "{}:{} PACK {package:08X}",
                        scene.editor_id, action.index
                    ));
                }
            }
        }

        for fragment in &scene.fragments {
            scene_fragment_scripts.insert(fragment.script_name.to_ascii_lowercase());
            let phase_index = match fragment.event {
                SceneFragmentEvent::PhaseStart { phase_index }
                | SceneFragmentEvent::PhaseCompletion { phase_index }
                | SceneFragmentEvent::UnknownPhase { phase_index, .. } => Some(phase_index),
                _ => None,
            };
            if phase_index.is_some_and(|index| index as usize >= scene.phases.len()) {
                invalid_fragment_phases.push(format!("{}:{:?}", scene.editor_id, fragment.event));
            }
        }
    }

    checks.record(
        "scene action structure",
        total_scene_actions > 0 && unknown_actions.is_empty() && invalid_ranges.is_empty(),
        format!(
            "{total_scene_actions} actions ({dialogue_actions} dialogue, {package_actions} package, {timer_actions} timer); {} unknown, {} invalid ranges",
            unknown_actions.len(),
            invalid_ranges.len()
        ),
    );
    checks.record(
        "scene condition coverage",
        unsupported_scene_condition_functions.is_empty(),
        if unsupported_scene_condition_functions.is_empty() {
            format!(
                "all {} authored CTDAs map to runtime functions",
                scene_condition_functions.values().sum::<usize>()
            )
        } else {
            format!("unsupported: {unsupported_scene_condition_functions:?}")
        },
    );
    checks.record(
        "scene actor linkage",
        unresolved_aliases.is_empty() && unresolved_action_actors.is_empty(),
        if unresolved_aliases.is_empty() && unresolved_action_actors.is_empty() {
            "all scene actors resolve through MQ101 aliases and action rosters".to_owned()
        } else {
            let mut failures = unresolved_aliases;
            failures.extend(unresolved_action_actors);
            format!("{} unresolved: {}", failures.len(), preview(&failures, 8))
        },
    );
    let mut alias_world = World::new();
    byroredux_scripting::register(&mut alias_world);
    for cell in index.cells.cells.values() {
        for placed in &cell.references {
            if index.npcs.contains_key(&placed.base_form_id) {
                let entity = alias_world.spawn();
                alias_world.insert(
                    entity,
                    SceneAliasCandidate {
                        reference_form_id: placed.form_id,
                        base_form_id: placed.base_form_id,
                        linked_refs: placed
                            .linked_refs
                            .iter()
                            .map(|link| (link.keyword, link.target))
                            .collect(),
                        location_ref_types: placed.location_ref_types.clone(),
                    },
                );
            }
        }
    }
    for grids in index.cells.exterior_cells.values() {
        for cell in grids.values() {
            for placed in &cell.references {
                if index.npcs.contains_key(&placed.base_form_id) {
                    let entity = alias_world.spawn();
                    alias_world.insert(
                        entity,
                        SceneAliasCandidate {
                            reference_form_id: placed.form_id,
                            base_form_id: placed.base_form_id,
                            linked_refs: placed
                                .linked_refs
                                .iter()
                                .map(|link| (link.keyword, link.target))
                                .collect(),
                            location_ref_types: placed.location_ref_types.clone(),
                        },
                    );
                }
            }
        }
    }
    for cell in index.cells.worldspace_persistent_cells.values() {
        for placed in &cell.references {
            if index.npcs.contains_key(&placed.base_form_id) {
                let entity = alias_world.spawn();
                alias_world.insert(
                    entity,
                    SceneAliasCandidate {
                        reference_form_id: placed.form_id,
                        base_form_id: placed.base_form_id,
                        linked_refs: placed
                            .linked_refs
                            .iter()
                            .map(|link| (link.keyword, link.target))
                            .collect(),
                        location_ref_types: placed.location_ref_types.clone(),
                    },
                );
            }
        }
    }
    let persistent_reference_count: usize = index
        .cells
        .worldspace_persistent_cells
        .values()
        .map(|cell| cell.references.len())
        .sum();
    install_scene_quest_aliases(&mut alias_world, [quest.clone()]);
    let resolved_alias_count = refresh_scene_actor_bindings(&alias_world);
    let unresolved_primary_aliases: Vec<String> = primary_scene
        .into_iter()
        .flat_map(|scene| &scene.actors)
        .filter(|actor| {
            alias_world
                .resource::<byroredux_scripting::SceneActorBindings>()
                .resolve(
                    byroredux_scripting::QuestFormId(MQ101_FORM_ID),
                    actor.actor_id as i32,
                )
                .is_none()
        })
        .map(|actor| actor.actor_id.to_string())
        .collect();
    checks.record(
        "primary alias resolution",
        primary_scene.is_some() && unresolved_primary_aliases.is_empty(),
        if unresolved_primary_aliases.is_empty() {
            format!(
                "all {} MQ101Scene1 actor slots bind from ACHR identity/XLRT ({resolved_alias_count} MQ101 aliases resolved total)",
                primary_scene.map_or(0, |scene| scene.actors.len()),
            )
        } else {
            format!(
                "unresolved MQ101Scene1 actor aliases: {}; indexed {} worldspace-persistent CELLs / {} refs",
                preview(&unresolved_primary_aliases, 13),
                index.cells.worldspace_persistent_cells.len(),
                persistent_reference_count,
            )
        },
    );
    checks.record(
        "scene record linkage",
        unresolved_topics.is_empty()
            && unresolved_packages.is_empty()
            && unresolved_package_templates.is_empty(),
        if unresolved_topics.is_empty()
            && unresolved_packages.is_empty()
            && unresolved_package_templates.is_empty()
        {
            format!(
                "all action DIAL/PACK FormIDs and {} custom package trees resolve",
                package_templates.len()
            )
        } else {
            let mut failures = unresolved_topics;
            failures.extend(unresolved_packages);
            failures.extend(unresolved_package_templates);
            format!("{} unresolved: {}", failures.len(), preview(&failures, 8))
        },
    );
    checks.record(
        "scene VMAD fragments",
        total_scene_fragments > 0 && invalid_fragment_phases.is_empty(),
        format!(
            "{total_scene_fragments} event bindings across {} compiled scene scripts; {} invalid phase indices",
            scene_fragment_scripts.len(),
            invalid_fragment_phases.len()
        ),
    );

    let scripts = BsaArchive::open(&scripts_path)?;
    let stage22_trigger = index
        .cells
        .cells
        .values()
        .flat_map(|cell| &cell.references)
        .chain(
            index
                .cells
                .exterior_cells
                .values()
                .flat_map(|grids| grids.values())
                .flat_map(|cell| &cell.references),
        )
        .chain(
            index
                .cells
                .worldspace_persistent_cells
                .values()
                .flat_map(|cell| &cell.references),
        )
        .find(|placed| placed.form_id == 0x000C_1F80);
    let stage22_recognized = stage22_trigger
        .and_then(|placed| placed.script_instance.as_ref())
        .and_then(|vmad| {
            scripts
                .extract("scripts\\defaultsetstagetrigspecificactor.pex")
                .ok()
                .and_then(|pex| translate_pex(&pex, index.game, Some(vmad), None))
        });
    let mut trigger_world = World::new();
    byroredux_scripting::register(&mut trigger_world);
    let trigger_entity = trigger_world.spawn();
    if let Some(recognized) = &stage22_recognized {
        (recognized.spawn)(&mut trigger_world, trigger_entity);
    }
    let trigger_component = trigger_world.get::<QuestAdvanceOnActivate>(trigger_entity);
    checks.record(
        "specific actor trigger",
        trigger_component.is_some_and(|component| {
            component.owning_quest == QuestFormId(MQ101_FORM_ID)
                && component.target_stage == 22
                && component.conditions.len() == 1
                && component.conditions[0].param_2 == 15
                && matches!(
                    component.activator_gate,
                    ActivatorGate::BaseForm(0x0006_54E5)
                )
                && component.disable_after_advance
        }),
        "0x000C1F80: MQ101Horse base gate + prereq 15 -> stage 22 + disable",
    );
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
    let mut missing_scene_fragment_scripts: Vec<String> = scene_fragment_scripts
        .iter()
        .filter_map(|script_name| {
            let path = format!("scripts\\{script_name}.pex");
            (!scripts.contains(&path)).then_some(path)
        })
        .collect();
    missing_scene_fragment_scripts.sort();
    checks.record(
        "scene fragment linkage",
        missing_scene_fragment_scripts.is_empty(),
        if missing_scene_fragment_scripts.is_empty() {
            format!(
                "all {} SCEN-bound script names resolve to PEX assets",
                scene_fragment_scripts.len()
            )
        } else {
            format!(
                "{} missing: {}",
                missing_scene_fragment_scripts.len(),
                preview(&missing_scene_fragment_scripts, 8)
            )
        },
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

    let callback_script = scripts
        .extract(CRITICAL_SCRIPTS[1])
        .ok()
        .and_then(|bytes| parse(&bytes).ok())
        .and_then(|pex| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decompile_script(&pex)))
                .ok()?
                .ok()
        });
    checks.record(
        "player callback contract",
        callback_script
            .as_ref()
            .is_some_and(mq101_player_callback_contract),
        "PlayImod => IMAD x2 + stage 145; IdleFurnitureExit => stage 160; both one-shot",
    );

    let callback_vmad = quest
        .script_instance
        .as_ref()
        .and_then(|vmad| vmad.script("MQ101QuestScript"));
    let modifier_forms = ["PlayerAlduinIMOD", "CGDragonAttackBlurLong"].map(|property| {
        (
            property,
            callback_vmad.and_then(|script| script.object_form_id(property)),
        )
    });
    let modifier_links_resolve = modifier_forms.iter().all(|(_, form_id)| {
        form_id.is_some_and(|form_id| index.imagespace_modifiers.contains_key(&form_id))
    });
    checks.record(
        "callback IMAD linkage",
        modifier_links_resolve,
        modifier_forms
            .iter()
            .map(|(property, form_id)| {
                form_id.map_or_else(
                    || format!("{property}=missing"),
                    |form_id| format!("{property}={form_id:08X}"),
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
    );
    let authored_imads: Vec<_> = modifier_forms
        .iter()
        .filter_map(|(_, form_id)| form_id.and_then(|id| index.imagespace_modifiers.get(&id)))
        .cloned()
        .collect();
    let curve_contract = authored_imads.len() == 2
        && authored_imads
            .iter()
            .all(|record| record.duration_seconds > 0.0 && !record.blur_radius.is_empty())
        && authored_imads
            .iter()
            .any(|record| !record.radial_blur_strength.is_empty());
    checks.record(
        "callback IMAD curves",
        curve_contract,
        authored_imads
            .iter()
            .map(|record| {
                format!(
                    "{}={:.1}s/{} blur keys/{} radial keys",
                    record.editor_id,
                    record.duration_seconds,
                    record.blur_radius.len(),
                    record.radial_blur_strength.len()
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
    );

    let mut imad_world = World::new();
    byroredux_scripting::register(&mut imad_world);
    imad_world.insert_resource(QuestStageState::default());
    install_image_space_modifiers(&mut imad_world, authored_imads);
    let applications: Vec<_> = modifier_forms
        .iter()
        .filter_map(|(_, form_id)| {
            form_id.map(|form_id| ImageSpaceModifierApplication {
                form_id,
                strength: 1.0,
            })
        })
        .collect();
    imad_world
        .resource_mut::<CinematicPresentationState>()
        .register_player_animation_event(
            CinematicAnimationEvent::PlayImod,
            QuestFormId(MQ101_FORM_ID),
            applications,
        );
    let callback_dispatched =
        dispatch_player_cinematic_animation_event(&imad_world, CinematicAnimationEvent::PlayImod)
            .is_some();
    image_space_modifier_system(&imad_world, 1.0);
    let sampled_imad = imad_world
        .resource::<CinematicPresentationState>()
        .image_space_modifier_frame;
    checks.record(
        "callback IMAD runtime",
        callback_dispatched
            && sampled_imad.blur_radius_pixels > 0.0
            && sampled_imad.radial_blur_strength > 0.0,
        format!(
            "t=1.0s blur={:.3}px radial={:.3} saturation={:.3}",
            sampled_imad.blur_radius_pixels,
            sampled_imad.radial_blur_strength,
            sampled_imad.saturation
        ),
    );

    let gate_script_instance = index
        .cells
        .cells
        .values()
        .flat_map(|cell| &cell.references)
        .find(|reference| reference.form_id == MQ101_GATE_1)
        .and_then(|reference| reference.script_instance.as_ref());
    let two_state_recognition = scripts
        .extract(TWO_STATE_ACTIVATOR_SCRIPT)
        .ok()
        .and_then(|bytes| translate_pex(&bytes, index.game, gate_script_instance, None));
    checks.record(
        "two-state gate runtime",
        two_state_recognition
            .as_ref()
            .is_some_and(|recognized| recognized.archetype.starts_with("two_state_activator@")),
        two_state_recognition.map_or_else(
            || "default2StateActivator PEX did not recognize".to_owned(),
            |recognized| format!("{} recognized from vanilla PEX", recognized.archetype),
        ),
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
                        // #2658 (SCR-D5-NEW11-03) — computed once per script,
                        // matching the production caller
                        // (`populate_quest_fragments_from_script`), and fed to
                        // `lower_fragment_with_quest_properties` at both call
                        // sites below so this conformance gate measures the
                        // SAME lowering path production runs.
                        let quest_properties = quest_property_names(&script);

                        let mut missing_bindings = Vec::new();
                        let mut no_op = 0usize;
                        let mut behavioral = 0usize;
                        let mut lowered = 0usize;
                        let mut effects = BTreeMap::<&'static str, usize>::new();
                        let mut declined_shapes = BTreeMap::<String, usize>::new();
                        let mut declined_samples = Vec::new();

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
                            if let Some(fragment_effects) = lower_fragment_with_quest_properties(
                                &function.body,
                                &quest_properties,
                            ) {
                                lowered += 1;
                                for effect in &fragment_effects {
                                    *effects.entry(effect_kind(effect)).or_default() += 1;
                                }
                            } else {
                                let shape = function
                                    .body
                                    .iter()
                                    .map(|statement| statement_shape(&statement.node))
                                    .collect::<Vec<_>>()
                                    .join(";");
                                *declined_shapes.entry(shape.clone()).or_default() += 1;
                                if declined_samples.len() < 16 {
                                    declined_samples
                                        .push(format!("{} {}", binding.fragment_name, shape));
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
                        let cart_init_effects =
                            functions.get("fragment_175").and_then(|function| {
                                lower_fragment_with_quest_properties(
                                    &function.body,
                                    &quest_properties,
                                )
                            });
                        let cart_init_kinds = cart_init_effects.as_ref().map(|effects| {
                            let mut kinds = BTreeMap::<&'static str, usize>::new();
                            for effect in effects {
                                *kinds.entry(effect_kind(effect)).or_default() += 1;
                            }
                            kinds
                        });
                        let cart_init_actor_gate_count =
                            cart_init_effects.as_ref().and_then(|effects| {
                                effects.iter().find_map(|effect| match effect {
                                    Effect::WaitForActors3DLoaded { actors, .. } => {
                                        Some(actors.len())
                                    }
                                    _ => None,
                                })
                            });
                        let cart_init_ok = cart_init_kinds.as_ref().is_some_and(|kinds| {
                            cart_init_actor_gate_count == Some(9)
                                && kinds.get("WaitForActors3DLoaded") == Some(&1)
                                && kinds.get("TetherToHorse") == Some(&2)
                                && kinds.get("Wait") == Some(&1)
                                && kinds.get("SetVehicle") == Some(&10)
                                && kinds.get("PlayIdle") == Some(&10)
                                && kinds.get("EquipItem") == Some(&1)
                        });
                        checks.record(
                            "cart init fragment",
                            cart_init_ok,
                            cart_init_kinds.map_or_else(
                                || "Fragment_175 did not lower".to_owned(),
                                |kinds| {
                                    format!(
                                        "{} effects, {:?} actors gated: {kinds:?}",
                                        cart_init_effects.unwrap().len(),
                                        cart_init_actor_gate_count
                                    )
                                },
                            ),
                        );
                        let stage_30_effects = functions.get("fragment_111").and_then(|function| {
                            lower_fragment_with_quest_properties(&function.body, &quest_properties)
                        });
                        let stage_30_ok = matches!(
                            stage_30_effects.as_deref(),
                            Some([
                                Effect::Disable { object, fade_out: false },
                                Effect::SetStage { quest: QuestRef::SelfRef, stage: 27 },
                                Effect::EvaluatePackage { actor: horse_1 },
                                Effect::EvaluatePackage { actor: horse_2 },
                            ]) if object.property_name().eq_ignore_ascii_case("CiviliansOutsideHelgenMarker")
                                && horse_1.property_name().eq_ignore_ascii_case("Alias_CartHorse1")
                                && horse_2.property_name().eq_ignore_ascii_case("Alias_CartHorse2")
                        );
                        checks.record(
                            "stage 30 handoff",
                            stage_30_ok,
                            stage_30_effects.map_or_else(
                                || "Fragment_111 did not lower".to_owned(),
                                |effects| format!("Fragment_111 lowered to {effects:?}"),
                            ),
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
                            println!("declined top-level shapes:");
                            for (shape, count) in declined_shapes.iter().take(20) {
                                println!("  {count:>3} {shape}");
                            }
                            println!("declined samples: {}", declined_samples.join(", "));
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
    println!("MQ101 scene records:    {}", mq101_scenes.len());
    println!("scene phases:           {total_scene_phases}");
    println!("scene actions:          {total_scene_actions}");
    println!(
        "package action stacks:  {:?} ({} end on phases with completion CTDAs)",
        package_action_stacks, package_actions_with_phase_completion_conditions
    );
    println!("package procedures:     {package_procedures:?}");
    for (procedure, samples) in &package_procedure_samples {
        println!("  {procedure}: {}", samples.join(", "));
    }
    println!("package templates:      {} unique", package_templates.len());
    for (template, count) in &package_templates {
        println!("  {template}: {count}");
    }
    println!("procedure-tree leaves:  {package_tree_procedures:?}");
    for (procedure, samples) in &package_tree_procedure_samples {
        println!("  {procedure}: {}", samples.join(", "));
    }
    println!("package execution plan: {package_execution_kinds:?}");
    if !unresolved_movement_targets.is_empty() {
        println!(
            "movement targets awaiting richer resolution: {}: {}",
            unresolved_movement_targets.len(),
            preview(&unresolved_movement_targets, 12)
        );
    }
    println!("scene event bindings:   {total_scene_fragments}");
    println!(
        "scene CTDAs:             {} across {} functions ({quest_alias_conditions} RunOn::QuestAlias)",
        scene_condition_functions.values().sum::<usize>(),
        scene_condition_functions.len(),
    );
    if !unsupported_scene_condition_functions.is_empty() {
        println!(
            "unsupported scene CTDA functions: {:?}",
            unsupported_scene_condition_functions
        );
        for (function, sites) in &unsupported_scene_condition_sites {
            println!("  {function}: {}", preview(sites, 8));
        }
    }
    if let Some(scene) = primary_scene {
        println!("primary scene actor aliases:");
        for actor in &scene.actors {
            let alias = quest
                .aliases
                .iter()
                .find(|alias| alias.alias_id == actor.actor_id as i32);
            match alias {
                Some(alias) => println!(
                    "  {:>3} {:<28} {:?}",
                    actor.actor_id, alias.name, alias.fill_type
                ),
                None => println!("  {:>3} <missing>", actor.actor_id),
            }
        }
    }
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
