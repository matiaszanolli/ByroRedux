# #4782: SAFE-D7-2026-09-23-01: The raw V-buffer temporal history has no non-finite guard: one NaN/Inf froxel self-perpetuates and spreads until a history reset

**Severity**: MEDIUM
**Labels**: medium, safety, renderer, shaders, bug
**Source**: docs/audits/AUDIT_SAFETY_2026-09-23.md (SAFE-D7-2026-09-23-01)

- **Severity**: MEDIUM (a defense-in-depth gap). No producer has been demonstrated on reference hardware, but the blast radius once one occurs is large.
- **Dimension**: GPU-Fed Data & Shader Loop Bounds (NaN/Inf)
- **Location**:
  - `crates/renderer/shaders/volumetrics_inject.comp:2891-2954`, where `current` is assembled, blended with history and stored.
  - `volumetrics_inject.comp:1411-1430` (`sampleHistoryColumn`).
  - `volumetrics_integrate.comp:61-94`.
  - `composite.frag:187-243` and `:622-627`.
- **Status**: NEW. This is the same class as the closed #903 (TAA history) and SVGF's history guard, applied to a history buffer those fixes did not cover.
- **Description**:
  - Within this same shader, the transported chemistry, dynamics and optical fields are scrubbed on every read and write: `sanitizedChemistry`, `sanitizedDynamics` and `sanitizedOptical` at `:1455-1512`, `:2091-2093` and `:2362-2370`.
  - TAA (`taa.comp:268`) and SVGF (`svgf_temporal.comp:171`) reject non-finite history for the same reason.
  - The raw V-buffer is the exception. Neither `current` nor the reprojected `history` (binding 6 / `lighting_volumes`) is ever checked before `current = mix(current, history, historyWeight)` and `imageStore(froxel, coord, current)`.
- **Evidence**: why the poisoning holds under any GLSL `min`/`max` NaN semantics.
  - If `history.rgb` is NaN, then `relativeRadianceDelta` is NaN (`dot(abs(NaN - c))`, `:2942`), and `emissionAgreement = exp(-2.5 * NaN * frac)` is NaN even when `frac == 0`. That makes `historyWeight` NaN, so `mix(...)` returns NaN and the texel is re-stored as NaN every frame.
  - If `history.a` is NaN, `relativeDensityDelta` (`:2916`) is NaN, with the same result.
  - `sampleHistoryColumn` blends `z0` and `z1` linearly (`:1425-1429`). NaN in either tap gives a NaN result (`NaN * 0 = NaN`), so the poison spreads one slice per frame along the column in both directions. Camera reprojection spreads it sideways.
  - Integration then carries it to every deeper slice of the column (`inscatter_total += NaN`).
  - Composite blends a 2×2 set of columns with no guard (`accumulated += columnValue * weight`, `combined = combined * vol.a + vol.rgb`), so NaN reaches the linear-HDR scene.
  - That scene feeds the bloom pyramid, whose bright pass has no guard (`bloom_downsample.comp:83-88`). Downsample and upsample smear it over a large screen area every frame.
  - Recovery comes only from `signal_history_reset` or `record_neutral_frame`: a cell transition or a skip streak.
  - Candidate producers, none confirmed on the reference 4070 Ti:
    - fp16 overflow of `current.rgb` above 65504. The target is RGBA16F. The worst case is dense transported soot (σ up to 0.2/BU plus multiscatter gain) next to a bright combustion surface light, whose inverse-square falloff reaches up to 2500× at the 0.02 m source-radius floor (`froxelLightAtten`, `:1250-1259`).
    - The implementation-defined NaN results of GLSL `max`/`clamp` on the `max(x, 0.0)` guards.
    - `normalize(vec3(0))` for `view_dir` when `jitter.z` is exactly 0 on slice 0, so `t == 0` and `world_pos == camera_pos`. This is reachable at large frame counts, but most drivers' IEEE `max` then scrubs it.
- **Impact**: a persistent screen-space corruption: black or blown patches that grow via bloom until the next history reset. The #2736 image-health counter (`presentation.frag:199`) would count it, but nothing corrects it. The damage is visual only, with no memory unsafety.
- **Related**: #903 (TAA NaN history), #2736 (non-finite pixel counter), #1021 (HG g clamp), #2241 (slab saturation). The missing composite and bloom guards are renderer-owned and outside this leg's scope.
- **Suggested Fix**:
  - In `volumetrics_inject.comp`, replace a non-finite `history` with `current` before the blend, the same way `taa.comp` does. Clamp `current` to finite fp16 range (for example `min(rgb, 6.0e4)`) and zero it if it is non-finite before `imageStore`.
  - Optionally, add a single finite check on `vol` in `sampleVolumetricColumn`.

**Source**: `docs/audits/AUDIT_SAFETY_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
