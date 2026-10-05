# #5297 — SCR-D2-2026-10-05-01: The spoken-line path claims the fragment journal cursor in Stage::Late and re-emits into a batch event_cleanup_system drains unread — a line spoken from an NPC activation never runs the new stage's quest fragment

- **Labels**: high,scripting,quests,dialogue,bug
- **Filed from**: `docs/audits/AUDIT_SCRIPTING_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5297

- **Severity**: HIGH (escalation: a transient marker drained out of stage order, so it is never consumed)
- **Dimension**: Fragment Dispatch & Locks (journal subscribers); Dim 3 has a pointer for marker drain
- **Untrusted-Input**: No
- **Location**:
  - `crates/scripting/src/fragment/systems.rs:229-237`: `apply_spoken_info_fragment` calls `apply_fragment_guard_free`,
    then `poll_fragment_generated_advances`.
  - `crates/scripting/src/fragment/effects.rs:548-584`: `poll_fragment_generated_advances` calls
    `poll_quest_events(FRAGMENT_QUEST_EVENT_SUBSCRIBER)`.
  - `crates/scripting/src/quest_stages.rs:699-717`: the journal `poll` returns every event since the cursor and moves the
    cursor to `next_sequence`, so it is destructive.
  - `byroredux/src/systems/npc_dialogue.rs:374-393`: `dispatch_spoken_fragment` calls `push_quest_stage_advances`.
  - `byroredux/src/boot/schedule/late.rs:437-466`: the selection system is registered in `Stage::Late`.
  - `byroredux/src/boot/schedule/late.rs:521`: `event_cleanup_system` is the last Late exclusive.
  - `crates/scripting/src/cleanup.rs:94`: the `drain_component::<QuestStageAdvancedBatch>` call.
  - `byroredux/src/boot/schedule/update.rs:434`: `quest_fragment_dispatch`, in `Stage::Update`.
- **Status**: NEW. It was introduced by `c59600138` (#5152). Today's ECS D5-02 (Access row) and concurrency #5066 (guard
  shadow) look at the same call chain but not at this.
- **Description**: `poll_fragment_generated_advances` was written for callers that run inside the fragment subscriber's
  own dispatch window:
  - `quest_fragment_dispatch_system` polls that subscriber itself at its head.
  - `scene_fragment_dispatch_system` runs immediately before it, in Update. Its re-emitted batch is read as legacy ingress
    a few systems later.

  #5152 reuses the helper from `npc_dialogue_selection`, which runs in `Stage::Late`, after `quest_fragment_dispatch`.
  The poll claims the spoken fragment's own `SetStage` transition and moves the subscriber's cursor past it. The helper
  returns the transition as an advance, and the caller pushes it onto the player's `QuestStageAdvancedBatch`. That batch
  has only two readers, and both run in Update:
  - `quest_fragment_dispatch_system` (`systems.rs:244`);
  - scene playback (`scene/playback.rs:552`).

  `event_cleanup_system` drains the batch at the end of the same Late stage. On the next frame the dispatcher sees
  neither the journal entry (already claimed) nor the mirror (already drained), so the newly-set stage's QUST fragment is
  never run.
- **Evidence**: the spoken path's own doc (`npc_dialogue.rs:328-332`) states the intended behaviour, which is what does
  not happen:
  > "the fixture's Eltrys entry line lowers to `GetOwningQuest().SetStage(..)`, which journals the transition for
  > `quest_fragment_dispatch_system` (Update stage — next frame) to run the stage fragment that displays the next objective."

  The test notes that the poll swept up other producers' events:
  > "The retained journal also carries the fixture/helper quest-start events the bound_world setup produced"
  (`npc_dialogue.rs:1119-1121`)
- **Impact**:
  - **The activation path is affected.** It is the opening line, the Blocking entry in the MS01 fixture. That line's
    `SetStage` advances `QuestStageState`, but the stage fragment never runs. Its effects (`SetObjectiveDisplayed`,
    `StartScene`, `MoveTo`, …) never apply, and nothing logs it.
  - **Other pending transitions are lost too.** Any transition still pending on the fragment cursor when a line is spoken
    is claimed and dropped the same way. This includes `quest_terminal_stage_system` successor starts/completions and
    `fragment_continuation_system` resumed `SetStage`s, which both run after `quest_fragment_dispatch` in Update.
  - **The UI re-selection and close paths still work.** `select_topic_by_form_id` and `end_open_conversation` are called
    from `main.rs` outside the scheduler, so their batch survives to the next Update and is dispatched as unmirrored
    legacy ingress.
- **Related**: #5152, #3012 (the "don't claim the journal while you have nothing to consume" rule), #3277
  (`push_quest_stage_advances`), ECS-2026-10-05-D5-02, #5066.
- **Suggested Fix**: in `apply_spoken_info_fragment`, return the direct advances without polling, as
  `fragment_continuation_system` already does. The journal entry then stays unclaimed for the next frame's
  `quest_fragment_dispatch_system`, which dedups the mirror. Alternatively, move activation selection into Update ahead
  of `quest_fragment_dispatch`. Extend the test so it runs `event_cleanup_system` and then `quest_fragment_dispatch_system`
  with a stage fragment installed, and asserts that the fragment's effect applied.

_Source: `AUDIT_SCRIPTING_2026-10-05.md` (SCR-D2-2026-10-05-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
