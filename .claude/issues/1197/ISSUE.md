# #1197 — PERF-DIM7-03: per-dispatch descriptor-set rewrite for skin compute

**Source**: docs/audits/AUDIT_PERFORMANCE_2026-05-19.md (Dim 7, MEDIUM)
**Severity**: medium
**Labels**: bug, medium, renderer, performance, vulkan, M29
**State**: OPEN (filed 2026-05-19)

## Cause

crates/renderer/src/vulkan/skin_compute.rs:425-446 — `SkinComputePipeline::dispatch` calls `update_descriptor_sets` with 3 writes (input/palette/output) on every dispatch. 102 writes / frame at 34 entities on Prospector.

## Fix

Move the 3 writes from `dispatch` to `mark_slot_resident`:
- Once on slot creation
- Once per cell transition (when global vertex SSBO changes — hook into `MeshRegistry::rebuild_geometry_ssbo` exit)

Keep per-FIF descriptor set array; rewrite only the FIF index entering a new cell.

## Risk

MEDIUM — stale descriptor read if cell transition bumps vertex buffer without triggering re-write. Mitigation: hook into the same generation-counter site used for static-BLAS-map invalidation.

## Estimated impact

~100-300 µs / frame at 34 entities. Below FPS-signal but moves the path closer to static-mesh overhead profile.

## Sibling check

Does `SkinPaletteComputePipeline` (skin_compute.rs:533-655) have the same per-dispatch rewrite pattern? Apply the same fix if so.
