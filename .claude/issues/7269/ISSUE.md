# #7269: INC-R-02: Traced sky-specular occlusion subtracts sky radiance from an authored-cubemap environment

- **Severity**: LOW
- **Dimension**: audit-renderer D6 (shaders)
- **Location**: `crates/renderer/shaders/triangle.frag:3151` (`exteriorSkyProbe`), `:3209-3229` (`ambientFallback`), `:3305-3317` (the traced block)
- **Changed in**: 5f51748d5 / 84104564e (RLT-IBL-004)
- **Status**: NEW
- **Description**: `exteriorSkyProbe = jitter.w > 0.5 && exteriorSkyTint.w > 0.5` ignores `hasAuthoredCubemap`. For a material with an authored DDS cube, `ambientFallback`, and so `envColor` (at `probeShare` = 1 − rayFade), is that cube's sample, not the sky probe. Yet each blocked lobe sample still removes `probeShare · lobeWeight · exteriorSkyRadianceOr(lobeL, …)`, which is sky-cube radiance. The Lagarde-style `specularOcclusion` is also applied to it.
- **Evidence**: `if (exteriorSkyProbe && probeShare > 0.0 && reflectionGlassRayEnabled && !helperLane && rtLOD < RT_LOD_REFLECT …) { … envColor -= probeShare * lobeWeight * exteriorSkyRadianceOr(lobeL, ambientFallback, 0.0); }`. When `hasAuthoredCubemap` is true, `ambientFallback = textureLod(cubemaps[mat.envMapIndex], …)`.
- **Impact**: Exterior props with authored env maps (FO3/FNV `envMapIndex != 0`), when rough enough or beyond the reflection ray's LOD reach, have the wrong energy removed: authored-cube colour minus sky colour. `envColor` is not clamped after the subtraction, so a bright sky sample over a dim cube can drive it negative.
- **Related**: #6604
- **Suggested Fix**: Set `probeShare = 0` (or skip the traced block) when `hasAuthoredCubemap`, and clamp `envColor` at 0 after the subtraction.

**Found by**: `docs/audits/AUDIT_INCREMENTAL_2026-11-22b.md`

## Completeness Checks
- [ ] **SIBLING**: other probe-only terms checked for hasAuthoredCubemap
- [ ] **TESTS**: a regression test pins this specific fix
