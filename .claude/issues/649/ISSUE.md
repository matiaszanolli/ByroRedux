# Issue #649: SH-4: caustic_splat ray origin self-intersects on thin refractive geometry

**File**: `crates/renderer/shaders/caustic_splat.comp:229-231`
**Dimension**: Shader Correctness

`origin = G + refr * 0.5, tmin = 0.0`. The 0.5-unit step into the refracted direction is meant as a self-intersection bias, but Bethesda glass meshes routinely have wall thickness < 0.5 units (drinkingglass01 mesh is ~0.3 units thick on the rim). For those thin meshes the start point is OUTSIDE the back face of the same glass mesh, and `tmin=0.0` allows zero-length intersection at the back face → the caustic is deposited on the surface that the light just came through, producing a self-luminous halo on the glass itself rather than under it.

Compare the analogous bias on triangle.frag glass refraction at line 1063: `fragWorldPos - N_geom_view * 0.1, 0.05` — uses the geometric normal (not the refracted dir, which has a much smaller normal-aligned component) AND a non-zero tmin.

**Fix**:
```glsl
rayQueryInitializeEXT(rq, topLevelAS,
    gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT, 0xFFu,
    G - N * 0.1,    // step OUT through the back of the glass
    0.05,           // non-zero tmin
    refr, 1000.0);
```

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
