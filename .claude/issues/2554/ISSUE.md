# FNV-D3-01: shadowFade multiplies the whole ReSTIR-DI direct-light estimate to zero past 12000 BU instead of fading the shadow term

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2554
**Finding ID**: FNV-D3-01

**Severity**: HIGH
**Dimension**: RT Lighting Pipeline (FNV Scenes)
**Location**: `crates/renderer/shaders/triangle.frag:2378-2385, 2583-2585, 2657, 2931`
**Status**: NEW

## Description
Lights needing visibility (`shadowPolicyNeedsVisibility`) skip the streaming-loop `Lo` contribution under ReSTIR, expecting their entire direct term to arrive later via the shadowed ReSTIR sample — but both the reservoir-streaming gate (`shadowFade > 0.01`) and the finalize scale (`frameContribution = rad * restirW * transmissionFrame * shadowFade`) are gated/scaled on the *same* `shadowFade`, which ramps 1→0 between 8000–12000 BU distance. So distance fades the *light itself* to zero instead of fading only the *shadow term* (the legacy, now-compiled-out WRS arm had the correct semantics: unconditional radiance add, `shadowFade` applied only to the shadow subtraction). `byroredux/src/render/lights.rs` never emits `SHADOW_POLICY_NONE` — the cell directional and every LIGH point/spot are FULL/STRUCTURE, all `needsVisibility == true` — so **100% of FNV lights, including the sun, route exclusively through the faded estimate**.

## Evidence
Confirmed directly:
```glsl
// streaming gate
if (rtEnabled && needsVisibility && shadowFade > 0.01) { ... }
// finalize scale
frameContribution = rad * restirW * transmissionFrame * shadowFade;
```
`shadowFade = 1.0 - smoothstep(SHADOW_FADE_START, SHADOW_FADE_END, worldDist);` — the same value gates both the streaming admission and the finalize multiply.

## Impact
FNV exteriors (`--grid`, radius ≥ 2) lose all sun + point direct lighting on terrain/architecture beyond ~171 m from camera; a distance-graded darkening band tracks the camera between ~114–171 m. Large FNV interiors (Hoover Dam, Vault corridors) can reach the 8000-BU ramp. The reservoir is also never streamed past the fade end, biasing the temporal/spatial history. Prospector Saloon (bench-of-record) is small enough to sit entirely under `SHADOW_FADE_START`, explaining why bench numbers don't expose this.

## Related
`docs/engine/lighting-from-cells.md:255-256`; ReSTIR rewrite `6b061120`; legacy-arm gate #1799. No open issue matches.

## Suggested Fix
Keep streaming the reservoir regardless of `shadowFade`; change finalize to `frameContribution = rad * restirW * mix(vec3(1.0), transmissionFrame, shadowFade)` (or equivalently add unshadowed radiance scaled by `1 - shadowFade`) so the estimator converges to the unshadowed BRDF value at range, matching retired-WRS semantics. Add a shader-reflection/unit pin asserting `shadowFade == 0 ⇒ direct light == unshadowed BRDF value`.

## Completeness Checks
- [ ] **TESTS**: A shader-reflection/unit pin asserts `shadowFade == 0 ⇒ direct light == unshadowed BRDF value`
- [ ] **SIBLING**: Confirm the legacy (compiled-out) WRS arm's correct semantics are the reference to match, not re-derived independently
