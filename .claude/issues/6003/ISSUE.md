# SCR-D5-2026-10-17-02: Story Manager dispatcher records a `StoryManagerNodeState <-> QuestStageState` lock-order cycle since 35f1cf051

Issue: #6003
Labels: medium, scripting, bug, concurrency

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D5-2026-10-17-02: Story Manager dispatcher records a `StoryManagerNodeState <-> QuestStageState` lock-order cycle since 35f1cf051

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Cinematic
- **Location**: `crates/scripting/src/story_manager.rs:715-735` (phase A: `node_state` read guard held across `dispatch_story_event` -> `evaluate`), `:751-790` (phase B: `QuestStageState` write guard held while `StoryManagerNodeState` is taken for write); comment at :748-750 claims "registry -> stages -> node_state" order for both phases
- **Status**: NEW (introduced by 35f1cf051, after the baseline)
- **Untrusted-Input**: No
- **Description**: 35f1cf051 replaced phase A's `QuestStageState` read guard with a `statuses()` snapshot (to stop the nested-read warning). Before it, phase A held `stages` -> `node_state`, consistent with phase B (`stages` -> `node_state`). Now the stages guard is a statement-scoped temporary, so during the walk phase A holds `{registry, aliases, node_state}` while every `evaluate` of a quest/stage/running gate (GetStage, GetStageDone, GetQuestRunning, ... via `try_resource::<QuestStageState>`) acquires `QuestStageState` read: edge `StoryManagerNodeState -> QuestStageState`. Phase B then holds `QuestStageState` write and acquires `StoryManagerNodeState` write (:784): edge `QuestStageState -> StoryManagerNodeState`. Both edges are observable in one production run (the first radiant start after a gated node evaluates). The detector records process-wide edges "even when the hold periods are sequential" (lock_tracker.rs:272-300), so `BYRO_LOCK_ORDER_CHECK=1` on a real SM start closes the cycle and aborts in a debug build. It is not a live deadlock today (both phases run inside one exclusive Update system), which is why this is MEDIUM; the CI lane cannot see it because no unit test installs `StoryManagerNodeState` (guard-run note above).
- **Impact**: a debug/lock-check engine run panics on the first SM quest start with a gated node and installed node state; the cycle becomes a real ABBA the moment any other (non-exclusive) system reads/writes either resource.
- **Related**: #5628, #5366 (earlier SM lock fixes), 35f1cf051.
- **Suggested Fix**: snapshot what phase A needs from `node_state` (clone or release before `dispatch_story_event`, as is already done for the stages) or take `node_state` for phase B before `stages`; add an SM dispatch test that installs `StoryManagerNodeState` and a `GetStage`-gated quest node, run under the lock-order lane.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and `BYRO_LOCK_ORDER_CHECK=1` lane stays green
- [ ] **TESTS**: A regression test pins this specific fix
