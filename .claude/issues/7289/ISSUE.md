# #7289: SCR-D2-01: A `Message.Show()` continuation re-queued by the frame budget loses the player's answer (resumes with button 0)

- **Severity**: MEDIUM
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment/systems.rs:90-99` and `:151-166`; `crates/scripting/src/messages.rs:191-197`
- **Status**: NEW (feature tracker Refs #6667; no open issue covers it)
- **Description**: `fragment_continuation_system` resolves a `MessageAnswered` continuation by calling `take_papyrus_answer`, which *removes* the answer from `PapyrusMessageAnswers::answered`, then stores the button in `state.answer` and pushes the run onto `ready`. If the interpreter frame budget is exhausted before this run's turn, the `ready` loop re-queues every remaining run with `..pending`, which keeps `resume_when: MessageAnswered { ticket }`. On the next frame `take_papyrus_answer` finds neither an answered entry nor a waiting ticket and returns the "box was lost" default `Some(0)`, and the system overwrites `state.answer` with it. The real answer is gone and the script branches on button 0.
- **Evidence**:
  ```rust
  // systems.rs:90
  FragmentResumeCondition::MessageAnswered { ticket } => match crate::messages::take_papyrus_answer(world, *ticket) {
      Some(button) => { ... state.answer = Some(button); ... ready.push(pending); }
  // systems.rs:151 (budget exhausted -> re-queue keeps resume_when)
  let mut waiting = vec![PendingFragmentExecution { remaining_seconds: 0.0, ..pending }];
  // messages.rs:191 — second call, answer already removed:
  if let Some(button) = answers.answered.remove(&ticket) { return Some(button); }
  (!answers.waiting.contains(&ticket)).then_some(0)
  ```
- **Impact**: Silent wrong branch of a quest script (MQ101's `If TempChooseSidesMessage.Show() == 0`) whenever the answer lands in a frame whose budget (10 000 statements) runs out before the run is reached — i.e. under exactly the load the budget exists for. Violates "never guess a value". No test covers a budget-cut ready `MessageAnswered` run (`message_show_waits_for_its_answer_and_branches_on_it` runs with a fresh budget).
- **Related**: #5763 (budget), #6667; `a_budget_is_spent_and_renewed`.
- **Suggested Fix**: When the answer is taken, also set the pending's `resume_when` to `DelayElapsed` (the answer already lives in the continuation's `resume.answer`), so a re-queue cannot re-poll the ticket. Add a test that exhausts the budget between answer and resume.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
