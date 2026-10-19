# PERF-SD-2026-10-19-05: `effective_absorbed_refs` rebuilds a SipHash `HashSet` on every apply slice

Labels: low,performance,game:fo4,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6127

**Severity**: LOW (FO4 only) · **Dimension**: Streaming & Cells

## Location
`byroredux/src/cell_loader/exterior.rs` (`ExteriorCellApplyJob::advance`) and `byroredux/src/cell_loader/load.rs` (`InteriorCellApplyJob::advance`); the function is `cell_loader/precombined.rs` `effective_absorbed_refs`.

## Description
Both `advance` methods recompute the absorbed-ref set from `cell.absorbed_ref_bakes` and `baked_hashes` on each call, i.e. once per frame slice until the REFR walk finishes, although the inputs are fixed once the precombine phase completes. It is empty and free when no precombine baked (every game except FO4).

## Evidence
Scratch replica (release): 1,000 bake entries 24 us, 5,000 -> 106 us, 20,000 -> 511 us per call (~21 ns per entry). The real `absorbed_ref_bakes` length per FO4 cell was not measured. Call sites confirmed at publish time (`exterior.rs` `advance`, `load.rs` `advance`).

## Impact
~0.1 ms (5k entries) per slice, ~1 % of the 16 ms apply budget, times the slices of every FO4 cell.

## Suggested Fix
Compute once when `baked_hashes` is finalised and keep the set in the job (an `FxHashSet` while there).

Source: docs/audits/AUDIT_PERFORMANCE_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
