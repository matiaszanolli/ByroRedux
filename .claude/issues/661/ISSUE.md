# Issue #661: SY-4: skin compute → BLAS refit barrier uses legacy ACCELERATION_STRUCTURE_READ_KHR

**File**: `crates/renderer/src/vulkan/context/draw.rs:559-572`
**Dimension**: Vulkan Sync

The `compute_to_blas` `MemoryBarrier` writes `dst_access = ACCELERATION_STRUCTURE_READ_KHR`. Per VK_KHR_acceleration_structure spec, AS *build inputs* (vertex / index / instance buffers) are best described by `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR` (sync2-only). `ACCELERATION_STRUCTURE_READ_KHR` is the access for *traversal* reads (TLAS dereferences inside ray queries), not build-input data reads.

The two are aliased on most drivers today, so no observable bug, but the comment block in acceleration.rs:603-605 already calls out the correct flag name. Inconsistency between the documented invariant and the actual barrier text.

**Fix**: Either rename the comment to match the code or, when the renderer migrates to sync2, switch dst access mask to `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
