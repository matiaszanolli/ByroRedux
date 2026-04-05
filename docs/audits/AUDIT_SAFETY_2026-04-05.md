# Safety Audit — 2026-04-05

**Scope**: Full codebase — unsafe blocks, Vulkan spec, GPU memory, thread safety, FFI
**Auditor**: Claude Opus 4.6 (renderer-specialist + ecs-specialist agents)

## Summary

| Severity | Count |
|----------|-------|
| HIGH | 3 |
| MEDIUM | 8 |
| LOW | 8 |

No CRITICAL findings. The codebase is fundamentally sound — Vulkan Drop ordering is correct, ECS lock ordering enforced for 2-component queries, FFI surface is trivially safe. The main risk areas are: (1) staging buffer leaks on GPU allocation error paths, (2) thread safety gaps that will become live deadlocks under parallel dispatch, and (3) a timing assumption in deferred texture destruction.

---

## HIGH

### SAFE-01: `write_mapped` silently truncates data exceeding buffer size
- **Severity**: HIGH
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/buffer.rs:175-176`
- **Status**: NEW
- **Description**: `write_mapped` uses `let len = bytes.len().min(mapped.len())` and copies only `len` bytes. If the caller passes data larger than the buffer, the excess is silently discarded. For scene buffers and TLAS instance data, this means lights or instances beyond capacity are dropped with no diagnostic.
- **Impact**: Silent data loss — shader reads partial data, incorrect rendering with no error.
- **Suggested Fix**: Log a warning when `bytes.len() > mapped.len()`. Ideally return an error.

### SAFE-02: `build_tlas` error swallowed during initial empty TLAS creation
- **Severity**: HIGH
- **Dimension**: Vulkan Spec
- **Location**: `crates/renderer/src/vulkan/context.rs:217`
- **Status**: NEW
- **Description**: `let _ = accel.build_tlas(...)` inside a `with_one_time_commands` closure discards the error. If buffer allocation fails mid-recording, the command buffer is in an inconsistent state but still gets submitted.
- **Impact**: Partially-recorded command buffer submitted, potentially causing GPU hangs.
- **Suggested Fix**: Propagate the error — either change the closure to return `Result` or handle the failure before submit.

### SAFE-03: Same-thread read-then-write deadlock on RwLock (no reentrant guard)
- **Severity**: HIGH
- **Dimension**: Thread Safety
- **Location**: `crates/core/src/ecs/world.rs:172-186`
- **Status**: NEW
- **Description**: `std::sync::RwLock` is not reentrant. `world.query::<T>()` followed by `world.query_mut::<T>()` on the same thread deadlocks silently. The `query_2_mut` API guards against this for 2-component queries, but nothing prevents ad-hoc same-type double-locking.
- **Impact**: Instant deadlock if a developer accidentally holds read + write on the same type. Already implicitly worked around in `main.rs` (BFS loop drops reads before writes).
- **Suggested Fix**: Debug-mode thread-local tracker that records locked TypeIds per thread and panics on same-type read→write.

---

## MEDIUM

### SAFE-04: Staging buffer leak on image allocation failure (from_rgba)
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/texture.rs:112-123`
- **Status**: NEW
- **Description**: After staging buffer is created and filled, if device-local image allocation fails, the `?` returns early. The staging VkBuffer handle and gpu-allocator Allocation are dropped without being freed.
- **Impact**: GPU memory leak on OOM error paths.
- **Suggested Fix**: Use a cleanup guard (scopeguard or RAII wrapper) around staging resources.

### SAFE-05: Same staging buffer leak in from_bc
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/texture.rs:345-363`
- **Status**: NEW
- **Description**: Same pattern as SAFE-04 in the BC texture upload path.

### SAFE-06: Same staging buffer leak in GpuBuffer::create_device_local_buffer
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/buffer.rs:272-302`
- **Status**: NEW
- **Description**: Same pattern as SAFE-04 in the buffer staging upload path.

