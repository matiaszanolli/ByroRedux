# D5-02: GpuBuffer::destroy leaves a dangling self.buffer handle

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2487
**Finding ID**: D5-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 5 — Memory/Lifecycle
**Location**: `crates/renderer/src/vulkan/buffer.rs:887` (`GpuBuffer::destroy`)
**Status**: NEW

## Description
`destroy()` takes `self.allocation`, destroys the `VkBuffer`, frees the allocation and drops the allocator `Arc` — but never nulls `self.buffer`. The struct stays alive with a stale, already-destroyed `vk::Buffer` in a `pub` field. Double-free is correctly prevented (the `allocation.take()` gate) and the `Drop` safety net correctly short-circuits, so the leak/double-free axes are clean. What is not defended is a *read*: any code that keeps the `GpuBuffer` and later reads `.buffer` gets a destroyed handle with no way to tell.

## Evidence
```rust
pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
    if let Some(allocation) = self.allocation.take() {
        unsafe { device.destroy_buffer(self.buffer, None); }   // self.buffer left as-is
        allocator.lock()...free(allocation).expect(...);
    }
    self.allocator = None;
}
```
Contrast the sibling helpers, which do null out: `destroy_depth_resources` nulls the view/image handles ("Each handle is nulled by the helper so a later Drop is a no-op"), and `TextureRegistry::destroy` nulls `depth_history_sampler`.

## Impact
Latent only. Today every call site either consumes the `GpuBuffer` or is in a teardown path with no subsequent read, so there is no live defect. The exposure is a future call site that destroys through a long-lived `&mut GpuBuffer` and then binds `.buffer` — a class of bug invisible to `cargo test` and only visible as a validation-layer complaint or GPU fault.

## Related
`#656` (Drop safety net), `#927` (allocator `Arc` release in `destroy`) — both hardened this same function; nulling the handle is the remaining sibling.

## Suggested Fix
Add `self.buffer = vk::Buffer::null();` after the `destroy_buffer` call (and the matching line in the `Drop` safety-net arm), matching the `destroy_depth_resources` convention.

## Completeness Checks
- [ ] **TESTS**: A unit test confirms `.buffer` reads as `vk::Buffer::null()` after `destroy()`
