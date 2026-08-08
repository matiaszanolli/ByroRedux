# OBL-D4-02: Legacy (non-PBR) Lambert diffuse differs by a factor of PI between the clustered per-light path and the no-cluster directional fallback

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2569
**Finding ID**: OBL-D4-02

**Severity**: MEDIUM
**Dimension**: Rendering Path for Oblivion Shaders
**Location**: `crates/renderer/shaders/include/lighting.glsl:154-166`, `crates/renderer/shaders/triangle.frag:2321-2332`
**Status**: NEW

## Description
Both sites branch on `MAT_FLAG_PBR_BSDF` and both take the Lambert `else` arm for 100% of Oblivion content (cross-referenced with OBL-D4-04's Disney-gate confirmation), but the two Lambert arms use different normalization: `lighting.glsl` has no `/PI` (documented as "the legacy non-/PI Lambert convention"); `triangle.frag` divides by `PI` then applies an extra `* vec3(0.8)`.

## Evidence
Confirmed directly: `lighting.glsl:166` — `diffuseBrdf = kD * albedo;` (no `/PI`); `triangle.frag:2328,2332` — `diffuseBrdf = kD * albedo / PI;` then `Lo = (diffuseBrdf + specular * specStrength * specColor) * vec3(0.8) * NdotL;`.

## Impact
An Oblivion surface lit by the no-cluster directional fallback is ~π× (≈3.14×) dimmer than the identical surface lit through the clustered per-light path — visible as a brightness pop crossing the cluster-population threshold, and as systematically dark Oblivion exteriors. Also affects FO3/FNV legacy content equally.

## Suggested Fix
Make the two sites agree (drop `/PI` at `triangle.frag:2331` and re-tune the `vec3(0.8)` fudge, per the `lighting.glsl` side being named the legacy reference), guarded by a shader-parity unit test; validate with a live capture before shipping, per project policy against speculative Vulkan/shader fixes.

## Completeness Checks
- [ ] **TESTS**: A shader-parity unit test asserts both Lambert arms produce the same value for identical inputs
- [ ] **UNSAFE**: N/A; needs a live RenderDoc capture confirming the visual fix before landing per the speculative-shader-fix policy
