# Issue #683: MEM-2-8: scene_buffers ray-budget HOST_VISIBLE buffer is 4 bytes — wastes a whole 64 KB block alignment

**File**: `crates/renderer/src/vulkan/scene_buffer.rs:599-604`
**Dimension**: GPU Memory

One `MAX_FRAMES_IN_FLIGHT × 4 B` buffer per frame for a single u32 atomic counter. `gpu-allocator` rounds host-visible sub-allocations up to its alignment requirement (typically 64 B+, padded to nonCoherentAtomSize 256 B). The 8 B of "real" content costs 512 B+ allocated, and the allocator likely reserves a fresh 16 MB host-visible block to satisfy the alignment-padded layout adjacent to the other UBOs.

**Fix**: Fold `ray_budget` into the camera UBO (it's already per-frame and read by the same fragment shader stage); save a buffer + descriptor binding + a HOST_VISIBLE allocation slot.

Alternative: use a single shared 8-byte buffer with frame-indexed offsets.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
