# #7290: SCR-D2-02: The VM check does not treat `Message.Show()` as a wait, so a function called for its value (or with an array local) raises the box, then the run stops

- **Severity**: MEDIUM
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment/vm/checker.rs:1099-1114` (check arm), `crates/scripting/src/fragment/vm/interp.rs:274-281` (`call_function`), `:1192-1224` (`message_show`); `crates/scripting/src/fragment/vm/natives/game_utility.rs:267-272` (the `Utility.Wait` arm it should mirror)
- **Status**: NEW (Refs #6667; the value-called-wait residual of #5764 is the related documented gap, but that issue covers *statements inside* such functions, not a check hole)
- **Description**: `Utility.Wait` is accepted only as a statement and sets `self.waits = true`, so `call_waits` rejects any function that waits when it is called for its value ("a wait in a function called for its value"). The new `Message.Show()` arm returns `Some(Kind::Num)` without setting `waits`. A helper such as `Int Function Ask() Return Msg.Show() EndFunction` called as `If Ask() == 0` therefore passes `check_function`. At run time the callee's `exec_block` turns the `AWAITS_MESSAGE` stop into `Flow::Suspend`, `call_function` rejects a suspension, and the run ends with `stop("a wait in a function called for its value")` — after `show_message_awaiting` already queued the box and `awaits_message = Some(ticket)`. The same applies to a frame holding an array/struct local (`frame_state` → "an array local across a wait"), which the check also never sees for `Show`.
- **Evidence**:
  ```rust
  // checker.rs:1104-1113 — no `self.waits = true`
  if arg_kinds.len() <= 9 && arg_kinds.iter().all(numeric) { return Some(Kind::Num); }
  // game_utility.rs:267 — the Wait arm
  if statement && matches!(arg_kinds, [..]) { self.waits = true; return Some(Kind::None); }
  // interp.rs:279
  Flow::Suspend(..) => return stop("a wait in a function called for its value"),
  ```
- **Impact**: Breaks the VM contract "check ahead of time, run whole or not at all": the player is shown a modal box whose answer is discarded and the script aborts mid-run (earlier effects of the run already applied). Secondary: `awaits_message` is never cleared on a stopped run, so a later `Utility.Wait` suspending in the same `apply_effects` call (several `Interpret` effects merged in one stage) is built with `resume_when: MessageAnswered { ticket: <stale> }` instead of `DelayElapsed` (effects.rs:400). The orphaned ticket stays in `waiting` until the 64-ticket cap.
- **Related**: SCR-D2-03, #5764.
- **Suggested Fix**: Set `self.waits = true` in the checker arm (so `call_waits` declines value-called callers and the check fails ahead of time), and clear `deferred.awaits_message` when `run_fragment` ends in a stop.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
