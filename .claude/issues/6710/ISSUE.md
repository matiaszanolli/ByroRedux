# RF6-2026-11-05-03: Split `record_geometry_pass` into per-phase functions

Labels: low,renderer,vulkan,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 6
**Status**: NEW. Part of #6680

## Location
- `crates/renderer/src/vulkan/context/geometry_pass.rs:22` (751 lines, 404 of code, pin 770)

## Why
This is the main raster recording function, with nesting depth 8 in #6143's measurement. It has a size pin but has not been split.

## Change
Extract one function per phase, keeping state-setting and draw order identical.

## Constraints
Part of #6143, which keeps its other open functions. Do #6371's host-side teardown cleanup first. Extract verbatim, one function per commit, and mix in no behaviour change. Both PROPOSED PRDs cite this file, and `gpu-driven-rendering.md:160` and `:336` cite exact lines. Check whether that work is starting first, so the two do not collide. The test at `geometry_pass.rs:959` holds a second pin for this function.

## Acceptance Criteria
- Call order and barrier and timer-bracket placement are unchanged
- A `BYRO_VALIDATION=1` run exercising the error paths matches the baseline's warning profile
- Non-timing tokens of the `bench:` line match a baseline build of the parent commit
- The size pin in `crates/renderer/src/vulkan/context/function_size_pin_tests.rs` is lowered to the new size
- `docs/prd/gpu-driven-rendering.md` and `docs/prd/virtual-geometry.md` cite the new locations

## Completeness Checks
- [ ] **DROP**: Teardown order in `crates/renderer/src/vulkan/context/teardown.rs` is unchanged
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit
- [ ] **VALIDATION**: A `BYRO_VALIDATION=1` run matches the baseline's warning profile; no barrier or timer bracket moved

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6710 --json state`)._
