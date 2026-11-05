# RF5-2026-11-05-02: The scheduler has no named ordering constraints, so system order is file position

Labels: low,ecs,tech-debt,enhancement

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 5
**Status**: NEW. Part of #6680

## Location
- `crates/core/src/ecs/scheduler.rs:167-354` (`add`, `add_to`, `add_to_with_access`, `add_exclusive`, `add_exclusive_with_access`)
- `crates/core/src/ecs/scheduler.rs:563` `system_names`

## Why
Order within a stage is registration order, and no API expresses `before` or `after`. A subsystem cannot register its own systems without some central list deciding where they land. That is why `register_late_systems` remains one function. No `register_systems` function exists anywhere yet.

## Change
Add named `before` and `after` constraints resolved when the schedule is built. A schedule with no constraints must keep today's order exactly. Unknown names and cycles are errors.

## Constraints
Part of #5802. This is an internal API, so it adds no host-facing contract.

## Acceptance Criteria
- A schedule with no constraints produces today's `system_names()` output
- Tests cover `before`, `after`, an unknown name and a cycle
- Parallel and exclusive phase semantics are unchanged (`exclusive_runs_after_parallel` still passes)
- `cargo test -p byroredux-core --features inspect scheduler` passes

## Completeness Checks
- [ ] **LOCK_ORDER**: TypeId-sorted lock acquisition is preserved wherever a RwLock scope moves
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6702 --json state`)._
