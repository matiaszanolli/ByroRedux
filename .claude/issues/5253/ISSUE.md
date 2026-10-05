# #5253 — SAVE-D4-2026-10-05-01: A console save in the frame a cover-less door transition tears down passes the stale mid-transition refusal and writes a permanently unloadable save (residual of #4138)

- **Labels**: medium,save-load,gameplay,bug
- **Filed from**: `docs/audits/AUDIT_SAVE_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5253

- **Severity**: MEDIUM
- **Dimension**: Save-Side Gates
- **Data-Loss Class**: irrecoverable-write
- **Location**:
  - `byroredux/src/app_step.rs:847-861` (`step_player_save_actions` publishes `CellTransitionInFlight` from `interior_transition.is_some() || loading_screen.active()`);
  - `byroredux/src/app_events.rs:968`, `:1031`, `:1042`, `:1093` (frame order: `scheduler.run` → `step_player_save_actions` → `step_cell_transition` → `render_one_frame`);
  - `byroredux/src/app_step.rs:1001-1009` (a failed `loading_screen.begin` falls through to an immediate teardown), `:1031-1041` (`unload_current_interior` in the same call);
  - `byroredux/src/cell_loader/transition.rs:104-111` (the doc claims the flag "cannot go stale"), `:433-436` (teardown removes `CurrentCellContext`);
  - `byroredux/src/save_io.rs:891-939` (`SaveCommand::execute`: the gate is the only context check);
  - `byroredux/src/main.rs:1661-1668` (native-overlay console executes `CommandRegistry` inside `render_one_frame` via `app_frame.rs:276`).
- **Status**: NEW. Residual of the closed #4138 (HIGH), whose fix and pin (`the_save_drain_publishes_the_transition_flag_before_draining`) cover only the player-action drain.
- **Description**:
  - `CellTransitionInFlight` is derived state, synced once per frame at the head of `step_player_save_actions`. That is correct for the queued F5/pause-menu drain, which runs immediately after the sync.
  - `SaveCommand`'s own lock comment (`save_io.rs:892-903`) names two other production callers:
    - the remote console, which runs in `DebugDrainSystem` inside `scheduler.run`, *before* the sync;
    - the native-overlay console, dispatched from `render_one_frame`, *after* `step_cell_transition`.
  - The no-cover case: a door transition whose loading cover cannot start, because `begin_artwork` finds no `LoadedCellIndex`, no load screen or missing art. In that case `step_cell_transition` tears down in the same call:
    - `drain_streaming_state` clears the exterior context;
    - `unload_current_interior` removes `CurrentCellContext`;
    - `InteriorCellApply::begin` starts the budgeted apply.
  - The flag published earlier in that frame is still `false`. Two windows follow:
    - an overlay `save` later in frame N;
    - a remote `save` in frame N+1's scheduler, before N+1 republishes.

  Both pass every gate. No gate checks for a missing context, so the save commits with neither context. It consumes the ring slot or overwrites an explicit slot. `LoadCommand` then refuses it forever as a "loose save", which is exactly #4138's outcome.
- **Evidence**: with a cover, `loading_screen.begin` returns `Ok` and the teardown moves to the next frame's `step_cell_transition`, after that frame's sync publishes `true`. That path is safe. The cover-less `Err(pending) => pending` arm continues straight into `unload_current_interior` within the same `step_cell_transition` call. The flag has exactly one writer (`rg CellTransitionInFlight`).
- **Impact**: an operator or smoke script that runs `save [slot]` right after a door transition can write a slot that can never be loaded, destroying that slot's previous save, with a success message. It needs no loading cover and the one-frame window. Player input is not affected.
- **Related**: #4138 (closed); `docs/smoke-tests/p5-door-transition.sh` (cross-cell door saves; uses the remote console).
- **Suggested Fix**: make the refusal read the invariant itself rather than a derived flag. Refuse when a `LoadedCellIndex` is installed but neither `CurrentCellContext` nor `CurrentExteriorContext` exists. Alternatively, publish `CellTransitionInFlight` wherever `interior_transition` is assigned. Extend the pin to the console ingress: a world with an index and no context refuses `save`.

_Source: `AUDIT_SAVE_2026-10-05.md` (SAVE-D4-2026-10-05-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
