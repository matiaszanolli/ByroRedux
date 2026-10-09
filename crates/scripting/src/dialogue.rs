//! Authored `DIAL` / `INFO` playback for scene dialogue actions.
//!
//! The scene runtime owns action sequencing. This module resolves each
//! dialogue [`SceneEvent::ActionStarted`] to an eligible INFO response,
//! exposes persistent subtitle/presentation state, and returns a
//! [`SceneActionCompletionBatch`] when the line's fallback timer or an
//! external presenter completes it. Voice decoding stays behind that external
//! presentation boundary: Skyrim FUZ/XWM support can consume the same line
//! metadata later without changing scene orchestration.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::{DialRecord, InfoRecord, SceneActionType};

use crate::condition::{evaluate, ConditionContext};
use crate::papyrus_demo::PapyrusPlayerEntity;
use crate::quest_stages::{QuestFormId, QuestStageState};
use crate::scene::{
    append_scene_completions, drain, snapshot, SceneAliasCandidate, SceneEvent, SceneEventBatch,
    ScenePlayer,
};

/// #5271 — authored quest priorities (QUST `priority`), for ordering a
/// multi-quest topic's INFOs: an INFO whose owning quest is running
/// competes by that quest's priority, higher first, with authored file
/// order inside a tie (GECK *Quest Data Tab*: the quest's priority orders
/// its INFOs). Installed once per load beside
/// [`install_dialogue_records`]; when absent the selection keeps
/// authored file order — the pre-#5271 behaviour, and the only sane
/// fallback for a registry built without quest data.
#[derive(Debug, Clone, Default)]
pub struct DialogueQuestPriorities {
    priority: HashMap<u32, u8>,
}

impl Resource for DialogueQuestPriorities {}

/// INFO FormIDs spoken at least once this save (#5367 Phase L). Keyed by
/// the INFO's own FormID; consulted by [`select_info`] so a said
/// Say-Once line stops qualifying until the save is reverted. This set
/// IS save-facing state — the bin crate registers it in the save schema.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct DialogueSpokenInfoForms(pub std::collections::HashSet<u32>);

impl Resource for DialogueSpokenInfoForms {}

/// The dialogue selection RNG (#5367 Phase L). xorshift64 state — every
/// Random pick advances it, so a pinned seed makes selection
/// deterministic for tests; a fresh world's time-seeded default is fine
/// for play. Which greeting an old save rolled is not state anything
/// reads back, so the state itself is never persisted.
#[derive(Debug, Clone, Copy)]
pub struct DialogueRandomState(pub u64);

impl Default for DialogueRandomState {
    fn default() -> Self {
        // Splitmix64 of the process clock — never the xorshift fixed
        // point 0, and not a trivially guessable first roll.
        let mut z = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        z = (z.wrapping_add(0x9E37_79B9_7F4A_7C15)) | 1;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Self(z ^ (z >> 31))
    }
}

impl Resource for DialogueRandomState {}

/// Record that `info_form_id` has been spoken (#5367 Phase L). Called
/// by the activation surface when a line is applied. A no-op when the
/// resource is absent — [`crate::register`] installs it, so that is
/// only a pre-registration test world.
pub fn note_info_spoken(world: &World, info_form_id: u32) {
    if let Some(mut spoken) = world.try_resource_mut::<DialogueSpokenInfoForms>() {
        spoken.0.insert(info_form_id);
    }
}

impl DialogueQuestPriorities {
    /// Build from `(form_id, priority)` pairs — `QustRecord`'s
    /// `form_id`/`priority` at the install site.
    pub fn from_quests(quests: impl IntoIterator<Item = (u32, u8)>) -> Self {
        Self {
            priority: quests.into_iter().collect(),
        }
    }

    /// The quest's authored priority, `0` when unknown (an unloaded or
    /// malformed QUST — lowest precedence, matching the GECK's default).
    pub fn priority_of(&self, quest: u32) -> u8 {
        self.priority.get(&quest).copied().unwrap_or(0)
    }
}

/// Immutable authored dialogue topics keyed by global-space DIAL FormID.
#[derive(Debug, Clone, Default)]
pub struct DialogueRegistry {
    topics: HashMap<u32, Arc<DialRecord>>,
}

impl Resource for DialogueRegistry {}

impl DialogueRegistry {
    /// Build a registry without an ECS world, useful for tools and probes.
    pub fn from_records(records: impl IntoIterator<Item = DialRecord>) -> Self {
        let mut registry = Self::default();
        for record in records {
            registry.topics.insert(record.form_id, Arc::new(record));
        }
        registry
    }

