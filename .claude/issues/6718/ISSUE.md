# RF8-2026-11-05-01: Only 9 of 172 systems run in parallel: profile first, then promote only what shows up

Labels: low,performance,ecs,tech-debt,enhancement

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 8
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/boot/schedule/` (registrations)
- `byroredux/src/systems/metrics.rs:387` (`top_systems_ms`)
- `crates/core/src/ecs/scheduler.rs:585` `access_report`

## Why
By source count, 9 systems use `add_to*` (Early 2, Late 4, Physics 1, PostUpdate 1, Update 1) and 163 use `add_exclusive*`. Serial is a sound default for script systems, and nothing here has been profiled, so there is no evidence yet that this costs frame time.

## Change
Capture `top_systems_ms` on representative scenes, such as an FNV interior, a Skyrim exterior and an FO4 interior. For systems that matter to the frame, check their declared access against `access_report()`. Promote only systems that are both expensive and free of conflicts. If nothing qualifies, close this with the data.

## Constraints
Measure before changing any registration. Follow-up issues per system are fine if the data calls for them.

## Acceptance Criteria
- The measured scenes and the top-systems table are recorded on this issue
- Each promoted system has a before and after CPU frame figure from the same scene
- Each promoted system is covered by `every_parallel_system_declares_everything_it_acquires`, and `known_conflict_count()` stays 0
- The lock-order detector job passes on the binary and scripting crates

## Completeness Checks
- [ ] **LOCK_ORDER**: TypeId-sorted lock acquisition is preserved wherever a RwLock scope moves
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6718 --json state`)._
