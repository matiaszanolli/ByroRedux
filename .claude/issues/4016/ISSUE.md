# #4016 — REN-2026-09-06-D19-01: the terrain-splat normal-map loop takes its implicit-LOD sample under a per-fragment `continue` — the last unswept instance of #3622's class

**Labels**: low, renderer, shaders, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D19-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Tangent-Space
- **Location**: `crates/renderer/shaders/triangle.frag` (the `terrainSplatActive` normal-perturbation loop), `crates/renderer/shaders/include/material_sampling.glsl` (`perturbNormal`)
- **Status**: NEW
- **Description**: The LAND TX01 loop skips layers per fragment and then calls `perturbNormal`, whose normal-map fetch uses the implicit-derivative form:

  ```glsl
  for (uint i = 0u; i < 8u; ++i) {
      float w = terrainSplat[i / 4u][i & 3u];
      uint layerNormalIdx = terrainTile.layerNormalIndex[i];
      if (w <= 0.0 || layerNormalIdx == 0u) continue;
      vec3 layerNormal = perturbNormal(terrainGeometryNormal, fragWorldPosRel, sampleUV, layerNormalIdx, fragTangent);
      …
  }
  ```
  `perturbNormal` opens with `texture(textures[nonuniformEXT(normalMapIdx)], uv)`. `layerNormalIdx` is tile-uniform, but `w` is a per-fragment interpolated splat weight, so at any splat boundary one lane of a quad executes the fetch on iteration *k* while its neighbour `continue`s — implicit-LOD sampling in non-uniform control flow, the class `parallaxDisplaceUV` was converted away from under #3622 and that `ray_hit.glsl`'s `resolveRayHitUV` has always avoided.
- **Evidence**: `#3622`'s own comment in `material_sampling.glsl` states the rule and names the two paths that were fixed; this third path was not swept. Terrain vertices carry `tangent: [1.0, 0.0, 0.0, -1.0]` (`crates/renderer/src/vertex.rs`, pinned by `terrain_vertex_carries_a_nonzero_tangent`), so `perturbNormal` takes Path 1 and the `dFdx(worldPos)` fallback is *not* additionally exposed here — the implicit texture LOD is the sole exposure.
- **Impact**: Spec-undefined mip selection on exterior LAND normal maps at layer boundaries; would read as inconsistent terrain-relief sharpness along splat seams. **Practically inert today** and reported as hardening, not as an observed artefact: unlike the POM marcher — whose `currentUV` genuinely changed per divergent iteration — `sampleUV` here is computed once in quad-uniform flow and is unchanged in every lane at the fetch, so the derivative real hardware computes is the correct one. It is filed because the project has already decided this class is worth closing, the fix is one hoisted line, and leaving one instance behind makes the rule look optional.
- **Related**: #3622 (REN-2026-08-30-D19-03), #3902 (the sibling secondary-ray role gap).
- **Suggested Fix**: Capture the mip level once before the loop — the enclosing `if (terrainSplatActive && (dbgFlags & DBG_BYPASS_NORMAL_MAP) == 0u)` is quad-uniform, so a `textureQueryLod(…, sampleUV).x` there is well defined — and give `perturbNormal` an explicit-LOD sibling (or an optional `lod` parameter defaulting to the implicit path) exactly as `sampleParallaxHeight` took one under #3622.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
