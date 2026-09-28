# #4946: REN-D10-2026-09-27-01: BGSM translucency (SSS) lobe is added from the unshadowed radiance of every cluster light, including the sun

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4946
- **Labels**: medium,renderer,shaders,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D10-2026-09-27-01**._

- **Severity**: MEDIUM
- **Dimension**: Disney BSDF
- **Location**: `crates/renderer/shaders/triangle.frag` cluster light loop. The gate is `sssGate` (~L3291). `vec3 unshadowedRadiance = lightColor * atten;` is at L3310. The `if ((mat.materialFlags & MAT_FLAG_TRANSLUCENCY) != 0u)` block runs from L3336, with `Lo += sssTint * … * unshadowedRadiance` at L3372. Contrast `shadowableLightRadiance` (`include/lighting.glsl` L174), whose `bethesdaBackFactor` term at L338 is inside the shadowed function.
- **Status**: NEW. Pre-existing code (#1147 Phase 2b; the gate driver is #3574), not a delta regression. No open or closed issue covers it; searched "translucency shadow" and "subsurface unshadowed".
- **Description**: The checklist premise is that every direct lobe goes through `shadowableLightRadiance`, so that ReSTIR selection, the legacy-WRS subtraction and TLAS visibility all see it. The Skyrim back-light lobe (`bethesdaBackFactor`) and the rim lobe obey this; their comment says so explicitly ("keeps ReSTIR selection, visibility, and the legacy shadow subtraction byte-consistent"). The FO4-family translucency lobe does not. It is added straight to `Lo` for every light in the fragment's cluster, and no visibility term is applied:
  - under ReSTIR, the needs-visibility light's normal diffuse/specular term is withheld from `Lo` and re-enters shadowed via the reservoir, but the SSS term was already added unshadowed;
  - under the legacy estimator, pass 2 subtracts only `shadowableLightRadiance`, so the SSS term is never occluded either.

  The directional light is a member of every cluster (`cluster_cull.comp` sets `intersects = true` for `lightType > 1.5`, L259), so it takes this path too.
- **Evidence**:
  ```glsl
  vec3 unshadowedRadiance = lightColor * atten;           // L3310
  vec3 shadowableRadiance = shadowableLightRadiance(...); // shadowed path
  if (!useRestir || !needsVisibility) { Lo += shadowableRadiance; }
  ...
  if ((mat.materialFlags & MAT_FLAG_TRANSLUCENCY) != 0u) {
      ...
      Lo += sssTint * mat.translucencyTransmissiveScale
          * thicknessShape * turbMod * unshadowedRadiance; // no visibility
  }
  ```
- **Impact**: This is a light leak on `bgsm.translucency` content (`forward_bgsm_phase1_flags`; the shader comment dates the field to BGSM v≥8). Examples:
  - back-lit leaves or fabric sitting in a building's or terrain's shadow still get the full sun transmission;
  - translucent surfaces behind a wall get SSS from a lamp in the next room whose range crosses the wall.

  The result is also inconsistent across games. Skyrim's back-light lobe is shadowed and FO4/FO76's translucency lobe is not, which breaks the one-composition-convention invariant. The effect is visual only, and it is whole-surface on affected materials.
- **Related**: REN-D2-2026-09-27-01 (ReSTIR candidate weight) and REN-D7-2026-09-27-01 (EMA). Neither covers this term.
- **Suggested Fix**: Move the translucency lobe into `shadowableLightRadiance`, next to `bethesdaBackFactor`, so that ReSTIR pHat, the finalize visibility and the legacy subtraction all include it. `sssGate` then stays as it is (it is only the early-out). Add a `shader_contract` pin that `MAT_FLAG_TRANSLUCENCY`'s lobe lives inside that function. A before/after capture on an FO76/translucent-foliage scene is needed (see Needs validation).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
