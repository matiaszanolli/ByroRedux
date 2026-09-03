//! Scene registry install + playback lifecycle tests.
//!
//! Extracted from `scene.rs`'s inline `mod tests` (#2408 / TD1-005),
//! following the repo's sibling-test convention. Contents unchanged.

use super::*;

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::script_instance::{SceneFragmentEvent, SceneScriptFragment};
use byroredux_plugin::esm::records::{
    ScenRecord, SceneAction, SceneActionType, SCENE_BEGIN_ON_QUEST_START,
};

use crate::papyrus_demo::PapyrusPlayerEntity;
use crate::quest_stages::{QuestFormId, QuestStageAdvancedBatch, QuestStageState};
use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue, RunOn};
use byroredux_plugin::esm::records::ScenePhase;

use crate::quest_stages::{install_engine_start_quest, quest_startup_system, QuestStageAdvanced};

const SCENE: u32 = 0x100;
const QUEST: u32 = 0x200;

fn fragment(event: SceneFragmentEvent, name: &str) -> SceneScriptFragment {
    SceneScriptFragment {
        event,
        script_name: "SF_TestScene_00000100".to_owned(),
        fragment_name: name.to_owned(),
    }
}

fn base_scene() -> ScenRecord {
    ScenRecord {
        form_id: SCENE,
        editor_id: "TestScene".to_owned(),
        quest_form_id: Some(QUEST),
        phases: vec![ScenePhase {
            name: "Opening".to_owned(),
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn setup(scene: ScenRecord) -> (World, EntityId, EntityId) {
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    assert_eq!(install_scene_records(&mut world, [scene]), 1);
    let entity = world
        .resource::<SceneRegistry>()
        .scene_entity(SCENE)
        .expect("scene entity");
    (world, entity, player)
}

fn state(world: &World, entity: EntityId) -> ScenePlayer {
    world
        .get::<ScenePlayer>(entity)
        .expect("scene player")
        .clone()
}

fn events(world: &World, entity: EntityId) -> Vec<SceneEvent> {
    world
        .get::<SceneEventBatch>(entity)
        .map(|batch| batch.0.clone())
        .unwrap_or_default()
}

#[test]
fn install_is_idempotent_and_refreshes_the_definition() {
    let scene = base_scene();
    let (mut world, entity, _) = setup(scene.clone());
    let mut replacement = scene;
    replacement.editor_id = "Refreshed".to_owned();

    assert_eq!(install_scene_records(&mut world, [replacement]), 1);
    let registry = world.resource::<SceneRegistry>();
    assert_eq!(registry.len(), 1);
    assert_eq!(registry.scene_entity(SCENE), Some(entity));
    assert_eq!(registry.definition(SCENE).unwrap().editor_id, "Refreshed");
}

#[test]
fn explicit_start_enters_phase_starts_action_and_dispatches_fragments() {
    let mut scene = base_scene();
    scene.actions.push(SceneAction {
        action_type: SceneActionType::Dialogue,
        actor_id: 7,
        index: 12,
        topic_form_id: Some(0x300),
        ..Default::default()
    });
    scene.fragments = vec![
        fragment(SceneFragmentEvent::Begin, "Fragment_0"),
        fragment(
            SceneFragmentEvent::PhaseStart { phase_index: 0 },
            "Fragment_1",
        ),
    ];
    let (mut world, entity, actor) = setup(scene);
    world
        .resource_mut::<SceneActorBindings>()
        .bind(QuestFormId(QUEST), 7, actor);
    world.insert(entity, SceneStartRequest);

    scene_playback_system(&world, 0.016);

    let player = state(&world, entity);
    assert_eq!(player.state, ScenePlaybackState::Playing);
    assert_eq!(player.current_phase, 0);
    assert_eq!(player.active_actions.len(), 1);
    assert!(events(&world, entity).contains(&SceneEvent::ActionStarted {
        action_index: 12,
        action_type: SceneActionType::Dialogue,
        actor_alias: 7,
        actor_entity: Some(actor),
        topic_form_id: Some(0x300),
        packages: Vec::new(),
    }));
    let invocations = world
        .get::<SceneFragmentInvocationBatch>(entity)
        .expect("fragment invocations");
    assert_eq!(invocations.0.len(), 2);
    assert_eq!(invocations.0[0].event, SceneFragmentEvent::Begin);
    assert_eq!(
        invocations.0[1].event,
        SceneFragmentEvent::PhaseStart { phase_index: 0 }
    );
}

#[test]
fn timer_and_external_completion_preserve_overlapping_action_lifetimes() {
    let mut scene = base_scene();
    scene.phases.push(ScenePhase {
        name: "Second".to_owned(),
        ..Default::default()
    });
    scene.actions = vec![
        SceneAction {
            action_type: SceneActionType::Package,
            actor_id: 4,
            index: 20,
            start_phase: 0,
            end_phase: 1,
            packages: vec![0x400],
            ..Default::default()
        },
        SceneAction {
            action_type: SceneActionType::Timer,
            actor_id: -1,
            index: 21,
            start_phase: 0,
            end_phase: 0,
            timer_seconds: Some(1.0),
            ..Default::default()
        },
    ];
    let (mut world, entity, _) = setup(scene);
    world.insert(entity, SceneStartRequest);
    scene_playback_system(&world, 0.0);
    assert_eq!(state(&world, entity).active_actions.len(), 2);

    scene_playback_system(&world, 0.5);
    assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);
    scene_playback_system(&world, 0.5);
    let after_timer = state(&world, entity);
    assert_eq!(after_timer.state, ScenePlaybackState::WaitingForPhaseStart);
    assert_eq!(after_timer.current_phase, 1);
    assert_eq!(after_timer.active_actions.len(), 1);
    assert_eq!(after_timer.active_actions[0].action_index, 20);

    scene_playback_system(&world, 0.0);
    assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);
    world.insert(entity, SceneActionCompletionBatch(vec![20]));
    scene_playback_system(&world, 0.0);

    let finished = state(&world, entity);
    assert_eq!(finished.state, ScenePlaybackState::Finished);
    assert!(finished.active_actions.is_empty());
    let final_events = events(&world, entity);
    assert!(final_events.contains(&SceneEvent::ActionCompleted { action_index: 20 }));
    assert!(final_events.contains(&SceneEvent::ActionStopped {
        action_index: 20,
        completed: true,
    }));
    assert!(final_events.contains(&SceneEvent::SceneFinished));
}

#[test]
fn completion_conditions_can_release_an_unfinished_package_action() {
    let mut scene = base_scene();
    scene.phases[0].completion_conditions.push(Condition {
        function_index: 58, // GetStage
        comparator: ComparisonOp::Eq,
        comparand: ConditionValue::Literal(10.0),
        param_1: QUEST,
        run_on: RunOn::Subject,
        ..Default::default()
    });
    scene.actions.push(SceneAction {
        action_type: SceneActionType::Package,
        actor_id: 4,
        index: 20,
        start_phase: 0,
        end_phase: 0,
        packages: vec![0x400],
        ..Default::default()
    });
    let (mut world, entity, _) = setup(scene);
    world.insert(entity, SceneStartRequest);
    scene_playback_system(&world, 0.0);
    scene_playback_system(&world, 0.0);
    assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);

    world
        .resource_mut::<QuestStageState>()
        .set_stage(QuestFormId(QUEST), 10);
    scene_playback_system(&world, 0.0);

    assert_eq!(state(&world, entity).state, ScenePlaybackState::Finished);
    assert!(events(&world, entity).contains(&SceneEvent::ActionStopped {
        action_index: 20,
        completed: false,
    }));
}

#[test]
fn empty_phase_with_completion_conditions_waits_for_them() {
    let mut scene = base_scene();
    scene.phases[0].completion_conditions.push(Condition {
        function_index: 58, // GetStage
        comparator: ComparisonOp::Eq,
        comparand: ConditionValue::Literal(10.0),
        param_1: QUEST,
        run_on: RunOn::Subject,
        ..Default::default()
    });
    let (mut world, entity, _) = setup(scene);
    world.insert(entity, SceneStartRequest);
    scene_playback_system(&world, 0.0);
    scene_playback_system(&world, 0.0);
    assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);

    world
        .resource_mut::<QuestStageState>()
        .set_stage(QuestFormId(QUEST), 10);
    scene_playback_system(&world, 0.0);
    assert_eq!(state(&world, entity).state, ScenePlaybackState::Finished);
}

