//! Quest stage runtime — the ECS-side store for Papyrus `SetStage` /
//! `GetStage` / `GetStageDone`.
//!
//! Lands as part of the R5 follow-up (the SetStage half of the
//! Papyrus quest-prototype evaluation; see
//! [`docs/r5-evaluation.md`](../../../docs/r5-evaluation.md)). The
//! resource holds the runtime state per quest; the parsed `QustRecord`
//! (in `byroredux_plugin::esm::records`) carries the static
//! ESM-side data and is intentionally separate.
//!
//! ## Why a `Resource`, not a per-quest entity
//!
//! Quest stages are global game state, not bound to any world stream
//! chunk, cell, or actor entity. They persist across cell unloads,
//! span every loaded plugin, and need single-lookup access from
//! every script system. A resource keyed by quest FormID matches the
//! Papyrus mental model exactly — `Quest Property Q Auto; Q.SetStage(N)`
//! resolves through the global Game state, not the local entity tree.
//!
//! ## What's deliberately NOT here yet
//!
//! - **`OnStageSet` event handlers.** Papyrus scripts can declare
//!   `Event OnStageSet(int auiStageID, int auiItemID)` callbacks on
//!   arbitrary (non-quest) scripts observing another quest's stage
//!   changes. Only the QUST-owned `Fragment_N` stage-fragment path
//!   (see [`crate::fragment::quest_fragment_dispatch_system`]) is
//!   wired today; generic cross-script `OnStageSet` subscription
//!   stays future work.
//!
//! Stage-fragment dispatch itself (the per-stage "fragment scripts"
//! that run when a stage advances, driven by the [`QuestStageAdvanced`]
//! marker this module emits) and stage objectives
//! (`SetObjectiveDisplayed` / `SetObjectiveCompleted` /
//! `SetObjectiveFailed`, held in [`QuestObjectiveState`] below) have
//! both shipped — see [`crate::fragment`].

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::{QustRecord, QUEST_FLAG_START_GAME_ENABLED};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::papyrus_demo::PlayerEntity;

/// Typed wrapper around a quest's `FormId` (u32). Same shape as the
/// raw `form_id: u32` field on `QustRecord`; the wrapper exists so
/// quest-keyed maps don't accidentally collide with other FormID
/// spaces (item, NPC, cell) when M47.2's transpiler emits typed
/// quest references from Papyrus `Quest Property X Auto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct QuestFormId(pub u32);

/// Runtime state of every active quest, keyed by FormID.
///
/// One `QuestStageData` per quest the player has interacted with;
/// quests the player has never touched live in their default state
/// (`current_stage = 0`, `stages_done` empty) and don't need a map
/// entry — the [`QuestStageState::get_stage`] / [`get_stage_done`]
/// accessors return defaults for missing entries.
///
/// Insertion happens lazily on the first [`set_stage`] for a given
/// quest. This keeps the resource size proportional to "quests the
/// player has touched" rather than "every quest in the loaded
/// plugins" (Skyrim ships ~360 quests + ~340 DLC; eager init would
/// allocate ~30 KB of map entries the player will never visit).
#[derive(Debug, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct QuestStageState {
    quests: HashMap<QuestFormId, QuestStageData>,
    /// Transient delivery state. Quest progress is saved; event history and
    /// subscriber cursors are process-local and restart cleanly after load.
    #[cfg_attr(feature = "save", serde(skip, default))]
    events: QuestEventRuntime,
}

impl Resource for QuestStageState {}

/// Per-quest runtime state. Matches Papyrus's observable surface:
///
/// - `current_stage` → `Quest.GetStage()`. Single integer, the
///   most recently `SetStage`-ed value.
/// - `stages_done` → `Quest.GetStageDone(N)`. A set, not a single
///   value: Papyrus quests can run through stages 10 → 20 → 30 →
///   25 (sidetrack) → 50, and `GetStageDone(20)` returns true even
///   after `SetStage(50)`. The set retains every stage the quest
///   ever passed through.
///
/// Pre-R5 follow-up this was modelled as just a `u16 current_stage`;
/// the DA10 translation surfaced that `GetStageDone(37) && !GetStageDone(40)`
/// requires per-stage history, not just "current". Bethesda's
/// runtime carries the same shape.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct QuestStageData {
    pub current_stage: u16,
    pub stages_done: HashSet<u16>,
}