### SAFE-07: `update_rgba` deferred destroy assumes 1 update = 1 frame
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/texture_registry.rs:242-259`
- **Status**: NEW
- **Description**: `pending_destroy` holds one texture. If `update_rgba` is called twice in the same frame, the texture from only 1 frame ago is destroyed while potentially still referenced by an in-flight command buffer (MAX_FRAMES_IN_FLIGHT = 2).
- **Impact**: Use-after-free on GPU if called >1x/frame for same handle.
- **Suggested Fix**: Use a ring buffer of size MAX_FRAMES_IN_FLIGHT, or tag with frame counter.

### SAFE-08: `nonCoherentAtomSize` alignment not enforced on flush
- **Severity**: MEDIUM
- **Dimension**: Vulkan Spec
- **Location**: `crates/renderer/src/vulkan/buffer.rs:183-190`
- **Status**: NEW
- **Description**: `flush_mapped_memory_ranges` uses `alloc.offset()` without rounding to `nonCoherentAtomSize`. With `WHOLE_SIZE`, the size requirement is met but offset alignment is not guaranteed. Validation error on hardware with atomSize > 1.
- **Impact**: Validation layer error. Practically rare on desktop (most memory is coherent).
- **Suggested Fix**: Round offset down to `nonCoherentAtomSize`. Use `alloc.size()` instead of `WHOLE_SIZE`.

### SAFE-09: No lock ordering for ad-hoc multi-query (N>2)
- **Severity**: MEDIUM
- **Dimension**: Thread Safety
- **Location**: `byroredux/src/main.rs:1758-1763`
- **Status**: NEW
- **Description**: The render system acquires `query_2_mut` (TypeId-ordered) then 5 additional read queries with no ordering. Currently safe (all reads, sequential scheduler), but becomes a deadlock risk under parallel dispatch.
- **Suggested Fix**: Provide `query_N_mut` builder or runtime lock-order validator for debug builds.

### SAFE-10: Resource locks have no ordering discipline
- **Severity**: MEDIUM
- **Dimension**: Thread Safety
- **Location**: `crates/core/src/ecs/world.rs:285-324`
- **Status**: NEW
- **Description**: Unlike component queries, resource access has zero TypeId ordering enforcement. Systems can acquire `resource_mut::<A>()` then `resource_mut::<B>()` in arbitrary order.
- **Impact**: ABBA deadlock risk under parallel dispatch.
- **Suggested Fix**: Add same thread-local tracking as SAFE-03. Consider `resource_2_mut` API.

### SAFE-11: Pipeline cache loaded from untrusted CWD path
- **Severity**: MEDIUM
- **Dimension**: Vulkan Spec
- **Location**: `crates/renderer/src/vulkan/context.rs:1250-1274`
- **Status**: NEW
- **Description**: `pipeline_cache.bin` is read from CWD and passed as initial_data to `vk::PipelineCacheCreateInfo`. Vulkan drivers validate the header, but a malicious file could exploit driver bugs.
- **Impact**: Low practical risk. Defense-in-depth concern.
- **Suggested Fix**: Validate cache header (vendor/device ID) before passing to driver. Use config dir, not CWD.

---

## LOW

### SAFE-12: Swapchain raw pointer to stack-local array (fragile but sound)
- **Severity**: LOW
- **Dimension**: Unsafe Blocks
- **Location**: `crates/renderer/src/vulkan/swapchain.rs:59-88`
- **Status**: NEW
- **Description**: `queue_family_indices.as_ptr()` stored as raw pointer and used in struct init. Array and struct are in the same function scope, so currently valid. No SAFETY comment.
- **Suggested Fix**: Switch to builder pattern (`.queue_family_indices(&arr)`) or add SAFETY comment.

### SAFE-13: `upload_lights` raw `ptr::copy_nonoverlapping` without SAFETY comments
- **Severity**: LOW
- **Dimension**: Unsafe Blocks
- **Location**: `crates/renderer/src/vulkan/scene_buffer.rs:226-239`
- **Status**: NEW
- **Description**: Two `copy_nonoverlapping` calls serialize `#[repr(C)]` structs. Correct but undocumented.
- **Suggested Fix**: Add SAFETY comments or use `bytemuck::bytes_of`.