    pub fn topic(&self, form_id: u32) -> Option<&DialRecord> {
        self.topics.get(&form_id).map(Arc::as_ref)
    }

    /// Insert (or refresh) one authored topic. The single insertion
    /// primitive: [`install_dialogue_records`] loops it under `&mut World`,
    /// and `&World` exclusive systems reach it through `resource_mut`.
    pub fn insert_topic(&mut self, record: DialRecord) {
        self.topics.insert(record.form_id, Arc::new(record));
    }

    pub fn len(&self) -> usize {
        self.topics.len()
    }

    pub fn is_empty(&self) -> bool {
        self.topics.is_empty()
    }
}

/// A selected INFO response ready for subtitle and voice presentation.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogueLine {
    pub scene_form_id: u32,
    pub action_index: u32,
    pub topic_form_id: u32,
    pub info_form_id: u32,
    pub owning_quest: Option<QuestFormId>,
    pub speaker: Option<EntityId>,
    pub text: String,
    pub designer_notes: String,
    pub emotion_type: u8,
    pub response_number: u8,
    pub topic_links: Vec<u32>,
    /// Fallback lifetime used when no audio/UI adapter acknowledges the line.
    pub estimated_duration_seconds: f32,
}

/// Mutable lifetime state for one selected line.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveDialogueLine {
    pub line: DialogueLine,
    pub remaining_seconds: f32,
}

/// Persistent active subtitle lines on a scene-player entity.
///
/// A vector is required because authored SCEN actions may overlap.
///
/// # Save registry — deliberately NOT registered (#2294 / SAVE-D1-11)
///
/// Same rationale as [`crate::scene::ScenePlayer`]'s doc comment: mid-line
/// subtitle progress is believed to be re-derived when the driving scene
/// action restarts on reload (via `QuestStageState`), rather than proven
/// here. See that comment for the full reasoning and the #1696 precedent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DialoguePlayback {
    pub active_lines: Vec<ActiveDialogueLine>,
}

impl Component for DialoguePlayback {
    type Storage = SparseSetStorage<Self>;
}