impl QuestStageState {
    /// Start a quest once, optionally advancing it to its authored Start Up
    /// Stage. Map membership is the lifecycle bit: this remains distinct from
    /// `current_stage == 0`, because a started quest may validly stay at zero.
    ///
    /// Returns a quest-start event on the first call and `None` thereafter.
    /// [`reset`](Self::reset) removes the entry, allowing a later restart.
    pub fn start_quest(
        &mut self,
        quest: QuestFormId,
        start_up_stage: Option<u16>,
    ) -> Option<QuestStageAdvanced> {
        if self.quests.contains_key(&quest) {
            return None;
        }

        let mut data = QuestStageData::default();
        if let Some(stage) = start_up_stage {
            data.current_stage = stage;
            data.stages_done.insert(stage);
        }
        self.quests.insert(quest, data);
        let event = QuestStageAdvanced {
            quest,
            previous_stage: 0,
            new_stage: start_up_stage.unwrap_or(0),
        };
        self.events.push(event);
        Some(event)
    }

    /// Whether a quest has entered its runtime lifecycle. This checks map
    /// membership rather than stage value so started-at-zero is observable.
    pub fn is_started(&self, quest: QuestFormId) -> bool {
        self.quests.contains_key(&quest)
    }

    /// Papyrus `Quest.SetStage(stage)`. Advances `current_stage` to
    /// `stage` and adds `stage` to `stages_done` so future
    /// `GetStageDone(stage)` returns `true`. Lazy-creates the quest
    /// entry if first touch.
    ///
    /// Returns the *previous* `current_stage` for callers that want
    /// to detect the transition (e.g., guarded "do something once"
    /// patterns the M47.0 fragment runtime will consume).
    ///
    /// Papyrus `SetStage` is a no-op if the quest has the
    /// `Allow Repeated Stages` flag off and the stage was already
    /// done (`stages_done` already contains the new stage).
    /// `QustRecord.quest_flags & 0x?` carries that bit but the
    /// caller is responsible for the gate — this method
    /// unconditionally writes, matching the un-flagged behaviour
    /// (which is also the implementation Skyrim's `SetStage` falls
    /// back to when the runtime can't resolve the flags).
    pub fn set_stage(&mut self, quest: QuestFormId, stage: u16) -> u16 {
        let entry = self.quests.entry(quest).or_default();
        let prev = entry.current_stage;
        entry.current_stage = stage;
        entry.stages_done.insert(stage);
        self.events.push(QuestStageAdvanced {
            quest,
            previous_stage: prev,
            new_stage: stage,
        });
        prev
    }

    /// Papyrus `Quest.GetStage()`. Returns the most recently
    /// `SetStage`-ed value, or `0` (the implicit "not started"
    /// stage) for a quest the player has never interacted with.
    pub fn get_stage(&self, quest: QuestFormId) -> u16 {
        self.quests
            .get(&quest)
            .map(|q| q.current_stage)
            .unwrap_or(0)
    }

    /// Papyrus `Quest.GetStageDone(N)`. Returns `true` iff the quest
    /// has ever passed through stage `N`. Differs from
    /// `get_stage() == N` because a quest can have advanced past `N`
    /// and still report `GetStageDone(N) == true`.
    ///
    /// Returns `false` for quests the player has never interacted
    /// with (the default empty `stages_done` set).
    pub fn get_stage_done(&self, quest: QuestFormId, stage: u16) -> bool {
        self.quests
            .get(&quest)
            .map(|q| q.stages_done.contains(&stage))
            .unwrap_or(false)
    }

    /// Reset a quest's state to "not started" (`current_stage = 0`,
    /// `stages_done` empty). Mirrors Papyrus's `Quest.Reset()`.
    /// Used by quest-restart sequences (Daedric quests that
    /// re-trigger on second playthrough, radiant quests that loop).
    pub fn reset(&mut self, quest: QuestFormId) {
        self.quests.remove(&quest);
    }

