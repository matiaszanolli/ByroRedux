//! Scene playback state machine.
//!
//! Split out of `scene.rs` (#2408 / TD1-005). This half owns the player /
//! action / event types and the per-frame tick that advances phases, starts
//! and stops actions, and emits the transient event + fragment batches
//! downstream executors consume. Contents moved verbatim.
//!
//! The issue also proposed a third `package_action.rs`. There is no such
//! unit to extract: `SceneActionType::Package` is one variant handled inline
//! by `tick_player` alongside Dialogue / Timer, not a separable executor.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::script_instance::{SceneFragmentEvent, SceneScriptFragment};
use byroredux_plugin::esm::records::{
    ScenRecord, SceneAction, SceneActionType, SCENE_BEGIN_ON_QUEST_START,
};

use super::{refresh_scene_actor_bindings, SceneActorBindings, SceneRegistry};
use crate::condition::{evaluate, ConditionContext};
use crate::papyrus_demo::PapyrusPlayerEntity;
use crate::quest_stages::{
    QuestFormId, QuestStageAdvancedBatch, QuestStageState, SCENE_QUEST_EVENT_SUBSCRIBER,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenePlaybackState {
    Dormant,
    WaitingForStart,
    WaitingForPhaseStart,
    Playing,
    Finished,
    Stopped,
}

/// Runtime state for one active action. The authored action remains in the
/// registry; this component stores only mutable lifecycle state.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveSceneAction {
    pub action_index: u32,
    pub action_type: SceneActionType,
    pub end_phase: u32,
    pub remaining_seconds: Option<f32>,
    pub completed: bool,
}

/// Persistent playback state for one scene definition.
///
/// # Save registry — deliberately NOT registered (#2294 / SAVE-D1-11)
///
/// Structurally similar to `AnimationPlayer`/`AnimationStack`, which were
/// deliberately excluded from `byroredux::save_io::build_save_registry`
/// (#1696) because the reloaded cell's systems reconstruct equivalent state
/// from scratch. The same call is believed to apply here: `current_phase`/
/// `active_actions`/`completed_actions` are re-derived from the persisted
/// `QuestStageState` (which IS saved) via the normal `SceneStartRequest`
/// flow when the driving quest stage re-fires on reload, rather than proven
/// byte-for-byte here — flagged by #2294 as a documented, not merely
/// omitted, decision. If a live path is ever found where a reload does NOT
/// re-derive this state, register `ScenePlayer` instead of leaving this
/// comment stale.
///
/// **Caveat (#2380 / SAVE-D1-15)**: the #2295 registry-completeness sweep
/// found sibling fragment-driven state (`ActorCinematicState` et al.) whose
/// superficially-similar "reload re-derives it" assumption did NOT hold —
/// `quest_fragment_dispatch_system` is edge-triggered off an acknowledged
/// event journal, so a `SetStage` transition never replays on reload. This
/// type's `SceneStartRequest`-driven mechanism is structurally different
/// (scene entities are (re)created from live `SceneEvent`s, not a one-shot
/// edge-triggered dispatch), so the distinction is believed to still hold,
/// but has not been independently re-verified since that finding.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenePlayer {
    pub scene_form_id: u32,
    pub state: ScenePlaybackState,
    pub current_phase: u32,
    pub active_actions: Vec<ActiveSceneAction>,
    /// Action indices completed during the current run. Skyrim's
    /// `IsSceneActionComplete` CTDA can query an action after it has left the
    /// active set, so completion cannot be represented only on
    /// [`ActiveSceneAction`]. Cleared whenever the scene starts again.
    pub completed_actions: HashSet<u32>,
}

