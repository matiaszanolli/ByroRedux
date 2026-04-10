# Renderer Audit — 2026-04-10b

**Auditor**: Renderer Specialist × 3 (Claude Opus 4.6, parallel)
**Scope**: Full Vulkan renderer — synchronization, GPU memory, pipeline state, render pass, command buffers, shaders, resource lifecycle
**Prior audit**: `AUDIT_RENDERER_2026-04-10.md`

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 1     |
| HIGH     | 2     |
| MEDIUM   | 3     |
| LOW      | 2     |

8 findings total (after deduplication against open issues and prior audits).

The CRITICAL finding is a use-after-free: the SSAO pipeline's descriptor sets reference the depth
image view, which is destroyed during swapchain recreation but the SSAO pipeline is never recreated.
Any window resize with SSAO active will read from a destroyed VkImageView on the GPU.

The two HIGH findings are: (1) RT reflection uses TLAS instance index as SSBO index, but the TLAS
skips meshes without BLAS, causing index misalignment; (2) scene descriptor set binding 7 (AO texture)
is never re-written after resize, compounding the SSAO issue.

---

## Findings

### SYNC-1: SSAO reads destroyed depth image view after swapchain recreation
- **Severity**: CRITICAL
- **Dimension**: Vulkan Synchronization / Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/resize.rs` (entire file — SSAO absent) and `crates/renderer/src/vulkan/ssao.rs:254-257`
- **Status**: NEW
- **Description**: During `recreate_swapchain()`, the depth image and its image view are destroyed and recreated (`resize.rs:26-27`). However, the `SsaoPipeline` is never destroyed or recreated. Its descriptor sets still reference the old, now-destroyed `depth_image_view` written at creation time (`ssao.rs:256`). The SSAO output image also remains at the old resolution. On the next frame after resize, the SSAO compute dispatch reads from a destroyed VkImageView — this is a use-after-free that violates VUID-VkDescriptorImageInfo-imageView-parameter.
- **Evidence**: `resize.rs:26`: `self.device.destroy_image_view(self.depth_image_view, None);` — SSAO descriptor sets still hold this handle. The word "ssao" does not appear anywhere in `resize.rs`.
- **Impact**: GPU crash or validation error on every window resize when SSAO is active. The AO texture also remains at the pre-resize resolution, causing sampling artifacts even if the image view somehow survived.
- **Suggested Fix**: In `recreate_swapchain()`, destroy the old SSAO pipeline (via `ssao.destroy()`) and recreate it with the new depth image view and new dimensions. Then re-write the AO texture into the scene descriptor sets (see LIFE-2).

### D6-1: RT reflection instance index mismatch (TLAS vs SSBO)
- **Severity**: HIGH
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/shaders/triangle.frag:174` and `crates/renderer/src/vulkan/acceleration.rs:262-292`
- **Status**: NEW
- **Description**: The fragment shader uses `rayQueryGetIntersectionInstanceIdEXT(rq, true)` to get the TLAS instance index and indexes into `instances[]` (the draw-command SSBO). However, `build_tlas` skips draw commands that lack a BLAS entry (`let Some(Some(blas)) = self.blas_entries.get(mesh_handle) else { continue; }`). This means TLAS instance N does not necessarily correspond to SSBO instance N. If any draw commands lack a BLAS, all subsequent TLAS indices will be offset from their SSBO counterparts.
- **Evidence**: `acceleration.rs:266` skips commands without BLAS; `triangle.frag:174` uses the result directly as `instances[hitInstanceIdx]` at line 179; `getHitUV()` at lines 131-134 reads vertex/index offsets from the wrong instance.
- **Impact**: RT reflections sample wrong textures and wrong UV coordinates for any scene where some meshes lack BLAS. The shadow ray path is unaffected (it only checks hit/miss, not instance data).
- **Suggested Fix**: Set `instance_custom_index_and_mask` to encode the draw command's SSBO index and use `rayQueryGetIntersectionInstanceCustomIndexEXT` instead of `InstanceIdEXT` in the shader. Alternatively, ensure every draw command has a BLAS (build a degenerate single-triangle BLAS as fallback).

