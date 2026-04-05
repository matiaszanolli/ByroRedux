# Renderer Audit — 2026-04-05

**Scope**: `crates/renderer/src/` — all Vulkan, pipeline, shader, and resource lifecycle code
**Auditor**: Claude Opus 4.6 (renderer-specialist agents)
**Prior audits**: 2026-04-02, 2026-04-04
**Dimensions**: Vulkan Synchronization, GPU Memory, Pipeline State, Render Pass, Command Buffer Recording, Shader Correctness, Resource Lifecycle

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 4 |

The renderer is in substantially better shape than the 2026-04-02 audit. The three HIGH swapchain recreation bugs (R-01/02/03) are fixed. The Mutex-wrapped graphics queue, GpuBuffer/Texture Drop guards, staging pool, shared sampler, pipeline cache, and BLAS scratch reuse are all correctly implemented. No new CRITICAL or HIGH findings.

---

## NEW Findings

### REN-01: Old shader modules leaked on each swapchain recreation
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context.rs:846-854`
- **Status**: NEW
- **Description**: During `recreate_swapchain`, `create_triangle_pipeline` creates new shader modules internally. The new modules are immediately destroyed (line 851-853), which is correct — pipelines capture shader state at creation. However, the old `self.vert_module` and `self.frag_module` remain allocated and are only destroyed in `Drop`. Since shader modules are not needed after pipeline creation, they could be destroyed immediately after the initial `new()` call. On each swapchain recreation, two new modules are created and destroyed (correct), but the original two persist unnecessarily for the lifetime of the context.
- **Impact**: Minor GPU object waste. Two VkShaderModule handles held longer than needed. Not a per-resize leak since the same two old modules are retained (not accumulated).
- **Suggested Fix**: Destroy `self.vert_module` and `self.frag_module` right after initial pipeline creation in `new()`, and remove the fields from the struct. The UI shader modules should follow the same pattern.

### REN-02: StagingPool grows unboundedly — no shrink/eviction
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/buffer.rs` (StagingPool)
- **Status**: NEW
- **Description**: `StagingPool::release` adds buffers to `free_list` with no cap. During a large cell load, staging buffers accumulate. After the burst, they remain allocated.
- **Impact**: Persistent memory overhead after loading spikes. Not a leak (freed at shutdown).
- **Suggested Fix**: Add a `trim()` method that shrinks to a configurable high-water mark. Call after asset loading phases.

### REN-03: Depth attachment store_op is STORE but depth is never read back
- **Severity**: LOW
- **Dimension**: Render Pass
- **Location**: `crates/renderer/src/vulkan/context.rs:1050`
- **Status**: NEW
- **Description**: Depth attachment uses `store_op: STORE` but the buffer is cleared each frame (`load_op: CLEAR`, `initial_layout: UNDEFINED`) and never sampled. `DONT_CARE` would let tile-based GPUs skip depth writeback.
- **Impact**: Missed optimization on mobile/Apple Silicon GPUs. No effect on desktop discrete.
- **Suggested Fix**: Change to `vk::AttachmentStoreOp::DONT_CARE`.

### REN-04: GLSL version mismatch between vertex (450) and fragment (460) shaders
- **Severity**: LOW
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/shaders/triangle.vert:1` vs `triangle.frag:1`
- **Status**: NEW
- **Description**: Vertex shader uses `#version 450`, fragment uses `#version 460` (needed for `GL_EXT_ray_query`). Mixing is allowed (SPIR-V is version-agnostic once compiled) but inconsistent.
- **Suggested Fix**: Bump vertex shader to `#version 460`.

