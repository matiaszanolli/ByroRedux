# Renderer Audit — 2026-04-02

**Scope**: `crates/renderer/src/` — all Vulkan, pipeline, shader, and resource lifecycle code
**Auditor**: Claude Opus 4.6 (renderer-specialist agents)
**Dimensions**: Vulkan Synchronization, GPU Memory, Pipeline State, Render Pass, Command Buffer Recording, Shader Correctness, Resource Lifecycle

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 3 |
| MEDIUM | 8 |
| LOW | 7 |

The shader interface (vertex layout, push constants, descriptor bindings) is fully correct. The render pass configuration is spec-compliant. The main risk area is **swapchain recreation**, which has three distinct bugs around semaphore resizing, pipeline staleness, and sync object lifecycle. Secondary concerns are the manual-destroy pattern for GPU resources (silent leaks on error paths) and vertex buffers in suboptimal memory.

---

## HIGH

### R-01: Semaphores not resized on swapchain recreation
- **Severity**: HIGH
- **Dimension**: Vulkan Synchronization / Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context.rs:590-673`, `crates/renderer/src/vulkan/sync.rs:35-49`
- **Status**: NEW
- **Description**: When `recreate_swapchain` is called, the swapchain image count may change. `reset_image_fences()` resizes the `images_in_flight` vector, but the `render_finished` semaphore vector is NOT resized. `render_finished` is indexed by swapchain image index (`img`) at line 524. If the new swapchain has more images than the original, this is an out-of-bounds panic.
- **Evidence**:
  ```rust
  // context.rs:664 — only resets fence tracking
  self.frame_sync.reset_image_fences(self.swapchain_state.images.len());
  // No semaphore recreation

  // context.rs:524 — indexed by swapchain image
  let signal_semaphores = [self.frame_sync.render_finished[img]];
  ```
- **Impact**: Panic/crash if swapchain recreation produces more images than the initial creation. Real scenario on some drivers/compositors.
- **Suggested Fix**: Add `FrameSync::recreate_for_swapchain(&mut self, device, new_image_count)` that destroys old semaphores and creates new ones matching the new image count.

### R-02: image_available semaphores over-allocated with misleading semantics
- **Severity**: HIGH
- **Dimension**: Vulkan Synchronization
- **Location**: `crates/renderer/src/vulkan/sync.rs:14-16,35-37`, `crates/renderer/src/vulkan/context.rs:263`
- **Status**: NEW
- **Description**: `image_available` semaphores are created with `swapchain_image_count` entries (typically 3) but indexed by `frame` which cycles through `0..MAX_FRAMES_IN_FLIGHT` (2). The comment says "One per swapchain image" but they are logically per-frame-in-flight. Currently safe because `swapchain_image_count >= MAX_FRAMES_IN_FLIGHT`, but if `MAX_FRAMES_IN_FLIGHT` is ever increased or the swapchain image count decreases, this is an out-of-bounds access.
- **Evidence**:
  ```rust
  // sync.rs:14-15 — misleading comment
  /// One per swapchain image — signaled when image is acquired.
  pub image_available: Vec<vk::Semaphore>,

  // context.rs:263 — indexed by frame
  self.frame_sync.image_available[frame],
  ```
- **Impact**: Latent out-of-bounds if constants change. Semantic mismatch confuses maintainers.
- **Suggested Fix**: Create exactly `MAX_FRAMES_IN_FLIGHT` semaphores for `image_available`. Fix comment to "One per frame-in-flight."

### R-03: Pipelines not recreated after render pass recreation
- **Severity**: HIGH
- **Dimension**: Pipeline State / Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context.rs:612-637`
- **Status**: NEW
- **Description**: During `recreate_swapchain`, the old render pass is destroyed (line 613) and a new one is created (line 636). The five pipelines (`pipeline`, `pipeline_alpha`, `pipeline_two_sided`, `pipeline_alpha_two_sided`, `pipeline_ui`) are NOT recreated. Pipelines capture render pass compatibility at creation time (not a live reference), so this is technically safe per spec as long as the new render pass is compatible. However, if the surface format ever changes across recreations (HDR toggle, format fallback), pipelines would be invalid.
- **Evidence**:
  ```rust
  self.device.destroy_render_pass(self.render_pass, None);
  // ... 20 lines later ...
  self.render_pass = create_render_pass(&self.device, self.swapchain_state.format.format)?;
  // No pipeline recreation follows
  ```
- **Impact**: Validation layer warnings. Real rendering corruption if swapchain format changes.
- **Suggested Fix**: Either recreate pipelines after render pass recreation, or add a runtime check that panics if the format changed (and document the assumption).

---

## MEDIUM

### R-04: One-time command submissions use queue_wait_idle
- **Severity**: MEDIUM
- **Dimension**: Vulkan Synchronization
- **Location**: `crates/renderer/src/vulkan/texture.rs:625-638`
- **Status**: NEW
- **Description**: `with_one_time_commands` submits with `Fence::null()` then calls `queue_wait_idle`, which serializes the entire queue. Currently only used at initialization, but will cause visible stalls if used during gameplay (texture streaming).
- **Evidence**:
  ```rust
  device.queue_submit(queue, &[submit_info], vk::Fence::null())?;
  device.queue_wait_idle(queue)?;
  ```
