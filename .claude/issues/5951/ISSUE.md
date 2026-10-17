# PERF-VOL-02: `flameSourceSampleRatio` runs for every froxel in a flame-bearing cluster even when the transported medium it scales is zero

Labels: low,performance,shaders,bug
Source: docs/audits/AUDIT_PERFORMANCE_2026-10-17.md

- **Severity**: LOW
- **Dimension**: GPU Pipeline (Dim 5)
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp:2975-2983` (call), `:1040-1095` (function)
- **Status**: NEW
- **Description**: The ratio is computed unconditionally inside `if (!transportExpired)` and then multiplied into `transported_combustion.extinction/scattering/emission`. When `transportedMediumActive` is false (`extinction <= 1e-7`), `combustionMedium` has returned an all-zero `LocalMedium` (`:2660-2663`), so the product is zero whatever the ratio is. The function loops up to 64 cluster volumes and runs `evaluateLocalFogVolume` twice for each flame volume.
- **Evidence**: `localFogCluster` returns early only when the cell holds no volumes, so every froxel in a 16 m cluster that contains any flame volume pays the loop, including the empty-air majority. The cost per non-flame volume is the cheap bounds test (`profileRadius >= 1.0` exits before the two noise samples, `:935-936`), so this is a small constant, not a hotspot.
- **Impact**: Wasted ALU in candle- and torch-heavy cells while transport is armed. Not measured; likely well under the inject total. Behaviour-neutral to remove.
- **Related**: #4784 (the stencil gate this complements).
- **Suggested Fix**: Guard the call with `transportedMediumActive`; leave the ratio at 1.0 otherwise.

## Completeness Checks
- [ ] **SHADER**: GLSL change recompiled with plain `glslangValidator -V` (no `-g0`); `.spv` committed; GpuInstance/UBO lockstep tests still green
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-10-17.md` (area-scoped `volumetrics-deep` suite run, HEAD e16a624e9). Fix with `/fix-issue <number>`._
