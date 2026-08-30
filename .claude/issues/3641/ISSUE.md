# #3641: FO4-2026-08-30-D1-02: precombine LOD tie-break relies on max_by_key's last-wins rather than a stated rule (49 of 46,422 objects)

**Source**: `docs/audits/AUDIT_FO4_2026-08-30.md` — Dimension 1
**Severity**: LOW
**Location**: `byroredux/src/cell_loader/precombined.rs` — `build_precombine_meshes`, the `(0..3).max_by_key(|&(c, _)| c)` LOD selection

## Description

Precombine LOD selection picks the LOD with the most triangles via `max_by_key`. Rust's
`max_by_key` returns the **last** maximum, so on a tie the highest LOD index wins **by
accident** rather than by a stated rule.

## Evidence

Verified 2026-08-30 — `byroredux/src/cell_loader/precombined.rs` carries the
`.max_by_key(|&(c, _)| c)` selection (and a sibling `.max_by_key(|&(count, _)| count)`).

MEASURED over the CSG corpus (46,422 shared-geometry objects decoded, 76,498 placed
instances, zero decode errors):

- **49 of 46,422 objects (0.11%)** have two or more LODs sharing the maximum triangle count.
- Selection distribution: LOD0 wins 28,494, LOD1 7,012, LOD2 10,916 — **38.6% of objects
  have their finest triangulation at index 1 or 2**, so the selection itself is doing real
  work and is not a candidate for simplification.

The 49 ties are alternative triangulations of one surface, so they are visually equivalent.

## Impact

Determinism/intent nit, not a rendering defect: the outcome is correct today but depends on
an unstated property of `max_by_key`, which makes it fragile to a refactor that swaps in
`max_by` or an iterator with different tie semantics.

## Suggested Fix

State the tie-break explicitly (e.g. prefer the lowest LOD index on equal triangle counts, or
document that the highest is intended) so the behaviour survives a refactor.

## Related

`a30c088a` (single-LOD handling), #1590 / #2369 (precombine owner + CSG routing).

## Completeness Checks
- [ ] **SIBLING**: the second `max_by_key` in the same file has the same tie exposure — settle both
- [ ] **TESTS**: a regression test pins the chosen tie-break on a synthetic two-LOD-equal object
