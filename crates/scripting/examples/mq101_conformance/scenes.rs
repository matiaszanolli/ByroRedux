//! SCEN stages: runtime/structure/linkage analysis over the quest-owned scenes.

use super::*;

/// Quest-owned SCEN records plus the primary MQ101Scene1 reference, shared
/// by the structural-analysis, linkage, fragment, and inventory stages.
pub(super) struct SceneSet<'a> {
    pub(super) scenes: Vec<&'a ScenRecord>,
    pub(super) primary: Option<&'a ScenRecord>,
}

/// Stage 2 — scene records, the runtime registry/player/dialogue plans the
/// live cell loader constructs, and the new-game bootstrap through the
/// engine-start quest path.
pub(super) fn check_scene_runtime<'a>(
    checks: &mut Checks,
    index: &'a EsmIndex,
    quest: &QustRecord,
) -> SceneSet<'a> {
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
        runtime_dialogue_registry.len() == mq101_topic_ids.len() && mq101_info_count > 0,
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
    bootstrap_world.insert_resource(PapyrusPlayerEntity(bootstrap_player));
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

    SceneSet {
        scenes: mq101_scenes,
        primary: primary_scene,
    }
}

/// Counters and backlog lists accumulated by the per-scene structural
/// analysis; consumed by the linkage checks, the script-ingress stage, and
/// the closing workload inventory.
#[derive(Default)]
pub(super) struct SceneInventory {
    pub(super) total_scene_phases: usize,
    pub(super) total_scene_actions: usize,
    pub(super) total_scene_fragments: usize,
    pub(super) dialogue_actions: usize,
    pub(super) package_actions: usize,
    pub(super) timer_actions: usize,
    pub(super) package_actions_with_phase_completion_conditions: usize,
    pub(super) unknown_actions: Vec<String>,
    pub(super) invalid_ranges: Vec<String>,
    pub(super) unresolved_aliases: Vec<String>,
    pub(super) unresolved_action_actors: Vec<String>,
    pub(super) unresolved_topics: Vec<String>,
    pub(super) unresolved_packages: Vec<String>,
    pub(super) unresolved_package_templates: Vec<String>,
    pub(super) unresolved_movement_targets: Vec<String>,
    pub(super) invalid_fragment_phases: Vec<String>,
    pub(super) scene_fragment_scripts: HashSet<String>,
    pub(super) package_action_stacks: BTreeMap<usize, usize>,
    pub(super) package_procedures: BTreeMap<u32, usize>,
    pub(super) package_procedure_samples: BTreeMap<u32, Vec<String>>,
    pub(super) package_templates: BTreeMap<String, usize>,
    pub(super) package_tree_procedures: BTreeMap<String, usize>,
    pub(super) package_tree_procedure_samples: BTreeMap<String, Vec<String>>,
    pub(super) package_execution_kinds: BTreeMap<String, usize>,
    pub(super) scene_condition_functions: BTreeMap<u32, usize>,
    pub(super) unsupported_scene_condition_functions: BTreeMap<u32, usize>,
    pub(super) unsupported_scene_condition_sites: BTreeMap<u32, Vec<String>>,
    pub(super) quest_alias_conditions: usize,
}

