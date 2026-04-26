# Issue #656: LIFE-M3: Texture::Drop has only debug_assert + log::warn — release builds silently leak GPU memory if destroy() was missed

**File**: `crates/renderer/src/vulkan/texture.rs:598-607`
**Dimension**: Resource Lifecycle

`Texture::Drop` checks `self.allocation.is_some()` and logs / debug_asserts. In release, the allocation (an `Option<gpu_allocator::Allocation>`) is just dropped on the floor. `gpu-allocator`'s `Allocation::Drop` does NOT free the GPU memory back to the allocator (the allocator owns the slab; the Allocation is just a handle).

Result: every dropped-without-destroy Texture leaks: VkImage, VkImageView, VkSampler, AND the GPU memory chunk. Pattern exists because Texture has no allocator handle to call `free` from Drop.

**Fix**: Either:
- (a) Hold `Arc<SharedAllocator>` inside Texture so Drop can self-free, OR
- (b) Keep the manual `destroy()` discipline but add a release-build counter that panics in CI.

Option (a) is the right answer given this is a lifecycle-correctness issue.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
