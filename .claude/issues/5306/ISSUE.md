# #5306: ECS-2026-10-05-D5-01: The LSCR loading-model turntable never turns, because the main loop zeroes `dt` for exactly the frames its stage exists

**Labels**: medium,ecs,ui,game:skyrim,game:fo4,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5306

**Source**: `docs/audits/AUDIT_ECS_2026-10-05.md` — `ECS-2026-10-05-D5-01` (HEAD `a2c24b16e`)

- **Severity**: MEDIUM. The documented behavior is dead and the authored LSCR "No Rotation" flag has no effect; the impact is visual only. Owner overlap: `/audit-ui` (loading cover).
- **Dimension**: 5 — Scheduler Wiring
- **Location**:
  - `byroredux/src/systems/loading_model.rs:16-28` (`loading_model_turntable_system`)
  - `byroredux/src/app_events.rs:700-703` (the dt override)
  - `byroredux/src/boot/schedule/update.rs:599` (the registration)
- **Status**: NEW (introduced by `e60911864`)
- **Description**:
  - The turntable rotates each `LoadingModelStage` root by `dt * LOADING_TURNTABLE_RAD_PER_S`, using the scheduler's `dt`.
  - `about_to_wait` sets `dt = 0.0` whenever `self.loading_screen.active()`, so simulation time holds still under the cover, and passes that `dt` to `scheduler.run` (`app_events.rs:968`).
  - The stage exists and draws only while the cover is active. `LoadingScreen::active_stage()` returns `None` unless `phase != Idle`, and the stage is retired when the cover dismisses.

  So every scheduler tick that sees a `LoadingModelStage` has `dt == 0`, and the model never turns. The commit message says the cover "turns it slowly (LSCR 0x8000 No Rotation honored)". The unit test (`the_turntable_rotates_the_root_and_the_no_rotation_flag_holds_it`) calls the system with `dt = 0.5` directly, so it cannot see this. The `p6-loading-model.sh` smoke gate checks presence and a screenshot floor, not motion.
- **Evidence**:
  ```rust
  // app_events.rs:700
  let dt = if self.loading_screen.active() { 0.0 } else if ... { ... } else { wall_dt };
  // loading_model.rs:24
  let spin = Quat::from_rotation_y(dt * LOADING_TURNTABLE_RAD_PER_S);
  ```
- **Impact**: Skyrim and FO4 loading-cover models are static. "No Rotation" records and rotating records look identical.
- **Suggested Fix**: Drive the turntable from wall-clock time that the loading screen owns, rather than from simulation `dt`. Either:
  - stamp a `LoadingCoverClock` resource with `wall_dt` next to the `DeltaTime` stamp, and have the system read it; or
  - advance the stage rotation from `LoadingScreen` on the App side.

  Add a test that runs the real scheduler path with an active cover and asserts rotation.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
