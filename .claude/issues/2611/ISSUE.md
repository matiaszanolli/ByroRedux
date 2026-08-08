# FO4-D8-001: SCOL->PKIN expansion never checks index.packins (symmetric gap to #1180)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2611
**Finding ID**: FO4-D8-001

**Severity**: LOW
**Dimension**: 8 (Cell/REFR Expansion)
**Location**: `byroredux/src/cell_loader/refr.rs:475-518` (`expand_scol_placements_with_depth`)
**Status**: NEW

## Description
`expand_scol_placements_with_depth` never checks `index.packins` for a SCOL
part whose `base_form_id` is a PKIN — this is the symmetric gap to #1180
(which fixed the PKIN→SCOL recursion direction) never being closed for the
SCOL→PKIN direction. Zero vanilla FO4 exposure — this only matters for
mod/DLC content that nests a PKIN inside a SCOL part.

## Evidence
`byroredux/src/cell_loader/refr.rs:475-518` walks SCOL parts but only
recurses into further SCOL/base-mesh cases, never checking `index.packins`
for a part `base_form_id` that resolves to a PKIN.

## Impact
Zero vanilla-content exposure — no vanilla FO4 SCOL nests a PKIN part. Would
silently under-expand for any mod/DLC content that does.

## Suggested Fix
Mirror #1180's PKIN→SCOL recursion check symmetrically: when a SCOL part's
`base_form_id` resolves via `index.packins`, recurse into the PKIN expansion
the same way #1180 made the reverse direction recurse into SCOL children.

## Related
#1180 (the PKIN→SCOL direction this closes the symmetric gap for).

## Completeness Checks
- [ ] **TESTS**: A regression test with a synthetic SCOL-nesting-PKIN fixture pins the new recursion