/// Presentation lifecycle output for subtitle, voice, lip-sync, and tools.
#[derive(Debug, Clone, PartialEq)]
pub enum DialoguePresentationEvent {
    Started(DialogueLine),
    Finished {
        action_index: u32,
        info_form_id: u32,
    },
    Stopped {
        action_index: u32,
        info_form_id: u32,
    },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DialoguePresentationEventBatch(pub Vec<DialoguePresentationEvent>);

impl Component for DialoguePresentationEventBatch {
    type Storage = SparseSetStorage<Self>;
}

/// Completion ingress for a real subtitle/voice presenter.
///
/// Insert this on the scene-player entity with action indices whose audio (or
/// user skip) finished. The dialogue system converts them into the scene
/// runtime's canonical completion batch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DialogueLineCompletionBatch(pub Vec<u32>);

impl Component for DialogueLineCompletionBatch {
    type Storage = SparseSetStorage<Self>;
}

/// Register dialogue storage and the immutable registry.
pub fn register(world: &mut World) {
    world.register::<DialoguePlayback>();
    world.register::<DialoguePresentationEventBatch>();
    world.register::<DialogueLineCompletionBatch>();
    if world.try_resource::<DialogueRegistry>().is_none() {
        world.insert_resource(DialogueRegistry::default());
    }
    if world.try_resource::<DialogueQuestPriorities>().is_none() {
        world.insert_resource(DialogueQuestPriorities::default());
    }
    // #5367 Phase L — Say-Once bookkeeping and the selection RNG.
    if world.try_resource::<DialogueSpokenInfoForms>().is_none() {
        world.insert_resource(DialogueSpokenInfoForms::default());
    }
    if world.try_resource::<DialogueRandomState>().is_none() {
        world.insert_resource(DialogueRandomState::default());
    }
}

/// Install parsed DIAL/INFO records. Re-installation refreshes definitions by
/// FormID and does not disturb active playback.
pub fn install_dialogue_records(
    world: &mut World,
    records: impl IntoIterator<Item = DialRecord>,
) -> usize {
    if world.try_resource::<DialogueRegistry>().is_none() {
        register(world);
    }
    let records: Vec<DialRecord> = records.into_iter().collect();
    let count = records.len();
    let mut registry = world.resource_mut::<DialogueRegistry>();
    for record in records {
        registry.insert_topic(record);
    }
    count
}

/// Conservative subtitle fallback duration: a short lead-in plus 2.6 words/s,
/// clamped so blank/non-subtitled voiced INFOs still reach a presenter.
pub fn estimate_dialogue_duration(text: &str) -> f32 {
    let words = text.split_whitespace().count() as f32;
    (0.55 + words / 2.6).clamp(1.5, 12.0)
}

fn actor_matches(info: &InfoRecord, world: &World, actor: Option<EntityId>) -> bool {
    if info.actor_form_id == 0 {
        return true;
    }
    let Some(actor) = actor else {
        return false;
    };
    world
        .get::<SceneAliasCandidate>(actor)
        .is_some_and(|identity| {
            identity.reference_form_id == info.actor_form_id
                || identity.base_form_id == info.actor_form_id
        })
}

fn select_info<'a>(
    topic: &'a DialRecord,
    world: &World,
    actor: Option<EntityId>,
    player: Option<EntityId>,
) -> Option<&'a InfoRecord> {
    let subject = actor.or(player).unwrap_or_default();
    let topic_quest = topic.quest_refs.first().copied().map(QuestFormId);
    // #5271 — a FO3/FNV/Oblivion INFO carries its OWN owning quest
    // (`QSTI`) and counts only while that quest is running (GECK Quest
    // Data Tab). A multi-quest topic lists every owner in `quest_refs`,
    // so gating on "any listed quest runs" lets a stopped quest's line
    // win in file order. INFOs without a QSTI (Skyrim+/FO4 — ownership
    // is DIAL-side QNAM) keep the topic-level ownership, which the
    // activation route already filtered for running state.
    let stages = world.try_resource::<QuestStageState>();
    let info_quest_is_running = |info: &InfoRecord| match stages.as_deref() {
        None => true,
        Some(stages) => info.quest == 0 || stages.is_running(QuestFormId(info.quest)),
    };
    // #5271 — quest priority orders the candidates (higher first,
    // authored file order inside a tie). Only reorder when priorities
    // are installed; the stable sort keeps file order otherwise.
    let mut order: Vec<usize> = (0..topic.infos.len()).collect();
    if let Some(priorities) = world.try_resource::<DialogueQuestPriorities>() {
        order.sort_by_key(|&i| {
            let quest = topic.infos[i].quest;
            let quest = if quest != 0 {
                quest
            } else {
                topic_quest.map(|q| q.0).unwrap_or(0)
            };
            std::cmp::Reverse(priorities.priority_of(quest))
        });
    }
    // filter_map, not find_map: the #5367 Phase L Random pool needs every
    // passing candidate, not just the first.
    let passing = order.into_iter().filter_map(|i| {
        let info = &topic.infos[i];
        if !info_quest_is_running(info) {
            return None;
        }
        // #5367 Phase L — a spoken Say-Once line stops qualifying for
        // the rest of the save (a missing spoken-set resource treats
        // nothing as said: tests and pre-install worlds).
        if info.say_once()
            && world
                .try_resource::<DialogueSpokenInfoForms>()
                .is_some_and(|spoken| spoken.0.contains(&info.form_id))
        {
            return None;
        }
        // The INFO's own quest drives quest-scoped condition functions
        // (GetStage & co.); topic ownership is the fallback for
        // QSTI-less INFOs.
        let context_quest = if info.quest != 0 {
            Some(QuestFormId(info.quest))
        } else {
            topic_quest
        };
        let mut context = ConditionContext::for_subject(subject);
        context.target = player;
        if let Some(quest) = context_quest {
            context = context.with_quest(quest);
        }
        (actor_matches(info, world, actor) && evaluate(&info.conditions, world, &context))
            .then_some((i, info))
    });

    // #5367 Phase L / #5397 — the authored positional rule (CS wiki,
    // GECK `Category:Dialogue`): the first passing INFO in
    // priority/file order wins. If it is Random it opens a stack: it and
    // the immediately following passing Random INFOs, ended by the next
    // passing non-Random INFO (excluded) or by a Random End INFO
    // (included), roll uniformly. Stacks are built over *passing* INFOs
    // only — the GECK: Random End "is only necessary if the next
    // qualifying info is also random". Deterministic under a pinned
    // `DialogueRandomState` seed; with no RNG resource installed
    // (pre-install test worlds) the stack's first INFO wins.
    let passing: Vec<(usize, &InfoRecord)> = passing.collect();
    let (_, first) = *passing.first()?;
    if !first.random() {
        return Some(first);
    }
    let mut stack = 0;
    for (_, info) in &passing {
        if !info.random() {
            break;
        }
        stack += 1;
        if info.random_end() {
            break;
        }
    }
    let pick = match world.try_resource_mut::<DialogueRandomState>() {
        Some(mut state) => xorshift64(&mut state.0) as usize % stack,
        None => 0,
    };
    passing.get(pick).map(|(_, info)| *info)
}

