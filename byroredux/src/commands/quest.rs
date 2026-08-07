//! Quest lifecycle, objective, target, and alias diagnostics.
//!
//! Read-only commands never force a derived alias refresh: `quest.aliases`
//! reports `refresh=pending` when the scheduled resolver has not run yet.
//! Mutating commands deliberately route through the same canonical fragment
//! effect path as Papyrus and refresh aliases afterward.

use super::shared::*;

use std::collections::BTreeSet;

use byroredux_plugin::esm::records::{AliasFillType, QuestObjective};
use byroredux_scripting::quest_stages::{
    QuestDefinitionRegistry, QuestObjectiveState, QuestStageData, QuestStageState,
};
use byroredux_scripting::translate::compose::QuestRef;
use byroredux_scripting::translate::effects::Effect;
use byroredux_scripting::{
    apply_effects, quest_alias_diagnostics, refresh_scene_actor_bindings,
    resolve_quest_objective_targets, resolve_quest_targets, QuestAliasDiagnostic,
    QuestAliasResolutionState, QuestFormId, QuestStatus, SceneActorBindings, SceneAliasCandidate,
};

#[derive(Default)]
struct QuestStaticSummary {
    defined: bool,
    editor_id: String,
    full_name: String,
    flags: u16,
    start_up_stage: Option<u16>,
    shut_down_stage: Option<u16>,
    stages: Vec<u16>,
    objectives: Vec<QuestObjective>,
    target_count: usize,
}

fn parse_quest_arg(args: &str, usage: &str) -> Result<QuestFormId, CommandOutput> {
    let mut tokens = args.split_whitespace();
    let Some(raw) = tokens.next() else {
        return Err(CommandOutput::error(format!("usage: {usage}")));
    };
    if tokens.next().is_some() {
        return Err(CommandOutput::error(format!("usage: {usage}")));
    }
    parse_console_u32(raw)
        .map(QuestFormId)
        .ok_or_else(|| CommandOutput::error(format!("bad quest FormID `{raw}`")))
}

fn static_summary(world: &World, quest: QuestFormId) -> QuestStaticSummary {
    let Some(definitions) = world.try_resource::<QuestDefinitionRegistry>() else {
        return QuestStaticSummary::default();
    };
    if !definitions.contains(quest) {
        return QuestStaticSummary::default();
    }
    QuestStaticSummary {
        defined: true,
        editor_id: definitions.editor_id(quest).unwrap_or_default().to_string(),
        full_name: definitions.full_name(quest).unwrap_or_default().to_string(),
        flags: definitions.flags(quest).unwrap_or_default(),
        start_up_stage: definitions.start_up_stage(quest),
        shut_down_stage: definitions.shut_down_stage(quest),
        stages: definitions.stages(quest).to_vec(),
        objectives: definitions
            .objectives(quest)
            .iter()
            .filter_map(|&index| definitions.objective(quest, index).cloned())
            .collect(),
        target_count: definitions.targets(quest).len(),
    }
}

fn status_name(state: Option<&QuestStageData>) -> &'static str {
    match state.map(|state| state.status) {
        None => "not-started",
        Some(QuestStatus::Running) => "running",
        Some(QuestStatus::Stopped) => "stopped",
        Some(QuestStatus::Completed) => "completed",
        Some(QuestStatus::Failed) => "failed",
    }
}

fn format_stage(stage: Option<u16>) -> String {
    stage.map_or_else(|| "none".to_string(), |stage| stage.to_string())
}

