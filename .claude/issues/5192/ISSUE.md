# #5192 — REN-D10-2026-10-03-02: Soft-light wrap, back-light and thick-translucency lobes self-occlude on closed meshes (zero inside shadowFade, unshadowed past it), the decision #5018 deferred

**Labels**: medium,renderer,shaders,game:skyrim,game:fo4,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: MEDIUM. This is a BSDF/visibility convention mismatch. It is visual only, and limited to flagged materials. Those include Skyrim skin and the tint family (`SLSF2_Soft_Lighting`, see #3458) and FO4 BGSM `translucency_thick_object` skin.
- **Dimension**: Disney BSDF / Soft Shadows
- **Location**: `crates/renderer/shaders/include/lighting.glsl`:
  - `shadowableLightRadiance`: `bethesdaDiffuseLightFactor` (wrap non-zero for `rawNdotL ∈ (-width, 0)`), `bethesdaBackFactor` (`max(-rawNdotL, 0)`), and the `MAT_FLAG_TRANSLUCENCY` block's `MAT_FLAG_TRANSLUCENCY_THICK_OBJECT` arm.

  `crates/renderer/shaders/triangle.frag`, ReSTIR finalize:
  - `offsetRayOriginForDirection(fragWorldPos, geometricNormal, L)` (#5018);
  - `visibility = mix(vec3(1.0), transmissionFrame, shadowFade)`.

  `crates/renderer/src/vulkan/acceleration/predicates.rs`, `shadow_mask_for_instance`: actors stay in `VISIBILITY_LAYER_DYNAMIC_ACTOR`, which is inside the FULL mask.
- **Status**: NEW. This is not a delta regression: before #5018 the fragment's own triangle zeroed these lobes, and now the far wall does. Dim 2 routed it here (`dim_2.md`, "Thick-object back lobes and the soft-light wrap still self-occlude"). No issue tracks it. Searched: "soft lighting", "thick object", "self-occlusion", "translucency", "back lighting", "wrap lighting", "subsurface". #5018's suggested fix said "Thick objects would still self-occlude ... whether their SSS should skip the own instance is a separate decision".
- **Description**:
  - Each of these lobes is non-zero only where `rawNdotL < 0` (the wrap only in `(-width, 0)`). There, on a watertight mesh, `Ng·L < 0` too, unless a normal map tilts N away from Ng. So #5018's direction-aware origin starts the ray just inside the body, and the ray travels through the interior toward the light.
  - Ray queries carry no facing-cull flag (#4580). So the far wall commits as an opaque hit in `VISIBILITY_MASK_ALL_OPAQUE`, and `traceShadowTransmittanceDetailed` returns `vec3(0.0)`. Visibility is therefore 0 for exactly the configurations these lobes model: the soft terminator past N·L = 0, back-lit skin and wax.
  - This holds for every light whose `visibilityMaskNeedsTrace` is true. That is every legacy light (`for_legacy_local_light()` = FULL) plus the sun and the XCLL key, and it holds inside `SHADOW_FADE_START` (8 000 BU). Between 8 000 and 12 000 BU the `mix(1, V, shadowFade)` blend fades the lobes in unshadowed, so they switch on with distance.
  - In the reference content model most local lights are not shadow casters. FO3/FNV author zero projection/shadow flags, and Skyrim+ shadow-cast only flagged LIGHs. So these lobes were authored to be visible under ordinary room lights.
  - Secondary effect: ReSTIR pHat includes these lobes. A light that sits just past the terminator, or behind the body, is selected in proportion to a contribution its ray always zeroes. The estimate stays unbiased, but the reservoir sample is spent on a guaranteed-dark candidate, which adds noise on characters.
- **Evidence**:
  ```glsl
  // lighting.glsl — bethesdaDiffuseLightFactor
  float wrapped = max((rawNdotL + width) / (1.0 + width), 0.0);   // > 0 for rawNdotL in (-width, 0)
  // bethesdaBackFactor / translucency
  return max(-rawNdotL, 0.0) * clamp(strength, 0.0, 4.0);
  float backDotL = max(-rawNdotL, 0.0);
  // triangle.frag — ReSTIR finalize (#5018)
  vec3 rayOrigin = offsetRayOriginForDirection(fragWorldPos, geometricNormal, L); // inside a closed body when Ng·L < 0
  visibility = mix(vec3(1.0), transmissionFrame, shadowFade);                     // lobe returns unshadowed past 12 000 BU
  ```
- **Impact**: Skyrim characters lose the authored soft terminator (`lightingEffect1` wrap, with the slot-2 mask) under every light at any practical viewing distance. FO4 thick-object skin SSS renders no back-lit transmission. Thin open cards (foliage, hair) are unaffected, since #5018 already fixed those.
- **Related**: #5018, #4946, #3574, #3458, #4580; REN-D2-2026-10-03-01 (the opposite-sign leak on *open* geometry from the same origin rule).
- **Suggested Fix**: Make the visibility convention per-lobe instead of one binary ray for the whole BRDF. Split `shadowableLightRadiance`'s return into a reflection part and a transmission part (back-light, translucency, and the wrap's `rawNdotL < 0` excess). Trace the transmission part with the fragment's own instance skipped (candidate-loop instance-id test), or leave it unshadowed with the existing `shadowFade` semantics. Then pin the split with a `shader_contract` test. Accept on a before/after capture of a Skyrim soft-lit head beside a cell light, and of an FO4 thick-translucency surface lit from behind (Needs RenderDoc). This pairs naturally with REN-D2-2026-10-03-01's fix, which needs the same split.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix
