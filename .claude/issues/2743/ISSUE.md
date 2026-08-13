# REN-D9-2026-08-12-02: skin-compute descriptor cache treats a raw vk::Buffer handle as stable identity

**Severity**: HIGH
**Dimension**: 9 — Skinning / Memory-Lifecycle
**Location**: `crates/renderer/src/vulkan/skin_compute.rs` — `SkinSlot::descriptor_bindings`, `SkinComputePipeline::dispatch`; interacts with `crates/renderer/src/mesh.rs` — `MeshRegistry::rebuild_geometry_ssbo`

## Description

`#1197` skips `vkUpdateDescriptorSets` when the live `(input_buffer, bone_buffer)` handle pair equals the cached key. Vulkan does not guarantee non-dispatchable handles are non-recycled — `rebuild_geometry_ssbo`'s `reclaim_before_rebuild` branch destroys and reallocates the global vertex SSBO inside the same call, the maximum-probability recycle window. Nothing invalidates the cache externally.

## Evidence

The RT side already treats this exact buffer as un-cacheable: `draw.rs` re-points bindings 8/9 every frame unconditionally, citing the same device-loss hazard in a comment (WATAL §0). No epoch counter exists in `MeshRegistry` to key against.

## Impact

A stale descriptor makes `skin_vertices.comp` read freed memory, feeding both AS build input (`build_skinned_blas_batched_on_cmd`) and shader reads (`GpuInstance.skinnedVertexAddress`) — garbage geometry up to device loss.

## Suggested Fix

Add a monotonic `geometry_generation: u64` to `MeshRegistry`, fold into the cache key — or drop the compare-and-skip for binding 0. CPU-side bookkeeping fix, no barrier/stage change needed.

## Related

#1197, #1782, #2374

Filed from `docs/audits/AUDIT_RENDERER_2026-08-12b.md` (finding REN-D9-2026-08-12-02).
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2743
