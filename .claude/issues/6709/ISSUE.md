# RF6-2026-11-05-02: Split `build_blas_batched` along its numbered phases

Labels: low,renderer,vulkan,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 6
**Status**: NEW. Part of #6680

## Location
- `crates/renderer/src/vulkan/acceleration/blas_static.rs:266` (966 lines, 458 of code, pin 985)

## Why
The function's comments already number its phases. Its failure paths consume `prepared` and the query pool, so a split must keep ownership right on every exit. That is why #6143 only pinned it.

## Change
Extract one function per phase, passing ownership explicitly on failure paths.

## Constraints
Part of #6143, which keeps its other open functions. Do #6371's host-side teardown cleanup first. Extract verbatim, one function per commit, and mix in no behaviour change. `build_blas_batched_releases_before_overwriting`, `static_batched_build_emits_a_scratch_barrier_before_its_first_build` and `every_shared_scratch_writer_self_emits_before_its_first_build` read this code as text, and must still see it after the split.

## Acceptance Criteria
- Call order and barrier and timer-bracket placement are unchanged
- A `BYRO_VALIDATION=1` run exercising the error paths matches the baseline's warning profile
- Non-timing tokens of the `bench:` line match a baseline build of the parent commit
- The size pin in `crates/renderer/src/vulkan/context/function_size_pin_tests.rs` is lowered to the new size
- Each failure path is exercised at least once, for example through fault injection, with no leak reported at teardown

## Completeness Checks
- [ ] **DROP**: Teardown order in `crates/renderer/src/vulkan/context/teardown.rs` is unchanged
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit
- [ ] **VALIDATION**: A `BYRO_VALIDATION=1` run matches the baseline's warning profile; no barrier or timer bracket moved

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6709 --json state`)._
