# Issue #641: SH-3: vertex shader writes fragPrevClipPos using current-frame bone palette — wrong motion vectors on every skinned actor pixel

**File**: `crates/renderer/shaders/triangle.vert:147-204`
**Dimension**: Shader Correctness

```glsl
vec4 worldPos = xform * vec4(inPosition, 1.0);     // current-frame skinned
vec4 currClip = viewProj * worldPos;
…
fragPrevClipPos = prevViewProj * worldPos;          // PREV viewProj × CURRENT worldPos
```

For rigid meshes correct (worldPos roughly stable). For skinned, `xform = Σ weight × bones[base + idx]` is the CURRENT-frame bone palette. The previous frame's bone palette is not consulted, so the motion vector encodes only camera + rigid motion, not per-vertex skin motion.

`outMotion = (currNDC - prevNDC) * 0.5` is wrong on every actor body / hand / face pixel. SVGF (svgf_temporal.comp:73) and TAA (taa.comp:91) reproject the wrong source pixel. SVGF mesh-ID consistency rejects cross-mesh disocclusions, but for in-mesh disocclusions (forearm crossing torso, both same mesh_id) reprojection writes ghost trails — visible as feathered shadows trailing actor limbs in motion at typical 60 FPS.

The vertex shader comment at lines 135-137 acknowledged this as accepted pre-M29. Now that GPU pre-skinning landed, fixable cheaply.

**Fix**: Plumb `bones_prev[]` as a new readonly SSBO at `set 1, binding 12`, populated CPU-side from the previous frame's palette upload. Compute `prevWorldPos = xformPrev * inPosition` in the vertex shader. ~256 KB/frame.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
