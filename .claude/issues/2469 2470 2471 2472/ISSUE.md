# Issues 2469, 2470, 2471, 2472 — Renderer shader fixes

All four are renderer/shader precision & correctness bugs found by the audit suite (dimensions 15-17: Water, Volumetrics, Disney BSDF).

## #2469 — REN-D15-NEW-02: foamFlowStreaks absolute-world hashing
- File: crates/renderer/shaders/water.frag
- foamFlowStreaks() hashes absolute vWorldPos, unlike sampleScrollingNormal which was rebased in #1997 to subtract uvOrigin first.
- Fix: add originOffset param to foamFlowStreaks, subtract from worldPos before projections; update 3 call sites.

## #2470 — REN-D16-2026-08-07-01: volumetric froxel texel-center mismatch
- Files: crates/renderer/shaders/volumetrics_integrate.comp, crates/renderer/shaders/composite.frag
- integrate stores cumulative state at slab BACK-face (u=(slice+1)/N), composite samples via sampler3D texel-center (u=(k+0.5)/N) with no correction -> half-slab forward fog bias.
- Fix: in composite.frag, remap normalized depth to texel-aligned coordinate: slice_tc = clamp((u*N - 0.5)/N, 0, 1).

## #2471 — REN-D17-NEW-01: specularAaRoughness filters alpha instead of alpha^2
- File: crates/renderer/shaders/include/pbr.glsl, specularAaRoughness (~210-217)
- roughness2 = roughness*roughness is actually alpha (not alpha^2); adding 2*kernelVariance and sqrt-ing back produces alpha_filtered = alpha + 2*sigma^2 instead of sqrt(alpha^2 + 2*sigma^2).
- Fix: filteredA2 = clamp(roughness2*roughness2 + 2*kernelVariance, floor, 1); return sqrt(sqrt(filteredA2)) (4th root round trip, Filament convention). Recheck 0.025 floor semantics.

## #2472 — REN-D17-NEW-02: pathEnvironmentRadiance asymmetric DALC/-XCLL /PI conversion
- File: crates/renderer/shaders/include/lighting.glsl, pathEnvironmentRadiance (~232-244); sibling at triangle.frag:2212-2213
- DALC arm (`sampleDalcCube`) divides by PI (#2244 fix) but sceneFlags.yzw (XCLL ambient) sky arm and interior fallback arm do not, despite being interchangeable irradiance sources elsewhere.
- Fix: apply * (1.0/PI) to all three arms (sky mix, DALC, XCLL fallback) in pathEnvironmentRadiance, and to the sceneFlags.yzw arm at triangle.frag:2212-2213.
