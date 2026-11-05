# RF9-2026-11-05-02: Function-size budgets count comments, so documenting a function can break its budget

Labels: low,test-gap,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, side notes
**Status**: NEW. Part of #6680

## Location
- `crates/core/src/source_scan.rs:210` `function_line_count`
- Local copies: `byroredux/src/asset_provider/material/merge.rs:2405`, `crates/renderer/src/vulkan/context/geometry_pass.rs:938`
- `crates/renderer/src/vulkan/context/draw.rs:1453` `DRAW_FRAME_LINE_BUDGET` (720)
- Pin tables: `crates/renderer/src/vulkan/context/function_size_pin_tests.rs`, `byroredux/src/scheduler_access_tests.rs:1283-1289`, `crates/physics/src/function_size_pin_tests.rs`, `crates/nif/src/anim/function_size_pin_tests.rs`

## Why
These budgets exist to stop code regrowing, but they count every line. `draw_frame` is 719 lines against a budget of 720, yet only 409 of them are code, so one more comment line or two fails its test while adding logic in place of comments passes. Two local copies of the counter and `draw_frame_body()` add more ways to measure.

## Change
Make `function_line_count` count code lines, skipping blank and comment lines. Remove the local copies, and re-baseline every pin to its code-line count in the same commit.

## Acceptance Criteria
- `function_line_count` ignores blank and comment lines, with a unit test
- No local copy of the counter remains
- Every function-size budget and pin is re-baselined, and adding a comment no longer fails one
- An added code line past a pin still fails it

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6720 --json state`)._