/// xorshift64 — deterministic, stateless-of-global-RNG uniform source.
fn xorshift64(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// Public activation-route wrapper around the SCEN-path [`select_info`]:
/// the first eligible INFO on `topic` — its speaker matching `actor`
/// (ANAM, or the GetIsID-derived speaker), its CTDA list passing with
/// the actor as subject and the player as target, and (FO3/FNV/Oblivion,
/// #5271) its own `QSTI` quest running — ordered by quest priority when
/// [`DialogueQuestPriorities`] is installed. The authored greeting/topic
/// pick.
pub fn select_first_info<'a>(
    topic: &'a DialRecord,
    world: &World,
    actor: Option<EntityId>,
    player: Option<EntityId>,
) -> Option<&'a InfoRecord> {
    select_info(topic, world, actor, player)
}

fn finish_line(
    action_index: u32,
    active: &mut Vec<ActiveDialogueLine>,
    events: &mut Vec<DialoguePresentationEvent>,
) -> bool {
    let Some(position) = active
        .iter()
        .position(|line| line.line.action_index == action_index)
    else {
        return false;
    };
    let line = active.remove(position).line;
    events.push(DialoguePresentationEvent::Finished {
        action_index,
        info_form_id: line.info_form_id,
    });
    true
}

fn stop_line(
    action_index: u32,
    active: &mut Vec<ActiveDialogueLine>,
    events: &mut Vec<DialoguePresentationEvent>,
) {
    let Some(position) = active
        .iter()
        .position(|line| line.line.action_index == action_index)
    else {
        return;
    };
    let line = active.remove(position).line;
    events.push(DialoguePresentationEvent::Stopped {
        action_index,
        info_form_id: line.info_form_id,
    });
}