- **Impact**: Performance stall on runtime texture uploads.
- **Suggested Fix**: Use a dedicated transfer fence. For streaming, use a transfer queue or timeline semaphore.

### R-05: Graphics queue shared without external synchronization
- **Severity**: MEDIUM
- **Dimension**: Vulkan Synchronization
- **Location**: `crates/renderer/src/vulkan/texture.rs:131`, `crates/renderer/src/vulkan/context.rs:148-156`
- **Status**: NEW
- **Description**: `graphics_queue` is a plain `vk::Queue` with no mutex. Texture uploads and `draw_frame` both submit to this queue. Vulkan requires queue submissions to be externally synchronized (VUID-vkQueueSubmit-queue-00893). Currently safe because uploads happen before the render loop, but fragile.
- **Impact**: Undefined behavior if texture loading is ever parallelized with rendering.
- **Suggested Fix**: Wrap the graphics queue in a `Mutex`, or use a dedicated transfer queue.

### R-06: Vertex/index buffers use CpuToGpu instead of GpuOnly
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/buffer.rs:114`
- **Status**: NEW
- **Description**: All vertex and index buffers are allocated with `MemoryLocation::CpuToGpu` (HOST_VISIBLE). Static geometry read many times per frame should be in DEVICE_LOCAL memory for optimal GPU read bandwidth (200+ GB/s vs 8-16 GB/s on discrete GPUs).
- **Evidence**:
  ```rust
  location: MemoryLocation::CpuToGpu,
  ```
- **Impact**: Reduced rendering performance, especially with many meshes. Source acknowledges "HOST_VISIBLE for now."
- **Suggested Fix**: Use staging buffer pattern (CpuToGpu upload, copy to GpuOnly), same as textures.

### R-07: Allocator Arc::try_unwrap failure silently leaks
- **Severity**: MEDIUM
- **Dimension**: GPU Memory / Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context.rs:727-731`
- **Status**: NEW
- **Description**: If `Arc::try_unwrap` fails (another Arc clone exists), the allocator leaks silently — no warning. The device is then destroyed while the allocator still holds VkDeviceMemory references.
- **Evidence**:
  ```rust
  if let Ok(mutex) = std::sync::Arc::try_unwrap(alloc_arc) {
      drop(mutex.into_inner().expect("allocator lock poisoned"));
  }
  // else: silently leaked!
  ```
- **Impact**: Use-after-free at Vulkan level if allocator outlives device.
- **Suggested Fix**: Add `log::error!` in the else branch. Consider `panic!` in debug builds.

### R-08: GpuBuffer has no Drop guard — silent leak on implicit drop
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/buffer.rs:10-17`
- **Status**: NEW
- **Description**: `GpuBuffer` requires manual `destroy()`. No `Drop` impl warns about leaks. Any error path that drops a GpuBuffer without calling destroy silently leaks the VkBuffer and gpu-allocator allocation.
- **Impact**: GPU memory leak on error paths.
- **Suggested Fix**: Add a `Drop` impl that logs a warning when `allocation.is_some()`.

### R-09: Texture has no Drop guard — same silent leak pattern
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/texture.rs:9-15`
- **Status**: NEW
- **Description**: Same as R-08. `Texture` requires manual `destroy()` with no Drop guard. Sampler, image view, image, and allocation all leak.
- **Impact**: GPU object leaks on error paths.
- **Suggested Fix**: Same as R-08.

### R-10: Normal transform ignores non-uniform scale
- **Severity**: MEDIUM
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/shaders/triangle.vert:22`
- **Status**: NEW
- **Description**: Normals are transformed with `mat3(pc.model) * inNormal`. This is only correct for uniform scale. Non-uniform scale requires `transpose(inverse(mat3(model)))` (the normal matrix). NIF meshes or scene nodes with non-uniform scale will have distorted lighting.
- **Evidence**:
  ```glsl
  fragNormal = mat3(pc.model) * inNormal;
  ```
- **Impact**: Incorrect lighting on non-uniformly scaled meshes.
- **Suggested Fix**: Compute inverse-transpose on CPU, pass via UBO (push constants are full at 128 bytes).

### R-11: Unsafe blocks lack safety comments
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context.rs:363-366,418-420`, `crates/renderer/src/vulkan/buffer.rs:67-69`, and others
- **Status**: NEW
- **Description**: Many `unsafe` blocks lack `// SAFETY:` comments. Notable examples: `slice::from_raw_parts` for push constants and buffer uploads.
- **Evidence**:
  ```rust
  let view_proj_bytes: &[u8] = std::slice::from_raw_parts(
      view_proj.as_ptr() as *const u8, 64,
  );
  ```
