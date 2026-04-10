# Concurrency & Synchronization Audit — 2026-04-10

**Auditor**: ECS Specialist × 2 + Renderer Specialist × 2 (Claude Opus 4.6, parallel)
**Scope**: ECS lock ordering, Vulkan synchronization, resource lifecycle, thread safety
**Depth**: Deep — traced concurrent paths and lock acquisition ordering

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0     |
| HIGH     | 1     |
| MEDIUM   | 7     |
| LOW      | 7     |

15 findings total across 4 dimensions. No data races or deadlocks found.

The single HIGH finding is `RL-4`: `recreate_swapchain` destroys old Vulkan handles, then
if any subsequent creation fails, Drop double-destroys the stale handles — undefined behavior.
The fix is straightforward: null out handles after destroying them.

The recurring MEDIUM theme is **error-path resource leaks**: 4 separate `::new()` constructors
(`SsaoPipeline`, `ClusterCullPipeline`, `SceneBuffers`, `VulkanContext`) all use sequential
`vkCreate* + ?` with no cleanup of prior successful creations on error.

The ECS locking infrastructure is sound — TypeId-sorted lock acquisition, debug-mode lock
tracker for reentrancy detection, and all systems use sequential lock/drop patterns.

---

## Dimension 1: ECS Locking

No new findings. The core locking infrastructure was verified correct:

