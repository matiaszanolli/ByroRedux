# REN-CMD-M1: accel.tick_deferred_destroy runs before wait_for_fences — use-after-free on BLAS eviction

**Issue**: #418 — https://github.com/matiaszanolli/ByroRedux/issues/418
**Labels**: bug, renderer, medium, sync

---

## Finding

`crates/renderer/src/vulkan/context/draw.rs:76-84` calls `accel.tick_deferred_destroy` BEFORE `wait_for_fences` at line 96.

```rust
// draw.rs:76-84 — tick_deferred_destroy
accel.tick_deferred_destroy(&self.device, allocator);
// ... more setup ...
// draw.rs:96 — wait_for_fences
self.device.wait_for_fences(&[in_flight_fence], true, u64::MAX)?;
```

For mesh and texture registries, the tick is fine — CPU-only bookkeeping. But `AccelerationManager::tick_deferred_destroy` calls `destroy_acceleration_structure` + `buffer.destroy`, which require the GPU to not be using those resources.

## Impact

If the BLAS aging out is still referenced by the other in-flight TLAS (on frame slot `prev`), this destroys it while the GPU is reading it.

Currently latent because the deferred counter is set to `MAX_FRAMES_IN_FLIGHT` (conservative), but the ordering violates the invariant "wait on GPU completion before destroying resources it might still touch". A change that shortens the deferred counter would make this a live bug.

## Fix

Move the `accel.tick_deferred_destroy` call to AFTER `wait_for_fences` (after `draw.rs:106`, alongside `reset_fences`):

```rust
self.device.wait_for_fences(&[in_flight_fence], true, u64::MAX)?;
self.device.reset_fences(&[in_flight_fence])?;
accel.tick_deferred_destroy(&self.device, allocator);
```

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check other `tick_*` or deferred-destroy patterns (mesh_registry, texture_registry) don't touch GPU resources. If they do, move them too.
- [ ] **DROP**: Verify `AccelerationManager::tick_deferred_destroy` doesn't assume a particular caller ordering — if it does, document at the call site.
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Stress test that rapidly loads + unloads cells should fire the BLAS eviction path with zero sync2 validation errors.

## Source

Audit: `docs/audits/AUDIT_RENDERER_2026-04-18.md`, Dim 5 M-1.