- **Impact**: Code review friction. No actual unsoundness.
- **Suggested Fix**: Add `// SAFETY:` comments. Consider `bytemuck::cast_slice` to eliminate unsafe entirely.

---

## LOW

### R-12: Texture destroy frees allocation before destroying image
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/texture.rs:575-590`
- **Status**: NEW
- **Description**: Allocation is freed before `destroy_image`. Correct order is destroy image first, then free memory. Practically safe with gpu-allocator's sub-allocation, but violates recommended destruction order.
- **Suggested Fix**: Swap the order: destroy image first, free allocation second.

### R-13: Depth store op DONT_CARE precludes future depth readback
- **Severity**: LOW
- **Dimension**: Render Pass
- **Location**: `crates/renderer/src/vulkan/context.rs:760`
- **Status**: NEW
- **Description**: Depth attachment uses `StoreOp::DONT_CARE`. Optimal for single-pass, but needs `STORE` for deferred shading, SSAO, or shadow mapping.
- **Suggested Fix**: Document decision. Change to `STORE` when adding post-processing.

### R-14: Identical rasterizer states (culling disabled on all pipelines)
- **Severity**: LOW
- **Dimension**: Pipeline State
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:97-118`
- **Status**: NEW
- **Description**: Both `rasterizer` and `rasterizer_no_cull` have `CullModeFlags::NONE`. The "two-sided" pipeline variants are currently redundant. Comment explains this is intentional until NIF winding is verified.
- **Suggested Fix**: Enable `CullModeFlags::BACK` on opaque pipelines once winding order is confirmed.

### R-15: Viewport/scissor not re-set after pipeline switch
- **Severity**: LOW
- **Dimension**: Command Buffer Recording
- **Location**: `crates/renderer/src/vulkan/context.rs:383-398`
- **Status**: NEW
- **Description**: Dynamic viewport/scissor set once before first pipeline bind. Per spec, dynamic state persists across pipeline binds if all pipelines declare it as dynamic. Currently correct but fragile.
- **Suggested Fix**: Add comment documenting the dependency.

### R-16: UI pipeline pushes unused push constants
- **Severity**: LOW
- **Dimension**: Pipeline State
- **Location**: `crates/renderer/src/vulkan/context.rs:468-492`
- **Status**: NEW
- **Description**: UI vertex shader has no push_constant block, but the shared pipeline layout requires them. Identity matrices are pushed unnecessarily. Correct per spec but wasteful.
- **Suggested Fix**: Acceptable. Dedicated UI layout would be cleaner but not worth the complexity now.

### R-17: No GPU memory budget tracking
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/allocator.rs:17-37`
- **Status**: NEW
- **Description**: No allocation tracking or `VK_EXT_memory_budget` integration. Unbounded asset loading will eventually exhaust VRAM with no early warning.
- **Suggested Fix**: Future work — integrate memory budget queries when asset streaming is implemented.

### R-18: Pipeline references destroyed render pass handle
- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context.rs:612-613`
- **Status**: NEW
- **Description**: After `recreate_swapchain`, pipelines internally reference the old (destroyed) render pass handle. Per Vulkan spec, pipelines capture compatibility at creation — not a live reference — so this is valid. But surprising to maintainers and may trigger warnings on some validation layer versions.
- **Suggested Fix**: Add comment explaining why this is spec-compliant.

---

## Positive Findings (no action needed)

| Area | What's Correct |
|------|---------------|
| Fence reset ordering | Fence waited, then image_in_flight checked, then reset — correct pattern |
| Subpass dependency | Covers `EARLY_FRAGMENT_TESTS` + `DEPTH_STENCIL_ATTACHMENT_WRITE` — correct |
| Depth allocation | `GpuOnly` — optimal |
| Texture staging | CpuToGpu staging → GpuOnly image with proper cleanup — correct |
| Allocator leak detection | `log_leaks_on_shutdown: true` — good observability |
| Vertex/push constant layout | Byte offsets match exactly between GLSL and Rust |
| Descriptor bindings | `set=0, binding=0` COMBINED_IMAGE_SAMPLER matches shader and layout |
| Drop order | Correct reverse-creation teardown in VulkanContext::Drop |
| Command pool flags | `RESET_COMMAND_BUFFER` set, matching per-buffer reset usage |
| Render pass config | CLEAR/STORE for color, correct layout transitions, proper dependencies |

---

## Priority Action Items

1. **Fix swapchain recreation** (R-01, R-02, R-03): Add `FrameSync::recreate_sync_objects()`, fix semaphore allocation semantics, and either recreate pipelines or add format-change guard. These three findings are interconnected and should be addressed together.

2. **Add Drop guards** (R-08, R-09): Diagnostic `Drop` impls on `GpuBuffer` and `Texture` to catch silent leaks.

3. **Allocator drop safety** (R-07): Log error on `Arc::try_unwrap` failure.

4. **Vertex buffer memory** (R-06): Move to GpuOnly with staging when loading real game content.

5. **Normal matrix** (R-10): Address before enabling non-uniform scale in NIF import.
