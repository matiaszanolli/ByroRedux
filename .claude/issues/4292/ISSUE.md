# #4292: REN-2026-09-14-D2-01: Sky-cubemap adoption covers only two of five exterior sky-miss sites — glass face-on reflection, glass refraction miss and water reflection miss still return the flat zenith blend

- **Labels**: medium,renderer,shaders,terrain-exterior,water,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4292
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: MEDIUM
- **Dimension**: Ray Queries (Sky/Weather)
- **Location**: `crates/renderer/shaders/triangle.frag` (`isExteriorGlass` `reflColor` fallback in the glass IOR block; `refrColor` miss arm in the glass refraction loop); `crates/renderer/shaders/water.frag` (`reflectionMiss`); pinned by `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` (the `reflectionMiss = mix(sceneFlags.yzw, skyTint.xyz, skyWeight);` assertion, and `path_environment_radiance_cites_live_sky_tint_evidence`'s `MISS_BLEND`)
- **Status**: NEW
- **Description**: `6db9eac2` replaced the direction-blind exterior miss with a `texture(skyCube, dir)` sample in `traceReflection` (`raytrace.glsl`) and `pathEnvironmentRadiance` (`lighting.glsl`). Its commit message and `docs/engine/skyal.md` describe that as the end of "every sky reflection … the same radiance toward the sun and away from it". Three other exterior sky-escape sites were not converted and still return the pre-SKYAL flat value:
  1. The glass face-on reflection, taken when `fresnelScalar <= 0.05`: `reflColor = isExteriorGlass ? (skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5) : sceneFlags.yzw`. Above the 0.05 threshold the same fragment calls `traceReflection`, which now misses into the cube. The sky seen in exterior glass therefore changes discontinuously at a Fresnel threshold: cube-sampled at grazing angles, flat at face-on angles.
  2. The glass refraction-ray miss: `refrColor = isExteriorGlass ? (skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5) : …`. Its own comment says it exists to "Match the `traceReflection` miss fallback above on the same gate". That equivalence stopped being true in `6db9eac2`, because the string survives in `raytrace.glsl` only as the no-bake branch.
  3. The water reflection miss: `reflectionMiss = mix(sceneFlags.yzw, skyTint.xyz, skyWeight)` in `water.frag`. Water is the most sky-dominated reflective surface in an exterior, and it now reflects a different, cloudless and azimuth-free sky than a mirror-like metal or glass edge a few metres away.

  `water.frag` already `#include`s `crates/renderer/shaders/include/bindings.glsl`, so it declares both `skyCube` and `CameraUBO.exteriorSkyTint`. The water pipeline layout uses the shared `scene_set_layout` at set 1 (`water.rs`), and binding 20 is `FRAGMENT`-visible. The cube is therefore reachable from all three sites with no layout change.

  `docs/engine/skyal.md`'s TODO list names prefiltered mips, irradiance projection and cloud type, but not these sites, so this isn't a known deferral.
- **Evidence**:
  ```glsl
  // raytrace.glsl — converted
  missCol = exteriorSkyTint.w > 0.5 ? texture(skyCube, direction).rgb
                                    : (skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5);
  // triangle.frag — glass face-on fallback, NOT converted
  vec3 reflColor = isExteriorGlass ? (skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5) : sceneFlags.yzw;
  if (fresnelScalar > 0.05) { vec4 reflRay = traceReflection(...); ... }
  // triangle.frag — refraction miss, comment claims parity with traceReflection
  refrColor = isExteriorGlass ? (skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5) : sceneFlags.yzw;
  // water.frag — NOT converted
  reflectionMiss = mix(sceneFlags.yzw, skyTint.xyz, skyWeight);
  ```
  `rg -n 'skyCube' crates/renderer/shaders` returns only `bindings.glsl` (declaration), `raytrace.glsl` and `lighting.glsl`.
- **Impact**: Visual only; no NaN, index or descriptor hazard (all three sites are exterior-gated and read only UBO scalars). Exterior glass shows a sky seam at the Fresnel threshold. Exterior water reflects a flat zenith/ambient gradient while adjacent RT reflections show the directional sun and baked clouds. The whole-frame sky-consistency goal of SKYAL is only partly met. Two contract tests pin the stale flat strings, so a contributor who notices will hit test failures and may conclude the flat blend is intended.
- **Related**: `6db9eac2`, `c379898f`; #3323 / #2226 (exterior-sky lanes); #3620 (`path_environment_radiance_cites_live_sky_tint_evidence`); `every_sky_cube_consumer_gates_on_the_ready_flag` (hand-listed, with no discovery walk).
- **Suggested Fix**:
  - Route all three sites through the same gated selection `traceReflection` uses. That means `exteriorSkyTint.w > 0.5 ? texture(skyCube, dir) : <current fallback>`, keeping the directional `smoothstep` horizon blend on water.
  - Update the two pinning tests to accept the gated form.
  - Make `every_sky_cube_consumer_gates_on_the_ready_flag` discover every `texture(skyCube,` user instead of listing two files.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
