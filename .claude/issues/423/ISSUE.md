# REN-MEM-C1: GpuBuffer uses HOST_VISIBLE|HOST_COHERENT for all vertex/index data — exhausts BAR heap

**Issue**: #423 — https://github.com/matiaszanolli/ByroRedux/issues/423
**Labels**: bug, renderer, critical, performance

---

## Finding

`crates/renderer/src/vulkan/buffer.rs` — every `GpuBuffer::new` allocation requests `MemoryLocation::CpuToGpu`. That includes the per-mesh vertex and index buffers populated by `MeshRegistry`.

On discrete GPUs, `CpuToGpu` lands in the **small (~256 MB on most NVIDIA parts) BAR / pinned-host-visible heap**, not `DEVICE_LOCAL` VRAM.

## Impact

Two distinct problems:

1. **Permanent bandwidth tax**: every draw fetches vertex data across PCIe instead of from VRAM. 5-10× vertex-fetch bandwidth vs the `GpuOnly` path.
2. **Hard allocation ceiling well below the 4 GB budget**: a cell like Anvil Heinrich Oaken Halls or a Starfield interior with thousands of unique NIF meshes will exhaust the ~256 MB BAR heap long before VRAM runs out. gpu-allocator either spills back to system memory (killing draw throughput) or fails outright.

This predates current milestones. The pattern for textures (`vulkan/texture.rs`) already uses stage-then-copy; vertex/index data just never got the same treatment.

## Fix

Stage once, copy to a `GpuOnly` buffer with `TRANSFER_DST | VERTEX_BUFFER | INDEX_BUFFER | STORAGE_BUFFER` usage, release the staging buffer after the upload fence signals. Identical pattern to texture uploads.

Steps:
1. Extend `GpuBuffer::new` (or add `GpuBuffer::new_device_local`) that takes a byte slice and performs stage→copy internally.
2. Migrate `MeshRegistry` vertex/index allocations to use the device-local path.
3. Migrate any other hot vertex/index buffer sites (UI quad, cube/triangle/quad helpers in `mesh.rs`) after verifying they're not updated per-frame.
4. Keep `CpuToGpu` for genuinely per-frame-updated UBOs (scene, instance SSBO if updated per-frame).

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit every `GpuBuffer::new` call site for whether its data is truly per-frame (keep HOST) or effectively static (migrate). Check `MEM-H2` (staging churn) — fixing C1 naturally exposes the need for a ring-buffer staging allocator.
- [ ] **DROP**: Staging buffer lifecycle — free after upload fence signals. Don't leak per-upload buffers.
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Load a 500+ unique-mesh cell with BAR heap monitoring. Before fix: BAR usage linear in mesh count. After fix: BAR usage bounded by staging ring size (~64 MB).

## Source

Audit: `docs/audits/AUDIT_RENDERER_2026-04-18.md`, Dim 2 C1. Part of the memory-shape trilogy (MEM-C1/C2/C3).
