# SCR-D6-2026-10-17-03: GetButtonPressed global slot - unspecified consumer order and stale answers

Issue: #6013
Labels: low, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D6-2026-10-17-03: GetButtonPressed global slot - unspecified consumer order and stale answers

- **Severity**: LOW
- **Dimension**: Legacy ObScript
- **Location**: `obscript_quests.rs:401-440` (`running` built from `QuestStageState::iter()` = HashMap order, `quest_stages.rs:105,407`); `messages.rs:136-153`
- **Status**: NEW (residual of the #5790 fix)
- **Untrusted-Input**: No
- **Description**: Since `173f9e155` the answer is one slot consumed by whichever quest script calls `GetButtonPressed` first. Due quests run in `HashMap` iteration order (random per process), so with two polling quests in the same tick the winner is nondeterministic (also any same-tick cross-quest `SetStage`/`GetStage` ordering). Additionally `record_button` never clears on a new box: a previous unread answer is returned while the next box is still open, where the CS says -1 until pressed (MessageBox page: "sends -1 while the window is still open").
- **Evidence**: The CS wiki itself is internally inconsistent (Notes: "will only react to a MessageBox created from within the same script"; spell example and the lead sentence imply a global slot). The global behaviour is a defensible reading; the nondeterminism is what is not.
- **Suggested Fix**: Sort `running` by form id (deterministic, matches record order); clear `ButtonPresses` when a script box is queued.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
