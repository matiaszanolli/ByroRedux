# SCR-D5-2026-10-17-06: Regression of #5682 — Topic-menu clicks re-roll a stochastic INFO gate (residual of #5682)

Issue: #6007
Labels: low, scripting, bug, dialogue

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

> Residual of closed #5682: the fix holds within one operation only. Filed as a new issue because #5682 is closed.

### SCR-D5-2026-10-17-06: Topic-menu clicks re-roll a stochastic INFO gate (residual of #5682)

- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic
- **Location**: `byroredux/src/systems/npc_dialogue.rs:213-225` (`topic_entries` keeps only `{topic_form_id, name}`), `:838-849` (`select_topic_by_form_id` calls `select_first_info` fresh)
- **Status**: Regression of #5682 (partial fix: within one operation only)
- **Untrusted-Input**: No
- **Description**: 8daa24105 carries the passing `&InfoRecord` from menu filtering into `select_on_topic`, so one open/click evaluates each INFO once. The list the player sees was filtered in an earlier operation, and the click is a new operation that evaluates the clicked topic's INFOs again. A topic whose INFOs are all gated on `GetRandomPercent` (fn 77) can pass at menu time and draw a failing roll at click time, yielding "no INFO on topic ... passes its conditions right now" for a topic the player just clicked - the second symptom #5682 listed. Closed as fixed; the cross-operation half is not.
- **Impact**: intermittent no-op click on fn-77-gated menu topics (vanilla reach near zero per #5682's census).
- **Related**: #5682, #5645.
- **Suggested Fix**: persist the chosen INFO FormID per menu entry in `NpcDialogueTopic.topics` and have the click look it up (re-validating non-stochastic gates only), or draw fn 77 once per menu build.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
