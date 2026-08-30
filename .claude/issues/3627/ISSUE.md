# #3627: OBL-D7-03: placement_lod.rs' module doc understates its own validation — 9889/9889 exact-consume, no toddland outlier

**Source**: `docs/audits/AUDIT_OBLIVION_2026-08-30.md` — Dimension 7 (Exterior Blocker Chain)
**Severity**: LOW
**Location**: `byroredux/src/cell_loader/placement_lod.rs` — module doc

## Description

The module doc understates its own validation: it claims the SoA layout consumes
*"9888/9889 files exactly (the lone outlier is `toddland`, the CS tutorial world, whose LOD
data is degenerate)"*. Re-measured today, there is no outlier.

## Evidence

Current doc (verified 2026-08-30):

> Validation across the corpus: the SoA layout consumes 9888/9889 files exactly (the lone
> outlier is `toddland`, the CS tutorial world, whose LOD data is degenerate); rotations are
> all within ±2π rad; scales are all positive.

Replicating `parse_placement_lod`'s SoA walk over all 9,889 vanilla `.lod` files in
`Oblivion - Meshes.bsa`:

```
exact-consume = 9889/9889   trailing_bytes = 0   overrun = 0   errors = 0
num_groups == 0 in 0 files
distinct base_form_ids = 208   total placements = 99,591
rotations outside ±2π = 1   non-positive scales = 0
```

The other two claims in the same sentence hold (scales all positive; the single
out-of-±2π rotation is a rotation-range note, not a consume failure).

## Impact

Harmless but pessimistic: the doc records a known-bad file that is no longer bad, which
invites a future reader to treat `toddland` as an accepted defect and to under-trust the
parser's exact-consume guarantee.

## Suggested Fix

One-line doc correction: 9889/9889 exact-consume, zero outliers, re-measured 2026-08-30.

## Related

Same file: `PLACEMENT_LOD_RADIUS_CELLS`' broken intra-doc link is filed separately from the
Skyrim report. #3518 (`parse_placement_lod` unvalidated group-count allocation) is a distinct
defect in the same function and is unaffected by this doc change.

## Completeness Checks
- [ ] **TESTS**: doc-only; if a corpus assertion is wanted, the 9889/9889 figure is the pin
