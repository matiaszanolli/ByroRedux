# #5406: SCR-D5-2026-10-08-04: Phase B ignores `start_quest`'s refusal. Completed and failed radiants stay "eligible" forever, are logged as started, consume pool marks and rewrite event fills, and stopped quests resume without Skyrim's start-reset

**Labels**: medium,scripting,quests,bug,game:skyrim
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5406

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-10-08.md` — `SCR-D5-2026-10-08-04` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: phase B calls `stages.start_quest(..)` and discards the result (`story_manager.rs` ~490); `start_quest` returns `None` unless status is `Stopped`.

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Dialogue (Story Manager); quest lifecycle touchpoint Dim 2
- **Untrusted-Input**: No
- **Location**:
  - `crates/scripting/src/story_manager.rs:486-524`: `stages.start_quest(..)`'s `Option` return is discarded. The
    fired/last-fire marks, the `started_event_fills` push and the "started quest" `info!` all follow unconditionally.
  - `:712-727`: eligibility is `!is_running`.
  - `crates/scripting/src/quest_stages.rs:172-195`: `start_quest` returns `None` unless the status is `Stopped`.
  - `crates/scripting/src/quest_stages.rs:240-254`: completed and failed quests also cannot be stopped.
- **Status**: NEW
- **Description**:
  - **Completed/Failed quests.** Phase 3 moved eligibility from `is_started` to `is_running` so radiants can re-fire.
    But a quest that reached a QSDT Complete/Fail stage is neither running nor startable. In a stacked pool it is
    `preferred[0]` on every event, so later entries never start. In a random or do-all pool it periodically wins the
    pick and wastes the event. Each time, phase B logs "started quest …", marks it fired, and overwrites its
    `StoryEventAliasFill`.
  - **Stopped quests.** These resume with their old `stages_done` and stage. The CK *Quest Data Tab*
    (`ck-uesp-wiki/(main)/Quest Data Tab.wiki`) says "Run Once: Prevents the Quest from being reset when it starts",
    which means a non-Run-Once quest **is** reset on start. A re-fired radiant therefore keeps its previous run's history,
    and `GetStageDone` gates plus non-repeatable `SetStage`s see a finished run.
- **Impact**:
  - Radiant pools jam once an entry completes.
  - The `sm1` attribution grep (`started quest … via node …`) can pass on a start that never happened.
  - Reruns of stopped radiants are semantically wrong.
- **Related**: SCR-D5-2026-10-08-02. The consume and start decisions should use the real start result.
- **Suggested Fix**: treat `start_quest == None` as "not started": no marks, no fill, no log, and no consume. For SM
  starts of non-Run-Once quests (QUST `DNAM` flag), reset the quest (`QuestStageState::reset` + start) so that completed
  and failed radiants can rerun. Pin both behaviours.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other `start_quest` callers that ignore the `Option` return)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
