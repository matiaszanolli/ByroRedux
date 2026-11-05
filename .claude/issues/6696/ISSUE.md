# RF3-2026-11-05-04: `App`: move the capture and debug-output fields into their own struct

Labels: low,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 3
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/main.rs:539` (`App`, 94 fields)

## Why
Nine fields configure one-off output, with 21 access sites in 4 files: `screenshot_path`, `screenshot_requested`, `screenshot_deadline_frames`, `linear_capture`, `export_mitsuba_dir`, `export_mitsuba_frame`, `material_dump_path`, `camera_pos_override`, `camera_forward_override`.

## Change
Add one capture sub-struct that owns these fields. `bench_capture` stays with #6693.

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

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6696 --json state`)._
