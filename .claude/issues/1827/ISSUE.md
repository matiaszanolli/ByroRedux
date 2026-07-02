# 1827: FO4-D4-02: Starfield BSGeometry leaves per-vertex bone indices/weights empty (informational, out of FO4 scope)

URL: https://github.com/matiaszanolli/ByroRedux/issues/1827
Labels: bug, nif-parser, low, legacy-compat

**Severity**: LOW
**Dimension**: 4 — NIF BSVER 130 (adjacent; Starfield, not FO4)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:169-173`
**Status**: NEW (documented gap, not an FO4 defect)

## Description

For the FO4 path (`BsTriShape`) skinned bone indices/weights honor the packed
layout correctly. The Starfield `BSGeometry` sibling resolves the skin chain
for bind matrices but intentionally leaves per-vertex bone indices/weights
empty. Raised only because the Dim-4 checklist phrases "skinned" broadly —
this is BSVER 172 (Starfield), not BSVER 130 (FO4), and is out of this
report's FO4 scope, but is filed for tracking since it's a real, confirmed gap.

## Evidence

- `extract_skin_bs_geometry` returns bind data only; the packed `BSGeometry` vertex bone channel is not decoded — see the in-code comment at `bs_geometry.rs:169-172` ("Per-vertex bone indices + weights are intentionally left empty here — the BSGeometry parser doesn't surface them yet (separate work)").

## Impact

Starfield skinned meshes render in bind pose. Zero FO4 impact.

## Suggested Fix

Track as Starfield skinning work (separate milestone) — decode the packed
`BSGeometry` per-vertex bone index/weight channel analogous to the FO4
`BsTriShape` path.

## Completeness Checks
- [ ] **SIBLING**: Compare against the FO4 `BsTriShape` packed bone-index/weight decode to confirm the same layout applies to `BSGeometry`.
- [ ] **TESTS**: A regression test pins non-empty bone indices/weights on a real Starfield skinned mesh once implemented.

