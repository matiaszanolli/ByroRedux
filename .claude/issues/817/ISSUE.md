# #817 — FO4-D4-NEW-05: 5 FO4-architecture maps invisible to EsmIndex::categories()

**Severity**: MEDIUM
**Location**: `crates/plugin/src/esm/records/mod.rs:360-450` (`categories()`); `mod.rs:2262-2280` (pin test)
**Source**: `docs/audits/AUDIT_FO4_2026-05-04_DIM4.md`
**Created**: 2026-05-04
**Related**: closed #634 (categories single-source-of-truth — incomplete for FO4)

## Summary

5 FO4-architecture maps (`scols`, `packins`, `movables`,
`material_swaps`, `texture_sets`) live on `EsmCellIndex`, not
`EsmIndex` — invisible to `total()` / `category_breakdown()` /
`categories_table_row_count_pinned`. Regression that wipes
`cells.scols` passes CI silently.

## Fix shape

Add 5 rows to `EsmIndex::categories()` referencing `s.cells.*.len()`.
Bump pin from 82 → 87. Pattern already established for `cells` and
`statics` rows.

## Sequencing

Land BEFORE #819 (FO4 ESM parse-rate harness depends on these
categories being visible to `category_breakdown()`).

## How to fix

```
/fix-issue 817
```
