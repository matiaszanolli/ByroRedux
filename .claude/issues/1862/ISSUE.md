# #1862: SAVE-07: QuestStageState/QuestObjectiveState absent from save registry — quest progress silently wiped on save/load

- **Severity**: HIGH
- **Labels**: `high`, `bug`
- **Source**: `docs/audits/AUDIT_SAVE_2026-07-03.md` (SAVE-07)
- **Dimension**: Snapshot Completeness & Determinism
- **Data-Loss Class**: silent-drop

## Location
- `crates/scripting/src/quest_stages.rs:64-69` (`QuestStageState`, `impl Resource for QuestStageState {}`)
- `crates/scripting/src/quest_stages.rs:175-180` (`QuestObjectiveState`, `impl Resource for QuestObjectiveState {}`)
- `byroredux/src/save_io.rs:157-187` (`build_save_registry` — neither type appears)

## Description
`QuestStageState` is the runtime backing for Papyrus `Quest.SetStage()`/`GetStage()`/`GetStageDone()` — one `QuestStageData` per quest the player has interacted with. `QuestObjectiveState` is its sibling for `SetObjectiveDisplayed`/`SetObjectiveCompleted`/`SetObjectiveFailed`. Both are installed as `World` resources and are live-wired (`quest_advance_dispatch` → `quest_fragment_dispatch` run every frame in `Stage::Update`). Neither carries a `Serialize`/`Deserialize` derive, so neither can be registered in `build_save_registry` — and isn't.

## Impact
Every quest's `current_stage`, `stages_done` history, and every objective's flags silently revert to default on every save→load cycle, with no validation-gate trip since this is an absence, not an inconsistency.

## Related
Same silent-drop class as #1834 (`ActorValues`) and #1835 (`PerkList`/`FactionRanks`) — different crate/root cause (`Resource` missing derive+registration vs. `Component` missing registry entry).

## Suggested Fix
Add `#[derive(serde::Serialize, serde::Deserialize)]` to `QuestStageState`, `QuestStageData`, `QuestObjectiveState`, `ObjectiveStatus`; register both resources in `build_save_registry`; add a round-trip test mirroring `player_pose_survives_snapshot_round_trip`.
