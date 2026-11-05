# RF5-2026-11-05-04: Update and PostUpdate stages: move registration into the owning subsystems

Labels: low,ecs,scripting,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 5
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/boot/schedule/update/script_runtime.rs` (44 systems)
- `byroredux/src/boot/schedule/update/quest_scene.rs` (34)
- `byroredux/src/boot/schedule/update/player_animation.rs` (23)
- `byroredux/src/boot/schedule/update/interaction_and_crime.rs` (9)
- `byroredux/src/boot/schedule/update/combat.rs` (4)
- `byroredux/src/boot/schedule/post_update.rs` (17)

## Why
These 131 systems are grouped by domain, but the rows are still hand-written in the hubs. New subsystems keep editing them.

## Change
Apply the #6703 method to these files, one domain per commit, and lower each size pin in `byroredux/src/scheduler_access_tests.rs:1283-1289` as its function shrinks.

## Constraints
Part of #5802.

## Acceptance Criteria
- `system_names()` and `access_report()` output are identical before and after, compared by a checked diff
- Each domain function is a flat list of subsystem calls
- `cargo test -p byroredux --bin byroredux boot::schedule` passes

## Completeness Checks
- [ ] **LOCK_ORDER**: TypeId-sorted lock acquisition is preserved wherever a RwLock scope moves
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6704 --json state`)._
