# FO4-D8-002: expand_pkin_placements doc comment still claims single-level SCOL recursion

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2612
**Finding ID**: FO4-D8-002

**Severity**: LOW
**Dimension**: 8 (Cell/REFR Expansion)
**Location**: `byroredux/src/cell_loader/refr.rs:347-350` (doc comment), `:406-419` (actual recursion)
**Status**: NEW

## Description
`expand_pkin_placements`'s doc comment still claims SCOL children "stay
single-level" — false since #1180, whose code (at `refr.rs:406-419`) already
recurses into SCOL children. Pure doc drift, no code defect.

## Evidence
`byroredux/src/cell_loader/refr.rs:347-350` doc comment says SCOL children
stay single-level; `:406-419` in the same file already recurses into them
(landed with #1180).

## Impact
Doc-only — misleads a reader into thinking recursion isn't implemented when
it already is.

## Suggested Fix
Update the doc comment at `:347-350` to describe the actual (recursive)
behavior implemented at `:406-419`, referencing #1180.

## Completeness Checks
- [ ] **TESTS**: N/A — doc-only change