fn one_line_text(text: &str, max_chars: usize) -> String {
    let flattened = text.replace(['\r', '\n'], " ");
    let mut chars = flattened.chars();
    let mut out: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

fn fill_type_name(fill: Option<&AliasFillType>) -> String {
    match fill {
        None => "find-matching".to_string(),
        Some(AliasFillType::ForcedReference(reference)) => {
            format!("forced-reference(0x{reference:08X})")
        }
        Some(AliasFillType::ForcedLocation(location)) => {
            format!("forced-location(0x{location:08X})")
        }
        Some(AliasFillType::UniqueActor(base)) => format!("unique-actor(0x{base:08X})"),
        Some(AliasFillType::CreatedObject {
            base,
            target_alias,
            create_mode,
            level,
        }) => format!(
            "created-object(base=0x{base:08X}, target={target_alias}, mode={create_mode}, level={level})"
        ),
        Some(AliasFillType::ExternalAlias { quest, alias_id }) => {
            format!("external(0x{quest:08X}:{alias_id})")
        }
        Some(AliasFillType::LocationAliasReference {
            alias_id,
            keyword,
            ref_type,
        }) => format!(
            "location-ref(alias={alias_id}, keyword={}, type={})",
            keyword.map_or_else(|| "none".to_string(), |id| format!("0x{id:08X}")),
            ref_type.map_or_else(|| "none".to_string(), |id| format!("0x{id:08X}")),
        ),
        Some(AliasFillType::NearAlias { alias_id, relation }) => {
            format!("near-alias(alias={alias_id}, relation={relation})")
        }
        Some(AliasFillType::FromEvent { event_type, data }) => {
            let event = String::from_utf8_lossy(event_type);
            format!("from-event({event}, data={data})")
        }
    }
}

fn alias_state(world: &World, diagnostic: &QuestAliasDiagnostic) -> String {
    match &diagnostic.state {
        QuestAliasResolutionState::Bound(entity) => {
            if let Some(candidate) = world.get::<SceneAliasCandidate>(*entity) {
                format!(
                    "bound entity={entity} ref=0x{:08X} base=0x{:08X}",
                    candidate.reference_form_id, candidate.base_form_id
                )
            } else {
                format!("bound entity={entity}")
            }
        }
        QuestAliasResolutionState::QuestNotRunning => {
            "unbound reason=quest-not-running".to_string()
        }
        QuestAliasResolutionState::LocationRuntimeUnavailable => {
            "unbound reason=location-runtime-unavailable".to_string()
        }
        QuestAliasResolutionState::ReferenceCollectionRuntimeUnavailable => {
            "unbound reason=reference-collection-runtime-unavailable".to_string()
        }
        QuestAliasResolutionState::CreatedObjectRuntimeUnavailable => {
            "unbound reason=created-object-runtime-unavailable".to_string()
        }
        QuestAliasResolutionState::StoryManagerEventUnavailable => {
            "unbound reason=story-manager-event-unavailable".to_string()
        }
        QuestAliasResolutionState::ExternalSourceUnbound { quest, alias_id } => format!(
            "unbound reason=external-source-unbound source=0x{:08X}:{alias_id}",
            quest.0
        ),
        QuestAliasResolutionState::DependencyAliasUnbound(alias_id) => {
            format!("unbound reason=dependency-alias-unbound alias={alias_id}")
        }
        QuestAliasResolutionState::ForceIntoSourcesUnbound(sources) => format!(
            "unbound reason=force-into-sources-unbound sources={}",
            sources
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ),
        QuestAliasResolutionState::NoEligibleLoadedCandidate => {
            "unbound reason=no-eligible-loaded-candidate".to_string()
        }
        QuestAliasResolutionState::NoFillMechanism => {
            "unbound reason=no-fill-mechanism".to_string()
        }
    }
}

fn quest_is_known(world: &World, quest: QuestFormId) -> bool {
    world
        .try_resource::<QuestDefinitionRegistry>()
        .is_some_and(|definitions| definitions.contains(quest))
}

fn apply_control_effect(
    world: &World,
    quest: QuestFormId,
    effect: Effect,
) -> Result<usize, CommandOutput> {
    if !quest_is_known(world, quest) {
        return Err(CommandOutput::error(format!(
            "unknown quest 0x{:08X}",
            quest.0
        )));
    }
    if world.try_resource::<QuestStageState>().is_none()
        || world.try_resource::<QuestObjectiveState>().is_none()
    {
        return Err(CommandOutput::error("quest runtime resources unavailable"));
    }
    let advances = {
        let (mut stages, mut objectives) =
            world.resource_2_mut::<QuestStageState, QuestObjectiveState>();
        apply_effects(&[effect], quest, None, world, &mut stages, &mut objectives)
    };
    // A control command is an explicit mutation, so make the corresponding
    // derived alias lifetime visible immediately to the next debug request.
    refresh_scene_actor_bindings(world);
    Ok(advances.len())
}

pub(crate) struct QuestShowCommand;

impl ConsoleCommand for QuestShowCommand {
    fn name(&self) -> &str {
        "quest.show"
    }

    fn description(&self) -> &str {
        "Show quest definition, lifecycle, stages, objectives, targets, and alias summary"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let quest = match parse_quest_arg(args, "quest.show <formid>") {
            Ok(quest) => quest,
            Err(error) => return error,
        };
        let static_data = static_summary(world, quest);
        let state = world
            .try_resource::<QuestStageState>()
            .and_then(|stages| stages.state(quest).cloned());
        if !static_data.defined && state.is_none() {
            return CommandOutput::error(format!("unknown quest 0x{:08X}", quest.0));
        }

        let label = match (
            static_data.editor_id.as_str(),
            static_data.full_name.as_str(),
        ) {
            ("", "") => String::new(),
            (editor_id, "") => format!(" ({editor_id})"),
            ("", full_name) => format!(" ({})", one_line_text(full_name, 80)),
            (editor_id, full_name) => {
                format!(" ({editor_id} — {})", one_line_text(full_name, 80))
            }
        };
        let mut lines = vec![format!("Quest 0x{:08X}{label}", quest.0)];
        lines.push(format!("  defined: {}", static_data.defined));
        lines.push(format!("  state: {}", status_name(state.as_ref())));
        lines.push(format!(
            "  active: {}",
            state.as_ref().is_some_and(|state| state.active)
        ));
        lines.push(format!(
            "  current-stage: {}",
            state.as_ref().map_or(0, |state| state.current_stage)
        ));
        let mut done_stages: Vec<u16> = state
            .as_ref()
            .map(|state| state.stages_done.iter().copied().collect())
            .unwrap_or_default();
        done_stages.sort_unstable();
        lines.push(format!(
            "  done-stages: {}",
            if done_stages.is_empty() {
                "none".to_string()
            } else {
                done_stages
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            }
        ));
        if static_data.defined {
            lines.push(format!("  flags: 0x{:04X}", static_data.flags));
            lines.push(format!(
                "  startup/shutdown: {}/{}",
                format_stage(static_data.start_up_stage),
                format_stage(static_data.shut_down_stage)
            ));
            lines.push(format!(
                "  authored-stages: {}",
                static_data
                    .stages
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }

        let touched_objectives: Vec<(i32, _)> = world
            .try_resource::<QuestObjectiveState>()
            .map(|objectives| objectives.iter_quest(quest).collect())
            .unwrap_or_default();
        let mut objective_indices: BTreeSet<i32> = static_data
            .objectives
            .iter()
            .map(|objective| objective.index)
            .collect();
        objective_indices.extend(touched_objectives.iter().map(|(index, _)| *index));
        lines.push(format!("  objectives: {}", objective_indices.len()));
        for index in objective_indices {
            let status = world
                .try_resource::<QuestObjectiveState>()
                .map(|objectives| objectives.get(quest, index))
                .unwrap_or_default();
            let authored = static_data
                .objectives
                .iter()
                .find(|objective| objective.index == index);
            let (resolved, total, text) = authored.map_or((0, 0, ""), |objective| {
                (
                    resolve_quest_objective_targets(world, quest, index).len(),
                    objective.targets.len(),
                    objective.text.as_str(),
                )
            });
            lines.push(format!(
                "    {index}: displayed={} completed={} failed={} targets={resolved}/{total} text=\"{}\"",
                status.displayed,
                status.completed,
                status.failed,
                one_line_text(text, 80)
            ));
        }
        lines.push(format!(
            "  record-targets: {}/{}",
            resolve_quest_targets(world, quest).len(),
            static_data.target_count
        ));

        match quest_alias_diagnostics(world, quest) {
            Some(diagnostics) => {
                let bound = diagnostics
                    .iter()
                    .filter(|diagnostic| {
                        matches!(diagnostic.state, QuestAliasResolutionState::Bound(_))
                    })
                    .count();
                let refresh =
                    world
                        .try_resource::<SceneActorBindings>()
                        .map_or("unavailable", |bindings| {
                            if bindings.is_dirty() {
                                "pending"
                            } else {
                                "current"
                            }
                        });
                lines.push(format!(
                    "  aliases: {bound}/{} bound (refresh={refresh})",
                    diagnostics.len()
                ));
            }
            None => lines.push("  aliases: not-installed".to_string()),
        }
        CommandOutput::lines(lines)
    }
}

pub(crate) struct QuestAliasesCommand;

impl ConsoleCommand for QuestAliasesCommand {
    fn name(&self) -> &str {
        "quest.aliases"
    }

    fn description(&self) -> &str {
        "Show authored quest aliases, live bindings, injections, and unbound reasons"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let quest = match parse_quest_arg(args, "quest.aliases <formid>") {
            Ok(quest) => quest,
            Err(error) => return error,
        };
        let Some(diagnostics) = quest_alias_diagnostics(world, quest) else {
            return CommandOutput::error(format!(
                "quest 0x{:08X} has no installed alias definition",
                quest.0
            ));
        };
        let bound = diagnostics
            .iter()
            .filter(|diagnostic| matches!(diagnostic.state, QuestAliasResolutionState::Bound(_)))
            .count();
        let refresh =
            world
                .try_resource::<SceneActorBindings>()
                .map_or("unavailable", |bindings| {
                    if bindings.is_dirty() {
                        "pending"
                    } else {
                        "current"
                    }
                });
        let mut lines = vec![format!(
            "Quest aliases 0x{:08X}: {bound}/{} bound refresh={refresh}",
            quest.0,
            diagnostics.len()
        )];
        for diagnostic in diagnostics {
            let alias = &diagnostic.alias;
            lines.push(format!(
                "  {} '{}' kind={} fill={} flags=0x{:08X} {} injections=factions:{} packages:{} spells:{} keywords:{} inventory:{}",
                alias.alias_id,
                one_line_text(&alias.name, 48),
                if alias.is_collection {
                    "collection"
                } else if alias.is_location {
                    "location"
                } else {
                    "reference"
                },
                fill_type_name(alias.fill_type.as_ref()),
                alias.flags.0,
                alias_state(world, &diagnostic),
                alias.injected.factions.len(),
                alias.injected.packages.len(),
                alias.injected.spells.len(),
                alias.injected.keywords.len(),
                alias.injected.inventory.len(),
            ));
        }
        CommandOutput::lines(lines)
    }
}

pub(crate) struct QuestStartCommand;

impl ConsoleCommand for QuestStartCommand {
    fn name(&self) -> &str {
        "quest.start"
    }

    fn description(&self) -> &str {
        "Start or resume an installed quest through its authored startup stage"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let quest = match parse_quest_arg(args, "quest.start <formid>") {
            Ok(quest) => quest,
            Err(error) => return error,
        };
        let before = world
            .try_resource::<QuestStageState>()
            .and_then(|stages| stages.state(quest).cloned());
        if let Err(error) = apply_control_effect(
            world,
            quest,
            Effect::StartQuest {
                quest: QuestRef::SelfRef,
            },
        ) {
            return error;
        }
        let after = world.resource::<QuestStageState>().state(quest).cloned();
        let result = match (before.as_ref().map(|state| state.status), after.as_ref()) {
            (None, Some(state)) if state.status == QuestStatus::Running => "started",
            (Some(QuestStatus::Stopped), Some(state)) if state.status == QuestStatus::Running => {
                "resumed"
            }
            _ => "unchanged",
        };
        CommandOutput::line(format!(
            "Quest 0x{:08X} result: {result} state={} stage={}",
            quest.0,
            status_name(after.as_ref()),
            after.as_ref().map_or(0, |state| state.current_stage)
        ))
    }
}

pub(crate) struct QuestStopCommand;

impl ConsoleCommand for QuestStopCommand {
    fn name(&self) -> &str {
        "quest.stop"
    }

    fn description(&self) -> &str {
        "Stop a running quest through its authored shutdown stage"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let quest = match parse_quest_arg(args, "quest.stop <formid>") {
            Ok(quest) => quest,
            Err(error) => return error,
        };
        let before = world
            .try_resource::<QuestStageState>()
            .and_then(|stages| stages.state(quest).cloned());
        if let Err(error) = apply_control_effect(
            world,
            quest,
            Effect::StopQuest {
                quest: QuestRef::SelfRef,
            },
        ) {
            return error;
        }
        let after = world.resource::<QuestStageState>().state(quest).cloned();
        let changed =
            before.as_ref().map(|state| state.status) != after.as_ref().map(|state| state.status);
        CommandOutput::line(format!(
            "Quest 0x{:08X} result: {} state={} stage={}",
            quest.0,
            if changed { "stopped" } else { "unchanged" },
            status_name(after.as_ref()),
            after.as_ref().map_or(0, |state| state.current_stage)
        ))
    }
}

pub(crate) struct QuestSetStageCommand;

impl ConsoleCommand for QuestSetStageCommand {
    fn name(&self) -> &str {
        "quest.setstage"
    }

    fn description(&self) -> &str {
        "Set an installed quest stage through the canonical fragment-effect path"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut tokens = args.split_whitespace();
        let (Some(raw_quest), Some(raw_stage)) = (tokens.next(), tokens.next()) else {
            return CommandOutput::error("usage: quest.setstage <formid> <stage>");
        };
        if tokens.next().is_some() {
            return CommandOutput::error("usage: quest.setstage <formid> <stage>");
        }
        let Some(quest) = parse_console_u32(raw_quest).map(QuestFormId) else {
            return CommandOutput::error(format!("bad quest FormID `{raw_quest}`"));
        };
        let Some(stage) = parse_console_u32(raw_stage).and_then(|stage| u16::try_from(stage).ok())
        else {
            return CommandOutput::error(format!("bad quest stage `{raw_stage}`"));
        };
        let before = world
            .try_resource::<QuestStageState>()
            .and_then(|stages| stages.state(quest).cloned());
        let advances = match apply_control_effect(
            world,
            quest,
            Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage,
            },
        ) {
            Ok(advances) => advances,
            Err(error) => return error,
        };
        let after = world.resource::<QuestStageState>().state(quest).cloned();
        let result = if advances == 0 { "unchanged" } else { "set" };
        CommandOutput::line(format!(
            "Quest 0x{:08X} result: {result} stage={} previous={} state={}",
            quest.0,
            after.as_ref().map_or(0, |state| state.current_stage),
            before.as_ref().map_or(0, |state| state.current_stage),
            status_name(after.as_ref())
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::{
        AliasInjectedData, QuestAlias, QuestObjective, QuestStage, QustRecord,
    };
    use byroredux_scripting::{install_scene_quest_aliases, install_start_game_quests};

    const QUEST: u32 = 0x1234;

    fn fixture() -> (World, EntityId) {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.insert_resource(QuestStageState::default());
        let record = QustRecord {
            form_id: QUEST,
            editor_id: "DebugQuest".to_string(),
            full_name: "A Diagnostic Quest".to_string(),
            start_up_stage: Some(5),
            shut_down_stage: Some(90),
            stages: vec![
                QuestStage {
                    index: 5,
                    ..Default::default()
                },
                QuestStage {
                    index: 10,
                    ..Default::default()
                },
                QuestStage {
                    index: 90,
                    ..Default::default()
                },
            ],
            objectives: vec![QuestObjective {
                index: 10,
                text: "Inspect the quest runtime".to_string(),
                ..Default::default()
            }],
            aliases: vec![QuestAlias {
                alias_id: 1,
                name: "Target".to_string(),
                fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                injected: AliasInjectedData {
                    inventory: vec![(0xC1, 1)],
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        };
        install_start_game_quests(&mut world, [record.clone()]);
        install_scene_quest_aliases(&mut world, [record]);
        let actor = world.spawn();
        world.insert(
            actor,
            SceneAliasCandidate {
                reference_form_id: 0xA1,
                base_form_id: 0xB1,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        (world, actor)
    }

    #[test]
    fn show_reports_static_and_not_started_state_without_refreshing_aliases() {
        let (world, _) = fixture();
        let output = QuestShowCommand.execute(&world, "0x1234").lines.join("\n");
        assert!(output.contains("Quest 0x00001234 (DebugQuest — A Diagnostic Quest)"));
        assert!(output.contains("state: not-started"));
        assert!(output.contains("startup/shutdown: 5/90"));
        assert!(output.contains("10: displayed=false completed=false failed=false"));
        assert!(output.contains("aliases: 0/1 bound (refresh=pending)"));
    }

    #[test]
    fn controls_share_lifecycle_semantics_and_refresh_aliases() {
        let (world, actor) = fixture();

        let started = QuestStartCommand.execute(&world, "0x1234").lines.join("\n");
        assert!(
            started.contains("result: started state=running stage=5"),
            "unexpected start output: {started}"
        );

        let aliases = QuestAliasesCommand
            .execute(&world, "0x1234")
            .lines
            .join("\n");
        assert!(aliases.contains("1/1 bound refresh=current"));
        assert!(aliases.contains(&format!("bound entity={actor}")));
        assert!(aliases.contains("inventory:1"));

        let set = QuestSetStageCommand
            .execute(&world, "0x1234 10")
            .lines
            .join("\n");
        assert!(set.contains("result: set stage=10 previous=5 state=running"));

        let stopped = QuestStopCommand.execute(&world, "0x1234").lines.join("\n");
        assert!(stopped.contains("result: stopped state=stopped stage=90"));
        let aliases = QuestAliasesCommand
            .execute(&world, "0x1234")
            .lines
            .join("\n");
        assert!(aliases.contains("0/1 bound refresh=current"));
        assert!(aliases.contains("reason=quest-not-running"));
    }

    #[test]
    fn controls_reject_unknown_quests_and_bad_arguments() {
        let (world, _) = fixture();
        assert!(QuestStartCommand.execute(&world, "nope").lines[0].contains("bad quest FormID"));
        assert!(QuestStartCommand.execute(&world, "0xDEAD").lines[0].contains("unknown quest"));
        assert!(
            QuestSetStageCommand.execute(&world, "0x1234 70000").lines[0]
                .contains("bad quest stage")
        );
    }
}
