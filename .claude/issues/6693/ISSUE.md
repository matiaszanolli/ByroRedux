# RF3-2026-11-05-01: `App`: move the 22 `bench_*` fields into a bench state struct

Labels: low,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 3
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/main.rs:539` (`App`, 94 fields)

## Why
22 of `App`'s 94 fields are bench state, with 84 access sites in 5 files: `bench_mode`, `bench_frames_target`, `bench_hold`, `bench_summary_printed`, `bench_frames_count`, `bench_camera`, `bench_path`, `bench_camera_origin`, `bench_camera_applied_pose`, `bench_camera_subject_distance`, `bench_camera_path_frame`, `bench_start`, `bench_systems_ns`, `bench_systems_ticks`, `bench_build_render_ns`, `bench_ui_ns`, `bench_render_ns`, `bench_frame_timings`, `bench_cpu_frame_ms`, `bench_rt_tier_hist`, `bench_rt_tier_sampled_frame`, `bench_capture`. This is the best place to start, because it carries no GPU risk.

## Change
Add one bench sub-struct, keeping its construction in `App`'s existing setup. Decide whether `pending_soak_cycle` belongs with it.

## Constraints
Run `grep -rn include_str` first: 37 `include_str!` sites read `main.rs` or `app_*.rs`, and a moved field can blind a scan. Move one group per commit.

## Acceptance Criteria
- The listed fields live on one sub-struct, and `App` no longer declares them
- `cargo test -p byroredux --bin byroredux` passes
- Non-timing tokens of the `bench:` line match a baseline build of the parent commit

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6693 --json state`)._
