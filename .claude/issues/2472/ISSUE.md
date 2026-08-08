# REN-D17-NEW-02: pathEnvironmentRadiance converts the DALC arm to radiance but not its sceneFlags.yzw siblings -- ~pi step between Skyrim and FO3/FNV cells

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2472
**Finding ID**: REN-D17-NEW-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 17 — Disney BSDF
**Location**: `crates/renderer/shaders/include/lighting.glsl:pathEnvironmentRadiance` (lines 232-244); same asymmetry at `crates/renderer/shaders/triangle.frag:2212-2213`
**Status**: NEW (follow-on to the #2244 fix in `c4cb2614`, not a regression of it)

## Description
#2244 correctly established that `sampleDalcCube` returns authored *irradiance* and therefore needs `* (1.0 / PI)` before feeding a path integrator's environment (radiance) term. But `sampleDalcCube` and `sceneFlags.yzw` (XCLL cell ambient) are the two arms of the same ambient term everywhere else in the shader — they're interchangeable elsewhere with identical downstream treatment. Yet in `pathEnvironmentRadiance` only the DALC branch is divided by π; the exterior branch and the interior non-DALC fallback are not.

## Evidence
```glsl
vec3 pathEnvironmentRadiance(vec3 direction) {
    vec3 rayDir = normalize(direction);
    if (jitter.w > 0.5) {
        float skyWeight = smoothstep(-0.2, 0.8, rayDir.y);
        return mix(sceneFlags.yzw, skyTint.xyz, skyWeight);   // no /PI
    }
    if (dalcFlags.x > 0.5) {
        return sampleDalcCube(rayDir) * (1.0 / PI);           // /PI  (#2244)
    }
    return sceneFlags.yzw * 0.5;                              // no /PI
}
```
and the reflection-miss sibling at `triangle.frag:2212`: `sampleDalcCube(R) * (1.0 / PI) : sceneFlags.yzw`.

## Impact
For identically-authored ambient, a Skyrim DALC-authored cell now gets a bounded-path escape / reflection-miss environment term ~π× (≈3.14×) dimmer than an FO3/FNV/Oblivion XCLL cell. That is the same systematic cross-game ambient gap REND-#1452 fixed on the direct-ambient path, re-opened on the indirect path. Visible as darker indirect floors / reflection misses in Skyrim interiors relative to Fallout interiors.

## Related
`AUDIT_RENDERER_2026-08-02.md:275-277` (fixed by #2244); the regression guard `bounded_path_converts_dalc_irradiance_to_environment_radiance` (`gpu_instance_layout_tests.rs:1148`) pins the DALC arm only.

## Suggested Fix
Pick one convention for the pair and apply it to all three arms of `pathEnvironmentRadiance` (and both arms at `triangle.frag:2212`). Since `ambient`/`sampleDalcCube(N)` are used interchangeably elsewhere, `sceneFlags.yzw` is also irradiance and should take the same `1/PI`; extend the regression test to cover the non-DALC arms.

## Completeness Checks
- [ ] **TESTS**: Extend `bounded_path_converts_dalc_irradiance_to_environment_radiance` to cover the non-DALC arms
