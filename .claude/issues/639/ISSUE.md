# Issue #639: LIFE-H1: AccelerationManager::destroy leaks every entry queued in pending_destroy_blas

**File**: `crates/renderer/src/vulkan/acceleration.rs:2198-2223`
**Dimension**: Resource Lifecycle

`drop_blas()` queues entries on `pending_destroy_blas` with a 2-frame countdown; `tick_pending_destroy` only fires from `draw_frame`. On Drop, `destroy()` only drains `blas_entries` and `tlas` — every entry whose countdown was still >0 leaks one VkAccelerationStructureKHR + one GpuBuffer.

Easy repro: load a cell, fast-travel (cell unload → BLAS retired to pending_destroy queue), quit the next frame.

**Fix**: At the top of `destroy()`, drain `self.pending_destroy_blas` and call the AS loader + buffer destroy on each entry. `device_wait_idle` in the parent Drop already covers in-flight cmd buf references.

```rust
for (mut entry, _countdown) in self.pending_destroy_blas.drain(..) {
    self.accel_loader.destroy_acceleration_structure(entry.accel, None);
    entry.buffer.destroy(device, allocator);
}
```

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
