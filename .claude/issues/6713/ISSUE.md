# RF7-2026-11-05-03: Z-up to Y-up coordinate helpers live in `cell_loader` but are used outside it

Labels: low,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 7
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/cell_loader/transition.rs:400` `position_zup_to_yup`, `:417` `rotation_zup_to_yup_quat`
- `byroredux/src/cell_loader/euler.rs:47` `euler_zup_to_quat_yup_refr`
- Consumers: `byroredux/src/loading_screen.rs`, `byroredux/src/asset_provider/script.rs`, `byroredux/src/systems/light_anim.rs`

## Why
These are pure math helpers, but using them means depending on the cell loader.

## Change
Move them, unchanged, to a low-level module, and update the callers.

## Constraints
Move items verbatim. Run `grep -rn include_str` for each source file first. This does not propose splitting the binary crate. Do not change the math. `byroredux/src/cell_loader/transition.rs:717` reads that file to find `pub fn rotation_zup_to_yup_quat`, so it must follow the move.

## Acceptance Criteria
- The named consumers no longer depend on `cell_loader` for coordinates
- The rotation tests pass with their expected values unchanged

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6713 --json state`)._
