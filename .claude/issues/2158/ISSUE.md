# 2158: RL-D6-05: FrameUpscaler teardown (incl. allocator-independent FSR SDK context) sits entirely inside the Some(allocator) guard

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2158
**Labels**: bug, low, vulkan

---

## Severity
LOW

## Dimension
Resource Lifecycle (GPU teardown ordering) — `/audit-concurrency` 2026-07-25

## Location
`crates/renderer/src/vulkan/context/mod.rs:3295-3299`, `crates/renderer/src/vulkan/frame_upscaler.rs:788-805`

## Description
`FrameUpscaler` is a mixed subsystem: its per-FIF output images need the gpu-allocator, but its `fsr3::Context` (SDK-side pipelines, descriptor pools, its own `VkDeviceMemory` outside gpu-allocator's view) does not. `destroy` calls `self.context.take()` first, but the whole call sits inside the `if let Some(ref alloc) = self.allocator` guard — so on any future allocator-`None` Drop path the SDK context would be dropped after `vkDestroyDevice` (or not at all), the exact failure mode #1483 was filed against.

## Evidence
`mod.rs:3169-3208` documents the #1483 rule and its exception list (only `skin_compute` is currently exempted, for descriptor-pool ordering reasons); `self.allocator` is only ever `take()`n inside Drop itself today, so this is latent, not live.

## Impact
None today. Becomes a driver-level use-after-free the moment an allocator-`None` Drop path is reintroduced — which #1426/#1483 show has happened before.

## Related
#1483 (closed), #1426 (closed), #665 (closed).

## Suggested Fix
Split `FrameUpscaler::destroy` into `destroy_device_objects(&device)` (SDK context) and `destroy_allocations(&device, &alloc)` (output images/views), hoisting the first into the allocator-independent block next to `presentation.destroy()`; or add the exception to the ordering comment at `mod.rs:3184-3188` so a future reader knows it was considered.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
