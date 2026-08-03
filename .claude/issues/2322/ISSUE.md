# SK-D1-02: #621's VF_FULL_PRECISION back-write rests on a false premise and is a no-op on all vanilla content

**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-08-03.md` (Dimension 1)
**GitHub issue**: #2322

**Severity**: LOW
**Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:529-543`

## Description

The comment justifying `shape.vertex_desc |= (VF_FULL_PRECISION as u64) <<
44` claims the dynamic array "overwrote" packed half-precision positions.
Measured against real data, a `BSDynamicTriShape`'s packed buffer has **no
position field at all** (`VF_VERTEX` clear) and `VF_FULL_PRECISION` is
**already set** in every observed descriptor — the `|=` is a no-op on every
vanilla block, and the rationale is wrong.

## Evidence

Cross-referenced against #2318's finding that `VF_VERTEX` is clear (no
packed position field exists to "overwrite") on every real
`BSDynamicTriShape` observed in `Skyrim - Meshes0.bsa`. Comment confirmed
present at HEAD (1ae86f62).

## Impact

No runtime effect today; stale/misleading rationale that let #2318
(SK-D1-01) survive three prior audits.

## Suggested Fix

Fold into the #2318 fix; drop the `|=` (or correct the comment) and record
the real invariant instead.

## Completeness Checks
- [ ] **SIBLING**: Check for the same stale-rationale pattern in nearby `#621`-tagged comments
- [ ] **TESTS**: A regression test pins the corrected invariant (or is folded into #2318's test)
