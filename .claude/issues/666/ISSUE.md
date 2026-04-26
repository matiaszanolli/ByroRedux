# Issue #666: SH-10: composite fog-far guard does not reject negative fog_near — fog floor at camera origin

**File**: `crates/renderer/shaders/composite.frag:262`
**Dimension**: Shader Correctness

Fog branch gates on `fog_params.y > fog_params.x` (fog_far > fog_near). FNV-authored CLMT records occasionally ship a negative fog_near (artistic intent: fog starts before the camera), which passes the gate but `smoothstep(neg, pos, dist)` produces nonzero attenuation at dist=0 → the camera's own viewpoint sees fog floor on every pixel. Composite uses 0.7 cap to limit visible damage but the floor is still pumped into every direct + indirect output.

**Fix**: CPU-side clamp at scene-buffer upload (`fog_near = max(fog_near, 0.0)`) — cheaper than per-fragment.

Alternative: Tighten the shader gate:
```glsl
if (depth < 0.9999
    && params.fog_color.w > 0.5
    && params.fog_params.y > max(params.fog_params.x, 0.0)
    && params.fog_params.x >= 0.0)
```

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