impl ScenePlayer {
    pub fn new(scene_form_id: u32) -> Self {
        Self {
            scene_form_id,
            state: ScenePlaybackState::Dormant,
            current_phase: 0,
            active_actions: Vec::new(),
            completed_actions: HashSet::new(),
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(
            self.state,
            ScenePlaybackState::WaitingForStart
                | ScenePlaybackState::WaitingForPhaseStart
                | ScenePlaybackState::Playing
        )
    }
}

impl Component for ScenePlayer {
    type Storage = SparseSetStorage<Self>;
}

/// Explicitly request that the scene attached to this entity start.
#[derive(Debug, Clone, Copy, Default)]
pub struct SceneStartRequest;

impl Component for SceneStartRequest {
    type Storage = SparseSetStorage<Self>;
}

/// Explicitly stop the scene attached to this entity.
#[derive(Debug, Clone, Copy, Default)]
pub struct SceneStopRequest;

impl Component for SceneStopRequest {
    type Storage = SparseSetStorage<Self>;
}

/// External completion signals for dialogue, package, or unknown actions.
/// Timer actions normally complete internally but accept the same signal.
#[derive(Debug, Clone, Default)]
pub struct SceneActionCompletionBatch(pub Vec<u32>);

impl Component for SceneActionCompletionBatch {
    type Storage = SparseSetStorage<Self>;
}

/// Scene/action lifecycle output consumed by dialogue, package, animation,
/// audio, and future quest orchestration systems.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneEvent {
    SceneStarted,
    PhaseStarted {
        phase_index: u32,
    },
    ActionStarted {
        action_index: u32,
        action_type: SceneActionType,
        actor_alias: i32,
        actor_entity: Option<EntityId>,
        topic_form_id: Option<u32>,
        packages: Vec<u32>,
    },
    ActionCompleted {
        action_index: u32,
    },
    ActionStopped {
        action_index: u32,
        completed: bool,
    },
    PhaseCompleted {
        phase_index: u32,
    },
    SceneFinished,
    SceneStopped,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SceneEventBatch(pub Vec<SceneEvent>);

impl Component for SceneEventBatch {
    type Storage = SparseSetStorage<Self>;
}

/// One SCEN VMAD binding selected by the player state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneFragmentInvocation {
    pub scene_form_id: u32,
    pub event: SceneFragmentEvent,
    pub script_name: String,
    pub fragment_name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SceneFragmentInvocationBatch(pub Vec<SceneFragmentInvocation>);

impl Component for SceneFragmentInvocationBatch {
    type Storage = SparseSetStorage<Self>;
}

fn snapshot_entities<T: Component>(world: &World) -> HashSet<EntityId> {
    world
        .query::<T>()
        .map(|query| query.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default()
}

fn drain<T: Component>(world: &World) {
    let Some(mut query) = world.query_mut::<T>() else {
        return;
    };
    let entities: Vec<EntityId> = query.iter().map(|(entity, _)| entity).collect();
    for entity in entities {
        query.remove(entity);
    }
}

fn fragments_for(
    scene: &ScenRecord,
    event: SceneFragmentEvent,
    out: &mut Vec<SceneFragmentInvocation>,
) {
    out.extend(
        scene
            .fragments
            .iter()
            .filter(|binding| binding.event == event)
            .map(|binding: &SceneScriptFragment| SceneFragmentInvocation {
                scene_form_id: scene.form_id,
                event: binding.event,
                script_name: binding.script_name.clone(),
                fragment_name: binding.fragment_name.clone(),
            }),
    );
}

fn actor_entity(
    scene: &ScenRecord,
    action: &SceneAction,
    bindings: &SceneActorBindings,
) -> Option<EntityId> {
    let quest = QuestFormId(scene.quest_form_id?);
    (action.actor_id >= 0)
        .then(|| bindings.resolve(quest, action.actor_id))
        .flatten()
}

fn start_phase_actions(
    player: &mut ScenePlayer,
    scene: &ScenRecord,
    bindings: &SceneActorBindings,
    events: &mut Vec<SceneEvent>,
) {
    for action in scene
        .actions
        .iter()
        .filter(|action| action.start_phase == player.current_phase)
    {
        if player
            .active_actions
            .iter()
            .any(|active| active.action_index == action.index)
        {
            continue;
        }
        player.active_actions.push(ActiveSceneAction {
            action_index: action.index,
            action_type: action.action_type,
            end_phase: action.end_phase,
            remaining_seconds: (action.action_type == SceneActionType::Timer)
                .then_some(action.timer_seconds.unwrap_or_default().max(0.0)),
            completed: false,
        });
        events.push(SceneEvent::ActionStarted {
            action_index: action.index,
            action_type: action.action_type,
            actor_alias: action.actor_id,
            actor_entity: actor_entity(scene, action, bindings),
            topic_form_id: action.topic_form_id,
            packages: action.packages.clone(),
        });
    }
}

fn stop_actions(player: &mut ScenePlayer, phase: Option<u32>, events: &mut Vec<SceneEvent>) {
    let mut retained = Vec::with_capacity(player.active_actions.len());
    for action in player.active_actions.drain(..) {
        if phase.is_none_or(|phase| action.end_phase <= phase) {
            events.push(SceneEvent::ActionStopped {
                action_index: action.action_index,
                completed: action.completed,
            });
        } else {
            retained.push(action);
        }
    }
    player.active_actions = retained;
}

fn finish_scene(
    player: &mut ScenePlayer,
    scene: &ScenRecord,
    events: &mut Vec<SceneEvent>,
    fragments: &mut Vec<SceneFragmentInvocation>,
) {
    stop_actions(player, None, events);
    player.state = ScenePlaybackState::Finished;
    events.push(SceneEvent::SceneFinished);
    fragments_for(scene, SceneFragmentEvent::End, fragments);
}

struct TickContext<'a> {
    world: &'a World,
    subject: EntityId,
    bindings: &'a SceneActorBindings,
    explicit_start: bool,
    auto_start: bool,
    stop_requested: bool,
    completions: &'a HashSet<u32>,
    dt: f32,
}

#[derive(Default)]
struct TickOutput {
    events: Vec<SceneEvent>,
    fragments: Vec<SceneFragmentInvocation>,
}

fn tick_player(
    player: &mut ScenePlayer,
    scene: &ScenRecord,
    context: &TickContext<'_>,
) -> TickOutput {
    let mut output = TickOutput::default();
    if context.stop_requested && player.is_running() {
        stop_actions(player, None, &mut output.events);
        player.state = ScenePlaybackState::Stopped;
        output.events.push(SceneEvent::SceneStopped);
        fragments_for(scene, SceneFragmentEvent::End, &mut output.fragments);
        return output;
    }

    if (context.explicit_start || context.auto_start) && !player.is_running() {
        player.current_phase = 0;
        player.active_actions.clear();
        player.completed_actions.clear();
        player.state = ScenePlaybackState::WaitingForStart;
    }

    let condition_context = scene.quest_form_id.map(QuestFormId).map_or_else(
        || ConditionContext::for_subject(context.subject),
        |quest| ConditionContext::for_subject(context.subject).with_quest(quest),
    );
    if player.state == ScenePlaybackState::WaitingForStart {
        if !evaluate(&scene.conditions, context.world, &condition_context) {
            return output;
        }
        player.state = ScenePlaybackState::WaitingForPhaseStart;
        output.events.push(SceneEvent::SceneStarted);
        fragments_for(scene, SceneFragmentEvent::Begin, &mut output.fragments);
        if scene.phases.is_empty() {
            finish_scene(player, scene, &mut output.events, &mut output.fragments);
            return output;
        }
    }

    if player.state == ScenePlaybackState::WaitingForPhaseStart {
        let Some(phase) = scene.phases.get(player.current_phase as usize) else {
            finish_scene(player, scene, &mut output.events, &mut output.fragments);
            return output;
        };
        if !evaluate(&phase.start_conditions, context.world, &condition_context) {
            return output;
        }
        output.events.push(SceneEvent::PhaseStarted {
            phase_index: player.current_phase,
        });
        if let Ok(phase_index) = u8::try_from(player.current_phase) {
            fragments_for(
                scene,
                SceneFragmentEvent::PhaseStart { phase_index },
                &mut output.fragments,
            );
        }
        start_phase_actions(player, scene, context.bindings, &mut output.events);
        player.state = ScenePlaybackState::Playing;
        // A newly-entered phase lives for at least one scheduler tick. This
        // gives fragment/action consumers a frame to react before empty
        // completion conditions can advance it.
        return output;
    }

    if player.state != ScenePlaybackState::Playing {
        return output;
    }

    for action in &mut player.active_actions {
        if action.completed {
            continue;
        }
        if context.completions.contains(&action.action_index) {
            action.completed = true;
        } else if let Some(remaining) = &mut action.remaining_seconds {
            *remaining -= context.dt.max(0.0);
            if *remaining <= 0.0 {
                action.completed = true;
            }
        }
        if action.completed {
            player.completed_actions.insert(action.action_index);
            output.events.push(SceneEvent::ActionCompleted {
                action_index: action.action_index,
            });
        }
    }

    let Some(phase) = scene.phases.get(player.current_phase as usize) else {
        finish_scene(player, scene, &mut output.events, &mut output.fragments);
        return output;
    };
    let ending_actions: Vec<&ActiveSceneAction> = player
        .active_actions
        .iter()
        .filter(|action| action.end_phase == player.current_phase)
        .collect();
    // Creation Kit phase semantics are alternatives: a phase ends when all
    // actions ending here reach Done OR its authored completion conditions
    // pass. This distinction matters for Sandbox/Follow-style packages that
    // never become Done and intentionally rely on CTDAs to release the phase.
    // An entirely empty, unconditional phase still advances; an empty phase
    // with CTDAs waits for those CTDAs instead of using vacuous `all(true)`.
    let ending_actions_complete = if ending_actions.is_empty() {
        phase.completion_conditions.is_empty()
    } else {
        ending_actions.iter().all(|action| action.completed)
    };
    let completion_conditions_pass = !phase.completion_conditions.is_empty()
        && evaluate(
            &phase.completion_conditions,
            context.world,
            &condition_context,
        );
    if !ending_actions_complete && !completion_conditions_pass {
        return output;
    }

    output.events.push(SceneEvent::PhaseCompleted {
        phase_index: player.current_phase,
    });
    if let Ok(phase_index) = u8::try_from(player.current_phase) {
        fragments_for(
            scene,
            SceneFragmentEvent::PhaseCompletion { phase_index },
            &mut output.fragments,
        );
    }
    stop_actions(player, Some(player.current_phase), &mut output.events);

    player.current_phase += 1;
    if player.current_phase as usize >= scene.phases.len() {
        finish_scene(player, scene, &mut output.events, &mut output.fragments);
    } else {
        player.state = ScenePlaybackState::WaitingForPhaseStart;
    }
    output
}

/// Advance all installed scenes by one frame.
///
/// Start/stop/completion inputs are consumed exactly once. Outputs replace the
/// previous frame's batches on each scene entity and are also drained by the
/// global end-of-frame cleanup system.
pub fn scene_playback_system(world: &World, dt: f32) {
    refresh_scene_actor_bindings(world);

    // Ensure direct unit-test/manual invocations retain one-frame semantics
    // even when the scheduler's cleanup system is not run between calls.
    drain::<SceneEventBatch>(world);
    drain::<SceneFragmentInvocationBatch>(world);

    let explicit_starts = snapshot_entities::<SceneStartRequest>(world);
    let stop_requests = snapshot_entities::<SceneStopRequest>(world);
    let completion_batches: HashMap<EntityId, HashSet<u32>> = world
        .query::<SceneActionCompletionBatch>()
        .map(|query| {
            query
                .iter()
                .map(|(entity, batch)| (entity, batch.0.iter().copied().collect()))
                .collect()
        })
        .unwrap_or_default();
    drain::<SceneStartRequest>(world);
    drain::<SceneStopRequest>(world);
    drain::<SceneActionCompletionBatch>(world);

    // The journal is authoritative and independently cursor-driven: scene
    // playback can run at any cadence without racing another consumer for a
    // one-frame marker. The component batch remains accepted as compatibility
    // ingress for tests/tools and is deduplicated by the HashSet.
    let mut quest_starts: HashSet<QuestFormId> = world
        .try_resource_mut::<QuestStageState>()
        .map(|mut state| {
            let read = state.poll_quest_events(SCENE_QUEST_EVENT_SUBSCRIBER);
            if read.missed_events > 0 {
                log::error!(
                    target: "scripting::scene",
                    "scene quest-event subscriber missed {} retained transition(s); \
                     Begin On Quest Start edges may require state resync",
                    read.missed_events
                );
            }
            read.events
                .into_iter()
                .filter(|advance| advance.event.previous_stage == 0)
                .map(|advance| advance.event.quest)
                .collect()
        })
        .unwrap_or_default();
    quest_starts.extend(
        world
            .query::<QuestStageAdvancedBatch>()
            .map(|query| {
                query
                    .iter()
                    .flat_map(|(_, batch)| batch.0.iter())
                    .filter(|advance| advance.previous_stage == 0)
                    .map(|advance| advance.quest)
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default(),
    );

    let players: Vec<(EntityId, ScenePlayer)> = world
        .query::<ScenePlayer>()
        .map(|query| {
            query
                .iter()
                .filter(|(entity, player)| {
                    !quest_starts.is_empty()
                        || player.is_running()
                        || explicit_starts.contains(entity)
                        || stop_requests.contains(entity)
                        || completion_batches.contains_key(entity)
                })
                .map(|(entity, player)| (entity, player.clone()))
                .collect()
        })
        .unwrap_or_default();
    if players.is_empty() {
        return;
    }

    let definitions: Vec<(EntityId, ScenePlayer, Arc<ScenRecord>)> = {
        let registry = world.resource::<SceneRegistry>();
        players
            .into_iter()
            .filter_map(|(entity, player)| {
                registry
                    .definition_arc(player.scene_form_id)
                    .map(|definition| (entity, player, definition))
            })
            .collect()
    };
    let bindings = world.resource::<SceneActorBindings>().clone();
    let subject = world
        .try_resource::<PapyrusPlayerEntity>()
        .map(|player| player.0)
        .unwrap_or_default();

    let empty_completions = HashSet::new();
    let mut updates = Vec::with_capacity(definitions.len());
    for (entity, mut player, scene) in definitions {
        let previous_state = player.state;
        let previous_phase = player.current_phase;
        let auto_start = scene.flags & SCENE_BEGIN_ON_QUEST_START != 0
            && scene
                .quest_form_id
                .is_some_and(|quest| quest_starts.contains(&QuestFormId(quest)));
        let output = tick_player(
            &mut player,
            &scene,
            &TickContext {
                world,
                subject,
                bindings: &bindings,
                explicit_start: explicit_starts.contains(&entity),
                auto_start,
                stop_requested: stop_requests.contains(&entity),
                completions: completion_batches
                    .get(&entity)
                    .unwrap_or(&empty_completions),
                dt,
            },
        );
        if player.state != previous_state || player.current_phase != previous_phase {
            log::info!(
                target: "scripting::scene",
                "SCEN {:08X} '{}' {:?}/phase {} -> {:?}/phase {}",
                scene.form_id,
                scene.editor_id,
                previous_state,
                previous_phase,
                player.state,
                player.current_phase,
            );
        }
        updates.push((entity, player, output.events, output.fragments));
    }

    {
        let mut query = world
            .query_mut::<ScenePlayer>()
            .expect("ScenePlayer storage registered");
        for (entity, player, _, _) in &updates {
            query.insert(*entity, player.clone());
        }
    }
    {
        let mut query = world
            .query_mut::<SceneEventBatch>()
            .expect("SceneEventBatch storage registered");
        for (entity, _, events, _) in &updates {
            if !events.is_empty() {
                query.insert(*entity, SceneEventBatch(events.clone()));
            }
        }
    }
    {
        let mut query = world
            .query_mut::<SceneFragmentInvocationBatch>()
            .expect("SceneFragmentInvocationBatch storage registered");
        for (entity, _, _, fragments) in updates {
            if !fragments.is_empty() {
                query.insert(entity, SceneFragmentInvocationBatch(fragments));
            }
        }
    }
}
