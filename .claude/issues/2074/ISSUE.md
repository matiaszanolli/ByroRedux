# TD3-102: MaterialTable VRAM-budget comment mixes decimal-MB and binary-MiB arithmetic inconsistently

**GitHub Issue**: #2074
**Labels**: low,vulkan,tech-debt,documentation

**Severity**: LOW
**Dimension**: 3 (Stale Documentation & Comments)
**Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs:114-116,167-169`

## Description
`MAX_INSTANCES` comment uses decimal MB (29.4 MB); the materials-table comment two blocks down computes the same way but the arithmetic is actually binary MiB (4.7 MB where decimal MB would read 4.9 MB). Both struct sizes are correctly pinned; only the comment's unit convention is inconsistent between the two blocks.

## Evidence
Confirmed live: the `MAX_INSTANCES` doc comment reads `262144 × sizeof(GpuInstance) = 262144 × 112 B = 29.4 MB / frame` (262144×112 = 29,360,128 bytes = 29.36 MB decimal — consistent with decimal MB). The materials-table comment reads `16384 × 300 B ≈ 4.7 MB per frame` — 16384×300 = 4,915,200 bytes = 4.92 MB decimal, but 4.69 MiB binary; the comment's "4.7 MB" figure is actually the binary-MiB value mislabeled as MB.

## Impact
Cosmetic — doesn't change the "well within budget" conclusion.

## Suggested Fix
Recompute the materials-table comment in decimal MB: `16384 × 300 B ≈ 4.9 MB per frame × 2 ≈ 9.8 MB total`.

**Effort**: trivial

## Completeness Checks
- [ ] **TESTS**: N/A (comment-only fix)
