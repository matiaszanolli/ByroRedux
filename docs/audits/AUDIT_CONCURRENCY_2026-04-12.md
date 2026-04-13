# Concurrency and Synchronization Audit — 2026-04-12

**Scope**: Full deep audit across 4 dimensions: ECS Locking, Vulkan Sync, Resource Lifecycle, Thread Safety.
**Engine version**: commit `f4f7fa2` (post inverse-viewProj + SSAO reorder).

## Summary

| Severity | Count | Dimensions |
|----------|-------|------------|
| HIGH     | 1     | Resource Lifecycle |
| MEDIUM   | 5     | Vulkan Sync (2), Resource Lifecycle (3) |
| LOW      | 6     | Vulkan Sync (2), Resource Lifecycle (3), Thread Safety (1) |

**1 HIGH**, **5 MEDIUM**, **6 LOW** findings across 12 total. ECS locking and thread safety are well-designed with no concrete bugs found — risks are architectural (no `query_3_mut`, thread-local lock tracker). Vulkan sync has one spec-level barrier gap (composite UBO) and one cross-frame RAW hazard (SVGF history). Resource lifecycle's main issue is the geometry SSBO rebuild destroying buffers that may still be in-flight.

### Dedup Notes
The following existing issues were checked and NOT re-reported:
- #267: Single SSAO AO image shared across frames-in-flight
- #92: update_rgba descriptor set writes race with in-flight command buffers
- #259: Command buffers indexed by swapchain image
- #33: Destruction order in recreate_swapchain vs Drop

---

## HIGH

