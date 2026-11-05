# RF5-2026-11-05-03: `register_late_systems`: let subsystems register their own Late-stage systems

Labels: low,ecs,scripting,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 5
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/boot/schedule/late.rs:15` (856 lines, 537 of code, 36 systems)
- Size pin: `byroredux/src/scheduler_access_tests.rs:1289` (858)

## Why
This is the largest registration hub left. `register_update_systems` is already a short dispatcher over domain files, but the Late stage is still one function, and each new subsystem edits it.

## Change
Have each subsystem expose a `register_systems(&mut Scheduler)` with named constraints in place of file position, and reduce the hub to a flat list. Depends on #6684, #6701 and #6702. Move one subsystem per commit.

## Constraints
Part of #5802. `crate::boot::SOURCES` must include any new file that holds registrations, or text scans stop seeing them.

## Acceptance Criteria
- `system_names()` and `access_report()` output are identical before and after, compared by a checked diff
- `register_late_systems` is a flat list, and its size pin is lowered to match
- `every_parallel_system_declares_everything_it_acquires` and the `*_declares_everything_it_acquires` tests still find every moved row, shown by one injected fault

## Completeness Checks
- [ ] **LOCK_ORDER**: TypeId-sorted lock acquisition is preserved wherever a RwLock scope moves
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6703 --json state`)._