    /// Iterate every quest currently tracked. Used by the save
    /// system (M45) to serialize quest state, and by debug-server
    /// inspectors. Stable iteration order is NOT promised — the
    /// HashMap backing is intentional (the save system sorts by
    /// FormID before writing).
    pub fn iter(&self) -> impl Iterator<Item = (QuestFormId, &QuestStageData)> {
        self.quests.iter().map(|(k, v)| (*k, v))
    }

    /// Create an independent event subscription beginning after all events
    /// currently committed. Satellite systems keep this ID and poll whenever
    /// their own scheduler runs; no 60 Hz coupling is required.
    pub fn subscribe_to_quest_events(&mut self) -> QuestEventSubscriberId {
        self.events.subscribe(false)
    }

    /// Create a subscription that replays all retained events before
    /// following new transitions.
    pub fn subscribe_to_retained_quest_events(&mut self) -> QuestEventSubscriberId {
        self.events.subscribe(true)
    }

    /// Atomically claim every retained event not yet observed by `subscriber`.
    /// Concurrent polls using the same ID cannot receive the same event twice.
    /// A positive `missed_events` means the subscriber fell behind bounded
    /// retention and must resynchronise from canonical quest state.
    pub fn poll_quest_events(&mut self, subscriber: QuestEventSubscriberId) -> QuestEventRead {
        self.events.poll(subscriber)
    }

    /// Advance a subscriber past every event currently committed. Fragment
    /// dispatch uses this after synchronously cascading its own SetStage calls
    /// so those same transitions are not dispatched again on its next cadence.
    pub fn acknowledge_quest_events(&mut self, subscriber: QuestEventSubscriberId) {
        self.events.acknowledge(subscriber);
    }

    pub fn unsubscribe_from_quest_events(&mut self, subscriber: QuestEventSubscriberId) {
        self.events.subscribers.remove(&subscriber);
    }
}

/// Runtime state of every quest objective the player has touched, keyed
/// by quest FormID then objective index.
///
/// The objectives sibling to [`QuestStageState`] anticipated in this
/// module's header. Papyrus's quest-stage *fragments* (the 69.5%
/// fragment population the M47.2 lowerer targets — see
/// [`docs/engine/m47-2-recognizer-scaling.md`]) overwhelmingly call
/// `SetObjectiveDisplayed` / `SetObjectiveCompleted` /
/// `SetObjectiveFailed`; lowering them needs a canonical store, and this
/// is it. Same lazy, quest-scoped shape as [`QuestStageState`]: only
/// objectives a fragment has actually touched get a map entry; untouched
/// objectives read their default (`ObjectiveStatus::default`).
///
/// No journal UI consumes this yet (same status as [`QuestStageAdvanced`]
/// at its introduction) — it ships so the fragment lowerer has a stable,
/// tested target, and stage/objective causality is observable.
#[derive(Debug, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct QuestObjectiveState {
    quests: HashMap<QuestFormId, HashMap<u16, ObjectiveStatus>>,
}

impl Resource for QuestObjectiveState {}

/// Per-objective display state. Papyrus exposes three independent
/// toggles; `completed` and `failed` are mutually exclusive in authored
/// content but stored independently (the runtime never enforces it — it
/// mirrors whatever the fragment set, matching Bethesda's store).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectiveStatus {
    /// `SetObjectiveDisplayed(idx, true/false)` — visible in the journal.
    pub displayed: bool,
    /// `SetObjectiveCompleted(idx, true/false)`.
    pub completed: bool,
    /// `SetObjectiveFailed(idx, true/false)`.
    pub failed: bool,
}

impl QuestObjectiveState {
    fn entry(&mut self, quest: QuestFormId, objective: u16) -> &mut ObjectiveStatus {
        self.quests
            .entry(quest)
            .or_default()
            .entry(objective)
            .or_default()
    }

    /// Papyrus `Quest.SetObjectiveDisplayed(idx, displayed)`.
    pub fn set_displayed(&mut self, quest: QuestFormId, objective: u16, displayed: bool) {
        self.entry(quest, objective).displayed = displayed;
    }

    /// Papyrus `Quest.SetObjectiveCompleted(idx, completed)`.
    pub fn set_completed(&mut self, quest: QuestFormId, objective: u16, completed: bool) {
        self.entry(quest, objective).completed = completed;
    }

