# PERF-D5-01: The underwater god-ray march inverts `inv_view_proj` per pixel, and the march grew from ALU-only to 144 depth taps per pixel without a measurement

Labels: low, performance, renderer, shaders, bug
Source: docs/audits/AUDIT_PERFORMANCE_2026-10-27.md

**Audit report**: `docs/audits/AUDIT_PERFORMANCE_2026-10-27.md` -- finding `PERF-D5-01` (LOW)
**Location (re-mapped to current code)**: `crates/renderer/shaders/composite.frag` (`underwaterGodRays`: `mat4 viewProj = inverse(params.inv_view_proj);` at ~:588; `underwaterSunVisibility`; added by 99475136b / baf2b78fd). Premise re-read at HEAD: the `inverse()` is still present.
**Status at filing**: NEW

## Description
The audit skill names a shader-side per-invocation `inverse()` of the camera matrix as a regression (`inv_vp` is computed once on the CPU). This is now the only such call in the shader tree. The composite UBO carries only `inv_view_proj`, so every submerged pixel recomputes a 4x4 inverse (~150 flops). The larger, unmeasured cost is structural: each of the 24 march steps calls `underwaterSunVisibility`, and each call does 6 x (`viewProj * vec4` + one `depthTex` fetch) = 144 depth fetches + 144 mat4*vec4 per submerged pixel, on top of two value-noise evaluations per step.

## Evidence
`composite.frag` `underwaterGodRays`: `mat4 viewProj = inverse(params.inv_view_proj); ... for (int i = 0; i < steps; ++i) { ... sum += focus * underwaterSunVisibility(p, toSun, sunDirection, sunPath, viewProj) * k * dt; }` with `const int steps = 24;`; `underwaterSunVisibility` loops `for (int i = 0; i < 6; ++i)`. Gated on `params.depth_params.x > 0.5 && params.underwater.w > 0.0 && params.sun_dir.w > 0.0`.

## Impact
Zero above water. Below the surface (derived, not measured): 1440p native ~3.7 M px x 144 ~= 530 M depth fetches per frame. The cost half is deferred to the runtime audit (submerged `composite_ms`); only the per-pixel `inverse()` is statically certain.

## Related
AUDIT_RENDERER_2026-10-27 REN-D8-01 (focus-noise pop), REN-D8-02 (depth through a linear sampler); #6415 and #6418 are also open on this function.

## Suggested Fix
Carry the forward `view_proj` in the composite params (the CPU already has it) and drop the `inverse`. Then measure `composite_ms` submerged (FNV / Skyrim lake, `--rt-test-ray-quality-tier` pinned) and only afterwards decide whether the 6-tap contact test needs fewer taps, an early-out on the first blocked tap, or a half-resolution march.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

