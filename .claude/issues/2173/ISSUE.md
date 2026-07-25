# 2173: PERF-D1-03: Draw-sort parallel threshold (2000) calibration predates the 10-to-11-tuple sort-key widening

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2173
**Labels**: bug, low, performance

---

## Severity
LOW

## Dimension
CPU Hot Paths (Dim 1) — `/audit-performance` 2026-07-25

## Location
`crates/renderer/src/vulkan/context/draw.rs` (draw-sort parallel-threshold constant)

## Description
The draw-sort parallel-dispatch threshold (2000 draws) was calibrated against the pre-`883f57cd` 10-tuple sort key. Commit `883f57cd` widened the sort key to 11 tuples (adding the stable surface ID), increasing per-comparison cost, but the threshold constant was not re-measured against the new key width.

## Impact
Potentially suboptimal parallel/serial crossover point for the draw sort on large scenes (MedTek-scale, ~14.5K draws). Not a correctness issue, purely a calibration staleness question.

## Related
Commit `883f57cd` (the sort-key widening).

## Suggested Fix
Re-measure the parallel/serial crossover with the current 11-tuple key on a representative large-batch scene (MedTek) and adjust the threshold constant if the crossover point moved meaningfully.

## Completeness Checks
- [ ] **TESTS**: Existing draw-sort tests unaffected; recommend a one-off bench comparison, not a new automated test
