# SAFE-D3-2026-09-21-01: `.expect()` on a poisoned allocator lock is reachable from `Drop` / teardown (double panic → abort)

**Labels**: low, safety, renderer, concurrency, bug

Filed via /audit-publish from docs/audits/AUDIT_SAFETY_2026-09-21.md.

**Severity**: LOW · **Dimension**: 3 — Leaks / drop ordering
**Location**:
- `crates/renderer/src/vulkan/buffer.rs`: `StagingGuard::cleanup`, called from `impl Drop for StagingGuard` (~:637-658)
- `crates/renderer/src/vulkan/buffer.rs`: `GpuBuffer::destroy` (~:1471-1475)
- `crates/renderer/src/vulkan/buffer.rs`: `StagingPool::trim_to` (~:343-347), reached from `StagingPool::destroy()` on every teardown since #4187
- `crates/renderer/src/vulkan/texture.rs`: `Texture::destroy` (~:631-635)
- `crates/renderer/src/vulkan/context/teardown.rs`: `impl Drop for VulkanContext`, `mutex.into_inner().expect("allocator lock poisoned")` (~:432)

**Status**: NEW. The code is pre-existing; the safety audit's Dim-3 rule names this pattern, but earlier runs did not file it.
**Verified against**: HEAD `f97775ca8`

## Description

- `StagingGuard` is the RAII unwind guard (#2164). Its `Drop` → `cleanup()` does `allocator.lock().expect("allocator lock poisoned").free(alloc).expect("Failed to free staging allocation")`. The destroy and teardown paths listed above repeat the pattern.
- A panic on any thread while it holds the shared allocator mutex poisons the mutex.
  - If a `StagingGuard` is then dropped during that panic's unwind, the `expect` panics a second time inside `Drop`, and the process aborts.
  - If `VulkanContext::drop` or a `destroy()` call reaches the poisoned lock later, that call panics, and the rest of teardown is skipped.
- #4089 moved only `GpuImage` (`crates/renderer/src/vulkan/image.rs`) onto `into_inner()` recovery. The workspace release profile is `panic = "unwind"`, so poisoning can happen in shipped builds.

## Evidence

```rust
// buffer.rs: StagingGuard::cleanup (called from Drop)
self.allocator.lock().expect("allocator lock poisoned").free(alloc).expect("Failed to free staging allocation");
// context/teardown.rs: VulkanContext::drop
Ok(mutex) => drop(mutex.into_inner().expect("allocator lock poisoned")),
// image.rs: GpuImage (#4089), the recovery the other sites lack
Err(poisoned) => poisoned.into_inner(),
```

## Impact

- The abort or panic skips the rest of teardown. `VulkanContext::drop` frees the texture registry, scene buffers and acceleration structures before `save_pipeline_cache`, `destroy_device` and `destroy_instance`. A poisoned lock hit there therefore loses the pipeline-cache save and the device and instance destroy.
- Triggering it requires a panic inside a lock-held region, hence LOW.

## Related

- #4089 (closed): the `GpuImage` poison policy these sites should share.
- #2398 (closed): silent recovery needs a rationale comment.
- #95 (closed): the poison-cascade class.
- SAFE-D2-2026-09-21-01 (#4593): the same `StagingGuard` / `StagingPool` free paths.
- `docs/audits/AUDIT_CONCURRENCY_2026-09-21.md` cites this finding without re-reporting it.

## Suggested Fix

- Route the free-side and `Drop`-side locks through the `GpuImage` recovery (`unwrap_or_else(PoisonError::into_inner)`, with the #2398 rationale comment).
- Inside `Drop`, log `free()` errors instead of calling `expect` on them.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D3-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every `.lock().expect("allocator lock poisoned")` on a free, destroy or `Drop` path in `crates/renderer` converted (allocation-side `expect`s may stay)
- [ ] **DROP**: with a poisoned allocator, `VulkanContext::drop` still reaches `save_pipeline_cache`, `destroy_device` and `destroy_instance`, in reverse order
- [ ] **LOCK_ORDER**: no new lock scope introduced by the recovery path
- [ ] **TESTS**: the shared recovery helper is unit-tested against a poisoned mutex, or a source pin forbids `expect("allocator lock poisoned")` on free paths
