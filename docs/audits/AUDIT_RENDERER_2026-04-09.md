# Renderer Audit — 2026-04-09

**Auditor**: Renderer Specialist × 3 (Claude Opus 4.6, parallel)
**Scope**: Full Vulkan renderer — synchronization, GPU memory, pipeline, render pass, commands, shaders, lifecycle
**Prior audit**: `AUDIT_RENDERER_2026-04-05b.md` (0 CRITICAL, 0 HIGH, 3 MEDIUM, 5 LOW, 3 INFO)

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0     |
| HIGH     | 1     |
| MEDIUM   | 3     |
| LOW      | 6     |

10 new findings total. No CRITICAL issues. The HIGH finding is a previously-identified TLAS resize stall
that has been upgraded due to additional analysis revealing stale descriptor references during the resize
window. The renderer is architecturally sound but has synchronization gaps in the acceleration structure
path that will become real bugs as scene complexity grows.

### Key themes
1. **TLAS resource sharing across frames-in-flight** — scratch/instance buffers are not double-buffered (SYNC-001)
2. **Push constants exceed Vulkan guaranteed minimum** — 132 bytes vs 128-byte spec floor (PIPE-001)
3. **Resource destruction ordering** — depth alloc freed before image, meshes before pipelines (MEM-001, LIFE-001)

---

## Findings

### SYNC-001: TLAS scratch/instance buffers shared across frames-in-flight
- **Severity**: MEDIUM
- **Dimension**: Vulkan Synchronization
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:114-139`, `crates/renderer/src/vulkan/acceleration.rs:374-429`
- **Status**: NEW
- **Description**: The TLAS is rebuilt every frame by recording `cmd_build_acceleration_structures` into the per-image command buffer. However, TLAS resources (result buffer, scratch buffer, instance buffer) are shared — there is only one `TlasState` and one `scratch_buffer` in `AccelerationManager`. When `MAX_FRAMES_IN_FLIGHT > 1`, frame N could be recording a TLAS build while frame N-1's command buffer (which also recorded a TLAS build using the same scratch and instance buffers) is still executing on the GPU. This creates a write-after-write hazard on scratch and a write-after-read hazard on the instance buffer.
- **Evidence**: `draw.rs:117` calls `accel.build_tlas(device, alloc, cmd, draw_commands)` which records into the per-image `cmd`. `acceleration.rs:374` writes to `tlas.instance_buffer` (shared), and `acceleration.rs:429` records a build using the shared `scratch_buffer`. The fence wait at `draw.rs:36` only waits for the current frame-in-flight slot, not for any other frame that may reference the same TLAS resources.
- **Impact**: On GPUs where frames actually overlap (mailbox present mode with fast CPU), the GPU could be reading the scratch buffer from frame N-1's build while frame N's build writes to it. Can cause corrupted acceleration structures, RT shadow glitches, or driver crashes. The `images_in_flight` fence check mitigates only when two frames target the same swapchain image.
- **Related**: MEM-002
- **Suggested Fix**: Double-buffer the TLAS (one per frame-in-flight with separate instance and scratch buffers), or add an explicit pipeline barrier between the previous frame's TLAS build completion and the current frame's build.

---

### MEM-002: TLAS resize calls device_wait_idle during command buffer recording
- **Severity**: HIGH
- **Dimension**: Vulkan Synchronization
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:269-285`
- **Status**: Existing: REN-005 (upgraded from MEDIUM to HIGH — new stale-descriptor analysis)
- **Description**: When the TLAS exceeds `max_instances`, `build_tlas` calls `device.device_wait_idle()` at line 280, then destroys and recreates the TLAS. This happens inside `draw_frame` AFTER the command buffer has been begun (draw.rs:84-88). The old TLAS that was destroyed may still be referenced by a descriptor set until `write_tlas` is called later at draw.rs:134. Between the destroy and the write, the descriptor set points to a destroyed acceleration structure handle. While the descriptor is only read during `cmd_draw_indexed` (which happens after `write_tlas`), the window of stale descriptor state is a spec violation. Additionally, the `device_wait_idle` creates a full GPU pipeline stall, causing a visible frame time spike.
- **Evidence**: `acceleration.rs:280`: `device.device_wait_idle().ok()` called while command buffer `cmd` is in recording state. Lines 281-284 destroy the old TLAS. `draw.rs:134` writes the new TLAS to descriptors, but this is after the destroy window.
- **Impact**: Full GPU stall when scene exceeds 4096 RT instances (large exteriors). Stale descriptor references are a Vulkan spec violation even if not read during the stale window.
- **Related**: SYNC-001
- **Suggested Fix**: Defer TLAS destruction via a ring buffer (destroy after N frames), or proactively resize at frame start after fence wait. Pre-allocate to a generous upper bound for the target game.

