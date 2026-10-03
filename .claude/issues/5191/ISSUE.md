# #5191 — REN-D2-2026-10-03-01: #5018's light-oriented ray origin drops the geometric-horizon clamp: in the N·L > 0 ≥ Ng·L band, open or two-sided surfaces take front-lobe light from behind their own plane

**Labels**: medium,renderer,shaders,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: MEDIUM
- **Dimension**: Ray Queries
- **Location**:
  - `crates/renderer/shaders/triangle.frag`, ReSTIR finalize: `vec3 rayOrigin = offsetRayOriginForDirection(fragWorldPos, geometricNormal, L);`. The compiled-out legacy-WRS arm has the same line.
  - `crates/renderer/shaders/include/lighting.glsl`, `shadowableLightRadiance`: `rawNdotL = dot(N, L)` against the shading normal; there is no geometric-normal term.
- **Status**: NEW. This is a side effect of the CLOSED #5018 fix (`b1c619fa7`), not a regression of an earlier fix. Before the change the origin was always on the viewer side: `fragWorldPos + N_bias * 0.05` until `2d904acd7`, then `shadowOffsetNormal`. Searched "shadow terminator" and "light leak wall": no issue.
- **Description**: The #5018 commit (and #5018's own suggested fix) says the direction-aware origin is "identical to the old viewer-side offset for front-lit surfaces". That holds only when "front-lit" is measured against the geometric normal `geometricNormal` (the `dFdx`/`dFdy` triangle plane), which is what `offsetRayOriginForDirection` tests. The lobes in `shadowableLightRadiance` are gated on the normal-mapped, viewer-flipped shading normal `N`, so there are two cases:
  - **N·L > 0 but Ng·L < 0** (light just behind the triangle plane; the normal map or the smooth vertex normal tilts N toward it). Before the change, the viewer-side origin made the ray cross its own triangle, so visibility was 0. That self-hit acted as a de facto geometric-horizon clamp.
  - **Same case after the change.** The origin is placed on the back side and the ray moves away from the plane. On closed meshes the far wall still occludes. On open geometry, such as two-sided cards and single-sided shells with nothing behind them within the light's reach, the ray reaches the light. The diffuse, specular and rim lobes are then added for a light that is geometrically behind the surface.

  `shadowableLightRadiance` takes no geometric normal, and nothing in the ReSTIR finalize gates on `dot(geometricNormal, L)`; grep finds no horizon test. The secondary-hit NEE paths stay consistent: `reflectionHitIrradiance` and the GI-hit loop offset along the oriented face normal and gate `giLightSample` on `dot(n, L) > 0` with that same normal. Only the primary direct path now offsets by Ng and shades by N.
- **Evidence**:
  ```glsl
  // triangle.frag (post-b1c619fa7)
  vec3 rayOrigin = offsetRayOriginForDirection(fragWorldPos, geometricNormal, L); // side chosen by Ng·L
  // lighting.glsl shadowableLightRadiance
  float rawNdotL = dot(N, L);            // N = normal-mapped shading normal, flipped to the viewer
  float NdotL = max(rawNdotL, 0.0);      // front lobes live wherever N·L > 0, regardless of Ng·L
  ```
  Under ray queries without a facing-cull flag (#4580), back faces of closed meshes still occlude, so the leak is confined to open or two-sided geometry.
- **Impact**: Visual only. Bumps on a back-lit opaque two-sided card, fence or sign, or on a single-sided shell piece with a light behind it, pick up speckled front-lobe light inside the shadow-fade range. The amount is bounded by how far N tilts past the plane and by the light's attenuation. It is not quantified: no capture was possible. The ReSTIR pHat already counted this band, so selection is unchanged and only its visibility flipped from 0 to roughly 1.
- **Related**: #5018, #4946, #4580. The thick-object and `MAT_FLAG_SOFT_LIGHTING` wrap note below is a separate observation, not filed.
- **Suggested Fix**: Restore the horizon clamp for the lobes that should not cross the plane. For example, zero the non-back lobes when `dot(Ng_viewer, L) < 0`: pass the viewer-oriented geometric normal into `shadowableLightRadiance`, or split its result so only the translucency/back-light terms use the light-side origin. Then pin it with a shader-contract test. Accept on a before/after capture of an opaque, normal-mapped, two-sided card lit from behind.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix
