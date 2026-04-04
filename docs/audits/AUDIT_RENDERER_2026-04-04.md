# Renderer Audit — 2026-04-04

**Auditor**: Renderer Specialist (Claude Opus 4.6)
**Commit**: 286fd89 (main)
**Scope**: Full Vulkan renderer — 7 dimensions

---

## Executive Summary

| Severity | New | Existing | Total |
|----------|-----|----------|-------|
| CRITICAL | 0   | 0        | 0     |
| HIGH     | 2   | 1        | 3     |
| MEDIUM   | 7   | 2        | 9     |
| LOW      | 7   | 1        | 8     |
| **Total**| **16**| **4**  | **20**|

**Key concerns:**
1. **RT performance** (R-5/R-6/R-25): Acceleration structure buffers in HOST_VISIBLE memory forces ray traversal over PCIe on discrete GPUs.
2. **TLAS instance barrier** (R-2): Missing HOST → AS_BUILD memory barrier before TLAS build could cause stale instance data on some drivers.
3. **Texture destroy order** (R-7/R-24): Allocation freed before VkImage destroyed — existing issue #18.

**Verified correct:** Vertex input layout, push constant offsets, descriptor set bindings, SSBO/UBO alignment, render pass attachment config, Drop impl destruction order, framebuffer/render-pass ordering, command pool RESET flag.

---

## Dimension 1: Vulkan Synchronization

### R-1: No host-visible memory flush after SSBO/UBO mapped writes
- **Severity**: HIGH
- **Dimension**: Vulkan Synchronization
- **Location**: `crates/renderer/src/vulkan/context.rs:464-484`
- **Status**: NEW
- **Description**: `upload_lights` and `upload_camera` write to host-visible mapped memory via `write_mapped()` with no `vkFlushMappedMemoryRanges`. gpu-allocator's `CpuToGpu` prefers but does not mandate `HOST_COHERENT`. On non-coherent memory (some mobile/integrated GPUs), the GPU may read stale data.
- **Evidence**: `GpuBuffer::write_mapped()` at `buffer.rs:106-118` writes directly to mapped slice with no flush. Buffers created at `scene_buffer.rs:80-91` with `MemoryLocation::CpuToGpu`.
- **Impact**: Rendering artifacts on non-coherent memory devices. Practically unlikely on desktop but spec-non-compliant.
- **Suggested Fix**: Assert `HOST_COHERENT` at allocation time, or add explicit flush after mapped writes.

### R-2: TLAS instance buffer written without memory barrier before AS build
- **Severity**: HIGH
- **Dimension**: Vulkan Synchronization
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:337`
- **Status**: NEW
- **Description**: Instance buffer written via `write_mapped()` then immediately consumed by `cmd_build_acceleration_structures` with no pipeline barrier. The Vulkan spec requires a HOST → AS_BUILD dependency.
- **Evidence**: `acceleration.rs:337` writes instances, `acceleration.rs:372` records build. No barrier between. The existing TLAS→fragment barrier only covers build output.
- **Impact**: TLAS build may read stale instance data on some implementations, causing incorrect shadows or GPU hangs.
- **Suggested Fix**: Insert barrier: `srcAccess=HOST_WRITE`, `dstAccess=ACCELERATION_STRUCTURE_BUILD_INPUT_READ_KHR`, `srcStage=HOST`, `dstStage=ACCELERATION_STRUCTURE_BUILD_KHR`.

### R-3: Present queue not wrapped in Mutex
- **Severity**: MEDIUM
- **Dimension**: Vulkan Synchronization
- **Location**: `crates/renderer/src/vulkan/context.rs:668-678`
- **Status**: NEW
- **Description**: `graphics_queue` is wrapped in `Mutex`, but `present_queue` is a bare `vk::Queue`. Safe today by single-threaded access, but fragile if multi-threading is added.
- **Evidence**: `context.rs:71` (Mutex) vs `context.rs:72` (bare).
- **Impact**: No immediate bug. Future multi-threading hazard.
- **Suggested Fix**: Wrap `present_queue` in `Mutex` or document single-thread invariant.

### R-4: Swapchain recreation does not pass old_swapchain
- **Severity**: LOW
- **Dimension**: Vulkan Synchronization
- **Location**: `crates/renderer/src/vulkan/swapchain.rs:82`
- **Status**: NEW
- **Description**: Always passes `old_swapchain: null`. Old swapchain destroyed before new one created. Misses atomic handoff optimization.
- **Impact**: Brief flicker during resize on some platforms.
- **Suggested Fix**: Pass old swapchain handle, create new first, then destroy old.

---

## Dimension 2: GPU Memory

### R-5: BLAS/TLAS result buffers use HOST_VISIBLE instead of DEVICE_LOCAL
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:114-120`, `:297-303`
- **Status**: NEW
- **Description**: AS result buffers allocated via `create_host_visible` (CpuToGpu). These are purely GPU-populated and GPU-read. On discrete GPUs, AS traversal goes over PCIe.
- **Impact**: Severe RT shadow performance penalty on discrete GPUs.
- **Suggested Fix**: Add `GpuBuffer::create_device_local` for AS result buffers.