#[test]
fn completed_action_history_drives_scene_action_completion_condition() {
    let mut scene = base_scene();
    scene.phases.push(ScenePhase {
        name: "WaitForOpeningAction".to_owned(),
        completion_conditions: vec![Condition {
            function_index: 550,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(1.0),
            param_1: SCENE,
            param_2: 21,
            run_on: RunOn::Subject,
            ..Default::default()
        }],
        ..Default::default()
    });
    scene.actions.push(SceneAction {
        action_type: SceneActionType::Timer,
        actor_id: -1,
        index: 21,
        start_phase: 0,
        end_phase: 0,
        timer_seconds: Some(0.0),
        ..Default::default()
    });
    let (mut world, entity, _) = setup(scene);
    world.insert(entity, SceneStartRequest);

    scene_playback_system(&world, 0.0); // enter phase 0
    scene_playback_system(&world, 0.0); // complete action 21
    scene_playback_system(&world, 0.0); // enter phase 1
    scene_playback_system(&world, 0.0); // CTDA observes completion history

    let finished = state(&world, entity);
    assert_eq!(finished.state, ScenePlaybackState::Finished);
    assert!(finished.completed_actions.contains(&21));

    world.insert(entity, SceneStartRequest);
    scene_playback_system(&world, 0.0);
    assert!(
        state(&world, entity).completed_actions.is_empty(),
        "a new run must not inherit completed actions from the old run"
    );
}

