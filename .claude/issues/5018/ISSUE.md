# REN-D10-2026-09-29-01: #4946 made the translucency lobe self-shadowing — the back-lobe visibility ray starts on the viewer's side and hits the fragment's own triangle

**Labels**: high,bug,renderer,shaders

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-29.md`
**Severity**: HIGH (special-rule floor: ray query self-intersection — wrong origin bias). Visual impact limited to materials carrying the translucency or back-light flags.
**Dimension**: Soft Shadows / Disney BSDF
**Location**:
- `crates/renderer/shaders/include/lighting.glsl`, `shadowableLightRadiance`: the translucency block (`backDotL = max(-rawNdotL, 0.0)`, added by `efc059f3a`) and `bethesdaBackFactor(mat, rawNdotL)`.
- `crates/renderer/shaders/triangle.frag`, ReSTIR finalize: `vec3 shadowOffsetNormal = dot(geometricNormal, V) < 0.0 ? -geometricNormal : geometricNormal; vec3 rayOrigin = offsetRayOrigin(fragWorldPos, shadowOffsetNormal);` (the compiled-out legacy-WRS arm has the same shape).

> **Needs RenderDoc**: this finding was derived from source reading only (no engine launch / capture was possible in the audit). The fix must be gated on a before/after capture of a translucency / back-lit material lit from behind.

## Description
Both back lobes are non-zero only for N·L < 0 (light on the far side of the surface from the viewer). The finalize offsets the ray origin toward the **viewer** and traces toward the light, so the ray crosses the fragment's own triangle a few hundred ulps out. `traceShadowTransmittanceDetailed` uses tMin 0.0 and `VISIBILITY_MASK_ALL_OPAQUE`, which includes this instance (`shadow_mask_for_instance` keeps translucent materials in their opaque layer bucket). It commits that hit and returns `vec3(0.0)`:
- a non-alpha-sensitive surface returns at once;
- an alpha-tested one is `covered` at essentially the fragment's own UV (it already passed its alpha test).

Visibility is therefore 0 in exactly the configuration the lobe models. Before #4946 the term was added unshadowed; now it is shadowed, including by its own surface. `offsetRayOriginForDirection` (`include/ray_origin.glsl`) is already used by the glass, GI and reflection rays, but not by the direct-light visibility ray.

## Evidence
```glsl
// lighting.glsl, inside shadowableLightRadiance (post-#4946)
float backDotL = max(-rawNdotL, 0.0);            // non-zero only when the light is behind
// triangle.frag, ReSTIR finalize
vec3 shadowOffsetNormal = dot(geometricNormal, V) < 0.0 ? -geometricNormal : geometricNormal;
vec3 rayOrigin = offsetRayOrigin(fragWorldPos, shadowOffsetNormal);   // viewer side
// shadow_transport.glsl
if (!alphaSensitive && !nearEmitter) return vec3(0.0);
if (covered && !sourceShell) return vec3(0.0);
```
`translucency_lobe_is_shadowed_with_the_other_direct_lobes` pins where the lobe lives, not that its visibility can be non-zero.

## Impact
- Thin-sheet transmission (leaves, paper, fabric) and thick-object SSS on BGSM v≥8 translucency content render no back-lit contribution from any cell light or the sun inside `shadowFade`. Past the fade the ray is skipped and the lobe reappears unshadowed, so it switches on with distance.
- Skyrim SLSF2 / FO4 back-light materials have had the same zeroed back-light lobe since that lobe moved into this function.
- ReSTIR pHat includes the back lobe, so a light behind the surface is selected in proportion to a contribution its ray always zeroes — unbiased, but spends the pixel's reservoir sample on a guaranteed-dark candidate (extra noise on the lit side).

## Related
#4946 (closed; the fix that introduced the translucency case), #3574, #1147 Phase 2b, `offsetRayOriginForDirection`.

## Suggested Fix
Orient the visibility-ray origin toward the light: `offsetRayOriginForDirection(fragWorldPos, geometricNormal, rayDir)`.
- For N·L > 0 this is identical to today.
- For the back lobes it starts on the light's side of a thin sheet.
- Thick objects would still self-occlude through their far wall; whether their SSS should skip the own instance is a separate decision.
- Gate the change on a before/after RenderDoc capture (ReSTIR `direct_only` view and `restir_light` selection view) of a flagged material lit from behind.

Validated at HEAD 9fcfdc3fc: `triangle.frag` still builds `shadowOffsetNormal` from V at both finalize sites and calls `offsetRayOrigin` (not `offsetRayOriginForDirection`); `lighting.glsl` `shadowableLightRadiance` contains the `backDotL = max(-rawNdotL, 0.0)` translucency block and `bethesdaBackFactor`.

## Completeness Checks
- [ ] **SIBLING**: every other visibility ray family checked for viewer- vs light-oriented origin (glass/GI/reflection already use `offsetRayOriginForDirection`)
- [ ] **TESTS**: A shader-contract pin asserts the direct-light visibility origin is direction-oriented; visual confirmation via capture
