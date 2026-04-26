# Issue #652: SH-7: cluster_cull.comp single-thread workgroups → ~1.5% GPU utilization on populated cells

**File**: `crates/renderer/shaders/cluster_cull.comp:11`
**Dimension**: Shader Correctness (perf)

`local_size_x=local_size_y=local_size_z=1` — every cluster is a 1-thread workgroup → 3456 dispatched WGs, each iterating 80–200 lights serially. On a 4070 Ti the dispatch uses ~1.5% of the GPU's compute capability for the worst case. Re-running every frame adds noticeable per-frame stall during scene-change-heavy moments (cell load).

**Fix**: `layout(local_size_x = 32) in;` with shared-memory accumulation:

```glsl
shared uint sharedCount;
shared uint sharedIndices[MAX_LIGHTS_PER_CLUSTER];
if (gl_LocalInvocationID.x == 0u) { sharedCount = 0u; }
barrier();
for (uint i = gl_LocalInvocationID.x; i < lightCount; i += 32u) {
    // sphere-vs-AABB test
    if (intersects && sharedCount < MAX_LIGHTS_PER_CLUSTER) {
        uint slot = atomicAdd(sharedCount, 1u);
        if (slot < MAX_LIGHTS_PER_CLUSTER) sharedIndices[slot] = i;
    }
}
barrier();
// thread 0 writes sharedIndices[0..count] into lightIndices[baseOffset..]
```

Estimated 8-16× speedup for the cluster-cull pass.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