    /// Papyrus `Quest.SetObjectiveFailed(idx, failed)`.
    pub fn set_failed(&mut self, quest: QuestFormId, objective: u16, failed: bool) {
        self.entry(quest, objective).failed = failed;
    }

    /// Papyrus `Quest.CompleteAllObjectives()` — marks every objective
    /// the quest has *displayed so far* completed. With the lazy store we
    /// only know objectives a fragment has touched; this completes those.
    /// (A future QUST-record objective table could complete the full
    /// authored set; the touched set is the faithful subset available
    /// without it — and matches what runtime state has actually shown.)
    pub fn complete_all(&mut self, quest: QuestFormId) {
        if let Some(objs) = self.quests.get_mut(&quest) {
            for status in objs.values_mut() {
                if status.displayed {
                    status.completed = true;
                }
            }
        }
    }

    /// Read an objective's status; default (all false) for untouched.
    pub fn get(&self, quest: QuestFormId, objective: u16) -> ObjectiveStatus {
        self.quests
            .get(&quest)
            .and_then(|objs| objs.get(&objective))
            .copied()
            .unwrap_or_default()
    }
}

/// One quest-stage advance, produced by a [`QuestStageState::set_stage`]-driven
/// system. Consumed by:
///
/// - The (future) stage-fragment dispatcher (M47.0).
/// - The (future) quest journal UI updater.
/// - Other scripts that subscribed via Papyrus's `RegisterFor*`
///   surface (which will lower to marker-component subscriptions
///   when M47.2 lands).
///
/// Plain data — not a [`Component`] itself. See [`QuestStageAdvancedBatch`]
/// for how a frame's advances reach the ECS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestStageAdvanced {
    pub quest: QuestFormId,
    pub previous_stage: u16,
    pub new_stage: u16,
}

/// Maximum number of quest transitions retained for independently scheduled
/// consumers. State remains authoritative when a slow consumer reports a gap.
pub const QUEST_EVENT_RETENTION: usize = 16_384;

/// Stable identity for one journal consumer. IDs are allocated under the same
/// `QuestStageState` write lock as cursor updates, so registration and polling
/// are race-free. Values 1 and 2 are reserved for built-in consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QuestEventSubscriberId(pub u64);

pub const SCENE_QUEST_EVENT_SUBSCRIBER: QuestEventSubscriberId = QuestEventSubscriberId(1);
pub const FRAGMENT_QUEST_EVENT_SUBSCRIBER: QuestEventSubscriberId = QuestEventSubscriberId(2);

/// One transition in commit order. Sequence numbers are monotonic within the
/// running process and let slower subsystems preserve order and deduplicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SequencedQuestStageAdvanced {
    pub sequence: u64,
    pub event: QuestStageAdvanced,
}

/// Result of polling one independent quest-event subscription.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestEventRead {
    pub events: Vec<SequencedQuestStageAdvanced>,
    /// Events pruned before this subscriber polled. A non-zero value requires
    /// a canonical-state resync before applying subsequent edge events.
    pub missed_events: u64,
    /// Cursor after this claim; useful for telemetry and checkpointing.
    pub next_sequence: u64,
}

#[derive(Debug)]
struct QuestEventRuntime {
    next_sequence: u64,
    next_subscriber_id: u64,
    events: VecDeque<SequencedQuestStageAdvanced>,
    subscribers: HashMap<QuestEventSubscriberId, u64>,
}

impl Default for QuestEventRuntime {
    fn default() -> Self {
        Self {
            next_sequence: 1,
            next_subscriber_id: 3,
            events: VecDeque::new(),
            subscribers: HashMap::new(),
        }
    }
}

impl QuestEventRuntime {
    fn oldest_sequence(&self) -> u64 {
        self.events
            .front()
            .map(|event| event.sequence)
            .unwrap_or(self.next_sequence)
    }

    fn push(&mut self, event: QuestStageAdvanced) {
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .expect("quest event sequence exhausted");
        if self.events.len() == QUEST_EVENT_RETENTION {
            self.events.pop_front();
        }
        self.events
            .push_back(SequencedQuestStageAdvanced { sequence, event });
    }

