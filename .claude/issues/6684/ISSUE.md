# RF4-2026-11-05-01: Seven schedule-order guards read registration text although `access_report()` exposes the built order

Labels: low,ecs,test-gap,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 4
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/boot/schedule/mod.rs:68` `activation_flush_is_scheduled_before_every_activate_event_consumer`
- `byroredux/src/boot/schedule/mod.rs:116` `hit_event_consumers_run_after_every_producer`
- `byroredux/src/boot/schedule/mod.rs:160` `trap_hit_flush_runs_after_the_script_systems_that_queue_into_it`
- `byroredux/src/boot/schedule/mod.rs:181` `extension_event_adapters_run_before_transient_cleanup`
- `byroredux/src/boot/schedule/mod.rs:245` `scene_and_fragment_dispatch_chain_stays_in_dependency_order`
- `byroredux/src/boot/schedule/mod.rs:284` `transient_cleanup_is_the_last_late_exclusive`
- `byroredux/src/boot/schedule/mod.rs:456` `walk_animation_registers_after_all_seven_movers`

## Why
These tests compare positions of registration text in `crate::boot::SOURCES`. `transient_cleanup_is_the_last_late_exclusive` even looks in a 200-byte window after each `add_exclusive` for the stage name. They depend on file layout rather than the schedule, and moving a registration can break them or leave them checking nothing. Building the schedule needs no device: `player_wind_read_is_declared_and_weather_writer_is_exclusive` (`byroredux/src/scheduler_access_tests.rs:104`) already reads order from `build_scheduler()` and `access_report()`.

## Change
Add a small helper over the built schedule that returns a system's position in a stage, and rewrite the seven tests on it.

## Constraints
`locomotion_registers_behind_the_single_kill_switch`, `locomotion_kill_switch_gates_exactly_the_motion_systems` and `scheduler_timings_insert_is_env_gated` pin environment-gated source branches, so they stay as text. Confirm the default environment registers every system the seven tests name.

## Acceptance Criteria
- The seven named tests read the built schedule, not `SOURCES`
- One injected reorder per test makes that test fail
- `cargo test -p byroredux --bin byroredux boot::schedule` passes

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6684 --json state`)._