### SAFE-14: Poisoned lock cascade — panicking system poisons all storages
- **Severity**: LOW
- **Dimension**: Thread Safety
- **Location**: `crates/core/src/ecs/world.rs` (all `.expect("...lock poisoned")`)
- **Status**: NEW
- **Description**: If a system panics while holding a lock, all subsequent systems touching the same storage panic with "lock poisoned", losing the original error.
- **Suggested Fix**: Use `catch_unwind` at scheduler level, or recover from poisoned locks.

### SAFE-15: Depth image leak on error in create_depth_resources
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/context.rs:1162-1204`
- **Status**: NEW
- **Description**: If `bind_image_memory` or `create_image_view` fails after allocation succeeds, the image and allocation leak.
- **Suggested Fix**: Cleanup guards.

### SAFE-16: `flush_mapped_memory_ranges` with `WHOLE_SIZE` flushes beyond sub-allocation
- **Severity**: LOW
- **Dimension**: Vulkan Spec
- **Location**: `crates/renderer/src/vulkan/buffer.rs:183-190`
- **Status**: NEW
- **Description**: With gpu-allocator sub-allocation, `WHOLE_SIZE` from `alloc.offset()` flushes to the end of the entire backing VkDeviceMemory, not just this allocation's range. Over-flushing is safe but wastes cache bandwidth.
- **Suggested Fix**: Use `alloc.size()` instead of `WHOLE_SIZE`.

### SAFE-17: CStr::from_ptr on driver strings without SAFETY comments
- **Severity**: LOW
- **Dimension**: Unsafe Blocks
- **Location**: `crates/renderer/src/vulkan/device.rs:53,80`
- **Status**: NEW
- **Description**: `CStr::from_ptr` on device name and extension names from Vulkan driver. Correct (spec guarantees null termination) but undocumented.

### SAFE-18: Union field initialization in acceleration.rs without SAFETY comments
- **Severity**: LOW
- **Dimension**: Unsafe Blocks
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:83-91,242-244`
- **Status**: NEW
- **Description**: `DeviceOrHostAddressConstKHR` and `AccelerationStructureReferenceKHR` union initializations lack SAFETY comments on device address validity.

### SAFE-19: `update_rgba` descriptor set write races with in-flight command buffers
- **Severity**: LOW
- **Dimension**: Vulkan Spec
- **Location**: `crates/renderer/src/texture_registry.rs:267-278`
- **Status**: NEW
- **Description**: All descriptor sets (all swapchain images) are rewritten simultaneously. An in-flight command buffer may reference a set that was just rewritten to point to the new image. The old image is still alive (pending_destroy), so no GPU fault, but the descriptor update races with in-flight usage.
- **Impact**: Validation warning. Brief visual glitch possible.
- **Suggested Fix**: Only update the current frame's descriptor set; update others when their fence signals.

---

## Positive Findings

| Area | What's Correct |
|------|---------------|
| VulkanContext::Drop | Correct reverse-creation order; device_wait_idle first |
| Allocator drop | Arc::try_unwrap with error logging; device destroyed after allocator |
| Semaphore indexing | image_available per frame-in-flight, render_finished per image |
| query_2_mut | TypeId ordering + same-type panic guard |
| Component trait | Storage associated type with Send + Sync bounds |
| FFI (cxx bridge) | Trivially safe — one struct, two functions, no raw pointers |
| ECS: no unsafe | Zero unsafe blocks in entire crates/core/src/ecs/ |

---

## Priority Action Items

1. **Fix SAFE-01** (write_mapped truncation) — add warning log + optional error return
2. **Fix SAFE-02** (build_tlas error swallowed) — propagate error out of closure
3. **Fix SAFE-04/05/06** (staging leaks) — add scopeguard cleanup for staging resources
4. **Fix SAFE-07** (deferred destroy timing) — use frame-counted ring buffer
5. **Design SAFE-03** (reentrant lock guard) — debug-mode thread-local TypeId tracker