---

### PIPE-001: Push constant size exceeds Vulkan guaranteed minimum
- **Severity**: MEDIUM
- **Dimension**: Pipeline State
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:184-192`
- **Status**: NEW
- **Description**: The push constant range is declared as 132 bytes (2× mat4 + uint). The Vulkan spec guarantees only `maxPushConstantsSize >= 128` bytes. The code comment at line 185-187 acknowledges the 128-byte minimum but incorrectly claims 132 is "well under" it — 132 is 4 bytes *over*. On hardware that exposes exactly 128 bytes (some Intel iGPUs, mobile/embedded GPUs), pipeline creation will fail. No runtime check against the device's actual `maxPushConstantsSize` exists.
- **Evidence**:
  ```rust
  // pipeline.rs:184-192
  // + padding to 4-byte alignment = 132 bytes. Well under the 128-byte
  // spec minimum caveat on old hardware — modern Vulkan drivers expose
  // at least 256 bytes.
  let push_constant_ranges = [vk::PushConstantRange {
      stage_flags: vk::ShaderStageFlags::VERTEX,
      offset: 0,
      size: 132,
  }];
  ```
  Grep for `maxPushConstants` returns zero hits — no runtime validation.
- **Impact**: Pipeline creation fails on devices with exactly 128-byte push constant limit. Engine becomes completely non-functional on that hardware.
- **Suggested Fix**: Either (a) query `maxPushConstantsSize` during device selection and assert >= 132 with a clear error, or (b) move `boneOffset` to the existing scene SSBO to stay within 128 bytes. Fix the misleading comment regardless.

---

### SYNC-002: Command pool not externally synchronized for one-time submits
- **Severity**: MEDIUM
- **Dimension**: Vulkan Synchronization
- **Location**: `crates/renderer/src/vulkan/texture.rs:601-671`
- **Status**: NEW
- **Description**: `with_one_time_commands` allocates a command buffer from the same `command_pool` used by `draw_frame`. Vulkan requires external synchronization on `VkCommandPool` (VUID-vkAllocateCommandBuffers-commandPool-00044). Currently safe because all loading happens before the render loop or during `device_wait_idle` pauses, but the API surface allows calling it during rendering. No mutex protects command pool access.
- **Evidence**: `texture.rs:617` allocates from `pool`; `draw.rs:77` resets a buffer from the same pool. Only queue submissions are mutex-protected, not pool access.
- **Impact**: Not currently triggered. Becomes a real race condition if async asset streaming is added (mentioned in ROADMAP).
- **Suggested Fix**: Create a dedicated transfer command pool for one-time uploads. A separate pool also enables using a transfer-only queue family if available.

---

### SHDR-001: Normal transform incorrect for non-uniform scale
- **Severity**: LOW
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/shaders/triangle.vert:56-57`
- **Status**: NEW
- **Description**: The vertex shader transforms normals using `mat3(xform) * inNormal`, which is only correct for uniform scale (as the comment acknowledges). Gamebryo NIF content frequently uses non-uniform scale on scene graph nodes (stretched rocks, squashed furniture, character morphs), causing incorrect lighting on those meshes.
- **Evidence**:
  ```glsl
  // Transform normal by xform's upper 3x3. For uniform scale this is
  // equivalent to inverse-transpose. Guard against zero-scale meshes...
  vec3 n = mat3(xform) * inNormal;
  ```
- **Impact**: Subtle shading artifacts on any mesh with non-uniform scale. Surfaces appear brighter/darker than intended.
- **Suggested Fix**: Use `transpose(inverse(mat3(xform))) * inNormal` for correct normal transformation. Alternatively, precompute the normal matrix CPU-side and pass via SSBO.

---

### MEM-001: Depth allocation freed before image destroyed
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:359-369`, `crates/renderer/src/vulkan/context/resize.rs:24-34`
- **Status**: NEW (related to #18 for textures; this is the depth buffer specifically)
- **Description**: In Drop, the depth allocation is freed (line 364) before the depth image is destroyed (line 369). Per Vulkan spec, the memory bound to an image must remain valid until the image is destroyed. With gpu-allocator's sub-allocation, the underlying VkDeviceMemory is not freed by `allocator.free()`, so this is practically harmless but technically a spec violation. Same pattern in `recreate_swapchain`.
- **Evidence**: `mod.rs:364` calls `allocator.free(alloc)` before `mod.rs:369` calls `device.destroy_image(self.depth_image)`.
- **Impact**: Validation layer warnings possible. Would become a real bug if the allocator frees the underlying VkDeviceMemory block.
- **Suggested Fix**: Swap the order: `destroy_image` then `allocator.free()`. Fix in both Drop and `recreate_swapchain`.

---

### LIFE-001: Mesh registry destroyed before pipelines in Drop
- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:350-379`
- **Status**: NEW
- **Description**: In Drop, `mesh_registry.destroy_all()` is called before pipelines are destroyed. Meshes are inputs consumed by pipelines at draw time, so logically they should outlive the pipelines. Currently harmless because `device_wait_idle()` was called at the top of Drop and no further drawing occurs. Semantically backwards ordering that could become a real bug if Drop is refactored.
- **Impact**: No runtime impact. Code hygiene issue.
- **Suggested Fix**: Move `mesh_registry.destroy_all` after the pipeline destruction block.

