# RF5-2026-11-05-01: `Access` has no way for a subsystem to contribute its own declarations

Labels: low,ecs,scripting,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 5
**Status**: NEW. Part of #6680

## Location
- `crates/core/src/ecs/access.rs:64` (`Access`)
- `byroredux/src/boot/schedule/mod.rs:1326` `npc_dialogue_selection_declares_everything_the_spoken_fragment_path_acquires`
- `byroredux/src/boot/schedule/late.rs` (the dialogue row)

## Why
This is the next step #5802 recorded on 2026-10-16. Crime and ownership subsystems own their resources and save rows, but their access lines stay in the dialogue row in `late.rs`, because the guard matches that row's literal `.reads` and `.writes` text.

## Change
Add an `Access::with(fn)` combinator so a subsystem can export its declarations. Teach the guard to follow a delegated declaration. Move the crime and ownership lines as the first user.

## Constraints
Part of #5802.

## Acceptance Criteria
- `Access::with` exists, with a unit test
- The guard sees access declared through a delegated function, and a deliberately removed declaration fails it
- `access_report()` output for the migrated row is unchanged

## Completeness Checks
- [ ] **LOCK_ORDER**: TypeId-sorted lock acquisition is preserved wherever a RwLock scope moves
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6701 --json state`)._
