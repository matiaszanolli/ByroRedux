# REN-D10-01: cluster_cull.comp differences two ABSOLUTE positions for ray direction

**Severity**: HIGH
**Dimension**: 10 — Camera-Relative Precision
**Location**: `crates/renderer/shaders/cluster_cull.comp` — `ndcToWorld`, and the `nearCorners`/`rayDir`/`corners` block in `main`

## Description

`ndcToWorld` correctly lifts render-origin-relative coordinates to absolute for the cluster AABB, but the very next operation is a small difference (`normalize(nearCorners[i] - camPos)`) between two large-magnitude absolutes. The near plane is only ~0.1 world units wide; at Markarth-scale origins (`|world| ≈ 176000`) that's below one f32 ULP.

## Evidence

Reproduced the shader arithmetic in f32 for 17 tile boundaries: adjacent boundaries collapse onto identical floats (zero-width frustum voxels); where they don't collapse, angular error reaches ~4.5° against a tile's own ~5.3° size.

## Impact

`sphereIntersectsAABB` under-reports lights per tile — point/spot lights silently drop out in per-tile patches. Directional/sun unaffected. Degrades gradually with origin magnitude: ~10% at 16k, ~42% at 65k, total collapse ≥131k. This week's LOD work (far plane now 400000) pushes exterior content routinely into the affected regime.

## Suggested Fix

Take the difference in relative space, then lift once: `camRel = cameraPos - renderOrigin` (exact in f32), `rayDir = normalize(nearCornerRel - camRel)`, then lift to absolute for the AABB. Pure shader arithmetic reordering, no Vulkan state change.

## Related

Same class as #1490, #1642, #1488

Filed from `docs/audits/AUDIT_RENDERER_2026-08-12b.md` (finding REN-D10-01, Cluster D-1).
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2744