/// Stage 3 — walk every quest-owned SCEN record and audit its structure:
/// action typing, phase ranges, actor/topic/package linkage, CTDA function
/// coverage, and VMAD fragment bindings.
pub(super) fn check_scene_structure(
    checks: &mut Checks,
    index: &EsmIndex,
    quest: &QustRecord,
    scenes: &SceneSet,
) -> SceneInventory {
    let mut inventory = SceneInventory::default();
    let quest_alias_ids: HashSet<i32> = quest.aliases.iter().map(|alias| alias.alias_id).collect();
    for scene in scenes.scenes.iter().copied() {
        analyze_scene(index, scene, &quest_alias_ids, &mut inventory);
    }

    checks.record(
        "scene action structure",
        inventory.total_scene_actions > 0
            && inventory.unknown_actions.is_empty()
            && inventory.invalid_ranges.is_empty(),
        format!(
            "{ts} actions ({d} dialogue, {p} package, {t} timer); {u} unknown, {r} invalid ranges",
            ts = inventory.total_scene_actions,
            d = inventory.dialogue_actions,
            p = inventory.package_actions,
            t = inventory.timer_actions,
            u = inventory.unknown_actions.len(),
            r = inventory.invalid_ranges.len()
        ),
    );
    checks.record(
        "scene condition coverage",
        inventory.unsupported_scene_condition_functions.is_empty(),
        if inventory.unsupported_scene_condition_functions.is_empty() {
            format!(
                "all {} authored CTDAs map to runtime functions",
                inventory.scene_condition_functions.values().sum::<usize>()
            )
        } else {
            format!(
                "unsupported: {:?}",
                inventory.unsupported_scene_condition_functions
            )
        },
    );
    checks.record(
        "scene actor linkage",
        inventory.unresolved_aliases.is_empty() && inventory.unresolved_action_actors.is_empty(),
        if inventory.unresolved_aliases.is_empty()
            && inventory.unresolved_action_actors.is_empty()
        {
            "all scene actors resolve through MQ101 aliases and action rosters".to_owned()
        } else {
            let mut failures = std::mem::take(&mut inventory.unresolved_aliases);
            failures.append(&mut inventory.unresolved_action_actors);
            format!("{} unresolved: {}", failures.len(), preview(&failures, 8))
        },
    );
    inventory
}

/// Per-scene half of the structural analysis: conditions, actor rosters,
/// actions (with package trees), and VMAD fragments.
pub(super) fn analyze_scene(
    index: &EsmIndex,
    scene: &ScenRecord,
    quest_alias_ids: &HashSet<i32>,
    inventory: &mut SceneInventory,
) {
    inventory.total_scene_phases += scene.phases.len();
    inventory.total_scene_actions += scene.actions.len();
    inventory.total_scene_fragments += scene.fragments.len();

    collect_scene_conditions(scene, inventory);

    let actor_ids: HashSet<i32> = scene
        .actors
        .iter()
        .map(|actor| actor.actor_id as i32)
        .collect();
    for actor in &scene.actors {
        if !quest_alias_ids.contains(&(actor.actor_id as i32)) {
            inventory
                .unresolved_aliases
                .push(format!("{}:alias{}", scene.editor_id, actor.actor_id));
        }
    }

    for action in &scene.actions {
        match action.action_type {
            SceneActionType::Dialogue => inventory.dialogue_actions += 1,
            SceneActionType::Package => analyze_package_action(index, scene, action, inventory),
            SceneActionType::Timer => inventory.timer_actions += 1,
            SceneActionType::Unknown(raw) => {
                inventory
                    .unknown_actions
                    .push(format!("{}:{} type {raw}", scene.editor_id, action.index));
            }
        }
        if action.start_phase > action.end_phase || action.end_phase as usize >= scene.phases.len() {
            inventory.invalid_ranges.push(format!(
                "{}:{} {}..{} of {}",
                scene.editor_id,
                action.index,
                action.start_phase,
                action.end_phase,
                scene.phases.len()
            ));
        }
        if action.actor_id >= 0 && !actor_ids.contains(&action.actor_id) {
            inventory
                .unresolved_action_actors
                .push(format!("{}:{} actor {}", scene.editor_id, action.index, action.actor_id));
        }
        if let Some(topic) = action.topic_form_id {
            if !index.dialogues.contains_key(&topic) {
                inventory
                    .unresolved_topics
                    .push(format!("{}:{} DIAL {topic:08X}", scene.editor_id, action.index));
            }
        }
        for package in &action.packages {
            if !index.packages.contains_key(package) {
                inventory
                    .unresolved_packages
                    .push(format!("{}:{} PACK {package:08X}", scene.editor_id, action.index));
            }
        }
    }

    for fragment in &scene.fragments {
        inventory
            .scene_fragment_scripts
            .insert(fragment.script_name.to_ascii_lowercase());
        let phase_index = match fragment.event {
            SceneFragmentEvent::PhaseStart { phase_index }
            | SceneFragmentEvent::PhaseCompletion { phase_index }
            | SceneFragmentEvent::UnknownPhase { phase_index, .. } => Some(phase_index),
            _ => None,
        };
        if phase_index.is_some_and(|index| index as usize >= scene.phases.len()) {
            inventory
                .invalid_fragment_phases
                .push(format!("{}:{:?}", scene.editor_id, fragment.event));
        }
    }
}

