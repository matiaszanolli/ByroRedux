# RF3-2026-11-05-03: `App`: move the UI and HUD fields into a UI state struct

Labels: low,tech-debt,bug,ui

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 3
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/main.rs:539` (`App`, 94 fields)

## Why
Ten fields hold UI state: `ui_manager`, `ui_input_state`, `ui_texture_handle`, `ui_reported_host_methods`, `ui_reported_host_methods_capped`, `ui_dropped_host_calls`, `ui_dropped_host_calls_menu`, `hud`, `scaleform_hud`, `objective_hud_cache`. The `ui_*` group alone has 15 access sites in 5 files.

## Change
Add one UI sub-struct that owns these fields.

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

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6695 --json state`)._