    fn subscribe(&mut self, replay_retained: bool) -> QuestEventSubscriberId {
        let subscriber = QuestEventSubscriberId(self.next_subscriber_id);
        self.next_subscriber_id = self
            .next_subscriber_id
            .checked_add(1)
            .expect("quest event subscriber id exhausted");
        let cursor = if replay_retained {
            self.oldest_sequence()
        } else {
            self.next_sequence
        };
        self.subscribers.insert(subscriber, cursor);
        subscriber
    }

    fn poll(&mut self, subscriber: QuestEventSubscriberId) -> QuestEventRead {
        let oldest = self.oldest_sequence();
        let requested = self.subscribers.get(&subscriber).copied().unwrap_or(oldest);
        let missed_events = oldest.saturating_sub(requested);
        let first = requested.max(oldest);
        let events = self
            .events
            .iter()
            .filter(|event| event.sequence >= first)
            .copied()
            .collect();
        let next_sequence = self.next_sequence;
        self.subscribers.insert(subscriber, next_sequence);
        QuestEventRead {
            events,
            missed_events,
            next_sequence,
        }
    }

    fn acknowledge(&mut self, subscriber: QuestEventSubscriberId) {
        self.subscribers.insert(subscriber, self.next_sequence);
    }
}

/// A frame's worth of [`QuestStageAdvanced`] events, attached once to a
/// designated "quest events" entity (set up at world init).
///
/// #1864 / SCR-D7-NEW-01 — `SparseSetStorage` allows exactly one component
/// instance per entity, overwriting in place on a repeat insert
/// (`crates/core/src/ecs/sparse_set.rs`). Both live producers
/// (`quest_advance_system`'s phase 3, `quest_fragment_dispatch_system`'s
/// cascade re-emission) can legitimately produce more than one advance in a
/// single system call — e.g. two independently-recognized quest-advance
/// REFRs firing in the same tick — and both write onto the same shared
/// sink entity (there is no per-source entity for global quest state).
/// Looping `insert()` once per event onto that one entity silently
/// collapsed every advance but the last. Batching every same-frame advance
/// into one `Vec`-wrapping component, inserted exactly once, fixes this
/// while keeping the existing per-frame marker + cleanup lifecycle
/// ([`crate::event_cleanup_system`]) unchanged.
#[derive(Debug, Clone, Default)]
pub struct QuestStageAdvancedBatch(pub Vec<QuestStageAdvanced>);

impl Component for QuestStageAdvancedBatch {
    type Storage = SparseSetStorage<Self>;
}

/// Parsed `Start Game Enabled` quests waiting for world bootstrap.
///
/// ESM records are installed before the player/event sink exists, so this
/// resource bridges static loading to the first live scheduler tick.
#[derive(Debug, Clone, Default)]
pub struct StartGameQuestRegistry {
    quests: HashMap<QuestFormId, Option<u16>>,
}

impl Resource for StartGameQuestRegistry {}

impl StartGameQuestRegistry {
    pub fn len(&self) -> usize {
        self.quests.len()
    }

    pub fn is_empty(&self) -> bool {
        self.quests.is_empty()
    }
}

/// Register quest-start storage/resources without disturbing already-loaded
/// definitions or live event batches.
pub fn register(world: &mut World) {
    world.register::<QuestStageAdvancedBatch>();
    if world.try_resource::<StartGameQuestRegistry>().is_none() {
        world.insert_resource(StartGameQuestRegistry::default());
    }
}

/// Refresh the static startup list from the merged plugin index. Runtime quest
/// state is left untouched, making repeated cell loads idempotent.
pub fn install_start_game_quests(
    world: &mut World,
    records: impl IntoIterator<Item = QustRecord>,
) -> usize {
    if world.try_resource::<StartGameQuestRegistry>().is_none() {
        register(world);
    }

    let quests: HashMap<QuestFormId, Option<u16>> = records
        .into_iter()
        .filter(|record| record.quest_flags & QUEST_FLAG_START_GAME_ENABLED != 0)
        .map(|record| (QuestFormId(record.form_id), record.start_up_stage))
        .collect();
    let count = quests.len();
    world.resource_mut::<StartGameQuestRegistry>().quests = quests;
    count
}

