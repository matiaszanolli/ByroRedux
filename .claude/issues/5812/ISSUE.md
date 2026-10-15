# TD2-2026-10-15-11: `StartScene`/`StopScene` copy the `StartQuest`/`StopQuest` lifecycle body, and the #5468 split put the two copies in sibling files without sharing them

Labels: low,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-11`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/fragment/effects/quest_scoped.rs:40-80` vs `effects/scene.rs:55-90`
- **Status**: NEW
- **Age**: `f5127c1cc` (2026-09-10) in `effects.rs`; moved into separate files by `adbd468e2` (#5468, 2026-10-14)
- **Effort**: trivial
- **Description**: `start_quest(quest, start_up_stage)` + `scene_actor_bindings_dirty = true` on `Some`, and `is_running(quest).then(|| shut_down_stage.map(|stage| { set_stage; QuestStageAdvanced { .., stage_set: true } })).flatten()` + `stages.stop(quest)` + dirty flag, are identical.
- **Evidence**: clone scan 5 identical windows `quest_scoped.rs:56-60` / `scene.rs:67-71`.
- **Impact**: a lifecycle rule (the #5406 "refused start" precedent in `story_manager.rs:792`, the Run-Once reset) has to be edited in both and in `quest_stages.rs`, `papyrus_demo/quest_advance.rs:473`.
- **Related**: #5406, #2539.
- **Suggested Fix**: `QuestStageState::start_with(quest, start_up_stage) -> Option<QuestStageAdvanced>` and `stop_with_shutdown(quest, shut_down_stage) -> (bool, Option<QuestStageAdvanced>)` in `quest_stages.rs`; both effect arms call them.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
