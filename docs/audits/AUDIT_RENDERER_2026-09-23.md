**HEAD**: `2237da9c3` · **Baseline**: `AUDIT_RENDERER_2026-09-21.md` (HEAD `f97775ca8`) · **Audited**: Dims 2, 3, 4, 5, 7, 8, 10, 12 (volumetrics slice only), plus Dim 1 as a TLAS *consumer* · **Unchanged since baseline (skimmed)**: none of the volumetrics paths changed materially. Only `2237da9c3` (doc sweep) and `77eb4b752` (exposure-meter gate) touched them. Per the `volumetrics-deep` preset the whole area was re-traced deep, not delta-skimmed. **Out of suite scope**: Dims 1 (BLAS/TLAS build), 6, 9, 11 (except composite's fog-driven FSR masks).

# Renderer Audit — 2026-09-23 (area-scoped: M55 volumetric fog)

This is one leg of `/audit-suite --preset volumetrics-deep`. One auditor ran every dimension in sequence, with no sub-agents. The audit is **area-scoped to the volumetric fog system (M55)**:

- **Pass**: `crates/renderer/src/vulkan/volumetrics.rs` + `volumetrics/`, and `volumetrics_inject.comp` / `volumetrics_integrate.comp` plus their includes (`froxel_slices.glsl`, `medium_transport.glsl`, `blue_noise.glsl`, `shadow_common.glsl`).
- **Host plumbing**: `context/post_passes.rs` (`record_volumetrics_pass`), the `context/draw.rs` call site, `assemble_camera_and_lights.rs` (combustion-light drain), and `resize.rs` / `teardown.rs` / `init.rs`.
- **Consumer**: `composite.frag`.
- **App-side producers**: `byroredux/src/render/fog_volumes.rs` and `fog_height_reference`.

No engine was launched and no source was modified.

Evidence beyond reading the code:

- The renderer guard tests: 61 volumetric, reflect, contract and latch tests, all passing, none ignored.
- `scripts/check-shader-artifacts.sh`: all 35 SPIR-V artifacts are byte-identical to their sources.
- Two numeric reproductions (Python mirrors of the shader math):
  - a float32 simulation of the froxel jitter;
  - an exact evaluation of the hybrid-Z field/texel offset.

**Totals (NEW): 0 CRITICAL · 1 HIGH · 3 MEDIUM · 2 LOW** (6 findings; none matches an open issue, none is a regression).

## Executive Summary

The pass's GPU plumbing is in good shape. Barriers, the skip/neutral latch, resize rebinding, teardown, the struct mirrors and the SPIR-V artifacts all trace clean, and every guard the skill names exists, runs and passes. The risk is in the **host argument plumbing** and in the **transported combustion field's numerics**:

- **HIGH: the froxel grid runs on a scale height of about 1 m, not 30–143 m** (REN-D8-2026-09-23-01). `draw_frame` passes `fog_coverage, fog_scale_height_meters` positionally into `record_post_passes`, whose parameters are declared in the opposite order. Both are `f32`, so it compiles.
  - **What the shader gets instead**:
    - The inject shader's exponential height term runs on an e-folding height of `coverage × 70` BU, i.e. 0.4–0.86 m.
    - The procedural coverage is pinned at 1.0.
  - **Visible effect**:
    - Froxel fog (and the interior "dust" that makes candle shafts visible) is 1.6–14 % of its intended density at eye height, and essentially zero a few metres up.
    - The analytic beyond-grid tail still uses the correct scale height, so the two models of the medium now disagree at the 128 m seam. This is exactly what the #3956 commit said must not happen.
  - **Origin**: introduced 2026-09-10 by `ea3ba6098` and missed by the last three renderer audits.
- **MEDIUM: the transported smoke/fire field drifts toward the camera by itself** (REN-D8-2026-09-23-02).
  - The field is written at each slab's arithmetic-mean distance but sampled as if it sat at the slice-coordinate texel centre, which is the geometric mean in the exponential region.
  - Every frame therefore backtraces 0.0072 texels too far. Beyond 5 m that is a camera-ward drift of about 2.6 % of the distance per second at 60 fps (6 % at 144 fps), even with the camera parked and zero velocity.
- **MEDIUM: residual soot freezes instead of finishing its decay** (REN-D8-2026-09-23-03).
  - Once the 78 s linger window lapses, `simulationDt` drops to 0. The dispatch keeps running whenever any global medium exists, which covers every lit interior via the dust floor.
  - With `dt = 0` the field is carried with no dissipation. The ≤ 3 % tail becomes a permanent world-anchored haze, and it still pays the per-light transmittance march.
- **MEDIUM: the froxel jitter degrades over a long session** (REN-D8-2026-09-23-04).
  - The float32 `fract(rank + frame × R3)` loses the rotation to rounding. By about 1.4 M frames (6.4 h at 60 fps, 2.7 h at 144 fps) only 8 distinct jitter levels are left, and the rotation is fully degenerate by about 2²³ frames.
  - The same rounding produces exact `jitter.z == 0` samples at the camera eye, which makes `normalize(0)` undefined in the phase math.

## RT Pipeline Assessment (volumetrics' use of AS, ray queries, temporal history)

- **TLAS consumption.**
  - The AS_BUILD → ray-query barrier names `COMPUTE_SHADER` on both build arms (#2931).
  - `write_tlas` rebinds per dispatching frame from `tlas_handle(frame)`.
  - On a failed build, the retained older TLAS may index a stale instance order in binding 19. `rigidBoundaryNormal` bounds-checks the instance, index and vertex ranges, so the worst case is one wrong boundary normal; there are no out-of-bounds reads.
  - The #4576 FX-card mask fix still holds (`shadow_mask_for_instance` routes kind 102 and additive blends to `VISIBILITY_LAYER_EFFECT`).
- **Ray queries.**
  - Sun rays use the opaque mask, plus a glass-only second pass bounded to the grid far plane in interiors.
  - Local lights use `tMin` 0.05 and `tMax = max(dist − sourceRadius, 0.05)`, so tMin ≤ tMax always holds.
  - Cluster lookup uses radial distance and the `fog_far` basis, identical to `cluster_cull.comp` and `triangle.frag`.
  - `combustionPathBlocked` uses `VISIBILITY_MASK_SOLID`.
- **Temporal history.**
  - Nearest-column XY and linear-Z history reprojection for radiance.
  - A linear transport sampler for chemistry, dynamics and optical.
  - `history_valid` is promoted only in `mark_frame_completed` after a successful submit.
  - `signal_temporal_discontinuity` and `record_neutral_frame` both reset it.
  - The findings here are numerical (REN-D8-02/03/04), not synchronisation.

## GPU-Struct & Memory Assessment

- **Structs.**
  - `VolumetricsParams` (2×mat4 + 13×vec4) matches the GLSL block field-for-field (hand diff).
  - `GpuFogVolume`, `GpuBoundaryInstance` (160 B stride), `CombustionLightMoment` (8 words) and `IntegrationParams` are all pinned by passing tests.
  - The one gap is that the `VolumetricsParams` pin checks size only (REN-D3-2026-09-23-01).
- **Memory / lifecycle.**
  - `destroy` frees everything: 6 volumes per FIF slot, 2 noise volumes, 6 buffer vectors, 2 pipelines, layouts, pools and 3 samplers. The buffer vectors are cleared so their `Arc<Allocator>` clones are released.
  - `teardown.rs` destroys the pass before the allocator unwrap.
  - Resize destroys and recreates the whole pass after device idle, rebinds composite binding 6, and resets `volumetrics_cleared_on_skip`.
  - `froxel_grid_cost_matches_the_memory_budget_doc` passes.
  - The moment and cluster buffers are seeded dirty so their first uploads cover non-zeroed allocations.

## Findings

### HIGH

#### REN-D8-2026-09-23-01: `fog_coverage` and `fog_scale_height_meters` are transposed at the `record_post_passes` call, collapsing the froxel medium to a ~1 m ground layer
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

### MEDIUM

#### REN-D8-2026-09-23-02: transported combustion field is written at the arithmetic-mean slab distance but sampled at the geometric-mean texel centre, a constant camera-ward drift of 0.0072 slices/frame
- **Severity**: MEDIUM (visual; affects all combustion content such as fire, smoke and explosions).
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp`:
  - `main` (`float fieldT = 0.5 * (fieldFrontT + fieldBackT)`, ~line 2574);
  - `reprojectHistory` (`sliceCoordinate(previousDistance)`);
  - `samplePreviousTransport` (`texture(previousCombustion*, previousUvw)` through the LINEAR `transport_sampler`).
- **Status**: NEW
- **Description**:
  - Texel *i* of `combustionState` / `Dynamics` / `Optical` is written for world position `froxel_to_world(fieldUv, fieldT)`, where `fieldT = (d_i + d_{i+1}) / 2`.
  - The next frame reads the field with `texture()` at `uvw.z = froxelSliceCoordinate(d)`. That places texel *i*'s centre at `froxelSliceDistance((i+0.5)/N)`, which in the exponential region (> 5 m) is the geometric mean `√(d_i·d_{i+1})`.
  - The two positions differ. A destination froxel that asks for its own position lands `δ = ln((1+r)/(2√r)) / ln r` texels too far, with `r = (far/linear)^(1/((1−f)N))`.
- **Evidence**: an exact evaluation with the default `VolumetricsConfig` (64 slices, 128 m far, 5 m linear floor, fraction 0.125):
  - δ = **+0.0072 texel** for every slice ≥ 8;
  - 0 in the linear region.

  Every carried value therefore becomes `(1−c)·T_i + c·T_{i+1}` with c = 0.0072 per frame. That is upwind advection toward the camera, independent of velocity and of `dt`: the `dt = 0` carry path at `chemistry = hadHistory ? probeChemistry : …` resamples the same way.
  - At 33 m (slice 40, slab 1.9 m) this is about 0.8 m/s at 60 fps, and frame-rate dependent (about 2 m/s at 144 fps).
  - A parked camera and a lingering cloud give `d(t) ≈ d₀·e^(−0.026 t)` at 60 fps.
  - The single-pass BFECC does not remove it. `probeChemistry` and the round-trip `errorChemistry` carry the same mapping bias, so their difference excludes it.
- **Impact**:
  - Transported smoke, fire plumes and explosion clouds lean and creep toward the viewer, faster at higher frame rates.
  - The resampling adds extra numerical diffusion along Z.
  - Emission sources re-inject at the correct place every frame, so the artefact is strongest on lingering (unsourced) media.
- **Related**: REN-D8-2026-09-23-03 (the frozen residual also drifts); #2470 (the analogous half-slab convention fix for the integrated volume, closed).
- **Suggested Fix**: Store the field at the texel-centre distance, `fieldT = sliceDistance((z + 0.5) / N)`, and derive the slab endpoints the same way, so that writing and sampling share one convention. Confirm on device: a static-camera A/B of a lingering explosion cloud at 20–40 m.

#### REN-D8-2026-09-23-03: after the combustion linger window, `simulationDt = 0` freezes the ≤ 3 % soot residual instead of decaying or clearing it, whenever the pass keeps dispatching
- **Severity**: MEDIUM (visual residue, plus ongoing per-froxel transmittance-march cost).
- **Dimension**: Volumetrics
- **Location**:
  - `crates/renderer/src/vulkan/volumetrics.rs`: `VolumetricsPipeline::dispatch` (`frame_params.fog_reference[3] = if combustion_active { simulation_dt } else { 0.0 }`), `combustion_transport_active` and `requires_dispatch`.
  - `crates/renderer/shaders/volumetrics_inject.comp`: `transportCombustion` (the `else { chemistry = hadHistory ? probeChemistry : … }` arm and the `if (dt > 0.0)` decay block).
- **Status**: NEW (not covered by #3131, which introduced the `dt = 0` gate as a performance optimisation).
- **Description**:
  - `AEROSOL_LINGER_SECONDS` (78 s) is sized so that `exp(−0.045·78)` ≤ `AEROSOL_LINGER_CUTOFF_FRACTION` (3 %). The design assumes the renderer *stops* at that point: "avoiding a visible tail cut when the renderer stops the otherwise-idle transport dispatch".
  - It stops only when `requires_dispatch` is false, which then triggers a neutral clear and `history_valid = false`.
  - `requires_dispatch` stays true whenever `scatter_coef > 0`. That is every fogged exterior, and every interior with a local emitter, because `INTERIOR_DUST_EXTINCTION_PER_METER` forces 0.006/m.
  - In those scenes the shader keeps running with `dt = 0`. `transportCombustion` then copies `probeChemistry` / `probeOptical` forward unchanged, and every dissipation term (`AEROSOL_DISSIPATION`, fuel removal, radiance removal, cooling) is multiplied by `dt` inside `if (dt > 0.0)`, so nothing decays.
- **Evidence**:
  - `combustion_transport_active` returns false once `simulation_time > combustion_active_until_seconds`.
  - `requires_dispatch` returns `has_global_medium || !fog_volumes.is_empty() || …`.
  - In the shader, `simulationDt = clamp(params.fog_reference.w, 0, 1/15)` → `transportCombustion(..., dt = 0)` → the `else` arm carries history.
  - `combustionMedium` treats any `sigmaT > 1e-7` as active medium.
- **Impact**:
  - Up to 3 % of an explosion or smoke cloud's extinction and scattering persists indefinitely as a static, world-anchored haze. It also keeps drifting per REN-D8-02 until it leaves the frustum or a discontinuity or resize resets history.
  - `transportedMediumActive` stays true for those froxels, so each keeps paying `transportedCombustionTransmittance` for the sun and for every admitted local light.
- **Related**: #3131 (the `dt = 0` gate), REN-D8-2026-09-23-02.
- **Suggested Fix**: When transport lapses while history is valid, either:
  - (a) keep feeding a real `dt` to the dissipation terms only (skip the advection stencil, which is what #3131 wanted to save); or
  - (b) have the shader zero the transported fields when `dt == 0` and the linger has expired. For example, pass an "expired" flag and write the empty state, so the residual is dropped exactly as it is in the no-medium path.

#### REN-D8-2026-09-23-04: froxel jitter `fract(rank + frame * R3)` is evaluated in float32 and collapses over a session; exact zero-jitter samples put slice 0 at the camera eye (`normalize(0)`)
- **Severity**: MEDIUM (visual; long-session trigger; the `normalize(0)` part is driver-dependent).
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp` `froxelJitter` (`return fract(rank + frame * vec3(0.754877666, 0.569840296, 0.438289))`) and `main` (`vec3 view_dir = normalize(params.camera_pos.xyz - world_pos)`, ~line 2615). The frame index comes from `volume_params.w = (frame_counter & 0x00ff_ffff) as f32` in `record_volumetrics_pass`.
- **Status**: NEW
- **Description**:
  - The Cranley-Patterson rotation `frame × c` is a float32 product. Its fractional resolution is `ulp(frame × c)`, which coarsens as the frame count grows. The 24-bit mask keeps `frame` exact but not the product.
  - `frame_counter` resets only on resize (`resize.rs`).
  - The same rounding also makes `rank + frame·c` land exactly on an integer on some frames. That gives `jitter.z == 0.0`, so for `coord.z == 0`, `t = mix(0, d₁, 0) = 0` and `world_pos == camera_pos` exactly.
- **Evidence** (float32 simulation over the 64 ranks):
  - Exact `jitter.z == 0` occurs in 0.2 % of (frame, rank) pairs within the first 100 k frames, and in ≥ 25 % at 8 M frames.
  - The x channel has only **8 distinct values** at frame 1.5 M.
  - For `c = 0.7549` the resolution reaches 1/8 at about 1.39 M frames (6.4 h at 60 fps, 2.7 h at 144 fps) and 1.0 at 2²³ frames.
  - With `world_pos == camera_pos`, `normalize(vec3(0))` evaluates to 0·∞. The following `clamp(NaN, …)` in `medium_henyey_greenstein` is implementation-defined (GLSL.std.450 FMin/FMax on NaN operands). NVIDIA returns the non-NaN bound; other drivers may propagate NaN.
  - A NaN in the raw V-buffer would self-sustain: `mix(current, history, historyWeight)` with a NaN weight. It would then poison the column in `volumetrics_integrate.comp` and the composite pixel.
- **Impact**:
  - Long sessions lose the jitter that hides the per-froxel single-ray banding. The Prospector banding M-LIGHT v2 was built to remove comes back: gradually by ~6 h, fully by ~39 h at 60 fps.
  - On non-NVIDIA drivers, there is a latent NaN path from slice-0 froxels at the eye.
- **Related**: #2509 (per-froxel ray budget / banding history). `math_common.glsl`'s IGN (`frameCount * 5.588238`) has the same float-product shape but is outside this suite's scope.
- **Suggested Fix**:
  - Build the rotation from the integer frame index in fixed point, e.g. `float(uint(frame) * 0x9E3779B9u >> 8) * (1.0 / 16777216.0)` per channel, or wrap the frame modulo a period before multiplying.
  - Use `view_dir = -ray_dir` (exact for every `t > 0`; cheaper), or clamp `t` to a positive minimum.

### LOW

#### REN-D8-2026-09-23-05: volumetrics doc rot — skip semantics, pass position, grid size, HG default, pipeline label, shader-pipeline.md rows
- **Severity**: LOW
- **Dimension**: Volumetrics
- **Location / Evidence**:
  - `context/post_passes.rs` `record_volumetrics_pass` doc says "composite reads the prior frame's integrated volume, which retains its last valid contents". `volumetrics.rs` `write_tlas` doc says "composite will reuse the prior frame's integrated volume". Both are false since #3685/#3647: a skip records `record_neutral_frame` (a neutral clear) through `skip_clear_decision`, and the code's own comment in the same function says "Never let a prior cell's integrated fog hang over a frame…".
  - `volumetrics.rs` `dispatch` doc: "Natural slot: between caustic and TAA in `draw.rs`". It is actually caustic → volumetrics → SSAO → composite, in `post_passes.rs`.
  - `context/resize.rs` (`volumetrics_cleared_on_skip` reset): "the froxel volume isn't resize-dependent (fixed grid, not screen-sized)". It is render-extent-derived and recreated in this very resize (`recreate_bloom_and_volumetrics`).
  - `context/init.rs` (composite volumetric views): "The 14 MiB × 2 / slot 3D-image allocation". The real figure is 44 B/froxel/slot; at 1080p (240×135×64) that is about 91 MB per slot.
  - `volumetrics_inject.comp` `henyey_greenstein` comment: "Current host default g = 0.4". `DEFAULT_PHASE_G` is 0.8.
  - `volumetrics/init.rs`: the inject pipeline is labelled `"Volumetrics clear"`, which becomes the error context "Volumetrics clear compute pipeline".
  - `docs/engine/shader-pipeline.md`:
    - the `volumetrics_inject.comp` row says "Inject sun-light into froxel grid", omitting clustered local lights, authored fog volumes and combustion transport;
    - binding 12 is described as a "transported emission field" when it is the scalar emissive-source share;
    - binding 13 is described as "for semi-Lagrangian backtrace" when it is used for temporal reprojection;
    - the `composite.frag` row still says "bloom add", although bloom is applied in place by `bloom_apply.comp` after composite.
- **Status**: NEW
- **Suggested Fix**: Correct the prose; relabel the pipeline "Volumetrics injection".

#### REN-D3-2026-09-23-01: `VolumetricsParams` has a size-only Rust↔GLSL pin; a lane transposition inside the 15-member UBO is invisible
- **Severity**: LOW (test gap; the layout is currently correct by hand diff).
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/reflect.rs` `volumetrics_ubo_sizes_match_host_structs_in_every_shader`; `volumetrics.rs` `VolumetricsParams`.
- **Status**: NEW
- **Description**: The reflection test compares block *sizes* only. `GpuFogVolume` has a field-order lockstep test (`gpu_fog_volume_glsl_field_order_matches_rust_struct`), but `VolumetricsParams` has none, although it is 13 same-typed vec4 lanes whose `.w` slots are overloaded (`render_origin.w` = is_exterior, `fog_reference.w` = dt, `wind_gust.y` = BFECC switch). REN-D8-01 shows that same-typed `f32` transpositions do happen in this plumbing.
- **Suggested Fix**: Add a GLSL-member-order test mirroring the `GpuFogVolume` one, parsing the `uniform VolumetricsParams { … }` block in `volumetrics_inject.comp`.

## Prioritized Fix Order

1. **REN-D8-01**: a one-line argument swap, plus retiring the positional hop and adding a lane test. It restores all near-field fog, godrays and interior dust in every game.
2. **REN-D8-03**: stop the frozen residual. Small, local and host-plus-shader; this also removes a standing performance cost.
3. **REN-D8-02**: align the field's write and sample Z convention. Needs a device A/B.
4. **REN-D8-04**: switch to a fixed-point jitter rotation and use `view_dir = -ray_dir`.
5. **REN-D3-01**, **REN-D8-05**: test gap and doc sweep.

## Needs-RenderDoc / live validation

The runtime audit runs after the suite; none of these was run here.

- **REN-D8-01, before/after**: FNV exterior at dusk (godrays through a canopy); FO4 exterior with authored `FNAM` height fog (should thicken markedly); a lit FNV/Skyrim interior (dust shafts at eye height); the 128 m seam on a vista. Use `render.debug` `VOLUMETRIC_TERM` for the grid in isolation.
- **REN-D8-02**: parked camera, lingering explosion cloud at 20–40 m, 60 vs 144 fps. The cloud's apparent distance should stay fixed after the fix.
- **REN-D8-03**: detonate, then wait more than 160 s (CPU linger plus renderer linger) in a fogged exterior. `VOLUMETRIC_TERM` should return to the no-combustion baseline.
- **REN-D8-04**: the `normalize(0)` path on a non-NVIDIA driver, and long-session banding (force `frame_counter` high via a debug hook rather than waiting).
- **Sync (Dim 4)**: a `BYRO_VALIDATION=1` synchronization pass across a skip → run → skip transition (cell load while the TLAS is absent). Traced clean in code; not device-confirmed.

## Stale skill premises (for the next `/audit-renderer` sync)

1. **Dim 8 checklist**: "inject = one `TerminateOnFirstHit` shadow ray per froxel" is still stale; the 2026-09-20 report flagged it and it was not synced. The real per-froxel load is:
   - up to 2 sun rays;
   - up to `MAX_FROXEL_LIGHTS` × 2 local-light rays;
   - the combustion `combustionPathBlocked` boundary queries on the transport path;
   - the `transportedCombustionTransmittance` marches.
2. **Dim 8 Paths / checklist**:
   - Add the host call-site hop: `context/draw.rs` `draw_frame` → `record_post_passes` → `VolumetricsPassInputs`. REN-D8-01 lived there, outside every listed path.
   - Add the transported-combustion checks: five of the six per-slot volumes are combustion or history fields; BFECC (`BYRO_BFECC`); the `dt = 0` gate; the linger/cutoff contract in `crates/core/src/combustion.rs`; the write/sample Z convention.
3. **Dim 3 guard line**: `every_remaining_uniform_block_size_matches_its_host_struct` and `volumetrics_ubo_sizes_match_host_structs_in_every_shader` are size-only; say so, and name `gpu_fog_volume_glsl_field_order_matches_rust_struct` as the pattern to extend.
4. **Dim 8**: "no-emitter interiors give neutral non-NaN output while rooms with real local emitters keep dust" is true of the host gate (`INTERIOR_DUST_EXTINCTION_PER_METER`), but until REN-D8-01 is fixed the dust is ~95 % suppressed at eye height. The checklist should require checking the medium's vertical profile, not only whether a medium exists.

## Guard posture

- `cargo test -p byroredux-renderer --lib` with filters `volumetric froxel fog_volume combustion record_volumetrics_pass uniform_block boundary_instance`: 50 pass. The second filter set (`record_volumetrics_pass gpu_boundary_instance_stride combustion_light_moment_abi skip_clear volumetrics_`) gives 11 pass. None is ignored, and none is vacuous: each asserts on live source, SPIR-V or computed values.
- `scripts/check-shader-artifacts.sh`: 35 shader artifacts match glslang 11:16.2.0.
- Source-shape guards cannot see REN-D8-01 through 04. All four are numeric or argument-order faults with no string signature.