### R-6: Scratch buffers use HOST_VISIBLE memory
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:135-140`, `:319-324`
- **Status**: NEW
- **Description**: BLAS/TLAS scratch buffers are temporary GPU workspace, never host-accessed, but allocated as `CpuToGpu`.
- **Impact**: Suboptimal AS build performance. Wastes limited HOST_VISIBLE memory.
- **Suggested Fix**: Use `GpuOnly` for scratch buffers.

### R-7: Texture::destroy frees allocation before destroying VkImage
- **Severity**: HIGH
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/texture.rs:585-601`
- **Status**: Existing: #18
- **Description**: `allocator.free(alloc)` called before `device.destroy_image(self.image, None)`. Per Vulkan spec, image must be destroyed before backing memory is freed.

### R-8: TLAS rebuild calls device_wait_idle mid-frame
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:247`
- **Status**: NEW
- **Description**: When TLAS instance count exceeds capacity, `device_wait_idle()` is called during command buffer recording. Stalls entire GPU mid-frame.
- **Impact**: 5-20ms frame hitch on first scene load or cell transition.
- **Suggested Fix**: Pre-size TLAS. Implement deferred cleanup queue for old TLAS.

### R-9: No explicit barrier for host SSBO/UBO writes
- **Severity**: MEDIUM
- **Dimension**: Vulkan Synchronization
- **Location**: `crates/renderer/src/vulkan/context.rs:464-494`
- **Status**: NEW
- **Description**: No explicit HOST → FRAGMENT_SHADER barrier for SSBO/UBO writes. Spec-correct due to implicit `vkQueueSubmit` host-write availability, but subtle.
- **Impact**: Functionally correct. Documentation/clarity concern.
- **Suggested Fix**: Add comment documenting reliance on implicit submit guarantee.

### R-10: Destruction order inconsistent between recreate_swapchain and Drop
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/context.rs:760-768`
- **Status**: NEW
- **Description**: `recreate_swapchain` and `Drop` use different destruction orders. Both are currently correct, but inconsistency is a maintenance hazard.
- **Suggested Fix**: Align orders or extract shared helper.

### R-11: One-time commands use queue_wait_idle
- **Severity**: MEDIUM
- **Dimension**: Vulkan Synchronization
- **Location**: `crates/renderer/src/vulkan/texture.rs:658-660`
- **Status**: Existing: #11

### R-12: No GPU memory budget tracking
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/allocator.rs`
- **Status**: Existing: #20

### R-13: Depth store DONT_CARE
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/context.rs:960`
- **Status**: Existing: #19

---

## Dimension 3: Pipeline State

