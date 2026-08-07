title:	SK-D1-02: #621's VF_FULL_PRECISION back-write rests on a false premise and is a no-op on all vanilla content
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, legacy-compat, low, nif-parser
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	2322
--
**Severity**: LOW
**Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:529-543`

## Description

The comment justifying `shape.vertex_desc |= (VF_FULL_PRECISION as u64) <<
44` claims the dynamic array "overwrote" packed half-precision positions.
Measured against real data, a `BSDynamicTriShape`'s packed buffer has **no
position field at all** (`VF_VERTEX` clear) and `VF_FULL_PRECISION` is
**already set** in every observed descriptor — the `|=` is a no-op on every
vanilla block, and the rationale is wrong. It also actively misleads a
future reader into thinking the packed buffer holds positions.

## Evidence

```rust
// crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:531
// #621 / SK-D1-04: the dynamic Vector4 array is full-
// precision f32 — it overwrote the (typically packed
// half-precision on FO4+ facegen) positions. Update
// `vertex_desc` so downstream consumers reading
// `vertex_attrs & VF_FULL_PRECISION` see the post-
// overwrite reality. ...
shape.vertex_desc |= (VF_FULL_PRECISION as u64) << 44;
```

Cross-referenced against SK-D1-01's finding that `VF_VERTEX` is clear (no
packed position field exists to "overwrite") on every real
`BSDynamicTriShape` observed in `Skyrim - Meshes0.bsa`.

## Impact

No runtime effect today; stale/misleading rationale that let SK-D1-01 (see
the sibling HIGH issue in this same audit) survive three prior audits.

## Suggested Fix

Fold into the SK-D1-01 fix; drop the `|=` (or correct the comment) and
record the real invariant instead (packed buffer has no position field when
`VF_VERTEX` is clear).

## Completeness Checks
- [ ] **SIBLING**: Check for the same stale-rationale pattern in nearby `#621`-tagged comments
- [ ] **TESTS**: A regression test pins the corrected invariant (or is folded into SK-D1-01's test)

