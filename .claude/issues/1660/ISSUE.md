**Severity**: MEDIUM · **Dimension**: Multi-Master Load Order + TES5 Cell-Load
**Location**: `crates/plugin/src/esm/cell/walkers.rs:759` (`parse_refr_group` / `PlacedRef` construction; `header.flags` never inspected) · struct `crates/plugin/src/esm/cell/mod.rs:337` (no `deleted`/flags field) · merge `crates/plugin/src/esm/cell/mod.rs:942` (no delete branch)
**Status**: NEW (confirmed still present 2026-06-18; self-documented gap, never filed)

## Description
The #1546 per-REFR merge keeps base REFRs the DLC didn't re-emit and overlays the ones it did. But a Bethesda override CELL can also *delete* a base REFR by re-emitting it with the record-level Deleted flag (0x20). `parse_refr_group` reads `header` then sub-records and builds `PlacedRef` from `header.form_id`/`base_form_id`/placement only — `header.flags & 0x20` is never consulted, and `PlacedRef` carries no flags field. `merge_cell_references` therefore has no way to distinguish a delete from an edit; a deleted REFR survives the merge as its base copy. The omission is acknowledged in `merge_cell_references`'s own doc comment.

## Evidence
- `struct PlacedRef` (`cell/mod.rs:337`) has no `deleted`/flags field; grep for `deleted` / `header.flags & 0x20` across `cell/walkers.rs` + `cell/mod.rs` returns zero hits.
- `merge_cell_references` (`cell/mod.rs:942`) is per-FormID last-write-wins with no delete branch.

## Impact
Base-game REFRs a DLC deletes render twice / in the wrong place under `--master Skyrim.esm --esm Dawnguard.esm --cell <overridden>`. Bounded — over-render of individual objects, never an empty/near-empty cell. Vanilla single-plugin loads (control bench) unaffected.

## Suggested Fix
Capture `header.flags & 0x20 != 0` into a `deleted: bool` on `PlacedRef`; in `merge_cell_references`, when an override ref carries the Deleted flag, remove the base entry and skip the tombstone rather than overlaying it. Low effort; the FormID key already exists.

## Completeness Checks
- [ ] **SIBLING**: The deleted-flag check covers every REFR-class group the cell walker builds (REFR/ACHR/PGRE etc.), not just `parse_refr_group`
- [ ] **TESTS**: A regression test pins a DLC-deleted base REFR (0x20 flag) being removed by `merge_cell_references` rather than over-rendered
