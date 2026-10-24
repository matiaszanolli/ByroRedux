# REN-D2-02: terrainParallaxDisplaceUV step-count clamp and interpolation epsilon

- **Issue**: #6346 (snapshot as filed; GitHub is authoritative for live state)
- **Labels**: low, renderer, shaders, terrain-exterior, bug
- **Source**: docs/audits/AUDIT_RENDERER_2026-10-24.md

- **Severity**: LOW (hardening; neither case is reachable from finite, non-degenerate inputs)
- **Dimension**: Ray Queries (shader robustness)
- **Location**: `crates/renderer/shaders/include/material_sampling.glsl` `terrainParallaxDisplaceUV`: `int steps = int(mix(maxSteps, minSteps, clamp(vz, 0.0, 1.0)) + 0.5);` and `float weight = afterDepth / (afterDepth - beforeDepth + 1.0e-6);`
- **Status**: NEW. The epsilon form is inherited verbatim from `parallaxDisplaceUV` and `ray_hit.glsl`.
- **Description**: (a) `vz` is NaN if `normalize(fragNormalEffective)` is NaN (a zero interpolated normal, or the flat-shading `normalize(cross(dFdx, dFdy))` on a degenerate quad). `clamp(NaN)` is implementation-defined and `OpConvertFToS` of NaN is undefined, so the loop bound is not provably in [6, 20]; each iteration costs up to 9 `textureGrad` reads (`byroTerrainBlendCore` loops 8 lanes plus base). The CPU mirror `step_count` uses Rust's saturating `as i32`, so the tests cannot see the difference. (b) After the `break`, `afterDepth <= 0 < beforeDepth`, so `afterDepth - beforeDepth` is strictly negative; adding `+1e-6` moves it toward zero and it reaches zero when `beforeDepth - afterDepth == 1e-6`, giving 0/0. The epsilon should be subtracted.
- **Evidence**: settings cannot inject inf or NaN: the sliders are bounded (0–0.3 m / 0–60 m), the `terrain.parallax` command filters `is_finite() && >= 0`, and `push_terrain_parallax` writes depth 0 unless both settings are > 0; a degenerate fade range (`end <= start`) is a hard cut with no division, and depth <= 0 returns early. All verified.
- **Impact**: none from authored data; a degenerate-geometry hazard only.
- **Suggested Fix**: `steps = clamp(steps, 6, 20)` (the generated-constant form) and `weight = afterDepth / min(afterDepth - beforeDepth, -1e-6)` in all three marchers; mirror both in `terrain_parallax_tests::step_count`.


## Completeness Checks
- [ ] **SIBLING**: the clamp and epsilon fix is applied to all three marchers (`terrainParallaxDisplaceUV`, `parallaxDisplaceUV`, `ray_hit.glsl`'s)
- [ ] **SHADER-SYNC**: rebuild the SPIR-V with plain `-V`; `scripts/check-shader-artifacts.sh` passes
- [ ] **TESTS**: mirror both fixes in `terrain_parallax_tests::step_count` (the CPU mirror's saturating `as i32` cannot see the NaN case today)

_Filed from `docs/audits/AUDIT_RENDERER_2026-10-24.md` (static review; no engine or Vulkan device was launched). Failures invisible to `cargo test` are flagged NEEDS-RENDERDOC in the report._
