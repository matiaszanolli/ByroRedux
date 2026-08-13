# REN-D17-05: Disney sheen tint multiplies raw albedo instead of the luminance-normalised tint

- **Severity**: MEDIUM
- **Dimension**: 17 — Disney BSDF
- **Location**: `crates/renderer/shaders/include/pbr.glsl` — `disneyDiffuseSplit` (the `sheenColor` line). Mirror docs: `GpuMaterial::sheen_tint` (`crates/renderer/src/vulkan/material.rs`) and `Material::sheen_tint` (`crates/core/src/ecs/components/material.rs`).
- **Description**: `disneyDiffuseSplit` builds sheen colour as `mix(vec3(1.0), albedo, sheenTint)`. Both cited references (Disney 2012, GLSL-PathTracer's `EvalDisneyDiffuse` — named verbatim in this function's own doc block) build it from a luminance-normalised tint (`Ctint = baseColor/Cdlum`). The normalisation exists so `sheenTint` transfers hue without changing sheen intensity. Raw albedo couples the two: at `sheenTint=1.0` a dark base colour (black velvet) scales the sheen lobe down ~20×; a base colour above 1.0 scales it up.
- **Evidence**: `vec3 sheenColor = mix(vec3(1.0), albedo, sheenTint);` … `o.sheen = FH * sheen * sheenColor;`. Every other term of the function verified to reproduce the reference exactly, which is what makes this one line stand out as unmarked rather than a deliberate simplification.
- **Impact**: Wrong sheen magnitude on any tinted-sheen material (cloth/silk/velvet lobe). Blast radius bounded today: `sheen`/`sheen_tint` have no source-format producer (NIFAL boundary writes `0.0` literally, #2514); only reachable via the `mat.set sheen_tint` console arm on the Cornell harness. Latent defect on the reference-validation path — activates the moment a sheen producer (BGSM v9+/Starfield .mat) lands.
- **Related**: #2514, #2489 (`mat.set` writes with no clamp — `sheenTint > 1` also extrapolates through this mix), the earlier π-scaling defect in the same lobe (`docs/audits/AUDIT_RENDERER_2026-05-24_DIM6_14.md`).
- **Suggested Fix**: Compute the tint the way both references do: `float lum = dot(albedo, vec3(0.3,0.6,0.1)); vec3 ctint = lum > 0.0 ? albedo/lum : vec3(1.0);` then `sheenColor = mix(vec3(1.0), ctint, sheenTint)` — or document the deviation if intentional.

## Completeness Checks
- [ ] TESTS: A Cornell `mat.set sheen_tint` probe with dark and bright albedo
- [ ] Do not tune blind — verify against a Cornell capture before landing

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2819
