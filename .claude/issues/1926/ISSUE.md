# REN-D8-01: Composite fog fallback branch is dead code post-VOLUMETRIC_OUTPUT_CONSUMED flip

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1926

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/shaders/composite.frag:510-549` (fog fallback), `:29-54` (fog_color/fog_params UBO fields); `crates/renderer/src/vulkan/volumetrics.rs:154` (`VOLUMETRIC_OUTPUT_CONSUMED = true`); `crates/renderer/src/vulkan/context/draw.rs:3510` (host mirror)
**Status**: NEW

## Description
The composite aerial-perspective + XCLL cubic fog fallback branch is gated `params.depth_params.x > 0.5 && depth < 0.9999 && params.depth_params.z < 0.5`. Since `VOLUMETRIC_OUTPUT_CONSUMED` is now `true`, `depth_params.z` is pinned to `1.0`, so `depth_params.z < 0.5` is permanently false and the whole branch is dead code. `fog_color` and `fog_params` are uploaded every frame but consumed by nothing.

## Evidence
The author's own comment at `composite.frag:486-491` explicitly states to drop this branch in lockstep with flipping `VOLUMETRIC_OUTPUT_CONSUMED = true`. The flip happened; the branch removal did not.

## Impact
No visual regression today (distance haze is carried by the volumetric froxel path instead). Dead code + wasted per-frame UBO writes, and a stale "restore this in lockstep" comment that is now misleading.

## Related
REN-D3-01 (composite.frag.spv staleness — the stale spv still contains this branch but it's dead there too)

## Suggested Fix
Remove the fog fallback branch per the author's own lockstep note; if `fog_color`/`fog_params` are being kept for a future density-tint feature, annotate them as reserved-and-unconsumed.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
