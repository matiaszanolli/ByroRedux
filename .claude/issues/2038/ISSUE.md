# GPU-D5-INFO-01: Volumetrics casts ~1.8M per-froxel shadow rays/frame with no temporal reprojection yet

**Labels**: low, performance, info

**Severity**: LOW (informational — a documented M55 Phase 5 roadmap gap, not a regression; not actionable as a bug today)
**Dimension**: GPU Pipeline
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## Location
`crates/renderer/src/vulkan/volumetrics.rs:916-919`, `crates/renderer/shaders/volumetrics_inject.comp:177-189`

## Description
Volumetrics casts ~1.8M per-froxel shadow rays/frame with no temporal reprojection yet. O(froxels), correctly not scaling with mesh count, but paid in full every frame because temporal reprojection is an explicitly-future phase (M55 Phase 5).

Verified current: `volumetrics_inject.comp` still casts a full per-froxel shadow ray every frame with no temporal accumulation/reprojection buffer.

## Impact
Recorded for the record as the largest per-froxel cost lever once Phase 5 lands. Not a bug — this is filed purely to keep the roadmap gap visible and cross-referenced from the performance-audit trail.

## Suggested Fix
No action needed until M55 Phase 5 (temporal reprojection for volumetrics) is scheduled. Tracked here so the cost lever isn't lost between audit passes.

## Completeness Checks
- [ ] **TESTS**: N/A — roadmap/future-work tracking item, not an active bug
