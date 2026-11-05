# RF7-2026-11-05-05: `systems` depends on `cell_loader` for shared index and context types

Labels: low,tech-debt,bug,esm-plugin

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 7
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/cell_loader/index.rs:51` `LoadedCellIndex`
- `byroredux/src/cell_loader/transition.rs:54` `LoadedPluginSet`, `:72` `CurrentCellContext`, `:158` `CurrentExteriorContext`
- `byroredux/src/cell_loader/load_order.rs:152` `GlobalFormIdResolver`

## Why
`systems` has 22 production references to `cell_loader`, mostly `LoadedCellIndex`, and `cell_loader` references `systems` in return, so the two form a cycle. Moving the types rather than the loading code is enough to remove this direction, and `components/indices.rs` already exists for this kind of type.

## Change
Move these types into `components`, leaving the loading code where it is. Confirm first that none of them would pull `cell_loader` along through its own fields or methods, and leave any that would.

## Constraints
Move items verbatim. Run `grep -rn include_str` for each source file first. This does not propose splitting the binary crate.

## Acceptance Criteria
- `systems` production code reaches these types without naming `cell_loader`
- `cargo test -p byroredux --bin byroredux` passes

## Completeness Checks
- [ ] **LOCK_ORDER**: TypeId-sorted lock acquisition is preserved wherever a RwLock scope moves
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6715 --json state`)._
