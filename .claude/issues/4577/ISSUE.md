# REN-D12-2026-09-21-01: `composite.frag.spv` is stale on two generated constants — both new debug views render full-frame magenta and the `DBG_VIZ_AO` oracle is contaminated (regression of #3322)

**Labels**: medium, renderer, shaders, test-gap, bug

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: MEDIUM (debug surface; same grade as #3322) · **Dimension**: Debug/Telemetry
**Location**: `crates/renderer/shaders/composite.frag.spv` vs `crates/renderer/shaders/composite.frag`: the `debugMode > RENDER_DEBUG_MODE_MAX` guard (~:418-423) and `DBG_VIZ_REQUIRES_RAW_OUTPUT(dbgFlags)` (~:430), both fed by the generated `crates/renderer/shaders/include/shader_constants.glsl`
**Status**: Regression of #3322 (closed 2026-08-27)
**Verified against**: HEAD `f97775ca8`

## Description

The committed `composite.frag.spv` was last rebuilt in `0e12f1f10` (2026-09-20). Two later commits changed generated constants that composite compiles in, and both recompiled only `ssao` and `triangle`:
- `09d9bc6f8` added `DBG_VIZ_AO` (0x200) to `DBG_VIZ_RAW_OUTPUT_ANY_MASK` (2215116800 → 2215117312).
- `f97775ca8` added `RENDER_DEBUG_FACING_RATIO` (14) and `RENDER_DEBUG_RESTIR_LIGHT` (15), bumping `RENDER_DEBUG_MODE_MAX` from 13 to 15.

#3322 fixed the same failure: a stale composite binary renders a debug view magenta. That fix was a recompile with no composite SPIR-V pin, and the triangle sibling's pin (#3120) does not cover composite.

## Evidence

- `spirv-dis` of the committed binary at HEAD:
  ```
  %uint_2215116800 = OpConstant %uint 2215116800
  %2858 = OpUGreaterThan %bool %2857 %uint_13
  %2874 = OpBitwiseAnd %uint %2872 %uint_2215116800
  ```
  The source constants are `RENDER_DEBUG_MODE_MAX 15u` and `DBG_VIZ_RAW_OUTPUT_ANY_MASK 2215117312u`.
- The audit ran `scripts/check-shader-artifacts.sh` with the matching glslang (11:16.2.0). `composite.frag.spv` is the only drifting artifact of 35, and a fresh compile differs in exactly these three instructions.
- CI: the **Shader source/artifact parity** job (`shader-artifacts` in `.github/workflows/ci.yml`) has reported `DRIFT crates/renderer/shaders/composite.frag.spv` on every main run from `42725fdcc` through `f97775ca8`. `42725fdcc` is the first run that contains `09d9bc6f8`.

## Impact

- **Both new views render magenta.** `render.debug facing` and `render.debug restir` (modes 14/15) trip composite's out-of-range guard. The host's `render_debug_requires_raw_output` routes every non-Final mode raw past bloom, TAA, FSR and presentation, so the magenta frame reaches the screen. The two views added for the single-sided-wall light-leak hunt are unusable.
- **The `DBG_VIZ_AO` oracle is contaminated.** In the legacy-flags path, the stale composite treats `DBG_VIZ_AO` as non-raw. It composites caustics (into direct), fog, volumetric transmittance and sky over the raw AO image, so the oracle is clean only in fog-free, caustic-free scenes (the Cornell box it was tuned on).
- **CI signal is lost.** The parity job has been red on main since the drift, which masks any further SPIR-V drift.

## Related

- #3322 (closed): the same stale-composite failure. This is its recurrence.
- #3120 (closed): the `triangle.frag.spv` sibling. That one is pinned by `triangle_frag_spv_debug_mode_guard_matches_render_debug_mode_max` (`crates/renderer/src/vulkan/reflect.rs`).
- #1917 (closed): an earlier stale-`composite.frag.spv` instance.
- REN-D2-2026-09-21-01 (#4582) and REN-D2-2026-09-21-02 (#4583): defects in the two new views themselves, which become visible once this is fixed.

## Suggested Fix

1. Recompile `composite.frag.spv` (plain `glslangValidator -V`, glslang 11:16.2.0) and confirm `scripts/check-shader-artifacts.sh` passes.
2. Extend the `max_u_greater_than_rhs_constant` pin in `triangle_frag_spv_debug_mode_guard_matches_render_debug_mode_max` to `composite.frag.spv`.
3. Add an `OpConstant == DBG_VIZ_RAW_OUTPUT_ANY_MASK` presence pin for every shader that uses `DBG_VIZ_REQUIRES_RAW_OUTPUT` (`composite.frag`, `presentation.frag`, `triangle.frag`).

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D12-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every shader that compiles `RENDER_DEBUG_MODE_MAX` or `DBG_VIZ_REQUIRES_RAW_OUTPUT` (`composite.frag`, `presentation.frag`, `triangle.frag`) is byte-fresh; `scripts/check-shader-artifacts.sh` is green
- [ ] **TESTS**: a SPIR-V constant pin for `composite.frag.spv` (debug-mode guard + raw-output mask), so a generated-constant bump without a recompile fails `cargo test`, not only CI's parity job
