# #7299: SCR-D2-05: `Message.Show()` resume paths: an unwound `AWAITS_MESSAGE` stop is uncaught in the resumed-`While` path, and two `Show()` calls in one statement exchange answers

- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment/vm/interp.rs:378-381` and `:457-461` (`resume_statement` `While` arm), `:394-396` (the only catch), `:88` / `:131` (`answer` slot)
- **Status**: NEW
- **Description**: (a) The `AWAITS_MESSAGE` stop is converted to a suspension only inside the statement loop of `exec_block`. After a `Utility.Wait` inside a `While` body whose *condition* contains `Msg.Show()`, resumption goes `resume_statement` → `Flow::Normal => self.exec_while(...)`; the condition's `Show` stops with `AWAITS_MESSAGE` through `?` and no catch, so `run_fragment` logs "stopped: a Message.Show() awaiting its answer" and the run ends (box raised, answer discarded). (b) `VmResume.answer` is one slot and `run_fragment` resets it to `None` on every suspend. In `If A.Show() == 0 && B.Show() == 1` the replay after A's answer raises B and suspends; after B's answer the replay's *first* `Show` (A) takes B's answer, so A reads B's button and B raises again.
- **Evidence**:
  ```rust
  // interp.rs:459 — resume path, no AWAITS_MESSAGE catch
  Flow::Normal => self.exec_while(frame, &condition.node, body),
  // interp.rs:1211
  if let Some(button) = self.answer.take() { return Ok(Some(button)); }
  ```
- **Impact**: Rare script shapes; a dead run after an unanswerable box, or a wrong value (not a guess-free decline) for the multi-`Show` statement. Neither is rejected by the checker.
- **Related**: SCR-D2-02.
- **Suggested Fix**: Catch `AWAITS_MESSAGE` in the resume arms too (or restart a resumed `While` through the statement loop); have the checker decline a statement containing more than one value-`Show`.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
