# RF4-2026-11-05-03: Binary crate: triage 301 standalone source scans and replace those that have a runtime check available

Labels: low,test-gap,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 4
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/boot/schedule/mod.rs` (53 include sites)
- `byroredux/src/systems/navmesh_path.rs` (21)
- `byroredux/src/boot/mod.rs` (18)
- `byroredux/src/cell_loader/object_lod.rs` (14)
- `byroredux/src/scheduler_access_tests.rs` (13)
- `byroredux/src/npc_spawn/tests.rs` (12)
- `byroredux/src/app_events.rs` (11)

## Why
The crate has 357 `include_str!` reads of Rust files. 56 are rows in `SOURCES` concat lists, leaving 301 standalone scans. Many are justified because their subject needs a Vulkan device or game data. The aim is to find the ones that are not, and this is not a mass rewrite. A scan can pass after a refactor without checking anything, which makes the rest of this backlog riskier.

## Change
Classify the scans, then replace the replaceable ones with behavioural tests, one file per commit. #6684 and #6685 already cover the schedule-order tests and `acquired_in`.

## Acceptance Criteria
- The issue records each scan's class: needs a device or game data, replaceable by a runtime check, pins prose, or pins syntax
- Every scan classed replaceable has a behavioural test, and the scan is deleted
- One injected fault per changed file fails its new test
- No guard is removed without a replacement or a stated reason

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6686 --json state`)._