### REN-05: Directional light shadow ray tmax may clip distant occluders
- **Severity**: LOW
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/shaders/triangle.frag:91-92`
- **Status**: NEW
- **Description**: Directional lights use `dist = 10000.0`, giving shadow ray tmax of `9999.9`. Bethesda exterior cells can span thousands of units; distant occluders beyond 10000 units won't cast shadows.
- **Suggested Fix**: Increase to `100000.0` or make configurable via scene UBO.

---

## Existing Issues (confirmed still present)

| Issue | Status |
|-------|--------|
| #11 | `queue_wait_idle` per upload — still present in `with_one_time_commands` |
| #18 | texture destroy order (alloc before image) — confirmed fixed (destroy image_view first now, then free alloc, then destroy image) — **CLOSED** |
| #33 | destruction order inconsistent — confirmed the recreate path now matches Drop |
| #34 | redundant layout/module create+destroy on resize — confirmed present (see REN-01) |
| #51 | unconditional depth bias per draw — confirmed present |
| #59 | forced texture rebind after pipeline switch — confirmed present |
| #92 | descriptor set write race on update_rgba — confirmed mitigated by deferred ring, but descriptor writes still update all images simultaneously |
| #97 | WHOLE_SIZE over-flush — confirmed present |

---

## Verified Correct (no issues)

| Area | Status |
|------|--------|
| Semaphore indexing | `image_available` per frame-in-flight, `render_finished` per image — correct |
| Fence reset ordering | Wait → image_in_flight check → reset → submit — correct |
| `images_in_flight` tracking | Correctly skips wait when same frame already owns image |
| Queue mutex scope | Lock held only for `queue_submit` / `queue_present`, not during recording |
| Allocator drop order | `Arc::try_unwrap` after all resources freed, before `destroy_device` |
| Old swapchain handoff | Old handle passed to `create_swapchain` for atomic transition |
| Depth resource destruction | View → free alloc → destroy image — correct in both recreate and Drop |
| Pipeline cache | Saved to disk before destruction in Drop |
| Descriptor pool/layout | Destroyed in `TextureRegistry::destroy` and `SceneBuffers::destroy` |
| Shared sampler | Created in `TextureRegistry::new`, destroyed in `TextureRegistry::destroy`, outlives all textures |
| StagingGuard RAII | Double-free prevented by `Option<Allocation>` pattern |
| Dynamic state persistence | Viewport/scissor set once before draw loop, persists across all pipeline binds per Vulkan spec |
| Vertex attribute layout | Locations 0-3 match Vertex struct field order exactly (pos/color/normal/uv) |
| Push constant layout | viewProj at offset 0 (64 bytes) + model at offset 64 (64 bytes) = 128 bytes, matches shader |
| Descriptor set bindings | Set 0 = COMBINED_IMAGE_SAMPLER (binding 0), Set 1 = SSBO (binding 0) + UBO (binding 1) + optional TLAS (binding 2) — all match shader declarations |
| Scene buffer frame indexing | Upload and bind both use `frame` index (frame-in-flight) — no cross-frame data race |
| TLAS barrier | Placed before render pass begin, correct stage/access masks |

---

## Delta from 2026-04-02 Audit

| Original Finding | Current Status |
|-----------------|---------------|
| R-01: render_finished semaphores not resized | **FIXED** — `recreate_for_swapchain` added |
| R-02: image_available over-allocated | **FIXED** — now `MAX_FRAMES_IN_FLIGHT` entries |
| R-03: pipelines not recreated after render pass | **FIXED** — full pipeline recreation in `recreate_swapchain` |
| R-06: vertex/index buffers in CpuToGpu | **FIXED** — staging upload to GpuOnly |
| R-07: allocator Arc::try_unwrap silent leak | **FIXED** — log::error + debug_assert |
| R-08/09: GpuBuffer/Texture no Drop guards | **FIXED** — Drop impls with warning + debug_assert |
| R-10: normal transform non-uniform scale | **FIXED** — inverse-transpose normal matrix via UBO |
| R-12: queue no external sync | **FIXED** — Mutex<vk::Queue> |

All 8 HIGH/MEDIUM findings from the April 2 audit are resolved. The renderer has matured significantly.
