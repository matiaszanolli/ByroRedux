# SAFE-VOL-2026-10-17-01: the finite-fp16 guard (#4782) is not carried through the integration store

Labels: low,safety,shaders,renderer,bug
Source: docs/audits/AUDIT_SAFETY_2026-10-17.md

- **Severity**: LOW. Defence-in-depth gap. Reachability depends on emitted radiance above the fp16 ceiling, and no current content was measured to reach it.
- **Dimension**: GPU-fed data / non-finite handling (Dim 7).
- **Location**:
  - `crates/renderer/shaders/volumetrics_integrate.comp:79-100` (`inscatter_total += inscatter * trans_cumulative * sourceIntegral;` then `imageStore(integrated, …, vec4(inscatter_total, trans_cumulative))`).
  - Upstream guard: `volumetrics_inject.comp:3290` (history scrub) and `:3345-3353` (clamp `rgb`/`a` to `6.0e4`, zero if non-finite).
  - CPU boundary: `byroredux/src/render/fog_volumes.rs:285-290` (`sanitize_emission` is finite + `max(0.0)` only, no ceiling).
- **Description**: The `integrated` image is RGBA16F (finite ceiling 65504). The inject pass clamps every stored lane to `6.0e4`, but the integration pass accumulates up to `froxel_z_slices` (≤ 256, default 64) slabs in fp32 and converts to half on `imageStore` with no clamp. A column whose source radiance (`inscatter * sourceIntegral`, which is the medium's source-function radiance) exceeds 65504 therefore stores `+Inf`. `composite.frag` has no `isnan`/`isinf` guard (grep: none) and computes `scene * vol.a + vol.rgb`. The #4782 comment itself notes that "composite/bloom reach the screen with no guard either", so one Inf froxel column becomes a bloom-spread white or NaN smear. Unlike the inject history, the integrated volume is not fed back, so the damage lasts one frame per occurrence. The `6.0e4` inject clamp does not bound the integral: with a small extinction the per-slab factor `sourceIntegral` is about `dt` (tens to hundreds of world units).
- **Disproof attempted**: In the physical regime `inscatter ≈ sigma * L` and `sourceIntegral ≈ 1/sigma`, so the integral is bounded by about `L`, the source radiance. An overflow therefore needs authored `L_e` or sun radiance above 65504, which no vanilla-derived value reaches (not measured on device). The CPU side only floors emission and applies no upper bound, so an absurd-but-finite authored/ECS value would pass `sanitize_emission`. That is why this is LOW and not MEDIUM.
- **Suggested fix**: Apply the same two-line `isnan/isinf → 0` plus `clamp(…, 0, 6.0e4)` to `vec4(inscatter_total, trans_cumulative)` before the integrate store, and optionally cap `sanitize_emission` at a documented radiance ceiling. This needs a `.spv` rebuild (plain `glslangValidator -V`); the reflection tests then pin the result.

## Completeness Checks
- [ ] **SHADER**: GLSL change recompiled with plain `glslangValidator -V` (no `-g0`); `.spv` committed; GpuInstance/UBO lockstep tests still green
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other init/teardown sites)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_SAFETY_2026-10-17.md` (area-scoped `volumetrics-deep` suite run, HEAD e16a624e9). Fix with `/fix-issue <number>`._
