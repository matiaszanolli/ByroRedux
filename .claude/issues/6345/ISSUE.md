# REN-D2-01: Terrain parallax: base layer sampled with implicit derivatives of the displaced UV

- **Issue**: #6345 (snapshot as filed; GitHub is authoritative for live state)
- **Labels**: medium, renderer, shaders, terrain-exterior, bug
- **Source**: docs/audits/AUDIT_RENDERER_2026-10-24.md

- **Severity**: MEDIUM (visual only: wrong mip and a skewed tangent frame on the base layer at relief edges, within the parallax range). NEEDS-RENDERDOC.
- **Dimension**: Tangent-Space
- **Location**: `crates/renderer/shaders/triangle.frag` `main()`: `sampleUV = terrainParallaxDisplaceUV(...)` (new in `b81a43443`, part of #6197), followed by `vec4 texColor = texture(textures[nonuniformEXT(fragTexIndex)], sampleUV);`, the `materialDecals` loop, and `N = perturbNormal(N, fragWorldPosRel, sampleUV, normalMapIdx, fragTangent);` (`perturbNormal` in `include/material_sampling.glsl` forwards `dFdx(uv)` / `dFdy(uv)` to `perturbNormalGrad`).
- **Status**: NEW (`b81a43443`, part of #6197 — #6197 is the feature tracker; this is a defect in its landed slice).
- **Description**: `splatUVdx/dy` and `splatPosdx/dy` are captured before the march from the undisplaced `sampleUV`; the march then overwrites `sampleUV` for terrain fragments. The splat diffuse/specular/normal loops, `byroTerrainHeightBlend` and `byroTerrainSplatAlbedo` all use `textureGrad` with the undisplaced gradients, which is standard POM practice. Three base-layer reads are still implicit, so their derivatives are taken of the displaced UV: the BTXT base diffuse sample, the decal samples, and the base normal map (LAND tiles bind one through `MaterialTextureHandles { normal: base_normal_index, .. }` in `byroredux/src/cell_loader/terrain.rs`; near LAND vertices carry no tangents, so `perturbNormalGrad` takes Path 2 and builds the TBN from `dFdx(displacedUV)`). The displaced UV's screen derivative is `dUV/dx` plus the derivative of the slide, which jumps where neighbouring quad lanes stop on different march layers. The reads sit in uniform control flow, so this is wrong LOD and a wrong TBN, not undefined behaviour.
- **Evidence**: orchestrator re-read: `vec2 splatUVdx = dFdx(sampleUV);` (before the march), `sampleUV = terrainParallaxDisplaceUV(...)` (the overwrite), then `texture(textures[nonuniformEXT(fragTexIndex)], sampleUV)`, the decal `texture(...)` loop and `perturbNormal(N, fragWorldPosRel, sampleUV, ...)` after it. `triangle_frag_displaces_before_the_blend_with_the_painted_weights` pins only that the gradients are captured before the call. Before `b81a43443` the terrain `sampleUV` was the plain BGSM-transformed UV, so implicit and explicit derivatives agreed.
- **Impact**: LAND within the parallax range (default 20 m, fading from 10 m) wherever the BTXT base shows through (weight-0 splat regions) or a base normal map is bound: blur or sparkle and normal-map orientation flicker along relief edges, worse at grazing angles. Not measured.
- **Related**: #6197, #4016 (the explicit-gradient capture), #3622.
- **Suggested Fix**: when `terrainSplatActive`, read the base diffuse and decals with `textureGrad(..., sampleUV, splatUVdx, splatUVdy)` and call `perturbNormalGrad(N, sampleUV, normalMapIdx, fragTangent, splatPosdx, splatPosdy, splatUVdx, splatUVdy)` for the base normal, as the layer loop already does; extend the pin test to cover it. Confirm with a RenderDoc capture or a screenshot A/B (`terrain.parallax 20 0.08` against `off`, grazing-angle LAND close-up).


## Completeness Checks
- [ ] **SIBLING**: every terrain-base read after the march (BTXT diffuse, decals, base normal, any specular/emissive base lookup) uses the captured gradients; `perturbNormalGrad` Path 2 builds its TBN from `splatUVdx`/`splatPosdx`
- [ ] **SHADER-SYNC**: rebuild the SPIR-V with plain `-V` and re-run `scripts/check-shader-artifacts.sh`
- [ ] **TESTS**: extend `triangle_frag_displaces_before_the_blend_with_the_painted_weights` to pin the explicit-gradient base reads
- [ ] **VALIDATION**: grazing-angle LAND close-up, `terrain.parallax 20 0.08` vs `off` (NEEDS-RENDERDOC / screenshot A/B)

_Filed from `docs/audits/AUDIT_RENDERER_2026-10-24.md` (static review; no engine or Vulkan device was launched). Failures invisible to `cargo test` are flagged NEEDS-RENDERDOC in the report._
