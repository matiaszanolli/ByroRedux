# Issue #680: MEM-2-5: per-frame HOST_VISIBLE buffers rely on gpu-allocator persistent mapping with no runtime assert

**File**: `crates/renderer/src/vulkan/buffer.rs:443-490` (create_host_visible) + `crates/renderer/src/vulkan/acceleration.rs:1866` (tlas.instance_buffer.write_mapped)
**Dimension**: GPU Memory

Every per-frame HOST_VISIBLE buffer (light, camera, bone, instance, indirect, ray-budget, TLAS instance staging) relies on `Allocation::mapped_slice_mut` returning a persistent mapping. `gpu-allocator` 0.27 maps `CpuToGpu` linear allocations once at `allocate()` and keeps them mapped for the allocation's lifetime — that contract is documented in gpu-allocator but not asserted at our buffer construction.

A future allocator config flag (or a backend swap) that defers mapping would silently make every `write_mapped` panic on "Buffer not mapped". Worth pinning with a startup-time assert.

**Fix**: Add the assert in `create_host_visible`:
```rust
debug_assert!(allocation.mapped_slice().is_some(), "per-frame HOST_VISIBLE buffer must be persistently mapped");
```
so a regression in mapping policy fails loudly at startup, not on the first write.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
