# DIM12-01: cluster_cull.comp missing from the documented per-frame submission-order list

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1931

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `docs/engine/shader-pipeline.md` (Per-Frame Submission Order block)
**Status**: NEW

## Description
`cluster_cull.comp` is absent from the numbered per-frame submission-order list even though it records every frame between the TLAS build and the main render pass. The list jumps from step 3 (AS build) directly to step 4 (main render pass). The `977eb95a` volumetrics rewrite additionally made `volumetrics_inject` consume cluster_cull's cluster-grid/light-index buffers — a new cross-pass data dependency the doc also does not mention.

## Evidence
`cluster_cull` dispatches at `draw.rs:2996-3029` after the TLAS build and before `record_geometry_pass`. Doc mentions cluster_cull only in the shader-file and descriptor tables, never in the ordered list.

## Impact
Documentation-only; no runtime effect. A future contributor reading the doc as authoritative would miss the ordering and the new volumetrics dependency — the exact ordering reasoning this class of audit dimension relies on.

## Related
commit `977eb95a` (volumetrics Phase-2b)

## Suggested Fix
Insert a "cluster_cull.comp" step between the AS-build and main-render-pass entries in the numbered list; note volumetrics_inject's dependency on its output.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
