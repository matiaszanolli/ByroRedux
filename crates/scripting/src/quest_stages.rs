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
//! - **Stage-fragment dispatch.** Papyrus quests carry per-stage
//!   "fragment scripts" that run when the stage advances. That's
//!   M47.0 territory — the runtime here exposes a
//!   [`QuestStageAdvanced`] marker event that fragment systems will
//!   consume once they land, but the dispatch loop itself stays
//!   future work.
//! - **Stage objectives.** Papyrus also has `SetObjectiveDisplayed` /
//!   `SetObjectiveCompleted` / `SetObjectiveFailed` — these mutate a
//!   parallel objectives state separate from stages. Same shape will
//!   land next to [`QuestStageState`] when a quest-UI consumer needs
//!   it; today the renderer has no journal UI to feed.
//! - **`OnStageSet` event handlers.** Papyrus scripts can declare
//!   `Event OnStageSet(int auiStageID, int auiItemID)` callbacks.
//!   M47.0 emits these as marker components consumed by
//!   per-quest-fragment systems.

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

/// Marker event emitted by [`QuestStageState::set_stage`]-driven
/// systems whenever a quest advances. Attached to a designated
/// "quest events" entity (set up at world init); consumed by:
///
/// - The (future) stage-fragment dispatcher (M47.0).
/// - The (future) quest journal UI updater.
/// - Other scripts that subscribed via Papyrus's `RegisterFor*`
///   surface (which will lower to marker-component subscriptions
///   when M47.2 lands).
///
/// Multiple advances in a frame stack: each `set_stage` call that
/// emits an advance event adds a fresh marker. Cleanup at end of
/// frame via [`crate::event_cleanup_system`].
///
/// **Today**, the event is unused — no consumer subsystems exist.
/// It ships in the R5 follow-up so the M47.0 wiring has a stable
/// contract to consume; pinning it now keeps stage-advance causality
/// observable in tests + debug-server inspection.
#[derive(Debug, Clone, Copy)]
pub struct QuestStageAdvanced {
    pub quest: QuestFormId,
    pub previous_stage: u16,
    pub new_stage: u16,
}

impl Component for QuestStageAdvanced {
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
