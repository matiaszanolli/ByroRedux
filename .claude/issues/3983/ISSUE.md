# #3983 — REN-2026-09-06-D17-01: `specularAaRoughness`'s screen-space derivatives run inside per-invocation-divergent control flow at all five call sites, and are recomputed once per cluster light for a fragment-invariant value

**Labels**: medium, renderer, shaders, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D17-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Disney BSDF
- **Location**: `crates/renderer/shaders/include/pbr.glsl` (`specularAaRoughness`), `crates/renderer/shaders/include/lighting.glsl` (`shadowableLightRadiance`), `crates/renderer/shaders/triangle.frag` (five call sites)
- **Status**: NEW
- **Description**: `shadowableLightRadiance` opens with

  ```glsl
  float aaRoughness = ((dbgFlags & DBG_DISABLE_SPECULAR_AA) != 0u)
      ? roughness
      : specularAaRoughness(N, roughness);
  ```

  and `specularAaRoughness` evaluates `dFdx(N)` / `dFdy(N)`. Every one of the function's five call sites in `triangle.frag` is reached only under control flow that diverges *per invocation*, never per quad:

  1. the cluster light loop, past `if (contribution < 0.001) continue;` — a per-fragment N·L / attenuation gate, so lanes in a quad can be on different `i`, or have exited the loop entirely;
  2. the ReSTIR **temporal** reuse gate (`sameSurface && rpLightIndex < lightCount && … && rp.W > 0.0 && !isnan(rp.W)`) — reprojection validity is per pixel;
  3. the ReSTIR **spatial** reuse gate (`rnSurfaceId == surfaceId && spatialDepthCompatible && … && dot(geomN, nGeomN) >= SPATIAL_NORMAL_COS`);
  4. the selected-light block (`if (restirY != 0xFFFFFFFFu && restirW > 0.0 && visibilityMaskNeedsTrace(...))`);
  5. the legacy-WRS shadow subtraction, `if (any(lessThan(transmission, vec3(0.999))))` — gated on a **ray-query result**, the most divergent predicate in the shader.

  GLSL/SPIR-V leave derivative results undefined in non-uniform control flow. This is the same defect class #3622 fixed in `parallaxDisplaceUV`, whose comment states the rule explicitly ("Implicit derivatives are undefined per the GLSL/Vulkan spec when the sample sits inside non-uniform control flow"); the sibling in the BRDF assembly was not swept. It is also aggravated by the alpha-test `discard` earlier in `main()`: a quad that lost a lane to `OpKill` has undefined derivatives for the survivors from that point on, and hoisting above the discard removes that exposure too.

  Independently of the spec question, the call is **redundant**. `N` is final by the terrain-splat/weather block and `roughness` by the weather-puddle/snow block — both hundreds of lines before the first call site, and neither is written inside the light loop. A 16-light cluster therefore executes the `dFdx`+`dFdy`+clamp+two-`sqrt` chain sixteen times per fragment to produce sixteen identical values, and forces the quad into lockstep at each one.
- **Evidence**:
  - `pbr.glsl`: `float specularAaRoughness(vec3 N, float roughness) { vec3 dNdx = dFdx(N); vec3 dNdy = dFdy(N); … }`
  - `lighting.glsl`: `shadowableLightRadiance` calls it unconditionally (modulo the uniform `DBG_DISABLE_SPECULAR_AA` UBO bit).
  - Invariance: `roughness` is assigned only at the gloss-map mix and the two `weatherPuddles`/`weatherSnow` mixes; `N` only in the normal-map / model-space-normal / terrain-splat / snow / glass blocks — all strictly before the lighting section.
  - Precedent: `material_sampling.glsl`'s `parallaxLod` comment (#3622) and `ray_hit.glsl`'s always-explicit `textureLod`.
- **Impact**: Undefined specular-AA roughness on lit fragments — the visible signature would be inconsistent specular-lobe width at light-cluster and ReSTIR-reuse boundaries, and near alpha-tested geometry. Because `shadowableLightRadiance` is *also* the ReSTIR `pHat` scorer and the legacy shadow-subtrahend, a divergent value there desynchronises the "unshadowed accumulation cancels bit-for-bit against the shadowed subtraction" invariant #1369 depends on. All games, every lit fragment. The wasted work is unconditional.
- **Related**: #3622 (the fixed sibling), #1369 (the refactor that moved the BRDF — and this derivative — into the per-light function), #2471 / #2806 (`specularAaRoughness`'s clamp history).
- **Suggested Fix**: Hoist the filter to uniform control flow: compute `float aaRoughness = (dbgFlags & DBG_DISABLE_SPECULAR_AA) != 0u ? roughness : specularAaRoughness(N, roughness);` once in `main()` immediately after `N` and `roughness` are final (and ideally before the alpha-test `discard`), and pass `aaRoughness` into `shadowableLightRadiance` in place of `roughness` — keeping the raw `roughness` as a separate parameter for `disneyDiffuseSplit`, which deliberately uses the unfiltered value. Pin with a `shader_contract_tests.rs` assertion that `specularAaRoughness` does not appear inside `lighting.glsl`. **Needs RenderDoc / an A-B capture to demonstrate a visual delta**: because the inputs are branch-invariant, a compiler that hoists the derivative already produces the correct value, so the correctness half is a conformance-and-hardening claim, not an observed artefact. The redundant-work half is a source-level fact and needs no capture.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
