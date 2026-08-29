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
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::{
    QuestObjective, QuestObjectiveTarget, QuestObjectiveTargetKind, QuestStageLogEntry, QustRecord,
    QUEST_FLAG_ACTIVE, QUEST_FLAG_ALLOW_REPEATED_STAGES, QUEST_FLAG_COMPLETED, QUEST_FLAG_FAILED,
    QUEST_FLAG_START_GAME_ENABLED, QUEST_LOG_FLAG_COMPLETE_QUEST, QUEST_LOG_FLAG_FAIL_QUEST,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

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
#[derive(Debug, Clone)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct QuestStageData {
    pub current_stage: u16,
    pub stages_done: HashSet<u16>,
    /// Running/stopped/completed state exposed by the Papyrus Quest API.
    pub status: QuestStatus,
    /// Whether the quest is selected in the player's active journal.
    pub active: bool,
}

impl Default for QuestStageData {
    fn default() -> Self {
        Self {
            current_stage: 0,
            stages_done: HashSet::new(),
            status: QuestStatus::Running,
            active: false,
        }
    }
}

/// Lifecycle state for a quest that has entered the runtime map. Absence from
/// [`QuestStageState`] remains the canonical "not started" representation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum QuestStatus {
    #[default]
    Running,
    Stopped,
    Completed,
    Failed,
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
        if let Some(data) = self.quests.get_mut(&quest) {
            if data.status != QuestStatus::Stopped {
                return None;
            }
            data.status = QuestStatus::Running;
            let previous_stage = data.current_stage;
            if let Some(stage) = start_up_stage {
                data.current_stage = stage;
                data.stages_done.insert(stage);
            }
            let event = QuestStageAdvanced {
                quest,
                previous_stage,
                new_stage: data.current_stage,
            };
            self.events.push(event);
            return Some(event);
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

    pub fn is_running(&self, quest: QuestFormId) -> bool {
        self.quests
            .get(&quest)
            .is_some_and(|data| data.status == QuestStatus::Running)
    }

    pub fn is_completed(&self, quest: QuestFormId) -> bool {
        self.quests
            .get(&quest)
            .is_some_and(|data| data.status == QuestStatus::Completed)
    }

    pub fn is_failed(&self, quest: QuestFormId) -> bool {
        self.quests
            .get(&quest)
            .is_some_and(|data| data.status == QuestStatus::Failed)
    }

    pub fn is_active(&self, quest: QuestFormId) -> bool {
        self.quests.get(&quest).is_some_and(|data| data.active)
    }

    /// Papyrus `Quest.Stop()`. Stopping preserves stage/history so `Start()`
    /// can resume it; completed quests remain completed until `Reset()`.
    pub fn stop(&mut self, quest: QuestFormId) -> bool {
        let Some(data) = self.quests.get_mut(&quest) else {
            return false;
        };
        if matches!(data.status, QuestStatus::Completed | QuestStatus::Failed) {
            return false;
        }
        let changed = data.status != QuestStatus::Stopped;
        data.status = QuestStatus::Stopped;
        data.active = false;
        changed
    }

    /// Papyrus `Quest.CompleteQuest()`.
    pub fn complete(&mut self, quest: QuestFormId) -> bool {
        let Some(data) = self.quests.get_mut(&quest) else {
            return false;
        };
        let changed = data.status != QuestStatus::Completed;
        data.status = QuestStatus::Completed;
        data.active = false;
        changed
    }

    /// Apply a terminal QSDT `Fail Quest` log entry.
    pub fn fail(&mut self, quest: QuestFormId) -> bool {
        let Some(data) = self.quests.get_mut(&quest) else {
            return false;
        };
        let changed = data.status != QuestStatus::Failed;
        data.status = QuestStatus::Failed;
        data.active = false;
        changed
    }

    /// Papyrus `Quest.SetActive(active)` journal selection.
    pub fn set_active(&mut self, quest: QuestFormId, active: bool) -> bool {
        let Some(data) = self.quests.get_mut(&quest) else {
            return false;
        };
        let changed = data.active != active;
        data.active = active;
        changed
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

    /// Read one quest's complete tracked state. `None` is the canonical
    /// not-started state and remains distinct from a running stage-zero quest.
    pub fn state(&self, quest: QuestFormId) -> Option<&QuestStageData> {
        self.quests.get(&quest)
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
    quests: HashMap<QuestFormId, HashMap<i32, ObjectiveStatus>>,
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
    fn entry(&mut self, quest: QuestFormId, objective: i32) -> &mut ObjectiveStatus {
        self.quests
            .entry(quest)
            .or_default()
            .entry(objective)
            .or_default()
    }

    /// Papyrus `Quest.SetObjectiveDisplayed(idx, displayed)`.
    pub fn set_displayed(&mut self, quest: QuestFormId, objective: i32, displayed: bool) {
        self.entry(quest, objective).displayed = displayed;
    }

    /// Papyrus `Quest.SetObjectiveCompleted(idx, completed)`.
    pub fn set_completed(&mut self, quest: QuestFormId, objective: i32, completed: bool) {
        self.entry(quest, objective).completed = completed;
    }

    /// Papyrus `Quest.SetObjectiveFailed(idx, failed)`.
    pub fn set_failed(&mut self, quest: QuestFormId, objective: i32, failed: bool) {
        self.entry(quest, objective).failed = failed;
    }

    /// Compatibility fallback for `Quest.CompleteAllObjectives()` when no
    /// installed QUST definition is available: complete displayed objectives
    /// already represented in the lazy runtime store. Normal fragment dispatch
    /// uses the full authored objective table in `QuestDefinitionRegistry`.
    pub fn complete_all(&mut self, quest: QuestFormId) {
        if let Some(objs) = self.quests.get_mut(&quest) {
            for status in objs.values_mut() {
                if status.displayed {
                    status.completed = true;
                }
            }
        }
    }

    /// Fail every objective already represented in runtime state. Used as a
    /// compatibility fallback when a caller has no installed QUST definition.
    pub fn fail_all(&mut self, quest: QuestFormId) {
        if let Some(objs) = self.quests.get_mut(&quest) {
            for status in objs.values_mut() {
                status.failed = true;
            }
        }
    }

    /// Complete every authored objective, creating lazy entries as needed.
    pub fn complete_all_authored(
        &mut self,
        quest: QuestFormId,
        objectives: impl IntoIterator<Item = i32>,
    ) {
        for objective in objectives {
            self.entry(quest, objective).completed = true;
        }
    }

    /// Papyrus `Quest.FailAllObjectives()`.
    pub fn fail_all_authored(
        &mut self,
        quest: QuestFormId,
        objectives: impl IntoIterator<Item = i32>,
    ) {
        for objective in objectives {
            self.entry(quest, objective).failed = true;
        }
    }

    pub fn reset(&mut self, quest: QuestFormId) {
        self.quests.remove(&quest);
    }

    /// Read an objective's status; default (all false) for untouched.
    pub fn get(&self, quest: QuestFormId, objective: i32) -> ObjectiveStatus {
        self.quests
            .get(&quest)
            .and_then(|objs| objs.get(&objective))
            .copied()
            .unwrap_or_default()
    }

    /// Iterate every runtime objective touched for one quest. Authored but
    /// untouched objectives are intentionally absent and should be joined with
    /// [`QuestDefinitionRegistry::objectives`] by diagnostic/UI consumers.
    pub fn iter_quest(
        &self,
        quest: QuestFormId,
    ) -> impl Iterator<Item = (i32, ObjectiveStatus)> + '_ {
        self.quests
            .get(&quest)
            .into_iter()
            .flat_map(|objectives| objectives.iter())
            .map(|(&index, &status)| (index, status))
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
pub const TERMINAL_QUEST_EVENT_SUBSCRIBER: QuestEventSubscriberId = QuestEventSubscriberId(3);

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
            next_subscriber_id: 4,
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

/// Append a producer's advances to the shared same-frame
/// [`QuestStageAdvancedBatch`] sink, merging with whatever a same-frame
/// producer already put there.
///
/// #3277 (SCR-D6-2026-08-24-01) — this is the invariant #1864's fix
/// documented but left as a convention every writer had to re-implement:
/// the sink is one `SparseSetStorage` slot on one shared entity, so a bare
/// `insert()` *replaces* it and silently drops any earlier producer's events
/// for that frame. Five of six writers open-coded the `get_mut`-then-`extend`
/// dance correctly; `quest_fragment_dispatch_system`'s tail did not, which
/// was harmless only while it happened to be the last writer in the schedule.
/// Two producers were then scheduled ahead of it
/// (`quest_alias_readiness_stage_system`, `scene_fragment_dispatch_system`)
/// and the omission became live data loss waiting for a consumer.
///
/// Routing every writer through here makes the invariant structural rather
/// than conventional — a new producer cannot get it wrong by not knowing
/// about it.
///
/// Deliberately has **no emptiness guard**: every call site already returns
/// early on an empty batch, and those early returns skip other work too
/// (logging, resource lookups). A second check here would be dead code that
/// reads as if it were load-bearing.
pub fn push_quest_stage_advances(
    world: &World,
    player: EntityId,
    advances: Vec<QuestStageAdvanced>,
) {
    let Some(mut batches) = world.query_mut::<QuestStageAdvancedBatch>() else {
        return;
    };
    if let Some(batch) = batches.get_mut(player) {
        batch.0.extend(advances);
    } else {
        batches.insert(player, QuestStageAdvancedBatch(advances));
    }
}

/// Parsed `Start Game Enabled` quests waiting for world bootstrap.
///
/// ESM records are installed before the player/event sink exists, so this
/// resource bridges static loading to the first live scheduler tick.
#[derive(Debug, Clone, Default)]
pub struct StartGameQuestRegistry {
    quests: HashMap<QuestFormId, Option<u16>>,
    initial_flags: HashMap<QuestFormId, u16>,
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

/// Engine-supplied equivalent of a quest alias script that advances once a
/// required reference set is resident. This preserves the data-driven gate
/// while alias-attached Papyrus method dispatch is still being generalized.
#[derive(Debug, Clone)]
pub struct QuestAliasReadinessGate {
    pub quest: QuestFormId,
    pub required_aliases: Vec<i32>,
    pub target_stage: u16,
    pub only_below_stage: u16,
}

#[derive(Debug, Clone, Default)]
pub struct QuestAliasReadinessGateRegistry(Vec<QuestAliasReadinessGate>);

impl Resource for QuestAliasReadinessGateRegistry {}

/// #2659 (SCR-D6-NEW11-02) — `definitions` is `Arc`-wrapped so
/// `#[derive(Clone)]` is an O(1) refcount bump, not an O(quest count) deep
/// copy. `DeferredFragmentEffects::new` (`fragment.rs`) clones this
/// registry unconditionally, once per `quest_fragment_dispatch_system`
/// call — measured up to the entire frame budget on a heavily-modded
/// (5,000-quest) load order when it was a deep clone. Sound because the
/// registry's only writers ([`install_start_game_quests`],
/// [`install_engine_start_quest`]) take `world: &mut World` — an
/// exclusive borrow that can't coexist with any system's outstanding
/// `Arc` clone from a prior read — so every write either uniquely owns
/// the `Arc` (in-place mutation via `Arc::make_mut`, no copy) or is the
/// registry's very first population.
#[derive(Debug, Clone, Default)]
pub struct QuestDefinitionRegistry {
    definitions: Arc<HashMap<QuestFormId, QuestDefinition>>,
}

impl Resource for QuestDefinitionRegistry {}

#[derive(Debug, Clone, Default)]
struct QuestDefinition {
    editor_id: String,
    full_name: String,
    flags: u16,
    start_up_stage: Option<u16>,
    shut_down_stage: Option<u16>,
    stages: Vec<u16>,
    objectives: Vec<i32>,
    objective_records: HashMap<i32, QuestObjective>,
    stage_logs: HashMap<u16, Vec<QuestStageLogEntry>>,
    targets: Vec<QuestObjectiveTarget>,
}

impl QuestDefinitionRegistry {
    pub fn contains(&self, quest: QuestFormId) -> bool {
        self.definitions.contains_key(&quest)
    }

    pub fn start_up_stage(&self, quest: QuestFormId) -> Option<u16> {
        self.definitions
            .get(&quest)
            .and_then(|definition| definition.start_up_stage)
    }

    pub fn editor_id(&self, quest: QuestFormId) -> Option<&str> {
        self.definitions
            .get(&quest)
            .map(|definition| definition.editor_id.as_str())
    }

    pub fn full_name(&self, quest: QuestFormId) -> Option<&str> {
        self.definitions
            .get(&quest)
            .map(|definition| definition.full_name.as_str())
    }

    pub fn flags(&self, quest: QuestFormId) -> Option<u16> {
        self.definitions
            .get(&quest)
            .map(|definition| definition.flags)
    }

    pub fn stages(&self, quest: QuestFormId) -> &[u16] {
        self.definitions
            .get(&quest)
            .map_or(&[], |definition| definition.stages.as_slice())
    }

    pub fn shut_down_stage(&self, quest: QuestFormId) -> Option<u16> {
        self.definitions
            .get(&quest)
            .and_then(|definition| definition.shut_down_stage)
    }

    pub fn allows_repeated_stages(&self, quest: QuestFormId) -> bool {
        self.definitions
            .get(&quest)
            .is_some_and(|definition| definition.flags & QUEST_FLAG_ALLOW_REPEATED_STAGES != 0)
    }

    pub fn objectives(&self, quest: QuestFormId) -> &[i32] {
        self.definitions
            .get(&quest)
            .map_or(&[], |definition| definition.objectives.as_slice())
    }

    pub fn objective(&self, quest: QuestFormId, index: i32) -> Option<&QuestObjective> {
        self.definitions
            .get(&quest)
            .and_then(|definition| definition.objective_records.get(&index))
    }

    pub fn stage_log_entries(&self, quest: QuestFormId, stage: u16) -> &[QuestStageLogEntry] {
        self.definitions
            .get(&quest)
            .and_then(|definition| definition.stage_logs.get(&stage))
            .map_or(&[], Vec::as_slice)
    }

    pub fn targets(&self, quest: QuestFormId) -> &[QuestObjectiveTarget] {
        self.definitions
            .get(&quest)
            .map_or(&[], |definition| definition.targets.as_slice())
    }
}

fn resolve_target_entities(
    world: &World,
    quest: QuestFormId,
    targets: Vec<QuestObjectiveTarget>,
) -> Vec<EntityId> {
    let mut seen = HashSet::new();
    targets
        .into_iter()
        .filter_map(|target| {
            let entity = match target.target {
                QuestObjectiveTargetKind::Reference(form_id) => {
                    crate::condition::resolve_entity_by_global_form_id(world, form_id)
                }
                QuestObjectiveTargetKind::Alias(alias_id) => world
                    .try_resource::<crate::scene::SceneActorBindings>()
                    .and_then(|bindings| bindings.resolve(quest, alias_id)),
            }?;
            let context = crate::condition::ConditionContext::for_subject(entity).with_quest(quest);
            if seen.contains(&entity)
                || !crate::condition::evaluate(&target.conditions, world, &context)
            {
                return None;
            }
            seen.insert(entity);
            Some(entity)
        })
        .collect()
}

/// Resolve one authored objective's currently-live reference/alias targets in
/// authoring order. Target CTDAs run against the resolved entity with the
/// owning quest available for `RunOn::QuestAlias` conditions.
pub fn resolve_quest_objective_targets(
    world: &World,
    quest: QuestFormId,
    objective_index: i32,
) -> Vec<EntityId> {
    let Some(objective) = world
        .try_resource::<QuestDefinitionRegistry>()
        .and_then(|definitions| definitions.objective(quest, objective_index).cloned())
    else {
        return Vec::new();
    };

    resolve_target_entities(world, quest, objective.targets)
}

/// Resolve quest-level reference targets authored after the alias table.
pub fn resolve_quest_targets(world: &World, quest: QuestFormId) -> Vec<EntityId> {
    let targets = world
        .try_resource::<QuestDefinitionRegistry>()
        .map(|definitions| definitions.targets(quest).to_vec())
        .unwrap_or_default();
    resolve_target_entities(world, quest, targets)
}

/// Register quest-start storage/resources without disturbing already-loaded
/// definitions or live event batches.
pub fn register(world: &mut World) {
    world.register::<QuestStageAdvancedBatch>();
    if world.try_resource::<StartGameQuestRegistry>().is_none() {
        world.insert_resource(StartGameQuestRegistry::default());
    }
    if world.try_resource::<QuestDefinitionRegistry>().is_none() {
        world.insert_resource(QuestDefinitionRegistry::default());
    }
    if world
        .try_resource::<QuestAliasReadinessGateRegistry>()
        .is_none()
    {
        world.insert_resource(QuestAliasReadinessGateRegistry::default());
    }
}

pub fn install_quest_alias_readiness_gate(
    world: &mut World,
    gate: QuestAliasReadinessGate,
) -> bool {
    if world
        .try_resource::<QuestAliasReadinessGateRegistry>()
        .is_none()
    {
        register(world);
    }
    let mut registry = world.resource_mut::<QuestAliasReadinessGateRegistry>();
    if let Some(existing) = registry.0.iter_mut().find(|item| item.quest == gate.quest) {
        let changed = existing.required_aliases != gate.required_aliases
            || existing.target_stage != gate.target_stage
            || existing.only_below_stage != gate.only_below_stage;
        *existing = gate;
        changed
    } else {
        registry.0.push(gate);
        true
    }
}

/// Advance configured engine-root quests once every required alias resolves.
/// Runs after the scheduled alias refresh and before scene playback, matching
/// the same-frame behavior of Skyrim's `RegisterStartingCellLoad` callback.
pub fn quest_alias_readiness_stage_system(world: &World, _dt: f32) {
    let Some(registry) = world.try_resource::<QuestAliasReadinessGateRegistry>() else {
        return;
    };
    let Some(bindings) = world.try_resource::<crate::scene::SceneActorBindings>() else {
        return;
    };
    let ready: Vec<QuestAliasReadinessGate> = registry
        .0
        .iter()
        .filter(|gate| {
            gate.required_aliases
                .iter()
                .all(|alias| bindings.resolve(gate.quest, *alias).is_some())
        })
        .cloned()
        .collect();
    drop(bindings);
    drop(registry);
    if ready.is_empty() {
        return;
    }

    let mut emitted = Vec::new();
    {
        let Some(mut stages) = world.try_resource_mut::<QuestStageState>() else {
            return;
        };
        for gate in ready {
            if !stages.is_running(gate.quest)
                || stages.get_stage(gate.quest) >= gate.only_below_stage
                || stages.get_stage_done(gate.quest, gate.target_stage)
            {
                continue;
            }
            let previous_stage = stages.set_stage(gate.quest, gate.target_stage);
            emitted.push(QuestStageAdvanced {
                quest: gate.quest,
                previous_stage,
                new_stage: gate.target_stage,
            });
        }
    }
    if emitted.is_empty() {
        return;
    }
    let Some(player) = world.try_resource::<PlayerEntity>().map(|player| player.0) else {
        return;
    };
    push_quest_stage_advances(world, player, emitted);
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

    let records: Vec<QustRecord> = records.into_iter().collect();
    let quests: HashMap<QuestFormId, Option<u16>> = records
        .iter()
        .filter(|record| record.quest_flags & QUEST_FLAG_START_GAME_ENABLED != 0)
        .map(|record| (QuestFormId(record.form_id), record.start_up_stage))
        .collect();
    let initial_flags: HashMap<QuestFormId, u16> = records
        .iter()
        .filter(|record| record.quest_flags & QUEST_FLAG_START_GAME_ENABLED != 0)
        .map(|record| {
            (
                QuestFormId(record.form_id),
                if record.skyrim_plus {
                    record.quest_flags
                } else {
                    record.quest_flags & QUEST_FLAG_START_GAME_ENABLED
                },
            )
        })
        .collect();
    let definitions = records
        .into_iter()
        .map(|record| {
            (
                QuestFormId(record.form_id),
                QuestDefinition {
                    editor_id: record.editor_id,
                    full_name: record.full_name,
                    flags: record.quest_flags,
                    start_up_stage: record.start_up_stage,
                    shut_down_stage: record.shut_down_stage,
                    stages: record.stages.iter().map(|stage| stage.index).collect(),
                    objectives: record
                        .objectives
                        .iter()
                        .map(|objective| objective.index)
                        .collect(),
                    objective_records: record
                        .objectives
                        .into_iter()
                        .map(|objective| (objective.index, objective))
                        .collect(),
                    stage_logs: record
                        .stages
                        .into_iter()
                        .filter(|stage| !stage.log_entries.is_empty())
                        .map(|stage| (stage.index, stage.log_entries))
                        .collect(),
                    targets: record.targets,
                },
            )
        })
        .collect();
    let count = quests.len();
    world.resource_mut::<StartGameQuestRegistry>().quests = quests;
    world.resource_mut::<StartGameQuestRegistry>().initial_flags = initial_flags;
    world.resource_mut::<QuestDefinitionRegistry>().definitions = Arc::new(definitions);
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
    let changed = world
        .resource_mut::<StartGameQuestRegistry>()
        .quests
        .insert(quest, start_up_stage)
        != Some(start_up_stage);
    Arc::make_mut(&mut world.resource_mut::<QuestDefinitionRegistry>().definitions)
        .entry(quest)
        .or_insert_with(|| QuestDefinition {
            editor_id: String::new(),
            full_name: String::new(),
            flags: 0,
            start_up_stage,
            shut_down_stage: None,
            stages: start_up_stage.into_iter().collect(),
            objectives: Vec::new(),
            objective_records: HashMap::new(),
            stage_logs: HashMap::new(),
            targets: Vec::new(),
        });
    changed
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
    let mut definitions: Vec<(QuestFormId, Option<u16>, u16)> = registry
        .quests
        .iter()
        .map(|(quest, stage)| {
            (
                *quest,
                *stage,
                registry
                    .initial_flags
                    .get(quest)
                    .copied()
                    .unwrap_or_default(),
            )
        })
        .collect();
    definitions.sort_by_key(|(quest, _, _)| quest.0);
    drop(registry);

    let Some(player) = world.try_resource::<PlayerEntity>().map(|player| player.0) else {
        return;
    };

    let Some(mut stages) = world.try_resource_mut::<QuestStageState>() else {
        return;
    };
    let advances: Vec<QuestStageAdvanced> = definitions
        .into_iter()
        .filter_map(|(quest, stage, flags)| {
            // Bootstrap is a one-shot transition. An explicit Papyrus
            // Stop() must not make a Start Game Enabled quest restart on the
            // next scheduler tick; only an explicit Start() resumes it.
            if stages.is_started(quest) {
                return None;
            }
            let event = stages.start_quest(quest, stage)?;
            if flags & QUEST_FLAG_ACTIVE != 0 {
                stages.set_active(quest, true);
            }
            if flags & QUEST_FLAG_FAILED != 0 {
                stages.fail(quest);
            } else if flags & QUEST_FLAG_COMPLETED != 0 {
                stages.complete(quest);
            }
            Some(event)
        })
        .collect();
    drop(stages);
    if advances.is_empty() {
        return;
    }
    crate::scene::mark_scene_actor_bindings_dirty(world);
    log::info!(
        target: "scripting::quest_startup",
        "Started {} bootstrap quest(s)",
        advances.len()
    );

    push_quest_stage_advances(world, player, advances);
}

/// Apply terminal QUST stage-log semantics after stage fragments have run.
/// Every passing `QSDT` entry may complete/fail its owning quest and `NAM0`
/// starts the authored successor quest at that quest's Start Up Stage.
pub fn quest_terminal_stage_system(world: &World, _dt: f32) {
    let Some(mut stages) = world.try_resource_mut::<QuestStageState>() else {
        return;
    };
    let read = stages.poll_quest_events(TERMINAL_QUEST_EVENT_SUBSCRIBER);
    drop(stages);
    if read.missed_events > 0 {
        log::error!(
            target: "scripting::quest_terminal",
            "quest terminal subscriber missed {} retained transition(s)",
            read.missed_events
        );
    }
    if read.events.is_empty() {
        return;
    }

    let Some(definitions) = world.try_resource::<QuestDefinitionRegistry>() else {
        return;
    };
    let pending: Vec<(QuestStageAdvanced, Vec<QuestStageLogEntry>)> = read
        .events
        .into_iter()
        .filter_map(|sequenced| {
            let event = sequenced.event;
            let logs = definitions
                .stage_log_entries(event.quest, event.new_stage)
                .to_vec();
            (!logs.is_empty()).then_some((event, logs))
        })
        .collect();
    if pending.is_empty() {
        return;
    }

    let player = world.try_resource::<PlayerEntity>().map(|player| player.0);
    let mut complete = HashSet::new();
    let mut fail = HashSet::new();
    let mut next_quests = Vec::new();
    let mut seen_next_quests = HashSet::new();
    for (event, logs) in pending {
        for log_entry in logs {
            let passes = log_entry.conditions.is_empty()
                || player.is_some_and(|subject| {
                    crate::condition::evaluate(
                        &log_entry.conditions,
                        world,
                        &crate::condition::ConditionContext::for_subject(subject)
                            .with_quest(event.quest),
                    )
                });
            if !passes {
                continue;
            }
            if log_entry.flags & QUEST_LOG_FLAG_FAIL_QUEST != 0 {
                fail.insert(event.quest);
            }
            if log_entry.flags & QUEST_LOG_FLAG_COMPLETE_QUEST != 0 {
                complete.insert(event.quest);
            }
            if let Some(next_quest) = log_entry.next_quest {
                let next_quest = QuestFormId(next_quest);
                if seen_next_quests.insert(next_quest) {
                    next_quests.push(next_quest);
                }
            }
        }
    }

    let start_stages: Vec<(QuestFormId, Option<u16>)> = next_quests
        .into_iter()
        .map(|quest| (quest, definitions.start_up_stage(quest)))
        .collect();
    drop(definitions);

    let Some(mut stages) = world.try_resource_mut::<QuestStageState>() else {
        return;
    };
    let mut lifecycle_changed = false;
    // A passing Fail Quest entry wins if malformed content makes both
    // terminal flags true for the same stage transition.
    let mut fail: Vec<_> = fail.into_iter().collect();
    fail.sort_by_key(|quest| quest.0);
    let mut complete: Vec<_> = complete.into_iter().collect();
    complete.sort_by_key(|quest| quest.0);
    for quest in fail {
        complete.retain(|completed| *completed != quest);
        lifecycle_changed |= stages.fail(quest);
    }
    for quest in complete {
        lifecycle_changed |= stages.complete(quest);
    }
    for (quest, start_stage) in start_stages {
        lifecycle_changed |= stages.start_quest(quest, start_stage).is_some();
    }
    drop(stages);

    if lifecycle_changed {
        crate::scene::mark_scene_actor_bindings_dirty(world);
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
    fn quest_lifecycle_preserves_progress_until_reset() {
        let mut state = QuestStageState::default();
        let quest = QuestFormId(0x1000);
        state.start_quest(quest, Some(10));
        state.set_stage(quest, 20);
        state.set_active(quest, true);
        assert!(state.is_running(quest));
        assert!(state.is_active(quest));

        assert!(state.stop(quest));
        assert!(!state.is_running(quest));
        assert!(!state.is_active(quest));
        assert_eq!(state.get_stage(quest), 20);
        let resumed = state
            .start_quest(quest, Some(10))
            .expect("Start resumes Stop through the authored start stage");
        assert_eq!(resumed.previous_stage, 20);
        assert_eq!(resumed.new_stage, 10);
        assert_eq!(state.get_stage(quest), 10);
        assert!(state.is_running(quest));

        assert!(state.complete(quest));
        assert!(state.is_completed(quest));
        assert!(state.start_quest(quest, None).is_none());
        state.reset(quest);
        assert!(!state.is_started(quest));
    }

    #[test]
    fn objective_bulk_operations_cover_authored_rows_and_reset() {
        let mut objectives = QuestObjectiveState::default();
        let quest = QuestFormId(0x1000);
        objectives.complete_all_authored(quest, [10, 20]);
        objectives.fail_all_authored(quest, [30]);
        objectives.set_displayed(quest, 40, true);
        objectives.fail_all(quest);
        assert!(objectives.get(quest, 10).completed);
        assert!(objectives.get(quest, 20).completed);
        assert!(objectives.get(quest, 30).failed);
        assert!(objectives.get(quest, 40).failed);
        objectives.reset(quest);
        assert_eq!(objectives.get(quest, 10), ObjectiveStatus::default());
        assert_eq!(objectives.get(quest, 30), ObjectiveStatus::default());
    }

    #[test]
    fn terminal_stage_logs_fail_complete_and_start_the_next_quest() {
        let mut world = World::new();
        crate::register(&mut world);
        world.insert_resource(QuestStageState::default());
        let player = world.spawn();
        world.insert_resource(PlayerEntity(player));

        let completed = QuestFormId(0x1000);
        let failed = QuestFormId(0x2000);
        let next = QuestFormId(0x3000);
        install_start_game_quests(
            &mut world,
            [
                QustRecord {
                    form_id: completed.0,
                    stages: vec![byroredux_plugin::esm::records::QuestStage {
                        index: 100,
                        log_entries: vec![QuestStageLogEntry {
                            flags: QUEST_LOG_FLAG_COMPLETE_QUEST,
                            next_quest: Some(next.0),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                QustRecord {
                    form_id: failed.0,
                    stages: vec![byroredux_plugin::esm::records::QuestStage {
                        index: 50,
                        log_entries: vec![QuestStageLogEntry {
                            flags: QUEST_LOG_FLAG_FAIL_QUEST,
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                QustRecord {
                    form_id: next.0,
                    start_up_stage: Some(10),
                    ..Default::default()
                },
            ],
        );

        {
            let mut stages = world.resource_mut::<QuestStageState>();
            stages.start_quest(completed, None);
            stages.set_stage(completed, 100);
            stages.start_quest(failed, None);
            stages.set_stage(failed, 50);
        }
        quest_terminal_stage_system(&world, 0.0);

        let stages = world.resource::<QuestStageState>();
        assert!(stages.is_completed(completed));
        assert!(stages.is_failed(failed));
        assert!(stages.is_running(next));
        assert_eq!(stages.get_stage(next), 10);
    }

    #[test]
    fn objective_targets_resolve_through_live_quest_aliases() {
        let mut world = World::new();
        crate::register(&mut world);
        let quest = QuestFormId(0x1000);
        let actor = world.spawn();
        install_start_game_quests(
            &mut world,
            [QustRecord {
                form_id: quest.0,
                objectives: vec![QuestObjective {
                    index: 10,
                    targets: vec![byroredux_plugin::esm::records::QuestObjectiveTarget {
                        target: QuestObjectiveTargetKind::Alias(7),
                        flags: 0,
                        keyword: None,
                        conditions: Vec::new(),
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        );
        world
            .resource_mut::<crate::scene::SceneActorBindings>()
            .bind(quest, 7, actor);

        assert_eq!(resolve_quest_objective_targets(&world, quest, 10), [actor]);
    }

    #[test]
    fn alias_readiness_gate_advances_once_after_every_alias_binds() {
        let mut world = World::new();
        crate::register(&mut world);
        world.insert_resource(QuestStageState::default());
        let player = world.spawn();
        world.insert_resource(PlayerEntity(player));

        let quest = QuestFormId(0x3372B);
        world
            .resource_mut::<QuestStageState>()
            .start_quest(quest, None);
        assert!(install_quest_alias_readiness_gate(
            &mut world,
            QuestAliasReadinessGate {
                quest,
                required_aliases: vec![1, 12],
                target_stage: 12,
                only_below_stage: 15,
            },
        ));

        let first_actor = world.spawn();
        world
            .resource_mut::<crate::scene::SceneActorBindings>()
            .bind(quest, 1, first_actor);
        quest_alias_readiness_stage_system(&world, 0.0);
        assert_eq!(world.resource::<QuestStageState>().get_stage(quest), 0);
        assert!(world
            .query::<QuestStageAdvancedBatch>()
            .unwrap()
            .get(player)
            .is_none());

        let second_actor = world.spawn();
        world
            .resource_mut::<crate::scene::SceneActorBindings>()
            .bind(quest, 12, second_actor);
        quest_alias_readiness_stage_system(&world, 0.0);
        assert_eq!(world.resource::<QuestStageState>().get_stage(quest), 12);
        let query = world.query::<QuestStageAdvancedBatch>().unwrap();
        let batch = query.get(player).expect("readiness advance batch");
        assert_eq!(batch.0.len(), 1);
        assert_eq!(batch.0[0].quest, quest);
        assert_eq!(batch.0[0].new_stage, 12);
        drop(query);

        quest_alias_readiness_stage_system(&world, 0.0);
        let query = world.query::<QuestStageAdvancedBatch>().unwrap();
        assert_eq!(query.get(player).unwrap().0.len(), 1);
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

    /// #3277 (SCR-D6-2026-08-24-01) regression — two same-frame producers
    /// writing the shared [`QuestStageAdvancedBatch`] sink must BOTH survive.
    ///
    /// The sink is one `SparseSetStorage` slot on one shared entity, so a
    /// bare `insert()` replaces it outright. `quest_fragment_dispatch_system`
    /// did exactly that, which was invisible only because it happened to be
    /// the last same-frame producer in the schedule — until
    /// `quest_alias_readiness_stage_system` and `scene_fragment_dispatch_system`
    /// were scheduled ahead of it.
    ///
    /// Asserts on [`push_quest_stage_advances`] itself, which every one of the
    /// six writers now routes through, so the guarantee holds for a seventh
    /// nobody has written yet.
    #[test]
    fn push_quest_stage_advances_merges_same_frame_producers() {
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        world.insert_resource(PlayerEntity(player));

        let ev = |quest: u32, new_stage: u16| QuestStageAdvanced {
            quest: QuestFormId(quest),
            previous_stage: 0,
            new_stage,
        };

        // Producer 1 — first writer of the frame, creates the component.
        push_quest_stage_advances(&world, player, vec![ev(0x1000, 10)]);
        assert_eq!(
            world
                .get::<QuestStageAdvancedBatch>(player)
                .expect("first producer must create the sink")
                .0
                .len(),
            1
        );

        // Producer 2 — must append, not replace.
        push_quest_stage_advances(&world, player, vec![ev(0x2000, 20), ev(0x3000, 30)]);
        // Producer 3 — the shape that used to clobber everything above.
        push_quest_stage_advances(&world, player, vec![ev(0x4000, 40)]);

        let batch = world
            .get::<QuestStageAdvancedBatch>(player)
            .expect("quest event batch");
        assert_eq!(
            batch.0.len(),
            4,
            "#3277: every same-frame producer's events must survive; a bare \
             insert() would leave only the last producer's {} event(s)",
            1
        );
        for quest in [0x1000, 0x2000, 0x3000, 0x4000] {
            assert!(
                batch.0.iter().any(|e| e.quest == QuestFormId(quest)),
                "#3277: quest {quest:#x}'s advance was dropped by a later producer"
            );
        }
        // Order matters: the sink is a journal, and the compatibility
        // consumers demux by `quest` in arrival order.
        assert_eq!(
            batch.0.iter().map(|e| e.new_stage).collect::<Vec<_>>(),
            vec![10, 20, 30, 40],
            "#3277: appends must preserve producer order"
        );
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
                        skyrim_plus: true,
                        quest_flags: QUEST_FLAG_START_GAME_ENABLED | QUEST_FLAG_ACTIVE,
                        start_up_stage: Some(10),
                        ..Default::default()
                    },
                    QustRecord {
                        form_id: 0x2000,
                        quest_flags: 0,
                        start_up_stage: Some(20),
                        ..Default::default()
                    },
                    QustRecord {
                        form_id: 0x3000,
                        skyrim_plus: true,
                        quest_flags: QUEST_FLAG_START_GAME_ENABLED | QUEST_FLAG_FAILED,
                        ..Default::default()
                    },
                ],
            ),
            2
        );

        quest_startup_system(&world, 0.0);
        let state = world.resource::<QuestStageState>();
        assert!(state.is_started(QuestFormId(0x1000)));
        assert_eq!(state.get_stage(QuestFormId(0x1000)), 10);
        assert!(state.is_active(QuestFormId(0x1000)));
        assert!(!state.is_started(QuestFormId(0x2000)));
        assert!(state.is_failed(QuestFormId(0x3000)));
        drop(state);
        let batch = world
            .get::<QuestStageAdvancedBatch>(player)
            .expect("quest event batch");
        assert_eq!(batch.0.len(), 3, "startup must append to live producers");
        assert!(batch
            .0
            .iter()
            .any(|event| event.quest == QuestFormId(0x1000)));
        assert!(batch
            .0
            .iter()
            .any(|event| event.quest == QuestFormId(0x3000)));
        drop(batch);

        assert!(world
            .resource_mut::<QuestStageState>()
            .stop(QuestFormId(0x1000)));
        quest_startup_system(&world, 0.0);
        assert!(!world
            .resource::<QuestStageState>()
            .is_running(QuestFormId(0x1000)));
        assert_eq!(
            world
                .get::<QuestStageAdvancedBatch>(player)
                .expect("quest event batch")
                .0
                .len(),
            3,
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

    // ── #2659 (SCR-D6-NEW11-02) — Arc-shared QuestDefinitionRegistry ──

    /// Regression pinning the actual perf fix: `QuestDefinitionRegistry::
    /// clone()` must share the underlying allocation (an `Arc` refcount
    /// bump), not deep-copy it. `DeferredFragmentEffects::new`
    /// (`fragment.rs`) calls this unconditionally on every
    /// `quest_fragment_dispatch_system` invocation — a deep clone there
    /// was measured at up to the entire frame budget on a heavily-modded
    /// (5,000-quest) load order.
    #[test]
    fn registry_clone_shares_the_same_allocation() {
        let mut world = World::new();
        world.insert_resource(QuestDefinitionRegistry::default());
        world.insert_resource(StartGameQuestRegistry::default());
        install_engine_start_quest(&mut world, QuestFormId(0x1000), Some(10));

        let original = world.resource::<QuestDefinitionRegistry>().clone();
        let snapshot = original.clone();
        assert!(
            Arc::ptr_eq(&original.definitions, &snapshot.definitions),
            "clone() must share the same Arc allocation, not deep-copy \
             the definitions map (#2659)"
        );
    }

    /// Correctness counterpart: once a snapshot clone is taken (as
    /// `DeferredFragmentEffects::new` does before the quest-state guards
    /// are acquired), a LATER write to the live registry (via
    /// `Arc::make_mut`, which clones-on-write when the Arc is shared)
    /// must NOT retroactively mutate that snapshot. If `Arc::make_mut`'s
    /// clone-on-write ever failed to trigger here, a prior frame's
    /// still-alive snapshot would silently observe a later frame's
    /// writes — exactly the class of bug sharing the allocation could
    /// introduce if done carelessly.
    #[test]
    fn write_after_snapshot_does_not_mutate_the_snapshot() {
        let mut world = World::new();
        world.insert_resource(QuestDefinitionRegistry::default());
        world.insert_resource(StartGameQuestRegistry::default());
        let quest_a = QuestFormId(0x1000);
        install_engine_start_quest(&mut world, quest_a, Some(10));

        // Snapshot BEFORE the second write, exactly like
        // `DeferredFragmentEffects::new` does before taking the
        // quest-state guards.
        let snapshot = world.resource::<QuestDefinitionRegistry>().clone();
        assert!(snapshot.contains(quest_a));
        assert!(!snapshot.contains(QuestFormId(0x2000)));

        let quest_b = QuestFormId(0x2000);
        install_engine_start_quest(&mut world, quest_b, Some(20));

        // The live registry sees both quests now...
        assert!(world
            .resource::<QuestDefinitionRegistry>()
            .contains(quest_a));
        assert!(world
            .resource::<QuestDefinitionRegistry>()
            .contains(quest_b));
        // ...but the earlier snapshot must still see only the first —
        // proving the shared allocation was cloned-on-write, not mutated
        // through the snapshot's own (still-live) Arc handle.
        assert!(snapshot.contains(quest_a));
        assert!(
            !snapshot.contains(quest_b),
            "a write after the snapshot was taken must not retroactively \
             appear in it (#2659 Arc clone-on-write correctness)"
        );
    }
}
