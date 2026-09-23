# #4773: REN-D8-2026-09-23-01: `fog_coverage` and `fog_scale_height_meters` are transposed at the `record_post_passes` call, collapsing the froxel medium to a ~1 m ground layer

**Severity**: HIGH
**Labels**: high, renderer, bug
**Source**: docs/audits/AUDIT_RENDERER_2026-09-23.md (REN-D8-2026-09-23-01)

- **Severity**: HIGH. Rendering correctness on the default path, in every game and every cell with fog or interior dust (decision tree: "rendering correctness → at least HIGH").
- **Dimension**: Volumetrics
- **Location**:
  - `crates/renderer/src/vulkan/context/draw.rs:2051-2053` — the `draw_frame` → `record_post_passes` call.
  - `crates/renderer/src/vulkan/context/post_passes.rs:245-247` — the `record_post_passes` signature.
  - Consumed in `record_volumetrics_pass` (`medium_params[3]` ← `fog_scale_height_meters`, `temporal_params[3]` ← `fog_coverage`).
- **Status**: NEW. Introduced by `ea3ba6098` (Fix #3956/#3957, 2026-09-10). No issue or prior report mentions it.
- **Description**: `ea3ba6098` added `fog_scale_height_meters` in different positions on the two sides:
  - it was inserted *before* `fog_coverage` in the `record_post_passes` signature and in `VolumetricsPassInputs`;
  - it was inserted *after* `fog_coverage` in the positional call in `draw_frame`.

  Both are `f32`, so the transposition compiles. The named struct `VolumetricsPassInputs` (#2258) protects only the second hop. The positional `record_post_passes` hop is where it happened.
- **Evidence**:
  ```
  // post_passes.rs record_post_passes(...)          // draw.rs draw_frame → record_post_passes(
  fog_single_scatter_albedo: f32,                     fog_single_scatter_albedo,
  fog_scale_height_meters: f32,     <-- receives --   fog_coverage,
  fog_coverage: f32,                <-- receives --   fog_scale_height_meters,
  fog_height_reference: f32,                          fog_height_reference,
  ```
  - **Data flow**:
    - `VolumetricsParams.medium_params.w` = `fog_coverage × 70` BU.
    - `fog_coverage_from_weather` yields 0.40–0.86, and `FogMedium::DISABLED` gives 0.55. The scale height is therefore 28–60 BU (0.4–0.86 m) instead of 2100 BU (30 m default) or the ~10 000 BU (143 m) median that FO4/FO76 author.
    - `temporal_params.w` = `fog_scale_height_meters.clamp(0.01, 1.0)` = 1.0 for every real scale height.
  - **Shader side**: `proceduralDensityScale` computes `exp(-heightAbove / max(medium_params.w, 1.0))`, anchored at `fog_height_reference` (the ground ray-cast). At the character eye (~116 BU above the ground ray: `eye_height` 52 + half-height 46 + radius 18) that is `e^(-116/28…60)` = **1.6–14 %** of the intended density, and `e^(-116/38.5)` ≈ 5 % indoors.
  - **Composite side**: the beyond-grid tail reads `height_fog_params.y`, built in `build_composite_params` from the *correctly named* field, so it uses the true scale height.
- **Impact**:
  - Within the 128 m froxel grid, authored CELL/WTHR fog, sun godrays and the interior dust floor are nearly gone above knee height, in every game.
  - The procedural coverage (the WTHR classification) is ignored and fixed at maximum occupancy.
  - At the grid far plane the tail continues from a near-empty grid while applying the full authored medium beyond it. This is a visible seam and the exact failure `#3956`'s commit message warns about.
  - #3956's own FO4/FO76 authored-altitude work is a no-op in the grid.
  - Nothing catches this: `composite_params_tests` pins only the composite hop, and there are no device tests.
- **Related**: #3956 (closed; the fix that introduced this), #2225 (height reference), #2258 (named-input struct).
- **Suggested Fix**:
  - Swap the two arguments at the `draw.rs` call.
  - Better, remove the positional hop: have `draw_frame` build `VolumetricsPassInputs` (named fields) and pass it through `record_post_passes`, as #2258 did for the inner call.
  - Add a unit test that the scale height reaches `medium_params[3]` and the coverage reaches `temporal_params[3]`. This mirrors the existing `height_fog_params[1]` assertion.

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
