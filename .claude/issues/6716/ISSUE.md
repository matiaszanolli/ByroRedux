# RF7-2026-11-05-06: `populate_scene_runtime` lives in `asset_provider` and calls up into `systems` and `cell_loader`

Labels: low,tech-debt,bug,scripting

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 7
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/asset_provider/script.rs:1046` `populate_scene_runtime` (452 lines, 350 of code)
- `install_*` calls into `systems` at `byroredux/src/asset_provider/script.rs:1068-1079`, `:1251-1253`
- `byroredux/src/asset_provider/script.rs:1596` `spawn_logical_quest_reference_with_scripts`
- `byroredux/src/asset_provider/material/provider.rs:340` `open_geometry_csg`

## Why
`asset_provider` is the layer for extracting data from archives, yet this function sets up scene runtime state across several systems and spawns references. #5802 also names it as a hub that keeps growing.

## Change
Move the orchestration out of `asset_provider` to where boot and scene setup live, leaving script lookup behind. Do the same review for the `open_geometry_csg` reference.

## Constraints
Move items verbatim. Run `grep -rn include_str` for each source file first. This does not propose splitting the binary crate.

## Acceptance Criteria
- `asset_provider` production code no longer names `systems`, and names `cell_loader` only where a stated reason applies
- `populate_scene_runtime`'s call order is unchanged
- `cargo test -p byroredux --bin byroredux` passes

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6716 --json state`)._