### LIFE-2: Scene descriptor set AO binding not updated after resize
- **Severity**: HIGH
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/resize.rs` and `crates/renderer/src/vulkan/scene_buffer.rs:614-633`
- **Status**: NEW
- **Description**: Even if SYNC-1 is fixed by recreating the SSAO pipeline, the scene descriptor sets (binding 7: `aoTexture`) still reference the old AO image view. `write_ao_texture` is called only during initial setup (`mod.rs:333-338`). After resize, the scene descriptor sets are NOT recreated or re-written. Binding 7 will point to the destroyed old AO image view.
- **Evidence**: `mod.rs:333-338` writes AO texture once at init; `resize.rs` never calls `scene_buffers.write_ao_texture()`.
- **Impact**: Same as SYNC-1 — reading from a destroyed image view in the fragment shader (`triangle.frag:546`: `max(texture(aoTexture, aoUV).r, 0.3)`). Must be fixed together with SYNC-1.
- **Suggested Fix**: After recreating SSAO resources during resize, call `scene_buffers.write_ao_texture()` for each frame-in-flight slot with the new AO image view and sampler.

### SYNC-2: Instance SSBO uploaded after cluster cull dispatch
- **Severity**: MEDIUM
- **Dimension**: Command Buffer Recording
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:207-227` (dispatch) and `draw.rs:323-327` (upload)
- **Status**: NEW
- **Description**: The cluster cull compute dispatch runs at line 227 BEFORE the instance SSBO is uploaded at line 323. The host barrier at line 212-225 mentions "host writes to light/camera/instance SSBOs" but instances have not been written yet. If the cluster cull shader reads instance data (e.g., for position-based culling), it would read stale data from the previous frame.
- **Evidence**: Upload order: lights (line 156) → camera (line 193) → bones (line 197) → cluster cull dispatch (line 227) → instance upload (line 323).
- **Impact**: If the cluster cull shader only reads lights and camera (which ARE uploaded before dispatch), impact is zero. If it reads instances, it gets previous-frame data. Likely benign given the shader's purpose (light-cluster assignment), but the barrier comment is misleading.
- **Suggested Fix**: Either move instance upload before the cluster cull dispatch, or update the barrier comment to accurately reflect that only lights and camera are uploaded at that point.

### SYNC-3: Missing HOST→VERTEX_SHADER barrier for instance upload
- **Severity**: MEDIUM
- **Dimension**: Command Buffer Recording
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:323-331`
- **Status**: NEW
- **Description**: After `upload_instances` writes to HOST_VISIBLE memory (line 324), there is no pipeline barrier before `cmd_begin_render_pass` (line 331) where the vertex shader reads the instance SSBO. The existing host barrier at line 212 covers only the cluster cull dispatch and is issued BEFORE the instance upload. Per Vulkan spec, even HOST_COHERENT memory requires a memory dependency. The implicit host-write visibility guarantee from `vkQueueSubmit` covers single-queue usage, making this safe in practice but not self-documenting.
- **Evidence**: No barrier with `srcAccess=HOST_WRITE, dstAccess=SHADER_READ, dstStage=VERTEX_SHADER` between lines 324 and 331.
- **Impact**: Theoretically unsafe on non-HOST_COHERENT memory (some mobile/integrated GPUs). Safe on desktop via implicit submit-order guarantee. Spec-pedantic but not a real-world bug on current targets.
- **Suggested Fix**: Add a `HOST→VERTEX_SHADER` memory barrier after the instance upload, or add a comment documenting the reliance on implicit submit-order guarantees.

### SYNC-4: Scene descriptor sets partially stale after resize (latent)
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/resize.rs`
- **Status**: NEW
- **Description**: `recreate_swapchain()` calls `texture_registry.recreate_descriptor_sets()` but does NOT recreate scene descriptor sets. Currently safe because cluster cull buffers, TLAS, and geometry SSBOs are not destroyed during resize. However, fixing SYNC-1/LIFE-2 by recreating SSAO will also require updating binding 7 in the scene descriptor sets. This is a latent issue that becomes real when SSAO resize is implemented.
- **Evidence**: `resize.rs` has no calls to `scene_buffers.write_ao_texture()`, `write_cluster_buffers()`, or `write_geometry_buffers()`.
- **Impact**: Latent — becomes real when SSAO resize fix is implemented. Currently no live stale bindings because SSAO AO texture is the only resource destroyed on resize that has a scene descriptor binding.
- **Suggested Fix**: When fixing SYNC-1, add `scene_buffers.write_ao_texture()` calls for each frame slot after SSAO recreation.