### C3-08: rebuild_geometry_ssbo destroys old SSBO without device_wait_idle
- **Dimension**: Resource Lifecycle
- **File(s)**: `crates/renderer/src/mesh.rs:208-233`
- **Trigger Conditions**: A mesh is uploaded after initial scene load (sets `geometry_dirty = true`), and the frame loop calls `rebuild_geometry_ssbo`. If this happens while a previous frame's command buffer still references the old global vertex/index SSBO (via scene descriptor set bindings 8 and 9), the destroy creates a use-after-free.
- **Description**: `rebuild_geometry_ssbo` immediately destroys the old `global_vertex_buffer` and `global_index_buffer`, then builds new ones. There is no synchronization — no `device_wait_idle`, no fence wait — to ensure the old buffers are not still referenced by in-flight command buffers. The scene descriptor sets (bindings 8 and 9) point at the old buffers until `write_geometry_buffers` is called with the new ones.
- **Evidence**:
```rust
// mesh.rs:216-222
if let Some(ref mut vb) = self.global_vertex_buffer {
    vb.destroy(device, allocator);  // GPU may still be reading this
}
```
- **Recommendation**: Call `device_wait_idle()` before destroying the old SSBO, or defer destruction for `MAX_FRAMES_IN_FLIGHT` frames using a deferred-destroy queue (similar to TextureRegistry's `pending_destroy` pattern).

---

## MEDIUM

### C2-01: Missing HOST->FRAGMENT_SHADER barrier for composite parameter UBO
- **Dimension**: Vulkan Sync
- **File(s)**: `crates/renderer/src/vulkan/context/draw.rs:587-609`
- **Trigger Conditions**: Every frame. The composite fragment shader reads the params UBO that was host-written in the same frame, with no memory barrier between the write and the read.
- **Description**: `composite.upload_params()` performs a host write to the per-frame composite parameter UBO via `write_mapped`. The composite render pass then starts via `composite.dispatch()`. Both SVGF and SSAO correctly emit HOST->COMPUTE_SHADER barriers for their respective UBO uploads; composite is the only pass that omits this. Per Vulkan spec, host writes require an explicit HOST->consuming-stage barrier even for HOST_COHERENT memory.
- **Recommendation**: Add a HOST->FRAGMENT_SHADER memory barrier between `upload_params` and `composite.dispatch()`:
```rust
let barrier = vk::MemoryBarrier::default()
    .src_access_mask(vk::AccessFlags::HOST_WRITE)
    .dst_access_mask(vk::AccessFlags::UNIFORM_READ);
self.device.cmd_pipeline_barrier(
    cmd, vk::PipelineStageFlags::HOST, vk::PipelineStageFlags::FRAGMENT_SHADER,
    vk::DependencyFlags::empty(), &[barrier], &[], &[],
);
```

### C2-02: SVGF reads previous frame-slot G-buffer without fence synchronization
- **Dimension**: Vulkan Sync
- **File(s)**: `crates/renderer/src/vulkan/svgf.rs:460-477`, `crates/renderer/src/vulkan/context/draw.rs:49-53`
- **Trigger Conditions**: Every frame with SVGF enabled and MAX_FRAMES_IN_FLIGHT=2. Frame N's SVGF dispatch reads G-buffer images from slot `prev = (N+1) % 2`, which was written by frame N-1. The fence wait at draw_frame only waits on `in_flight[N]`, not `in_flight[prev]`.
- **Description**: SVGF's descriptor set binds `mesh_id_views[prev]`. The fence wait guarantees this slot's previous use is complete, but not the other slot's. With sequential submission to the same queue, frame N-1's work may not be complete when frame N's SVGF reads its outputs. This is a spec-level RAW hazard.
- **Recommendation**: Wait on `in_flight[prev]` before the SVGF dispatch when SVGF is enabled, or wait on both in-flight fences at the top of draw_frame (costs nothing in practice since the GPU is rarely more than 1 frame behind).

### C3-01: GBuffer recreate_on_resize leaks partially-allocated attachments on failure
- **Dimension**: Resource Lifecycle
- **File(s)**: `crates/renderer/src/vulkan/gbuffer.rs:283-309`
- **Trigger Conditions**: `recreate_on_resize` destroys all old attachments unconditionally, then allocates new ones sequentially with `?` operators. If allocation fails partway through, already-allocated new attachments leak.
- **Recommendation**: Allocate into temporaries and swap on success, or wrap in a rollback guard.

### C3-02: SVGF recreate_on_resize leaks history images on partial failure
- **Dimension**: Resource Lifecycle
- **File(s)**: `crates/renderer/src/vulkan/svgf.rs:725-777`
- **Trigger Conditions**: Same pattern as C3-01. Old images destroyed first, then new images allocated in a loop. Partial failure leaves allocated images in the vecs without cleanup.
- **Recommendation**: Same as C3-01.

### C3-03: Composite recreate_on_resize leaks HDR images on partial failure
- **Dimension**: Resource Lifecycle
- **File(s)**: `crates/renderer/src/vulkan/composite.rs:645-799`
- **Trigger Conditions**: Same pattern as C3-01/C3-02 but for the composite pipeline's HDR images.
- **Recommendation**: Same as C3-01.

---

## LOW

### C2-03: graphics_queue and present_queue may alias same VkQueue under separate Mutex
- **Dimension**: Vulkan Sync
- **File(s)**: `crates/renderer/src/vulkan/context/mod.rs:130-134`
- **Trigger Conditions**: When graphics and present queue families are the same (common on desktop GPUs), two Mutex instances wrap the same VkQueue handle. The gap between submit and present allows interleaving.
- **Recommendation**: Use a single shared `Mutex<vk::Queue>` when queue families match.

### C2-04: BLAS build via with_one_time_commands blocks the main thread
- **Dimension**: Vulkan Sync
- **File(s)**: `crates/renderer/src/vulkan/acceleration.rs:222-232`
- **Trigger Conditions**: Every `build_blas` call submits a command buffer and blocks on a fence. Causes frame hitches during streaming loads.
- **Recommendation**: Acceptable for scene load; for streaming, batch BLAS builds into the frame's command buffer.

### C3-05: StagingPool Drop warns but does not destroy GPU resources
- **Dimension**: Resource Lifecycle
- **File(s)**: `crates/renderer/src/vulkan/buffer.rs:29-213`
- **Trigger Conditions**: StagingPool dropped without calling `destroy()`. Currently used transiently so low risk.
- **Recommendation**: If ever stored as a VulkanContext field, add to Drop chain.

### C3-09: FrameSync::recreate_for_swapchain does not reset in_flight fences
- **Dimension**: Resource Lifecycle
- **File(s)**: `crates/renderer/src/vulkan/sync.rs:95-123`
- **Trigger Conditions**: Swapchain recreation where image count changes. Verified as correct — fences are per-frame-in-flight (constant count), and device_wait_idle ensures they're signaled.
- **Recommendation**: No action needed. Noted for documentation.

### C3-10: Texture Drop warns but does not destroy — intentional leak-on-drop design
- **Dimension**: Resource Lifecycle
- **File(s)**: `crates/renderer/src/vulkan/texture.rs:583-592`
- **Trigger Conditions**: Texture dropped without `destroy()`. By design — structs don't store device/allocator. VulkanContext Drop chain is the safety net.
- **Recommendation**: Ensure all Texture creation paths register in the registry.

### C4-01: Lock tracker is thread-local — cross-thread deadlocks undetected
- **Dimension**: Thread Safety
- **File(s)**: `crates/core/src/ecs/lock_tracker.rs:20`, `crates/core/src/ecs/scheduler.rs:129-131`
- **Trigger Conditions**: Two parallel systems make separate `query`/`query_mut` calls on the same two component types in different orders. The thread-local tracker on each thread sees only its own locks.
- **Description**: The lock_tracker uses `thread_local!` storage to detect reentrant lock acquisition. When the parallel scheduler dispatches systems via `par_iter_mut().for_each()`, each runs on a separate rayon thread. The tracker cannot detect cross-thread ABBA deadlocks. The TypeId-sorted `query_2_mut` API prevents this for two-component queries, but systems making multiple independent single-type queries are not protected.
- **Recommendation**: (1) Document that parallel systems must use `query_2_mut` for multi-component access. (2) Consider adding a debug-mode global lock order tracker. (3) Long-term: compile-time system parameter declaration (like Bevy's `SystemParam`).

---

## Verified Correct

| Area | Status | Notes |
|------|--------|-------|
| Frame-in-flight fence wait before cmd buffer reuse | PASS | draw.rs:50-53 |
| Semaphore acquire→render→present chain | PASS | Correct ordering |
| TLAS build barrier (AS_WRITE→AS_READ) | PASS | draw.rs:157-168 |
| SVGF compute→composite barriers | PASS | svgf.rs:692-716 |
| SSAO barriers (depth read, AO write→read) | PASS | ssao.rs:525-546 |
| Swapchain recreate device_wait_idle | PASS | resize.rs:19 |
| GpuCamera UBO size (Rust=GLSL=256 bytes) | PASS | 3× mat4 + 4× vec4 |
| VulkanContext Drop reverse-order destruction | PASS | Correct chain |
| Allocator freed last | PASS | `take()` pattern |
| Component/Resource Send+Sync bounds | PASS | Trait-level enforcement |
| System trait Send+Sync bounds | PASS | Required for rayon |
| No raw pointers in ECS components | PASS | Grep confirmed |
| Storage access via guards only | PASS | RwLock guards |
| UiManager kept outside ECS (not Send+Sync) | PASS | Correct design |
| Vulkan queue Mutex wrapping | PASS | Both queues locked |
| TypeId-sorted multi-component queries | PASS | query_2_mut/query_2_mut_mut |
