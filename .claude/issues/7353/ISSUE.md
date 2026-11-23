# #7353: SAVE-08: A quicksave deferred by the SAVE-4 wait is dropped silently when `engine.quit` / a script `QuitGame` is in the same batch

- **Severity**: LOW
- **Dimension**: Save-Side Gates (quiescent deferral, `MAX_SAVE_DEFERRALS`)
- **Data-Loss Class**: silent-drop (an explicitly requested save is not written; nothing is reported)
- **Location**: `byroredux/src/save_io/commands.rs:618-628` (deferral), `byroredux/src/app_events.rs:112-122` (`GracefulExitRequested` docs: the queued save "execute[s] in that frame's post-scheduler step before the teardown begins"), `app_events.rs:1534-1542` (top of `about_to_wait` runs `shutdown` before any drain), `shutdown` at `app_events.rs:138` (no `PendingPlayerSaveActions` flush).
- **Status**: NEW
- **Description**: SAVE-4 (`d72e1f45e`) made the player-action drain return an empty batch (leaving the `Quicksave` queued) for up to `MAX_SAVE_DEFERRALS` = 3 ticks while `PendingRelocations` / `PendingActorMoves` are non-empty. `step_actor_relocations` runs inside `step_cell_transition`, *after* `step_player_save_actions` in `about_to_wait` (`app_events.rs:1719` vs `1730`), so any script move raised during the scheduler run makes the drain defer. The `engine.quit` contract (one-frame-later polling so `input.press quicksave` + `engine.quit` in one batch still saves) was written before the deferral; the next frame's `about_to_wait` now calls `shutdown` at its head, before the queued save can run, and `shutdown` does not drain the queue.
- **Evidence**:
  ```rust
  // commands.rs:623
  if moves_pending && pending.0.contains(&PlayerSaveAction::Quicksave) && pending.1 < MAX_SAVE_DEFERRALS {
      pending.1 += 1;  return Vec::new();
  }
  // app_events.rs:1538  (before step_player_save_actions)
  if self.world.try_resource::<GracefulExitRequested>().is_some_and(|r| r.0) { self.shutdown(event_loop); return; }
  ```
- **Impact**: Needs a same-tick actor move plus a same-batch quit; the window is one tick, so rare, but the result is a save the operator/automation asked for that never lands and no message. Smoke/automation harnesses that quicksave then quit are the exposed users.
- **Related**: SAVE-4 / SAVE-7 (fixed 11-17); the 11-17 report did not examine the quit ordering.
- **Suggested Fix**: have `shutdown` (or the `GracefulExitRequested` branch) run `execute_pending_player_save_actions` once with deferral disabled (call `save_now` path) before teardown; or make the quit poll wait while `PendingPlayerSaveActions` is non-empty and deferrals remain.
- **Dimension agent ID**: SAVE-D4-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other parked-state paths, load-drain steps, notification callers and `atomic_write` callers the finding names)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_SAVE_2026-11-23.md`