### MEM-1: Staging pool buffers never returned after use
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/buffer.rs:530-537`
- **Status**: Existing: #99
- **Description**: Staging buffers acquired from `StagingPool` are wrapped in `StagingGuard` which destroys them on drop rather than returning them to the pool via `pool.release()`. The pool provides zero reuse benefit — every upload creates and destroys a staging buffer through gpu-allocator. This was already noted in #99 (StagingPool grows unboundedly).

### D6-2: Stale GpuInstance size comment
- **Severity**: LOW
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/src/vulkan/scene_buffer.rs:44-46`
- **Status**: Existing: PIPE-1 in `AUDIT_RENDERER_2026-04-10.md`
- **Description**: Doc comment says "Layout: 112 bytes" but actual struct is 128 bytes. Already reported in today's earlier audit.

---

## Deduplicated — Existing Issues Verified Still Open

The following open issues were identified by the audit agents but already have tracking issues:

| Issue | Finding | Status |
|-------|---------|--------|
| #18 | Texture `destroy()` frees allocation before destroying image | OPEN — still present in `texture.rs:564-578` |
| #92 | `update_rgba` descriptor writes race with in-flight command buffers | OPEN — still present in `texture_registry.rs:325-336` |
| #99 | StagingPool grows unboundedly / no reuse | OPEN — confirmed, pool.release() never called |
| #33 | Destruction order inconsistent between recreate and Drop | OPEN — related to SYNC-1/LIFE-2 |
| #101 | GLSL version mismatch (vert 450 vs frag 460) | OPEN |

---

## Informational (No Issues Found)

The following areas were audited and found correct:

- **Vertex input layout**: Rust `Vertex` struct matches `triangle.vert` exactly (6 locations, correct formats/offsets, stride 76 bytes). `UiVertex` matches `ui.vert` (2 locations, stride 20 bytes). Confirmed by `offset_of!` tests.
- **Push constant ranges**: No push constants in pipeline layout; all per-draw data lives in instance SSBO. Shader and Rust side agree.
- **Dynamic state**: All scene pipelines declare `[VIEWPORT, SCISSOR, DEPTH_BIAS]`; UI declares `[VIEWPORT, SCISSOR]`. Set correctly each frame in `draw_frame()`.
- **Pipeline/render pass compatibility**: All 5 pipelines created with correct render pass and subpass(0). Color blend count matches attachment count.
- **Render pass configuration**: Color CLEAR+STORE, UNDEFINED→PRESENT_SRC_KHR. Depth CLEAR+STORE (for SSAO), UNDEFINED→DEPTH_STENCIL_READ_ONLY. Subpass dependencies cover all required stages/access.
- **Command pool flags**: Draw pool has RESET_COMMAND_BUFFER; transfer pool has TRANSIENT. Both correct.
- **Begin/end balance**: All command buffer begin/end, render pass begin/end pairs balanced in `draw_frame()`.
- **Render pass boundaries**: All draw commands inside render pass; all compute/acceleration structure commands outside. No violations.
- **Drop ordering**: Framebuffers before render pass, image views before swapchain, pipelines/shader modules/descriptor pools all destroyed before device. Allocator drop is defensive (leak rather than use-after-free on outstanding Arc refs).
- **Compute/SSAO shader modules**: Destroyed immediately after pipeline creation (correct per best practice).
- **TLAS descriptor**: Re-written per frame in `draw_frame()`, survives resize correctly.

---

## Priority Fix Order

1. **SYNC-1 + LIFE-2** (SSAO resize use-after-free) — CRITICAL, any window resize with SSAO crashes GPU
2. **D6-1** (RT reflection TLAS/SSBO index mismatch) — HIGH, visually incorrect reflections
3. **SYNC-2 + SYNC-3** (instance upload ordering/barriers) — MEDIUM, spec compliance
4. **#18** (texture destroy order) — HIGH existing issue, spec violation on every texture destruction
