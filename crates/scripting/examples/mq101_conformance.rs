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
use byroredux_papyrus::ast::ScriptItem;
use byroredux_pex::{decompile::decompile_script, parse};
use byroredux_plugin::esm::records::condition::RunOn;
use byroredux_plugin::esm::records::script_instance::SceneFragmentEvent;
use byroredux_plugin::esm::records::SceneActionType;
use byroredux_scripting::translate::effects::{lower_fragment, Effect};
use byroredux_scripting::{
    install_scene_quest_aliases, refresh_scene_actor_bindings, ConditionFunction,
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

    let quest_alias_ids: HashSet<i32> = quest.aliases.iter().map(|alias| alias.alias_id).collect();
    let mut dialogue_actions = 0usize;
    let mut package_actions = 0usize;
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
                SceneActionType::Package => package_actions += 1,
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
        unresolved_topics.is_empty() && unresolved_packages.is_empty(),
        if unresolved_topics.is_empty() && unresolved_packages.is_empty() {
            "all action DIAL and PACK FormIDs resolve".to_owned()
        } else {
            let mut failures = unresolved_topics;
            failures.extend(unresolved_packages);
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
    println!("MQ101 scene records:    {}", mq101_scenes.len());
    println!("scene phases:           {total_scene_phases}");
    println!("scene actions:          {total_scene_actions}");
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
