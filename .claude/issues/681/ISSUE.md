# Issue #681: MEM-2-6: skin compute output buffer requests unused VERTEX_BUFFER usage flag for deferred Phase 3

**File**: `crates/renderer/src/vulkan/skin_compute.rs:274-282`
**Dimension**: GPU Memory

`SkinSlot::output_buffer` is allocated with `STORAGE_BUFFER | SHADER_DEVICE_ADDRESS | ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR | VERTEX_BUFFER`. Phase 3 (raster reading skinned output as VBO) is explicitly deferred per the file's module docstring.

The unused `VERTEX_BUFFER` flag itself is harmless on most drivers but tightens the memory-type mask gpu-allocator must satisfy — on a unified-memory mobile / iGPU config it can force the allocation onto a smaller heap than necessary.

**Fix**: Drop `VERTEX_BUFFER` from the usage mask until Phase 3 lands; re-add in the same commit that wires the raster path.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
