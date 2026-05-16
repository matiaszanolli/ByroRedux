# CONC-D3-NEW-01: AccelerationManager::destroy does not drain skinned_blas

**GitHub**: #1138
**Severity**: MEDIUM
**Audit**: AUDIT_CONCURRENCY_2026-05-16.md
**Status**: CONFIRMED

## Location
- `crates/renderer/src/vulkan/acceleration/mod.rs:244-287` (destroy impl — missing skinned_blas loop)
- `crates/renderer/src/vulkan/context/mod.rs:2067-2085` (only correct production caller)

## Summary
`destroy()` drains `blas_entries`, `tlas`, `scratch_buffers`, `blas_scratch_buffer` but NOT
`skinned_blas: HashMap<EntityId, BlasEntry>`. Production shutdown works because `context/mod.rs`
pre-drains via `drop_skinned_blas` before calling `destroy()`. Any direct `destroy()` call
(tests, error-path refactors) leaks `VkAccelerationStructureKHR` handles.

## Fix
Add loop inside `destroy()`:
```rust
for (_eid, mut entry) in self.skinned_blas.drain() {
    self.accel_loader.destroy_acceleration_structure(entry.accel, None);
    entry.buffer.destroy(device, allocator);
}
```
Place before `blas_scratch_buffer` teardown. App-level pre-drain becomes an optimization
(deferred destruction), not a correctness requirement.
