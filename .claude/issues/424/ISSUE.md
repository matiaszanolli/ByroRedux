# REN-MEM-C2: BLAS scratch buffers allocated per-build — heap fragmentation + allocator contention

**Issue**: #424 — https://github.com/matiaszanolli/ByroRedux/issues/424
**Labels**: bug, renderer, critical, performance

---

## Finding

`crates/renderer/src/vulkan/acceleration.rs` — each BLAS build allocates a fresh scratch buffer sized to `build_sizes.build_scratch_size`, uses it once inside the build command, and drops it when the `BlasEntry` is finalized.

The batched-BLAS path added in M31 amortizes the command submission but still allocates **one scratch per BLAS**. A cell with ~500 unique meshes = 500 transient `GpuOnly` allocations funneled through gpu-allocator in one frame.

## Impact

- **Frame hitches on cell-load frames**: allocator lock contention (MEM-H1) compounds as every BLAS build takes the shared mutex.
- **Heap fragmentation**: gpu-allocator's sub-allocation free lists are not designed for this churn. The DEVICE_LOCAL heap fragments.
- **Long-tail risk of `VK_ERROR_OUT_OF_DEVICE_MEMORY`** on cells with heavy BLAS churn.
- The BLAS LRU eviction path (also M31) compounds the churn because evicted-then-revived meshes re-allocate scratch every time.

## Fix

Keep a single **growable scratch buffer per frame-in-flight**, sized to `max(build_scratch_size)` across the current batch. Reset-reuse instead of alloc-free.

Pattern:
```rust
struct BlasScratchPool {
    per_frame: [GpuBuffer; MAX_FRAMES_IN_FLIGHT],
    capacity: [u64; MAX_FRAMES_IN_FLIGHT],
}
impl BlasScratchPool {
    fn ensure_capacity(&mut self, frame: usize, required: u64) -> DeviceAddress {
        if self.capacity[frame] < required {
            // grow with max(required, capacity[frame] * 2) and rebuild
            // GPU-only buffer with SHADER_DEVICE_ADDRESS + STORAGE_BUFFER
        }
        self.per_frame[frame].device_address
    }
}
```

This is the pattern used by every shipped RT renderer (UE5 Lumen, RTXGI).

## Dependencies

- Interacts with MEM-H1 (single-mutex allocator). If MEM-H1 is fixed via a frame-scoped arena, scratch pooling becomes part of the arena's responsibility.
- Interacts with AS-M4 (batched build blocks the graphics queue). Scratch pooling is a prerequisite for chunked submission.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check TLAS scratch buffer handling in `acceleration.rs` — if also per-build, apply the same pooling.
- [ ] **DROP**: Scratch buffer lifetime tied to `AccelerationManager` — destroyed with it in `VulkanContext` Drop (after wait_idle).
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Load a 500-mesh cell; verify scratch buffer count stays at `MAX_FRAMES_IN_FLIGHT` (not 500). Allocator report shows no BLAS scratch allocations after first cell load.

## Source

Audit: `docs/audits/AUDIT_RENDERER_2026-04-18.md`, Dim 2 C2. Part of the memory-shape trilogy.