---

### MEM-003: BLAS creation leaks accel structure + buffer on build failure
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:122-215`
- **Status**: NEW
- **Description**: In `build_blas`, if `with_one_time_commands` at line 186 fails, the function returns via `?`. At this point, `result_buffer` (a `GpuBuffer`) and `accel` (a `VkAccelerationStructureKHR`) have been created but are not stored anywhere and not destroyed.
- **Evidence**: `acceleration.rs:122-128` creates `result_buffer`, `acceleration.rs:136-140` creates `accel`. Line 186 can fail and return early without cleanup.
- **Impact**: GPU memory leak on BLAS build failure. Rare in practice.
- **Suggested Fix**: Explicitly destroy both `accel` and `result_buffer` in the error path, or use a RAII guard.

---

### RPASS-001: Missing outgoing subpass dependency for presentation
- **Severity**: LOW
- **Dimension**: Render Pass
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs:83-104`
- **Status**: NEW
- **Description**: The render pass declares only an external→subpass0 dependency but no subpass0→external dependency. The implicit subpass dependency has `dst_stage_mask = BOTTOM_OF_PIPE` and `dst_access_mask = 0`. The `render_finished` semaphore covers the gap, but the render pass is not self-documenting for synchronization. Would become a latent bug if post-render-pass compute work is added before present.
- **Evidence**: Only one dependency declared (lines 83-104, `src_subpass: SUBPASS_EXTERNAL, dst_subpass: 0`). No outgoing dependency exists.
- **Impact**: None currently — the semaphore provides the necessary guarantee. Fragile under refactoring.
- **Suggested Fix**: Add a second `VkSubpassDependency` from subpass 0 to `SUBPASS_EXTERNAL` with `src_stage = COLOR_ATTACHMENT_OUTPUT`, `src_access = COLOR_ATTACHMENT_WRITE`.

---

### PIPE-002: UI pipeline feeds unused bone attributes
- **Severity**: LOW
- **Dimension**: Pipeline State
- **Location**: `crates/renderer/shaders/ui.vert:1-16`, `crates/renderer/src/vulkan/pipeline.rs:364-368`
- **Status**: NEW
- **Description**: The UI pipeline uses the full `Vertex::attribute_descriptions()` (6 attributes including `bone_indices` and `bone_weights`) but the UI shader only consumes locations 0-3. Per Vulkan spec 22.2, unconsumed attributes are silently ignored. The GPU reads 76 bytes/vertex when only 44 are needed — ~43% wasted vertex input bandwidth for UI draws.
- **Impact**: Minor bandwidth waste on UI draws. Functionally correct.
- **Suggested Fix**: By design (single vertex format). No action unless UI perf becomes a bottleneck.

---

## Prior Audit Status

From `AUDIT_RENDERER_2026-04-05b.md`:
- **REN-001** (depth STORE op): **FIXED** — now uses `DONT_CARE` as recommended
- **REN-002** (swapchain raw pointer): Open as #93
- **REN-003** (pipeline cache validation): Open as #91
- **REN-004** (mixed descriptor indexing): Informational, unchanged
- **REN-005** (TLAS resize stall): **Upgraded** — see MEM-002 above (HIGH)
- **REN-006** (shader module retention): Open as #98
- **REN-007** (viewport/scissor after UI bind): Open as #133
- **REN-008** (depth bias every draw): Open as #51
- **REN-009–011**: Informational, unchanged

---

## Dimensions with Clean Results

- **Command Buffer Recording**: All balanced (begin/end, render pass begin/end). RESET_COMMAND_BUFFER flag set on pool. TLAS build and uploads correctly recorded before render pass. No out-of-scope commands.
- **Render Pass**: Correct load/store ops (CLEAR+STORE color, CLEAR+DONT_CARE depth). Layout transitions correct (UNDEFINED→attachment→PRESENT_SRC). Subpass dependency stage/access masks include LATE_FRAGMENT_TESTS for discard paths. Framebuffer attachment order matches render pass.
- **Push Constants**: Byte offsets match between Rust-side pushes and GLSL layout (0→64 viewProj, 64→128 model, 128→132 boneOffset).