/// Tally scene-start and phase CTDA functions, flagging functions the
/// runtime does not know yet.
pub(super) fn collect_scene_conditions(scene: &ScenRecord, inventory: &mut SceneInventory) {
    let mut record_condition =
        |scope: String, condition: &byroredux_plugin::esm::records::condition::Condition| {
            *inventory
                .scene_condition_functions
                .entry(condition.function_index)
                .or_default() += 1;
            if matches!(
                ConditionFunction::from_index(condition.function_index),
                ConditionFunction::Unknown(_)
            ) {
                *inventory
                    .unsupported_scene_condition_functions
                    .entry(condition.function_index)
                    .or_default() += 1;
                inventory
                    .unsupported_scene_condition_sites
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
                inventory.quest_alias_conditions += 1;
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
}

/// Stage 4 — resolve MQ101Scene1's actor aliases against the indexed NPC
/// references, then check DIAL/PACK/fragment linkage for every scene.
pub(super) fn check_scene_linkage(
    checks: &mut Checks,
    index: &EsmIndex,
    quest: &QustRecord,
    scenes: &SceneSet,
    inventory: &mut SceneInventory,
) {
    let mut alias_world = World::new();
    byroredux_scripting::register(&mut alias_world);
    spawn_alias_candidates(&mut alias_world, &index.npcs, index.cells.cells.values());
    spawn_alias_candidates(
        &mut alias_world,
        &index.npcs,
        index.cells.exterior_cells.values().flat_map(|grids| grids.values()),
    );
    spawn_alias_candidates(
        &mut alias_world,
        &index.npcs,
        index.cells.worldspace_persistent_cells.values(),
    );
    let persistent_reference_count: usize = index
        .cells
        .worldspace_persistent_cells
        .values()
        .map(|cell| cell.references.len())
        .sum();
    install_scene_quest_aliases(&mut alias_world, [quest.clone()]);
    let resolved_alias_count = refresh_scene_actor_bindings(&alias_world);
    let unresolved_primary_aliases: Vec<String> = scenes
        .primary
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
        scenes.primary.is_some() && unresolved_primary_aliases.is_empty(),
        if unresolved_primary_aliases.is_empty() {
            format!(
                "all {} MQ101Scene1 actor slots bind from ACHR identity/XLRT ({resolved_alias_count} MQ101 aliases resolved total)",
                scenes.primary.map_or(0, |scene| scene.actors.len()),
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
        inventory.unresolved_topics.is_empty()
            && inventory.unresolved_packages.is_empty()
            && inventory.unresolved_package_templates.is_empty(),
        if inventory.unresolved_topics.is_empty()
            && inventory.unresolved_packages.is_empty()
            && inventory.unresolved_package_templates.is_empty()
        {
            format!(
                "all action DIAL/PACK FormIDs and {} custom package trees resolve",
                inventory.package_templates.len()
            )
        } else {
            let mut failures = std::mem::take(&mut inventory.unresolved_topics);
            failures.append(&mut inventory.unresolved_packages);
            failures.append(&mut inventory.unresolved_package_templates);
            format!("{} unresolved: {}", failures.len(), preview(&failures, 8))
        },
    );
    checks.record(
        "scene VMAD fragments",
        inventory.total_scene_fragments > 0 && inventory.invalid_fragment_phases.is_empty(),
        format!(
            "{total} event bindings across {scripts} compiled scene scripts; {invalid} invalid phase indices",
            total = inventory.total_scene_fragments,
            scripts = inventory.scene_fragment_scripts.len(),
            invalid = inventory.invalid_fragment_phases.len()
        ),
    );
}

/// Spawn a `SceneAliasCandidate` for every placed NPC reference in the
/// given cells — the indexed input the alias resolver binds against.
pub(super) fn spawn_alias_candidates<'a>(
    world: &mut World,
    npcs: &HashMap<u32, NpcRecord>,
    cells: impl Iterator<Item = &'a CellData>,
) {
    for cell in cells {
        for placed in &cell.references {
            if npcs.contains_key(&placed.base_form_id) {
                let entity = world.spawn();
                world.insert(
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