### R-14: All four pipeline rasterizers identical — two-sided pipelines are dead duplicates
- **Severity**: MEDIUM
- **Dimension**: Pipeline State
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:105-121`
- **Status**: NEW
- **Description**: Both `rasterizer` and `rasterizer_no_cull` use `CullModeFlags::NONE`. Four pipeline objects created but only two unique configurations exist.
- **Impact**: No visual effect from two-sided pipeline switch. When backface culling is enabled, will silently start culling geometry not flagged two-sided.
- **Suggested Fix**: Set `rasterizer` to `CULL_MODE_BACK` or make cull mode a dynamic state.

### R-15: Per-vertex inverse(mat3) can produce NaN for degenerate matrices
- **Severity**: MEDIUM
- **Dimension**: Pipeline State
- **Location**: `crates/renderer/shaders/triangle.vert:25`
- **Status**: NEW
- **Description**: `transpose(inverse(mat3(pc.model)))` per vertex. If model matrix has zero scale on any axis (NIF placeholder nodes, animated transitions), `inverse()` divides by zero producing NaN normals.
- **Impact**: Visual corruption on zero-scale meshes.
- **Suggested Fix**: Compute normal matrix on CPU, or guard against zero determinant in shader.

---

## Dimension 4: Render Pass

### R-16: Subpass dependency omits LATE_FRAGMENT_TESTS
- **Severity**: LOW
- **Dimension**: Render Pass
- **Location**: `crates/renderer/src/vulkan/context.rs:985-1000`
- **Status**: NEW
- **Description**: `dst_stage_mask` includes `EARLY_FRAGMENT_TESTS` but not `LATE_FRAGMENT_TESTS`. Fragment shader uses `discard`, which may defer depth writes to late test stage.
- **Impact**: Technically a spec gap. Benign on desktop; could cause depth corruption on strict implementations.
- **Suggested Fix**: Add `LATE_FRAGMENT_TESTS` to both src and dst stage masks.

### R-17: Hardcoded D32_SFLOAT without format support query
- **Severity**: LOW
- **Dimension**: Render Pass
- **Location**: `crates/renderer/src/vulkan/context.rs:23`
- **Status**: NEW
- **Description**: Depth format is `const D32_SFLOAT` with no `get_physical_device_format_properties` call. Universally supported on desktop but not spec-mandated.
- **Impact**: Would crash on a device not supporting D32_SFLOAT as depth attachment.
- **Suggested Fix**: Query format properties, fallback through D32→D32S8→D24S8→D16.

---

## Dimension 5: Command Buffer Recording

### R-18: Host writes inside active render pass recording block
- **Severity**: LOW
- **Dimension**: Command Buffer Recording
- **Location**: `crates/renderer/src/vulkan/context.rs:464-484`
- **Status**: NEW
- **Description**: `upload_lights`/`upload_camera` host writes occur between `cmd_begin_render_pass` and draw calls. Valid (CPU-side ops, not commands), but fragile and misleading.
- **Suggested Fix**: Move uploads before `cmd_begin_render_pass`.

### R-19: TLAS rebuild device_wait_idle mid-frame (dup of R-8)
- **Severity**: MEDIUM
- **Dimension**: Command Buffer Recording
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:247`
- **Status**: NEW (see R-8)

---

## Dimension 6: Shader Correctness

### R-20: Per-vertex inverse(mat3) is redundant work (dup of R-15)
- **Severity**: MEDIUM
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/shaders/triangle.vert:25`
- **Status**: NEW (see R-15)
- **Note**: Same root cause as R-15 but from performance angle: 100K redundant 3x3 inversions per frame.

### R-21: UI pipeline dead DEPTH_BIAS dynamic state
- **Severity**: LOW
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:357-360`
- **Status**: NEW
- **Description**: UI pipeline has `depth_bias_enable(false)` but declares `DEPTH_BIAS` as dynamic state. No effect.
- **Suggested Fix**: Remove `DEPTH_BIAS` from UI pipeline dynamic states.

### R-22: Shader compilation and layout validation — PASS
- All shaders compile cleanly. Push constants, vertex inputs, SSBO/UBO layouts match between GLSL and Rust.

---

## Dimension 7: Resource Lifecycle

### R-23: Redundant pipeline layout/module create+destroy on resize
- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context.rs:792-808`
- **Status**: NEW
- **Description**: `recreate_swapchain` creates new pipeline layout and shader modules then immediately destroys them. Wasteful but harmless.
- **Suggested Fix**: Refactor to accept existing layout/modules on resize.

### R-24: Drop impl destruction order — PASS
- Full audit confirms correct reverse-creation order: sync → commands → framebuffers → textures/scene/accel → depth → meshes → pipelines → render pass → swapchain → allocator → device.

### R-25: AS buffers in HOST_VISIBLE memory (dup of R-5/R-6)
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:114-120`, `:297-303`
- **Status**: NEW (see R-5/R-6)

---

## Prioritized Fix Order

1. **R-5/R-6/R-25**: AS memory → DEVICE_LOCAL (biggest RT performance win)
2. **R-2**: TLAS instance buffer barrier (spec compliance, potential GPU hangs)
3. **R-7**: Fix texture destroy order (#18) — allocation after image destroy
4. **R-8**: Pre-size TLAS, deferred cleanup (eliminates mid-frame stalls)
5. **R-1**: Assert HOST_COHERENT or add flush (spec compliance)
6. **R-15/R-20**: CPU-side normal matrix (GPU ALU + NaN safety)
7. **R-14**: Enable backface culling (rendering correctness)
8. **R-16**: Add LATE_FRAGMENT_TESTS to subpass dependency

---

Suggest: `/audit-publish docs/audits/AUDIT_RENDERER_2026-04-04.md`
