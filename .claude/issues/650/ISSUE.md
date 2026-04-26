# Issue #650: SH-5: svgf_temporal.comp 2×2 bilinear consistency uses ONLY mesh_id — same-mesh disocclusions ghost on long walls

**File**: `crates/renderer/shaders/svgf_temporal.comp:85-134`
**Dimension**: Shader Correctness

The reprojection consistency loop weights each of the 4 nearest history taps by `bilinear[i] × (prevID == currID)`. mesh_id is the strongest disocclusion signal but it's not the only one paper SVGF uses (Schied 2017 §4.2 specifies depth + normal rejection too).

When the camera orbits a long static mesh and a part previously self-occluded by the same mesh becomes visible — same mesh_id, both old and new positions — the bilinear sample picks up the WRONG point on the wall and blends a different lighting integration into the new pixel. Visible as ghost streaks on receding walls during fast pans on interior cells.

The `outNormal` G-buffer attachment exists (triangle.frag:644 writes octahedral normal). A `prevNormalTex` is not currently bound to svgf_temporal.comp, so the rejection cannot be done.

**Fix**: Bind `prevNormalTex` (the previous-frame RG16_SNORM normal G-buffer) at `set = 0, binding = 9` of svgf_temporal.comp. In the consistency loop:

```glsl
vec3 currN = octDecode(texelFetch(currNormalTex, p, 0).rg);
…
vec3 prevN = octDecode(texelFetch(prevNormalTex, q, 0).rg);
if (dot(currN, prevN) < 0.9) continue;  // ~25° rejection cone
```

Closes the wall-pan ghosting and gives Phase 4 spatial filter a cleaner history.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
