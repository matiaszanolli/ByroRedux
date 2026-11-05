# RF5-2026-11-05-05: `build_save_registry`: move the 83 direct rows into per-subsystem `register_saved`

Labels: low,save-load,scripting,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 5
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/save_io/registry.rs:236` (379 lines, 155 of code)

## Why
The function still has 83 direct rows beside 21 `register_saved` calls, so each saved subsystem edits a central table. #5802 established the per-subsystem seam on the crime and ownership family.

## Change
Move rows into their subsystem's `register_saved`, one subsystem per commit, and call it at the rows' old position.

## Constraints
Part of #5802. `SaveRegistry::schema_fingerprint` is order-sensitive, so rows cannot be reordered. The shape guard reads rows from `registry.rs` and from files that define `fn register_saved(`.

## Acceptance Criteria
- `schema_fingerprint` and the saved-shape fingerprint are unchanged by every commit
- `registry_completeness_tests` passes
- The direct-row count falls, and each row left has a reason, such as its type living in the binary crate

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6705 --json state`)._