#[test]
fn phase_start_conditions_are_retried_until_true() {
    let mut scene = base_scene();
    scene.phases[0].start_conditions.push(Condition {
        function_index: 58,
        comparator: ComparisonOp::Eq,
        comparand: ConditionValue::Literal(10.0),
        param_1: QUEST,
        run_on: RunOn::Subject,
        ..Default::default()
    });
    let (mut world, entity, _) = setup(scene);
    world.insert(entity, SceneStartRequest);

    scene_playback_system(&world, 0.0);
    assert_eq!(
        state(&world, entity).state,
        ScenePlaybackState::WaitingForPhaseStart
    );

    world
        .resource_mut::<QuestStageState>()
        .set_stage(QuestFormId(QUEST), 10);
    scene_playback_system(&world, 0.0);
    assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);
}

#[test]
fn quest_start_event_auto_starts_flagged_scene() {
    let mut scene = base_scene();
    scene.flags = SCENE_BEGIN_ON_QUEST_START;
    let (mut world, entity, _) = setup(scene);
    let sink = world.spawn();
    world.insert(
        sink,
        QuestStageAdvancedBatch(vec![QuestStageAdvanced {
            quest: QuestFormId(QUEST),
            previous_stage: 0,
            new_stage: 10,
        }]),
    );

    scene_playback_system(&world, 0.0);

    assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);
    assert!(events(&world, entity).contains(&SceneEvent::SceneStarted));
}

#[test]
fn engine_root_quest_bootstrap_auto_starts_flagged_scene() {
    let mut scene = base_scene();
    scene.flags = SCENE_BEGIN_ON_QUEST_START;
    let (mut world, entity, _) = setup(scene);
    assert!(install_engine_start_quest(
        &mut world,
        QuestFormId(QUEST),
        Some(10),
    ));

    quest_startup_system(&world, 0.0);
    scene_playback_system(&world, 0.0);

    assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);
    assert!(events(&world, entity).contains(&SceneEvent::SceneStarted));
    assert_eq!(
        world
            .resource::<QuestStageState>()
            .get_stage(QuestFormId(QUEST)),
        10
    );
}

#[test]
fn phase_condition_resolves_run_on_quest_alias() {
    use byroredux_core::character::CharacterLevel;

    let mut scene = base_scene();
    scene.phases[0].start_conditions.push(Condition {
        function_index: 80,
        comparator: ComparisonOp::Eq,
        comparand: ConditionValue::Literal(12.0),
        run_on: RunOn::QuestAlias,
        extra_data_id: 7,
        ..Default::default()
    });
    let (mut world, entity, actor) = setup(scene);
    world.register::<CharacterLevel>();
    world.insert(actor, CharacterLevel { level: 12, xp: 0 });
    world
        .resource_mut::<SceneActorBindings>()
        .bind(QuestFormId(QUEST), 7, actor);
    world.insert(entity, SceneStartRequest);

    scene_playback_system(&world, 0.0);

    assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);
}

#[test]
fn stop_request_stops_active_actions_and_dispatches_end_fragment() {
    let mut scene = base_scene();
    scene.actions.push(SceneAction {
        action_type: SceneActionType::Package,
        index: 1,
        end_phase: 0,
        ..Default::default()
    });
    scene.fragments = vec![fragment(SceneFragmentEvent::End, "Fragment_End")];
    let (mut world, entity, _) = setup(scene);
    world.insert(entity, SceneStartRequest);
    scene_playback_system(&world, 0.0);
    world.insert(entity, SceneStopRequest);

    scene_playback_system(&world, 0.0);

    assert_eq!(state(&world, entity).state, ScenePlaybackState::Stopped);
    assert!(events(&world, entity).contains(&SceneEvent::ActionStopped {
        action_index: 1,
        completed: false,
    }));
    assert_eq!(
        world
            .get::<SceneFragmentInvocationBatch>(entity)
            .expect("end fragment")
            .0[0]
            .event,
        SceneFragmentEvent::End
    );
}