- `query_2_mut` / `query_2_mut_mut` sort by TypeId before acquiring locks and panic on same-type
- All systems use sequential lock/drop patterns — no two component locks held simultaneously
  (except Children read held across BFS in `transform_propagation_system`, which is existing #46)
- The lock tracker correctly detects same-thread reentrant deadlocks in debug builds
- `animation_system` nested queries (AnimationStack read → Name read → NameIndex write) are
  sequential with explicit drops between acquisitions — no ordering conflict

---

## Dimension 2: Vulkan Synchronization

### SYNC-07: AO image sampled in UNDEFINED layout on first frame
- **Severity**: MEDIUM
- **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/ssao.rs:71,260` and `crates/renderer/src/vulkan/context/draw.rs:465`
- **Trigger Conditions**: First frame after SSAO pipeline creation, and first frame after swapchain resize
- **Status**: NEW
- **Description**: The AO image is created with `initial_layout = UNDEFINED` (ssao.rs:71). The scene descriptor set binding 7 is written with `image_layout = SHADER_READ_ONLY_OPTIMAL`. On the first frame, the render pass fragment shader samples the AO texture BEFORE the SSAO dispatch has run (SSAO runs AFTER the render pass). So on frame 0, the image is in UNDEFINED layout but the descriptor claims SHADER_READ_ONLY_OPTIMAL — sampling an image in UNDEFINED layout is undefined behavior per the Vulkan spec.
- **Evidence**: SSAO dispatch at draw.rs:465 runs after the render pass (draw.rs:461 `cmd_end_render_pass`). Fragment shader reads AO texture during the render pass via `triangle.frag:546`.
- **Impact**: Undefined behavior on frame 0 and first frame after resize. Typically reads garbage (0 or noise), so AO is wrong for one frame. Validation layers will flag this. Some drivers may crash.
- **Suggested Fix**: After creating the SSAO pipeline, issue a one-time command to transition the AO image from UNDEFINED to SHADER_READ_ONLY_OPTIMAL and clear it to white (1.0 = no occlusion).

### SYNC-05: Descriptor set writes to all frames without UPDATE_AFTER_BIND
- **Severity**: MEDIUM
- **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/texture_registry.rs:166-172, 200-207, 254-259`
- **Trigger Conditions**: Texture load while a frame's command buffer referencing the same descriptor set is in flight
- **Status**: NEW
- **Description**: When a new texture is registered, `write_texture_to_all_sets` writes to ALL per-frame bindless sets unconditionally. PARTIALLY_BOUND is enabled but UPDATE_AFTER_BIND is not. Per VUID-vkUpdateDescriptorSets-None-03047, sets must not be updated while in use by a submitted command buffer unless UPDATE_AFTER_BIND is enabled. Currently safe in practice because only previously-unbound array elements are written, but technically a spec violation.
- **Evidence**: `texture_registry.rs:66` enables PARTIALLY_BOUND but not UPDATE_AFTER_BIND_BIT.
- **Impact**: Validation layer warnings. Practically safe because only unused array elements are written. The real danger is `update_rgba` (existing #92) which writes to actively-used elements.
- **Suggested Fix**: Enable `VK_DESCRIPTOR_BINDING_UPDATE_AFTER_BIND_BIT` on the bindless array binding (requires pool and layout flag changes), or defer descriptor writes to after the fence wait.

### SYNC-03: SSAO param UBO host write lacks barrier before compute dispatch
- **Severity**: MEDIUM
- **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/ssao.rs:329`
- **Trigger Conditions**: Every frame when SSAO is enabled, on non-HOST_COHERENT memory
- **Status**: NEW
- **Description**: `param_buffers[frame].write_mapped()` performs a host write at line 329. The compute dispatch at line 368 reads this UBO. The intervening image barrier (lines 332-353) covers only the AO image layout transition — its access masks are `SHADER_READ → SHADER_WRITE` on the image, not `HOST_WRITE` on the UBO. The param UBO host write is not made available to the compute shader.
- **Evidence**: No barrier with `src_access=HOST_WRITE, dst_access=UNIFORM_READ` between lines 329 and 368.
- **Impact**: On non-HOST_COHERENT memory, compute shader reads stale SSAO parameters. Desktop GPUs are typically coherent.
- **Suggested Fix**: Add a memory barrier: `src_stage=HOST, dst_stage=COMPUTE_SHADER, src_access=HOST_WRITE, dst_access=UNIFORM_READ`.

### SYNC-02: Missing HOST-to-FRAGMENT memory dependency for scene SSBOs
- **Severity**: LOW
- **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:207-241`
- **Trigger Conditions**: Non-HOST_COHERENT CpuToGpu memory (mobile/embedded GPUs)
- **Status**: NEW
- **Description**: The HOST→COMPUTE barrier (lines 212-225) makes host writes to light/camera SSBOs visible to the compute stage. The COMPUTE→FRAGMENT barrier (lines 228-240) only has `src_access=SHADER_WRITE`, which covers compute output but does not re-include the prior host writes for fragment visibility. On HOST_COHERENT memory (all desktop GPUs), the execution dependency chain suffices. On non-coherent memory, the fragment shader could read stale data.
- **Impact**: Only affects non-coherent mobile GPUs. Desktop unaffected.
- **Suggested Fix**: Add HOST_WRITE to the COMPUTE→FRAGMENT barrier's src_access_mask, or document HOST_COHERENT as a device requirement.

### SYNC-06: `recreate_swapchain` does not reset `current_frame` counter
- **Severity**: LOW
- **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/context/resize.rs`
- **Status**: NEW
- **Description**: After recreate, `current_frame` is not reset to 0. Since new fences start SIGNALED and `images_in_flight` is cleared, this is safe — but the frame-in-flight counter may cause one redundant fence wait.
- **Impact**: No corruption. Minor performance micro-stall at most.
- **Suggested Fix**: Add `self.current_frame = 0;` at the end of `recreate_swapchain`.

### SYNC-08: Present and graphics queue alias behind separate Mutexes
- **Severity**: LOW
- **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:103-107`
- **Status**: NEW
- **Description**: When graphics and present use the same queue family, both Mutexes wrap the same VkQueue handle. Currently safe (locks never held simultaneously), but a latent hazard if async upload threads are added.
- **Impact**: No current bug. Design smell.
- **Suggested Fix**: When families match, use a single `Arc<Mutex<VkQueue>>` cloned for both fields.

---

## Dimension 3: Resource Lifecycle

### RL-4: `recreate_swapchain` double-destroys stale handles on mid-resize error
- **Severity**: HIGH
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:20-181`
- **Trigger Conditions**: Any fallible creation operation failing during resize (swapchain, depth, render pass, pipelines, descriptor sets, command buffers, sync objects)
- **Status**: NEW
- **Description**: Lines 20-53 destroy old resources (framebuffers, depth image/view, pipelines, render pass, swapchain image views). Then lines 57+ use `?` to propagate errors from creation calls. If any creation fails, Drop will re-destroy the stale handles still stored in `self` — double-free / use-after-destroy on Vulkan objects.
- **Evidence**: Lines 21-53 destroy old handles, but `self.framebuffers`, `self.depth_image`, `self.render_pass`, etc. still hold the old (now-invalid) values until overwritten by successful creation. On `?` error return, Drop fires on the inconsistent state.
- **Impact**: Vulkan validation errors, potential driver crashes, undefined behavior. Triggered by any transient allocation failure during resize.
- **Suggested Fix**: Null out handles immediately after destroying them (`self.framebuffers.clear()`, `self.depth_image = vk::Image::null()`, etc.). Vulkan destroy functions accept null handles as no-ops.

### RL-15: `VulkanContext::new()` leaks partially-created state on error
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:136-407`
- **Trigger Conditions**: Any `?` propagation after device/instance/allocator are created
- **Status**: NEW
- **Description**: `new()` creates ~20 Vulkan objects sequentially with `?`. If any mid-chain creation fails, all prior objects are leaked — Drop never runs because the struct was never fully constructed. Instance, device, surface, allocator all leak, preventing clean re-initialization.
- **Impact**: On init failure, Vulkan instance + device + surface + allocator leak. Since init failure typically means the app exits, practical impact is moderate.
- **Suggested Fix**: Wrap early resources in their own Drop types, or build the struct incrementally with a partial-context that implements Drop.

### RL-11: `SsaoPipeline::new()` leaks GPU resources on mid-creation error
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/ssao.rs:56-302`
- **Trigger Conditions**: Any allocation/creation failure after `create_image` succeeds
- **Status**: NEW
- **Description**: Sequential `vkCreate* + ?` with no cleanup. If `ao_image` is created but allocation fails, the image leaks. Each subsequent `?` accumulates more leaked resources.
- **Impact**: GPU memory leak on partial SSAO creation failure. SSAO failure is handled gracefully (no AO), so leaked resources go unnoticed.
- **Suggested Fix**: Use scopeguard or error-path cleanup, as done in `AccelerationManager::build_blas()`.

### RL-12: `ClusterCullPipeline::new()` leaks buffers on mid-creation error
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/compute.rs:51-263`
- **Trigger Conditions**: Buffer or pipeline creation failure mid-constructor
- **Status**: NEW
- **Description**: Same pattern as RL-11. Buffers created in a loop with `?`, no cleanup of successfully-created buffers on later failure.
- **Impact**: GPU buffer leaks on partial creation. Cluster cull failure is handled gracefully.
- **Suggested Fix**: Same as RL-11.

### RL-13: `SceneBuffers::new()` leaks buffers on mid-creation error
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/scene_buffer.rs:186-463`
- **Trigger Conditions**: Buffer creation failure mid-loop
- **Status**: NEW
- **Description**: Same pattern. Four buffer types created per frame-in-flight slot. Failure in later buffers leaks earlier ones.
- **Impact**: SceneBuffers creation is fatal (no fallback), so leaked resources are reclaimed at exit. Low practical impact.
- **Suggested Fix**: Same as RL-11.

### RL-6: `StagingPool` has no Drop impl — silent resource leak if dropped without `destroy()`
- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/buffer.rs:14-114`
- **Trigger Conditions**: Currently theoretical — StagingPool is never instantiated
- **Status**: NEW
- **Description**: No Drop impl. If dropped without calling `destroy()`, GPU buffers and allocations leak silently. Inconsistent with `GpuBuffer`'s defensive Drop pattern.
- **Impact**: Currently zero — StagingPool is never instantiated.
- **Suggested Fix**: Add a Drop impl with `log::warn!` + `debug_assert!` if `free_list` is non-empty.

### RL-14: `TextureRegistry::new()` leaks descriptor pool on sampler creation failure
- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/texture_registry.rs:56-161`
- **Trigger Conditions**: Sampler creation failure after descriptor pool/layout creation
- **Status**: NEW
- **Description**: Same error-path leak pattern. Sampler creation failure is extremely rare.
- **Impact**: Low — fatal path, resources reclaimed at exit.
- **Suggested Fix**: Same as RL-11.

---

## Dimension 4: Thread Safety

### TS-01: Lock tracker compiles to no-ops in release builds
- **Severity**: MEDIUM
- **Dimension**: Thread Safety
- **Location**: `crates/core/src/ecs/lock_tracker.rs:194-246`
- **Trigger Conditions**: Release build with parallel scheduler. A system accidentally acquires a write lock while holding a read lock on the same component type.
- **Status**: NEW
- **Description**: The lock tracker that detects same-thread reentrant deadlocks is `#[cfg(not(debug_assertions))]` compiled to no-ops. A bug that panics helpfully in debug will silently hard-deadlock in release.
- **Impact**: Defense-in-depth gap. Not a current bug, but a latent risk for future development.
- **Suggested Fix**: Use `RwLock::try_write()` with timeout in the World query methods even in release, or keep the tracker active with minimal overhead (thread-local HashMap check is cheap vs RwLock).

### TS-02: Parallel systems can contend on same component storage without warning
- **Severity**: LOW
- **Dimension**: Thread Safety
- **Location**: `crates/core/src/ecs/scheduler.rs:127-131`
- **Trigger Conditions**: Two systems in same stage both call `world.query_mut::<Transform>()`
- **Status**: NEW
- **Description**: RwLock serializes correctly (no data race), but parallelism benefit is lost. No warning is emitted. The lock tracker is thread-local and cannot detect cross-thread contention.
- **Impact**: Performance regression, not correctness.
- **Suggested Fix**: Add debug-mode cross-thread contention logger per stage.

### TS-03: SwfPlayer Arc<Mutex> wrapper is architectural (not a bug)
- **Severity**: LOW
- **Dimension**: Thread Safety
- **Location**: `crates/ui/src/player.rs:24-28`
- **Status**: NEW (informational)
- **Description**: `SwfPlayer` wraps Ruffle's `Player` in `Arc<Mutex>` because that's what `PlayerBuilder::build()` returns. The Arc is never cloned. `UiManager` correctly lives outside the ECS (not a Resource) because Ruffle's backends are not Send+Sync. Well-documented at `lib.rs:7`.
- **Impact**: None. Correct design.

### TS-04: Allocator lock poisoning cascades as panic
- **Severity**: LOW
- **Dimension**: Thread Safety
- **Location**: `crates/renderer/src/vulkan/allocator.rs:45`
- **Status**: NEW
- **Description**: All allocator `.lock()` calls use `.expect()`, so a single poisoning event cascades into total renderer failure. For a game engine, panicking on corrupted GPU allocator state is arguably correct — there is no meaningful recovery.
- **Impact**: Acceptable behavior. Could use `unwrap_or_else(|e| e.into_inner())` in Drop for graceful shutdown.

---

## Skipped (Existing Issues)

| Issue | Finding | Status |
|-------|---------|--------|
| #18 | Texture destroy frees allocation before image | OPEN |
| #33 | Destruction order inconsistent between recreate and Drop | OPEN |
| #46 | Transform propagation acquires 4 locks per BFS node | OPEN |
| #92 | update_rgba descriptor writes race with in-flight frames | OPEN |
| #96 | Depth image leak on error in create_depth_resources | OPEN |
| #99 | StagingPool grows unboundedly | OPEN |
| #140 | Query/resource docs don't mention lock_tracker panics | OPEN |
| #196 | Instance SSBO uploaded after cluster cull dispatch | OPEN |
| #197 | Missing HOST→VERTEX_SHADER barrier | OPEN |

---

## Cross-Dimension Patterns

**Error-path resource leaks** (RL-11, RL-12, RL-13, RL-14, RL-15): Five constructors share the
same anti-pattern — sequential `vkCreate* + ?` with no cleanup of prior creations on failure.
The codebase already has the correct pattern in `AccelerationManager::build_blas()`. A single
`create_or_cleanup` helper or consistent use of `scopeguard` would fix all five.

**Host-to-device barrier gaps** (SYNC-02, SYNC-03, existing #197): Three separate host write
→ shader read paths lack explicit barriers, relying on HOST_COHERENT memory guarantees. A
consistent `flush_host_writes` barrier helper after all host uploads would close all three.

---

## Priority Fix Order

1. **RL-4** (HIGH) — Null out handles after destroying in `recreate_swapchain` to prevent double-free
2. **SYNC-07** (MEDIUM) — Transition AO image to valid layout on creation (one-time command)
3. **SYNC-05** (MEDIUM) — Enable UPDATE_AFTER_BIND for bindless descriptor array
4. **RL-11/12/13/15** (MEDIUM) — Add error-path cleanup to constructors (batch fix)
5. **TS-01** (MEDIUM) — Keep lock tracker active in release builds