/// Resolve new scene dialogue actions, maintain subtitle lifetime, and signal
/// scene completion. Schedule immediately after [`crate::scene_playback_system`].
pub fn scene_dialogue_system(world: &World, dt: f32) {
    // Our output survives through the rest of the frame for presentation and
    // is retired at the start of our next update.
    drain::<DialoguePresentationEventBatch>(world);

    let external_completions = snapshot::<DialogueLineCompletionBatch>(world);
    drain::<DialogueLineCompletionBatch>(world);
    let scene_events = snapshot::<SceneEventBatch>(world);
    let mut playbacks = snapshot::<DialoguePlayback>(world);
    let scene_form_ids: HashMap<EntityId, u32> = world
        .query::<ScenePlayer>()
        .map(|query| {
            query
                .iter()
                .map(|(entity, player)| (entity, player.scene_form_id))
                .collect()
        })
        .unwrap_or_default();
    let topics = world
        .try_resource::<DialogueRegistry>()
        .map(|registry| registry.topics.clone())
        .unwrap_or_default();
    let player_entity = world
        .try_resource::<PapyrusPlayerEntity>()
        .map(|player| player.0);

    let mut entities: HashSet<EntityId> = playbacks.keys().copied().collect();
    entities.extend(scene_events.keys().copied());
    entities.extend(external_completions.keys().copied());

    let mut outputs: HashMap<EntityId, Vec<DialoguePresentationEvent>> = HashMap::new();
    let mut scene_completions: HashMap<EntityId, Vec<u32>> = HashMap::new();
    let dt = dt.max(0.0);

    for entity in entities {
        let mut playback = playbacks.remove(&entity).unwrap_or_default();
        let mut started_this_tick = HashSet::new();
        let output = outputs.entry(entity).or_default();

        if let Some(batch) = scene_events.get(&entity) {
            for event in &batch.0 {
                match event {
                    SceneEvent::ActionStarted {
                        action_index,
                        action_type: SceneActionType::Dialogue,
                        actor_entity,
                        topic_form_id,
                        ..
                    } => {
                        if playback
                            .active_lines
                            .iter()
                            .any(|line| line.line.action_index == *action_index)
                        {
                            continue;
                        }
                        let Some(topic_form_id) = *topic_form_id else {
                            log::warn!(
                                target: "scripting::dialogue",
                                "Scene dialogue action {action_index} has no DIAL topic; completing it"
                            );
                            scene_completions
                                .entry(entity)
                                .or_default()
                                .push(*action_index);
                            continue;
                        };
                        let Some(topic) = topics.get(&topic_form_id) else {
                            log::warn!(
                                target: "scripting::dialogue",
                                "Scene dialogue action {action_index} references missing DIAL {topic_form_id:08X}; completing it"
                            );
                            scene_completions
                                .entry(entity)
                                .or_default()
                                .push(*action_index);
                            continue;
                        };
                        let Some(info) = select_info(topic, world, *actor_entity, player_entity)
                        else {
                            log::debug!(
                                target: "scripting::dialogue",
                                "No eligible INFO for DIAL {topic_form_id:08X} on action {action_index}; completing it"
                            );
                            scene_completions
                                .entry(entity)
                                .or_default()
                                .push(*action_index);
                            continue;
                        };
                        let duration = estimate_dialogue_duration(&info.response_text);
                        let line = DialogueLine {
                            scene_form_id: scene_form_ids.get(&entity).copied().unwrap_or_default(),
                            action_index: *action_index,
                            topic_form_id,
                            info_form_id: info.form_id,
                            owning_quest: topic.quest_refs.first().copied().map(QuestFormId),
                            speaker: *actor_entity,
                            text: info.response_text.clone(),
                            designer_notes: info.designer_notes.clone(),
                            emotion_type: info.emotion_type,
                            response_number: info.response_number,
                            topic_links: info.topic_links.clone(),
                            estimated_duration_seconds: duration,
                        };
                        output.push(DialoguePresentationEvent::Started(line.clone()));
                        playback.active_lines.push(ActiveDialogueLine {
                            line,
                            remaining_seconds: duration,
                        });
                        started_this_tick.insert(*action_index);
                    }
                    SceneEvent::ActionCompleted { action_index } => {
                        finish_line(*action_index, &mut playback.active_lines, output);
                    }
                    SceneEvent::ActionStopped { action_index, .. } => {
                        stop_line(*action_index, &mut playback.active_lines, output);
                    }
                    SceneEvent::SceneFinished | SceneEvent::SceneStopped => {
                        let action_indices: Vec<u32> = playback
                            .active_lines
                            .iter()
                            .map(|line| line.line.action_index)
                            .collect();
                        for action_index in action_indices {
                            stop_line(action_index, &mut playback.active_lines, output);
                        }
                    }
                    _ => {}
                }
            }
        }

        if let Some(batch) = external_completions.get(&entity) {
            for action_index in &batch.0 {
                if finish_line(*action_index, &mut playback.active_lines, output) {
                    scene_completions
                        .entry(entity)
                        .or_default()
                        .push(*action_index);
                }
            }
        }

        let mut elapsed = Vec::new();
        for active in &mut playback.active_lines {
            if started_this_tick.contains(&active.line.action_index) {
                continue;
            }
            active.remaining_seconds -= dt;
            if active.remaining_seconds <= 0.0 {
                elapsed.push(active.line.action_index);
            }
        }
        for action_index in elapsed {
            if finish_line(action_index, &mut playback.active_lines, output) {
                scene_completions
                    .entry(entity)
                    .or_default()
                    .push(action_index);
            }
        }

        if !playback.active_lines.is_empty() {
            playbacks.insert(entity, playback);
        }
    }

    if let Some(mut query) = world.query_mut::<DialoguePlayback>() {
        let existing: Vec<EntityId> = query.iter().map(|(entity, _)| entity).collect();
        for entity in existing {
            if !playbacks.contains_key(&entity) {
                query.remove(entity);
            }
        }
        for (entity, playback) in playbacks {
            query.insert(entity, playback);
        }
    }
    if let Some(mut query) = world.query_mut::<DialoguePresentationEventBatch>() {
        for (entity, events) in outputs {
            if !events.is_empty() {
                query.insert(entity, DialoguePresentationEventBatch(events));
            }
        }
    }
    append_scene_completions(world, scene_completions);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SceneActionCompletionBatch;
    use byroredux_plugin::esm::records::{ScenRecord, SceneAction, ScenePhase};

    const SCENE: u32 = 0x100;
    const TOPIC: u32 = 0x200;
    const QUEST: u32 = 0x300;
    const INFO: u32 = 0x400;
    const ACTION: u32 = 12;

    // ── #5367 Phase L: line-lifetime selection semantics ───────────

    fn flagged_info(form_id: u32, text: &str, flags1: u8) -> InfoRecord {
        InfoRecord {
            form_id,
            response_text: text.to_owned(),
            data: Some(byroredux_plugin::esm::records::InfoDataHeader {
                flags1,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// #5397 (OBL-2026-10-08-D2-01) — the positional rule: an earlier
    /// passing Random set is spoken from; the later plain line only ends
    /// the stack. Pre-fix the plain line won (the inverted rule).
    #[test]
    fn earlier_random_set_wins_over_a_later_plain_line() {
        let mut world = World::new();
        super::register(&mut world);
        let actor = world.spawn();
        let record = topic(vec![
            flagged_info(0x401, "roll me", 0x02),     // Random
            flagged_info(0x402, "the fixed line", 0), // plain — ends the stack
        ]);
        world.insert_resource(DialogueRandomState(12345));
        for _ in 0..8 {
            let picked = select_first_info(&record, &world, Some(actor), None);
            assert_eq!(picked.expect("selects").form_id, 0x401);
        }
    }

    /// A plain line before any Random INFO wins outright — the
    /// deterministic quest line the P4 route gates on.
    #[test]
    fn earlier_plain_line_wins_over_a_later_random_set() {
        let mut world = World::new();
        super::register(&mut world);
        let actor = world.spawn();
        let record = topic(vec![
            flagged_info(0x401, "the fixed line", 0),
            flagged_info(0x402, "roll me", 0x02),
            flagged_info(0x403, "or me", 0x02),
        ]);
        world.insert_resource(DialogueRandomState(12345));
        for _ in 0..8 {
            let picked = select_first_info(&record, &world, Some(actor), None);
            assert_eq!(picked.expect("selects").form_id, 0x401);
        }
    }

    /// #5397 — Random End closes a stack even when the next passing INFO
    /// is also Random (vanilla's adjacent guard-arrest sets), and a
    /// non-Random INFO stops later Random INFOs from pooling with the
    /// first run.
    #[test]
    fn random_end_and_plain_lines_bound_the_random_stack() {
        let mut world = World::new();
        super::register(&mut world);
        let actor = world.spawn();
        let record = topic(vec![
            flagged_info(0x401, "halt, lawbreaker", 0x02),
            flagged_info(0x402, "halt, criminal", 0x02 | 0x20), // Random End
            flagged_info(0x403, "halt, scofflaw", 0x02),        // next set
            flagged_info(0x404, "plain", 0),
            flagged_info(0x405, "later random", 0x02),
        ]);
        world.insert_resource(DialogueRandomState(0xC0FFEE));
        let mut distinct = std::collections::HashSet::new();
        for _ in 0..64 {
            distinct.insert(
                select_first_info(&record, &world, Some(actor), None)
                    .expect("the first set passes")
                    .form_id,
            );
        }
        assert_eq!(
            distinct,
            [0x401, 0x402].into_iter().collect(),
            "only the first stack (up to and including its Random End) rolls"
        );
    }

    /// An all-Random pool rolls: with a pinned seed the picks vary
    /// across repeated selections (the state advances every roll).
    #[test]
    fn all_random_pool_rolls_under_a_pinned_seed() {
        let mut world = World::new();
        super::register(&mut world);
        let actor = world.spawn();
        let record = topic(vec![
            flagged_info(0x401, "greeting one", 0x02),
            flagged_info(0x402, "greeting two", 0x02),
            flagged_info(0x403, "greeting three", 0x02),
        ]);
        world.insert_resource(DialogueRandomState(0xC0FFEE));
        let mut distinct = std::collections::HashSet::new();
        for _ in 0..24 {
            distinct.insert(select_first_info(&record, &world, Some(actor), None)
                .expect("a pool member passes")
                .form_id);
        }
        assert!(
            distinct.len() >= 2,
            "the pool must roll, not always take the first: {distinct:?}"
        );
    }

    /// A spoken Say-Once line stops qualifying; without the spoken-set
    /// resource nothing is disqualified.
    #[test]
    fn say_once_line_disqualifies_after_being_spoken() {
        let mut world = World::new();
        super::register(&mut world);
        let actor = world.spawn();
        let record = topic(vec![
            flagged_info(0x401, "said it once", 0x04), // Say Once
            flagged_info(0x402, "the follow-up", 0),
        ]);

        // Before: the Say-Once line is the first passing candidate.
        assert_eq!(
            select_first_info(&record, &world, Some(actor), None)
                .expect("selects")
                .form_id,
            0x401
        );
        // Spoken: disqualified, the follow-up takes over.
        world.insert_resource(DialogueSpokenInfoForms([0x401].into_iter().collect()));
        assert_eq!(
            select_first_info(&record, &world, Some(actor), None)
                .expect("selects")
                .form_id,
            0x402
        );
    }

    fn info(form_id: u32, actor_form_id: u32, text: &str) -> InfoRecord {
        InfoRecord {
            form_id,
            actor_form_id,
            response_text: text.to_owned(),
            designer_notes: "calmly".to_owned(),
            emotion_type: 5,
            response_number: 2,
            topic_links: vec![0x500],
            ..Default::default()
        }
    }

    fn topic(infos: Vec<InfoRecord>) -> DialRecord {
        DialRecord {
            form_id: TOPIC,
            quest_refs: vec![QUEST],
            infos,
            ..Default::default()
        }
    }

    fn setup(records: Vec<DialRecord>) -> (World, EntityId, EntityId, EntityId) {
        let mut world = World::new();
        crate::scene::register(&mut world);
        super::register(&mut world);
        let player = world.spawn();
        let actor = world.spawn();
        let scene = world.spawn();
        world.insert_resource(PapyrusPlayerEntity(player));
        world.insert(scene, ScenePlayer::new(SCENE));
        install_dialogue_records(&mut world, records);
        (world, scene, actor, player)
    }

    fn emit_start(world: &mut World, scene: EntityId, actor: EntityId) {
        world.insert(
            scene,
            SceneEventBatch(vec![SceneEvent::ActionStarted {
                action_index: ACTION,
                action_type: SceneActionType::Dialogue,
                actor_alias: 7,
                actor_entity: Some(actor),
                topic_form_id: Some(TOPIC),
                packages: Vec::new(),
            }]),
        );
    }

    fn replace_scene_events(world: &mut World, scene: EntityId, events: Vec<SceneEvent>) {
        world.insert(scene, SceneEventBatch(events));
    }

    #[test]
    fn authored_info_becomes_subtitle_then_times_out_into_scene_completion() {
        let (mut world, scene, actor, _) = setup(vec![topic(vec![info(
            INFO,
            0,
            "Hey, you. You're finally awake.",
        )])]);
        emit_start(&mut world, scene, actor);

        // A newly emitted line receives its complete first presentation frame,
        // even when the caller's dt is larger than its fallback duration.
        scene_dialogue_system(&world, 30.0);

        let playback = world.get::<DialoguePlayback>(scene).expect("active line");
        assert_eq!(playback.active_lines.len(), 1);
        let line = &playback.active_lines[0].line;
        assert_eq!(line.scene_form_id, SCENE);
        assert_eq!(line.topic_form_id, TOPIC);
        assert_eq!(line.info_form_id, INFO);
        assert_eq!(line.owning_quest, Some(QuestFormId(QUEST)));
        assert_eq!(line.speaker, Some(actor));
        assert_eq!(line.text, "Hey, you. You're finally awake.");
        assert_eq!(line.designer_notes, "calmly");
        assert_eq!(line.emotion_type, 5);
        assert_eq!(line.response_number, 2);
        assert_eq!(line.topic_links, vec![0x500]);
        let duration = line.estimated_duration_seconds;
        drop(playback);
        assert!(world.get::<SceneActionCompletionBatch>(scene).is_none());
        assert!(matches!(
            world
                .get::<DialoguePresentationEventBatch>(scene)
                .expect("start output")
                .0
                .as_slice(),
            [DialoguePresentationEvent::Started(started)] if started.info_form_id == INFO
        ));

        replace_scene_events(&mut world, scene, Vec::new());
        scene_dialogue_system(&world, duration);

        assert!(world.get::<DialoguePlayback>(scene).is_none());
        assert_eq!(
            world
                .get::<SceneActionCompletionBatch>(scene)
                .expect("scene completion")
                .0,
            vec![ACTION]
        );
        assert_eq!(
            world
                .get::<DialoguePresentationEventBatch>(scene)
                .expect("finish output")
                .0,
            vec![DialoguePresentationEvent::Finished {
                action_index: ACTION,
                info_form_id: INFO,
            }]
        );
    }

    #[test]
    fn actor_restriction_selects_matching_info() {
        let matching_base = 0xA00;
        let (mut world, scene, actor, _) = setup(vec![topic(vec![
            info(0x401, 0xBAD, "wrong actor"),
            info(INFO, matching_base, "matching actor"),
        ])]);
        world.insert(
            actor,
            SceneAliasCandidate {
                reference_form_id: 0xB00,
                base_form_id: matching_base,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        emit_start(&mut world, scene, actor);

        scene_dialogue_system(&world, 0.0);

        let playback = world.get::<DialoguePlayback>(scene).expect("active line");
        assert_eq!(playback.active_lines[0].line.info_form_id, INFO);
        assert_eq!(playback.active_lines[0].line.text, "matching actor");
    }

    #[test]
    fn presenter_completion_finishes_early_and_merges_scene_batch() {
        let (mut world, scene, actor, _) =
            setup(vec![topic(vec![info(INFO, 0, "A deliberately long line")])]);
        emit_start(&mut world, scene, actor);
        scene_dialogue_system(&world, 0.0);
        replace_scene_events(&mut world, scene, Vec::new());
        world.insert(scene, SceneActionCompletionBatch(vec![99]));
        world.insert(scene, DialogueLineCompletionBatch(vec![ACTION]));

        scene_dialogue_system(&world, 0.0);

        assert!(world.get::<DialoguePlayback>(scene).is_none());
        assert!(world.get::<DialogueLineCompletionBatch>(scene).is_none());
        assert_eq!(
            world
                .get::<SceneActionCompletionBatch>(scene)
                .expect("merged scene completion")
                .0,
            vec![99, ACTION]
        );
    }

    #[test]
    fn unresolved_dialogue_does_not_stall_scene() {
        let (mut world, scene, actor, _) = setup(Vec::new());
        emit_start(&mut world, scene, actor);

        scene_dialogue_system(&world, 0.0);

        assert!(world.get::<DialoguePlayback>(scene).is_none());
        assert_eq!(
            world
                .get::<SceneActionCompletionBatch>(scene)
                .expect("fail-safe completion")
                .0,
            vec![ACTION]
        );
    }

    #[test]
    fn scene_stop_clears_subtitle_without_completing_action() {
        let (mut world, scene, actor, _) = setup(vec![topic(vec![info(INFO, 0, "Wait")])]);
        emit_start(&mut world, scene, actor);
        scene_dialogue_system(&world, 0.0);
        replace_scene_events(
            &mut world,
            scene,
            vec![
                SceneEvent::ActionStopped {
                    action_index: ACTION,
                    completed: false,
                },
                SceneEvent::SceneStopped,
            ],
        );

        scene_dialogue_system(&world, 0.0);

        assert!(world.get::<DialoguePlayback>(scene).is_none());
        assert!(world.get::<SceneActionCompletionBatch>(scene).is_none());
        assert_eq!(
            world
                .get::<DialoguePresentationEventBatch>(scene)
                .expect("stop output")
                .0,
            vec![DialoguePresentationEvent::Stopped {
                action_index: ACTION,
                info_form_id: INFO,
            }]
        );
    }

    #[test]
    fn install_refreshes_topic_definition() {
        let (mut world, _, _, _) = setup(vec![topic(vec![info(INFO, 0, "old")])]);
        install_dialogue_records(&mut world, vec![topic(vec![info(INFO, 0, "new")])]);

        let registry = world.resource::<DialogueRegistry>();
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.topic(TOPIC).unwrap().infos[0].response_text, "new");
    }

    #[test]
    fn scene_and_dialogue_systems_complete_an_authored_action_end_to_end() {
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        let actor = world.spawn();
        world.insert_resource(PapyrusPlayerEntity(player));
        install_dialogue_records(&mut world, vec![topic(vec![info(INFO, 0, "Wake up")])]);
        crate::scene::install_scene_records(
            &mut world,
            [ScenRecord {
                form_id: SCENE,
                quest_form_id: Some(QUEST),
                phases: vec![ScenePhase {
                    name: "Opening".to_owned(),
                    ..Default::default()
                }],
                actions: vec![SceneAction {
                    action_type: SceneActionType::Dialogue,
                    actor_id: 7,
                    index: ACTION,
                    start_phase: 0,
                    end_phase: 0,
                    topic_form_id: Some(TOPIC),
                    ..Default::default()
                }],
                ..Default::default()
            }],
        );
        world
            .resource_mut::<crate::scene::SceneActorBindings>()
            .bind(QuestFormId(QUEST), 7, actor);
        let scene = world
            .resource::<crate::scene::SceneRegistry>()
            .scene_entity(SCENE)
            .expect("scene entity");
        world.insert(scene, crate::scene::SceneStartRequest);

        crate::scene::scene_playback_system(&world, 0.0);
        scene_dialogue_system(&world, 0.0);
        let duration = world
            .get::<DialoguePlayback>(scene)
            .expect("dialogue started from scene output")
            .active_lines[0]
            .line
            .estimated_duration_seconds;

        crate::scene::scene_playback_system(&world, 0.0);
        scene_dialogue_system(&world, duration);
        crate::scene::scene_playback_system(&world, 0.0);
        scene_dialogue_system(&world, 0.0);

        let scene_player = world.get::<ScenePlayer>(scene).expect("scene player");
        assert_eq!(
            scene_player.state,
            crate::scene::ScenePlaybackState::Finished
        );
        assert!(scene_player.completed_actions.contains(&ACTION));
        assert!(scene_player.active_actions.is_empty());
        assert!(world.get::<DialoguePlayback>(scene).is_none());
    }
}
