# Issue #651: SH-6: skin_vertices.comp has no bounds check on bone_offset + boneIdx — corrupt NIF skin data writes garbage geometry into BLAS

**File**: `crates/renderer/shaders/skin_vertices.comp:118-122`
**Dimension**: Shader Correctness

```glsl
xform = boneW.x * bones[bone_offset + boneIdx.x]
      + boneW.y * bones[bone_offset + boneIdx.y]
      + boneW.z * bones[bone_offset + boneIdx.z]
      + boneW.w * bones[bone_offset + boneIdx.w];
```

No bounds check against MAX_BONES_PER_MESH (128) or against the actual skinned-mesh bone count. Bethesda meshes routinely ship with 4-byte bone indices where only the bottom byte is used and the upper three bytes are random — a corrupted index byte (rare but observed in modded NIFs) reads outside the per-mesh palette into another mesh's bones, producing wild transforms.

The skinned vertex output then feeds Phase 2's per-mesh BLAS refit; out-of-bounds vertices place triangles at gigantic distances in the TLAS, breaking ray queries cluster-wide for that frame.

triangle.vert:158-161 has the same lack of check, but raster mode is more forgiving (degenerate triangle off-screen). The compute path's output drives BLAS geometry and is harder to recover from.

**Fix**: Either clamp in-shader:
```glsl
uint maxIdx = uint(bones.length()) - bone_offset;
uvec4 idxClamped = min(boneIdx, uvec4(maxIdx - 1u));
```

Or validate the bone palette range CPU-side before dispatch (skin_compute.rs already plumbs bone_offset per dispatch — extend with a max-index assertion against the bone-palette length).

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