/// Add one engine-defined new-game root that is not represented by the
/// authored Start Game Enabled bit. Bethesda hardcodes a small number of
/// bootstrap quests (notably Skyrim's MQ101); keeping this explicit prevents
/// format metadata from being falsified just to enter the normal startup path.
pub fn install_engine_start_quest(
    world: &mut World,
    quest: QuestFormId,
    start_up_stage: Option<u16>,
) -> bool {
    if world.try_resource::<StartGameQuestRegistry>().is_none() {
        register(world);
    }
    world
        .resource_mut::<StartGameQuestRegistry>()
        .quests
        .insert(quest, start_up_stage)
        != Some(start_up_stage)
}

/// Start every installed `Start Game Enabled` quest exactly once and append
/// the transitions to the shared quest-event sink. This runs immediately
/// before scene playback, so `Begin On Quest Start` scenes see the event in
/// the same frame.
pub fn quest_startup_system(world: &World, _dt: f32) {
    let Some(registry) = world.try_resource::<StartGameQuestRegistry>() else {
        return;
    };
    if registry.quests.is_empty() {
        return;
    }
    let definitions: Vec<(QuestFormId, Option<u16>)> = registry
        .quests
        .iter()
        .map(|(quest, stage)| (*quest, *stage))
        .collect();
    drop(registry);

    let Some(player) = world.try_resource::<PlayerEntity>().map(|player| player.0) else {
        return;
    };

    let Some(mut stages) = world.try_resource_mut::<QuestStageState>() else {
        return;
    };
    let advances: Vec<QuestStageAdvanced> = definitions
        .into_iter()
        .filter_map(|(quest, stage)| stages.start_quest(quest, stage))
        .collect();
    drop(stages);
    if advances.is_empty() {
        return;
    }
    log::info!(
        target: "scripting::quest_startup",
        "Started {} bootstrap quest(s)",
        advances.len()
    );

    let Some(mut batches) = world.query_mut::<QuestStageAdvancedBatch>() else {
        return;
    };
    if let Some(batch) = batches.get_mut(player) {
        batch.0.extend(advances);
    } else {
        batches.insert(player, QuestStageAdvancedBatch(advances));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_state_returns_zero_for_unknown_quest() {
        let s = QuestStageState::default();
        // No interaction yet — GetStage / GetStageDone return
        // defaults (0 / false respectively) without a map entry.
        assert_eq!(s.get_stage(QuestFormId(0xDEADBEEF)), 0);
        assert!(!s.get_stage_done(QuestFormId(0xDEADBEEF), 10));
    }

    #[test]
    fn start_quest_at_zero_is_distinct_from_never_started() {
        let mut state = QuestStageState::default();
        let quest = QuestFormId(0x1000);

        let event = state.start_quest(quest, None).expect("first start");
        assert_eq!(event.quest, quest);
        assert_eq!(event.previous_stage, 0);
        assert_eq!(event.new_stage, 0);
        assert!(state.is_started(quest));
        assert_eq!(state.get_stage(quest), 0);
        assert!(!state.get_stage_done(quest, 0));
        assert!(state.start_quest(quest, Some(10)).is_none());

        state.reset(quest);
        assert!(!state.is_started(quest));
        assert!(state.start_quest(quest, Some(10)).is_some());
        assert_eq!(state.get_stage(quest), 10);
        assert!(state.get_stage_done(quest, 10));
    }

    #[test]
    fn independent_subscribers_consume_at_different_cadences() {
        let mut state = QuestStageState::default();
        let fast = state.subscribe_to_quest_events();
        let slow = state.subscribe_to_quest_events();
        let quest = QuestFormId(0x1000);

        state.set_stage(quest, 10);
        let first_fast = state.poll_quest_events(fast);
        assert_eq!(first_fast.events.len(), 1);
        assert_eq!(first_fast.events[0].event.new_stage, 10);

        state.set_stage(quest, 20);
        let second_fast = state.poll_quest_events(fast);
        assert_eq!(second_fast.events.len(), 1);
        assert_eq!(second_fast.events[0].event.new_stage, 20);

        let slow_read = state.poll_quest_events(slow);
        assert_eq!(
            slow_read
                .events
                .iter()
                .map(|event| event.event.new_stage)
                .collect::<Vec<_>>(),
            vec![10, 20]
        );
        assert_eq!(slow_read.missed_events, 0);
        assert!(state.poll_quest_events(fast).events.is_empty());
        assert!(state.poll_quest_events(slow).events.is_empty());
    }

    #[test]
    fn concurrent_start_has_one_winner_and_one_committed_event() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Barrier;

        const WORKERS: usize = 16;
        let mut world = World::new();
        world.insert_resource(QuestStageState::default());
        let subscriber = world
            .resource_mut::<QuestStageState>()
            .subscribe_to_quest_events();
        let barrier = Barrier::new(WORKERS);
        let winners = AtomicUsize::new(0);

        std::thread::scope(|scope| {
            for _ in 0..WORKERS {
                scope.spawn(|| {
                    barrier.wait();
                    if world
                        .resource_mut::<QuestStageState>()
                        .start_quest(QuestFormId(0x1000), Some(10))
                        .is_some()
                    {
                        winners.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }
        });

        assert_eq!(winners.load(Ordering::Relaxed), 1);
        let mut state = world.resource_mut::<QuestStageState>();
        assert_eq!(state.get_stage(QuestFormId(0x1000)), 10);
        let read = state.poll_quest_events(subscriber);
        assert_eq!(read.events.len(), 1);
        assert_eq!(read.events[0].event.quest, QuestFormId(0x1000));
    }

    #[test]
    fn concurrent_distinct_transitions_are_ordered_and_never_lost() {
        use std::collections::HashSet;
        use std::sync::Barrier;

        const WORKERS: usize = 32;
        let mut world = World::new();
        world.insert_resource(QuestStageState::default());
        let subscriber = world
            .resource_mut::<QuestStageState>()
            .subscribe_to_quest_events();
        let barrier = Barrier::new(WORKERS);

        std::thread::scope(|scope| {
            for index in 0..WORKERS {
                let world = &world;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    world
                        .resource_mut::<QuestStageState>()
                        .set_stage(QuestFormId(0x1000 + index as u32), 10);
                });
            }
        });

        let read = world
            .resource_mut::<QuestStageState>()
            .poll_quest_events(subscriber);
        assert_eq!(read.events.len(), WORKERS);
        assert!(read
            .events
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence));
        let quests: HashSet<_> = read.events.iter().map(|event| event.event.quest).collect();
        assert_eq!(quests.len(), WORKERS);
        assert_eq!(read.missed_events, 0);
    }

    #[test]
    fn lagging_subscriber_gets_explicit_retention_gap() {
        let mut state = QuestStageState::default();
        let subscriber = state.subscribe_to_quest_events();
        const OVERFLOW: usize = 5;
        for stage in 0..(QUEST_EVENT_RETENTION + OVERFLOW) {
            state.set_stage(QuestFormId(0x1000), stage as u16);
        }

        let read = state.poll_quest_events(subscriber);
        assert_eq!(read.events.len(), QUEST_EVENT_RETENTION);
        assert_eq!(read.missed_events, OVERFLOW as u64);
        assert_eq!(read.events[0].sequence, OVERFLOW as u64 + 1);
    }

    #[test]
    fn startup_system_filters_flags_emits_once_and_preserves_existing_batch() {
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        world.insert_resource(PlayerEntity(player));
        world.insert_resource(QuestStageState::default());
        world.insert(
            player,
            QuestStageAdvancedBatch(vec![QuestStageAdvanced {
                quest: QuestFormId(0xDEAD),
                previous_stage: 5,
                new_stage: 10,
            }]),
        );

        assert_eq!(
            install_start_game_quests(
                &mut world,
                [
                    QustRecord {
                        form_id: 0x1000,
                        quest_flags: QUEST_FLAG_START_GAME_ENABLED,
                        start_up_stage: Some(10),
                        ..Default::default()
                    },
                    QustRecord {
                        form_id: 0x2000,
                        quest_flags: 0,
                        start_up_stage: Some(20),
                        ..Default::default()
                    },
                ],
            ),
            1
        );

        quest_startup_system(&world, 0.0);
        let state = world.resource::<QuestStageState>();
        assert!(state.is_started(QuestFormId(0x1000)));
        assert_eq!(state.get_stage(QuestFormId(0x1000)), 10);
        assert!(!state.is_started(QuestFormId(0x2000)));
        drop(state);
        let batch = world
            .get::<QuestStageAdvancedBatch>(player)
            .expect("quest event batch");
        assert_eq!(batch.0.len(), 2, "startup must append to live producers");
        assert_eq!(batch.0[1].quest, QuestFormId(0x1000));
        drop(batch);

        quest_startup_system(&world, 0.0);
        assert_eq!(
            world
                .get::<QuestStageAdvancedBatch>(player)
                .expect("quest event batch")
                .0
                .len(),
            2,
            "already-started quests must not emit twice"
        );
    }

    #[test]
    fn set_stage_updates_current_and_adds_to_done_set() {
        let mut s = QuestStageState::default();
        let q = QuestFormId(0x1000);
        let prev = s.set_stage(q, 20);
        assert_eq!(prev, 0, "first SetStage from default returns prev=0");
        assert_eq!(s.get_stage(q), 20);
        assert!(s.get_stage_done(q, 20));
        // Earlier stages weren't touched, still false.
        assert!(!s.get_stage_done(q, 10));
    }

    #[test]
    fn set_stage_returns_previous_current_stage() {
        let mut s = QuestStageState::default();
        let q = QuestFormId(0x1000);
        s.set_stage(q, 10);
        let prev = s.set_stage(q, 20);
        assert_eq!(prev, 10);
    }

    #[test]
    fn get_stage_done_retains_history_across_advances() {
        // The load-bearing behaviour from the DA10 translation: a
        // quest at current_stage 40 still reports GetStageDone(37)
        // == true. The Papyrus runtime carries the full history;
        // ours has to match.
        let mut s = QuestStageState::default();
        let q = QuestFormId(0x1000);
        s.set_stage(q, 10);
        s.set_stage(q, 37);
        s.set_stage(q, 40);
        assert_eq!(s.get_stage(q), 40);
        assert!(s.get_stage_done(q, 10));
        assert!(s.get_stage_done(q, 37));
        assert!(s.get_stage_done(q, 40));
        // Stages the quest never visited stay false.
        assert!(!s.get_stage_done(q, 20));
        assert!(!s.get_stage_done(q, 50));
    }

    #[test]
    fn set_stage_on_already_done_stage_remains_idempotent() {
        // Re-setting the current stage is a no-op observable: the
        // set still happens (matches Papyrus's unconditional write
        // semantic), but GetStageDone was already true and stays
        // true. previous_stage equals stage (the read-modify-write
        // is atomic from the caller's perspective).
        let mut s = QuestStageState::default();
        let q = QuestFormId(0x1000);
        s.set_stage(q, 40);
        let prev = s.set_stage(q, 40);
        assert_eq!(prev, 40);
        assert_eq!(s.get_stage(q), 40);
        assert!(s.get_stage_done(q, 40));
    }

    #[test]
    fn set_stage_backwards_is_allowed_but_updates_current() {
        // Papyrus permits backwards SetStage (e.g., quest restart
        // sequences advancing to a stage that resets later progress).
        // current_stage tracks the most recent write, NOT the
        // numerical maximum; stages_done accumulates regardless.
        let mut s = QuestStageState::default();
        let q = QuestFormId(0x1000);
        s.set_stage(q, 50);
        s.set_stage(q, 10);
        assert_eq!(s.get_stage(q), 10, "current_stage is most-recent-write");
        assert!(s.get_stage_done(q, 50), "stages_done retains everything");
        assert!(s.get_stage_done(q, 10));
    }

    #[test]
    fn reset_clears_all_state_for_quest() {
        let mut s = QuestStageState::default();
        let q = QuestFormId(0x1000);
        s.set_stage(q, 10);
        s.set_stage(q, 20);
        s.reset(q);
        assert_eq!(s.get_stage(q), 0);
        assert!(!s.get_stage_done(q, 10));
        assert!(!s.get_stage_done(q, 20));
    }

    #[test]
    fn reset_leaves_other_quests_intact() {
        let mut s = QuestStageState::default();
        let q1 = QuestFormId(0x1000);
        let q2 = QuestFormId(0x2000);
        s.set_stage(q1, 10);
        s.set_stage(q2, 30);
        s.reset(q1);
        assert_eq!(s.get_stage(q1), 0);
        assert_eq!(s.get_stage(q2), 30, "reset must be quest-scoped");
        assert!(s.get_stage_done(q2, 30));
    }
}
