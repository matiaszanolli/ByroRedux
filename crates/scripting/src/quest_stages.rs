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
use std::collections::{HashMap, HashSet};

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
        self.quests.entry(quest).or_default().entry(objective).or_default()
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
#[derive(Debug, Clone, Copy)]
pub struct QuestStageAdvanced {
    pub quest: QuestFormId,
    pub previous_stage: u16,
    pub new_stage: u16,
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
