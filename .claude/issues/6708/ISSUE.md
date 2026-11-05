# RF6-2026-11-05-01: Split `record_skinned_blas_refit` into per-phase functions

Labels: low,renderer,vulkan,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 6
**Status**: NEW. Part of #6680

## Location
- `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:79` (971 lines, 382 of code, nesting depth 10, pin 985)

## Why
This is the deepest function in the renderer's recording path. A live `Some(ref mut accel)` borrow of `self.accel_manager` spans the whole body, so its phases cannot simply become `&mut self` methods. That is why #6143 only pinned it.

## Change
Reshape the borrow using disjoint fields, which #6697 prepares, then extract one function per phase or bracket.

## Constraints
Part of #6143, which keeps its other open functions. Do #6371's host-side teardown cleanup first. Extract verbatim, one function per commit, and mix in no behaviour change. Use a cell with animated NPCs, because refit only runs for skinned actors.

## Acceptance Criteria
- Call order and barrier and timer-bracket placement are unchanged
- A `BYRO_VALIDATION=1` run exercising the error paths matches the baseline's warning profile
- Non-timing tokens of the `bench:` line match a baseline build of the parent commit
- The size pin in `crates/renderer/src/vulkan/context/function_size_pin_tests.rs` is lowered to the new size

## Completeness Checks
- [ ] **DROP**: Teardown order in `crates/renderer/src/vulkan/context/teardown.rs` is unchanged
- [ ] **LOCK_ORDER**: TypeId-sorted lock acquisition is preserved wherever a RwLock scope moves
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit
- [ ] **VALIDATION**: A `BYRO_VALIDATION=1` run matches the baseline's warning profile; no barrier or timer bracket moved

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6708 --json state`)._
