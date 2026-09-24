# #4807: PERF-D5-2026-09-23b-04: The per-cluster density cap rose 8× (8 → 64) and the volume cap 4× (128 → 512) with no cost characterisation

**Severity**: LOW
**Labels**: low, performance, renderer, shaders, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D5-2026-09-23b-04)

- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `shaders/include/shader_constants.glsl:342`; `volumetrics.rs:177`; the loops at `volumetrics_inject.comp:879` (`sampleLocalMedium`), `:942`, `:1145` (`applyCombustionSources`), `:751`
- **Status**: NEW (worsens #4787, #4784)
- **Description**:
  - Each admitted volume costs a profile evaluation with 2 noise fetches per sample, and transported volumes are still fully evaluated before being discarded (#4787), now up to 64 per cluster.
  - Off-screen shafts bypass the frustum and occupy density slots.
- **Impact**: Up to 8× the per-froxel loop work in dense fire or beam clusters. The ray tier can't reduce it, and it is unmeasured.
- **Suggested Fix**: Land #4787's reorder first; keep low-density LightShaft candidates out of the density list; record max `count`/`portal_count` in telemetry so the caps can be sized from data.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
