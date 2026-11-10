**HEAD**: `dbc815c26` · **Baseline**: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (HEAD `9409e7b20`) · **Audited**: Dim 8 (participating media, primary), plus the volumetrics-touching parts of Dim 3 (struct layout), Dim 4 (recording / sync), Dim 5 (memory / teardown / resize) and Dim 12 (GPU timer rows) · **Unchanged since baseline (skimmed)**: Dims 1, 2, 6, 7 (only the TAA silhouette change landed; not volumetrics), 9, 10, 11

# Renderer Audit — 2026-11-09 (volumetrics-deep, delta)

Scope: the `volumetrics-deep` suite surface (`vulkan/volumetrics.rs`, `vulkan/volumetrics/`, `volumetrics_{inject,integrate,local_sim}.comp`, `include/volumetrics_bindings.glsl`) and its direct seams (`post_passes.rs` bracket, `composite.frag` consumption, `GpuTimers` rows, fog-cluster host prep, `VolumetricsConfig`, resize / teardown). Read-only: no engine launch, no `BYRO_VALIDATION=1` run, nothing GPU-side executed.

## Executive Summary

| Severity | NEW | Existing / Regression | Findings |
|---|---:|---:|---|
| CRITICAL | 0 | 0 | none |
| HIGH | 0 | 1 | REN-D8-06 (Regression of #4781) |
| MEDIUM | 1 | 0 | REN-D8-07 |
| LOW | 4 | 0 | 2 test-gap, 1 doc-rot, 1 hot-path CPU |

- **The suite brief's premise is wrong.** `SUITE_RULES.md` says there are zero commits on the volumetrics paths since 2026-10-31. Against the baseline HEAD (`9409e7b20`) there are twelve commits under `crates/renderer/src/vulkan/volumetrics*`, `composite.{frag,rs}` and `fog.rs`: the eleven baseline fixes (#6742, #6743, #6749-#6753, #6755, #6748) and one real change, `3500d2737` (#6728, the analytic fog tail now continues the grid's medium, and exterior surfaces fade through `sky_air_terms`). The two findings that matter below both come from that last commit and from a gap in an older fix (#4781).
- **All baseline findings are gone.** REN-D1-01, D2-01, D2-02, D3-01, D3-02, D4-01, D5-01 (#6737), D5-02 (#6738), D5-03, D6-01, D7-01, D7-02 (#6748), D8-01 to D8-05 and D12-01 (#6754) / D12-02 (#6755) each have a landing commit; the issue snapshot has no open #6736-#6760. I spot-checked the pins in code (below), not just the commit titles.
- **Guards**: renderer lib **1627 passed, 0 failed, 1 ignored** (the device-only `sky_cube` filter test). `scripts/check-shader-artifacts.sh`: 40 shaders + the early-test variant match glslang 11:16.4.0 byte for byte. Every Dim 8 Guard name exists exactly once, none `#[ignore]`d, and the baseline's missing pins now exist (`the_local_grid_follows_the_skip_and_completion_wiring`, the `reflect.rs` local-grid layout case, the `GpuLocalGridParams` pin, the sunlit/sunless pipeline pair).
- **What the guards cannot see** produced both real findings. REN-D8-06 is a bound that was applied to the froxel image and not to the sibling image added later. REN-D8-07 is shader arithmetic whose only test is a substring scan.

## Findings

### REN-D8-06: the local-grid irradiance image is twice the froxel width, but `fit_froxel_divisor_to_device` only bounds the froxel — a 3D image over `maxImageDimension3D` is fatal at boot
- **Severity**: HIGH (Vulkan spec violation floor; reachability is narrow, see Impact)
- **Dimension**: Volumetrics / Memory-Lifecycle
- **Location**: `irradiance_extent` and `LocalGrid::build` (`crates/renderer/src/vulkan/volumetrics/local_grid.rs`); `fit_froxel_divisor_to_device` and `VolumetricsPipeline::new` (`volumetrics.rs`, `volumetrics/init.rs`); consumers `create_volumetrics` (`context/init_stages.rs`), `recreate_bloom_and_volumetrics` (`context/resize.rs`), `local_fire_views_required` (`context/init.rs`)
- **Status**: Regression of #4781 (closed; its fix bounds the six froxel volumes but the stage-1b irradiance field was added afterwards)
- **Description**: #4781 raised the XY divisor until `froxel_extent(..)` fits `maxImageDimension3D`. `irradiance_extent` is `width: froxel.width * 2` (radiance and mean light direction side by side, `include/local_irradiance.glsl`), height and depth unchanged. That image is created per frame in flight in `LocalGrid::build` with no limit check, and `fit_froxel_divisor_to_device` never sees it. So a froxel width in `(limit/2, limit]` fits the fitted divisor and yields an irradiance image over the limit (VUID-VkImageCreateInfo-extent-02254).
- **Evidence**: `fit_froxel_divisor_to_device` computes `needed = max(w,h).div_ceil(max_image_dimension_3d)`; `grep -rn "max_image_dimension" crates/renderer/src` finds only the three `VolumetricsPipeline::new` call chains and no check in `local_grid.rs`. The test `froxel_divisor_is_raised_to_fit_the_device_3d_limit` asserts only `froxel_extent(..).width <= MESA_MAX_3D` and even states "default divisor × 2048 must cover a 16384-wide render": true for the froxel, false for the irradiance field (4096 wide). On a 2048-limit device (Mesa ANV / RADV / lavapipe, the limit #4781 was written against) the image is over the limit whenever `ceil(render_w / divisor) > 1024`: the default divisor 8 above an 8192-wide render, divisor 4 above 4096, divisor 2 above 2048 (a 2560-wide render). Engine init treats a missing volumetrics pipeline as a hard error (`init.rs`: "Volumetric pipeline failed to initialize — composite requires the integrated 3D froxel volume"), and resize returns `Err` (fatal in `app_events.rs` / `app_frame.rs`).
- **Impact**: with `--froxel-xy-divisor 2..4` at 1440p+ on a 2048-limit driver, the engine that booted before 2026-10-31 refuses to start. NVIDIA (limit 16384, the dev card) and the default configuration at ≤ 8192-wide render are unaffected, hence the narrow reach. Not run on such a device: the arithmetic is from the code (PLAUSIBLE on the failure mode, CONFIRMED on the missing bound).
- **Related**: #4781, #6533 (local grid), REN-D8-01 (the 2× width is the same fact the ledger once missed).
- **Suggested Fix**: bound the irradiance width too: in `fit_froxel_divisor_to_device` compute `needed` from `(2 * max_xy)` for the width (or take `limit / 2` for the X axis), and extend the test to assert `irradiance_extent(froxel_extent(..)).width <= MESA_MAX_3D` for every divisor at every render extent the existing loop covers. Alternatively stack the two halves along Z.

### REN-D8-07: the new `heightFogOpticalDepth` overflows (full-opacity fog, or NaN) for a descending ray starting more than ~80 scale heights above the reference height
- **Severity**: MEDIUM (visual only; reachable by the free camera / `--camera-pos` benches, not by gameplay)
- **Dimension**: Volumetrics
- **Location**: `heightFogOpticalDepth` and `expRunOpticalDepth` (`crates/renderer/shaders/composite.frag`), introduced by `3500d2737` (#6728)
- **Status**: NEW (regression introduced by #6728; the previous form clamped its log-sigma to ±80 and stayed finite)
- **Description**: for `aboveStart >= 0` and a descending ray, the shader forms `sigmaStart = sigma0 * exp(-min(aboveStart/H, 80))` and then `expRunOpticalDepth(sigmaStart, slope<0, crossing) = sigmaStart * (1 - exp(-slope*crossing)) / slope`. `-slope*crossing = aboveStart/H` (call it r), so the numerator evaluates `exp(r)`. Above r = 80 the `min(.., 80)` clamp makes `sigmaStart` too large by `e^(r-80)`; at r ≥ 88.7 `exp(r)` overflows fp32 to `inf`, so the segment's optical depth is `inf`, and `exp(-min(tau, 80))` reads that as total opacity. If `sigmaStart` flushes to zero (`sigma0` small times `e^-80` is subnormal), `0 * inf` is NaN and the NaN goes into the HDR target.
- **Evidence**: an fp32 emulation of the function against a 200 000-step numeric integral of the clamped profile (sigma0 = 0.002 / m, H = 30 m, ray slope -0.5): r = 66.7 and r = 80.0 match to 1e-7; r = 86.7 gives 99.5 against a true 5.32; r = 90 gives `inf` against 5.52; r = 166.7 gives `inf` against 10.1. With the default 30 m scale height r = 80 is 2.4 km above the reference (the ground near the camera, `camera_pos.w`); with an authored FO4 / FO76 143 m scale it is ~11 km. The true integral is bounded by `sigma0 * (1 - e^-r) / |slope|`.
- **Impact**: a free camera more than ~2.4 km above the ground reference (default scale height), looking down at geometry past the grid, gets a solid fog colour (or a NaN pixel that TAA / bloom then spread) instead of the thin fog it should. Gameplay cameras never get there. The old code's own comment called this function "a plausible source of exterior white-outs", which is why the horizontal branch is explicit.
- **Suggested Fix**: compute the descending-to-reference segment from its end values instead of an overflowing exponent: `tau = (sigma0 - sigmaStart) / abs(slope)` for the segment that ends at the reference (its end density is exactly `sigma0`), and `(sigmaEnd - sigmaStart) / abs(slope)` with `sigmaEnd = sigma0 * exp(-(aboveStart + dirY*distance)/H)` when it does not reach the reference. That is finite for every `r` and does not need the `80` clamp. A fp32 oracle for it is REN-D8-08. Needs RenderDoc to confirm on a GPU only in the sense of the NaN case; the `inf` case follows from IEEE arithmetic.

### REN-D8-08: the host `height_fog_optical_depth` no longer mirrors the shader, and the new piecewise tail has no numeric test
- **Severity**: LOW (test-gap / stale mirror)
- **Dimension**: Volumetrics
- **Location**: `pub fn height_fog_optical_depth` (`crates/renderer/src/vulkan/volumetrics.rs`) and its two tests `horizontal_height_fog_ray_is_finite_and_matches_constant_density`, `steep_downward_height_fog_ray_stays_finite`; `the_analytic_tail_and_the_grid_share_one_height_profile` (`vulkan/sky_dome.rs`)
- **Status**: NEW
- **Description**: after #6728 the shader integrates `sigma0 * exp(-max(y - y0, 0) / H)` piecewise (constant below the reference, exponential above). The host function still integrates the unclamped `sigma0 * exp(-(y - y0)/H)` and is used only by those two tests. The only test of the new shader code, `the_analytic_tail_and_the_grid_share_one_height_profile`, is a substring scan (`crossing`, `expRunOpticalDepth(`, `exp(-max(aboveStart, 0.0)`), so REN-D8-07 passes every test. The "steep downward ray stays finite" test now certifies a function nothing ships.
- **Evidence**: `grep -rn height_fog_optical_depth crates byroredux --include='*.rs'` finds the definition and the two tests only; the doc comment still reads `sigma_t(y) = sigma0 * exp(-(y - base_height) / scale_height)`.
- **Impact**: a regression in the tail's math (including REN-D8-07's class) is invisible to `cargo test`; the stale mirror reads as the contract.
- **Suggested Fix**: port the piecewise function to the host, delete the unclamped one, and test it against a numeric integral (horizontal, up, down, crossing both ways, start 100+ scale heights up) — the same shape as `sky_dome.rs`'s `chapman_column_matches_numerical_integration` for the air column.

### REN-D4-02: the #4773 positional-`f32` pin covers `record_post_passes` only; `build_and_upload_instances`, where #6728 threaded `fog_coverage`, is unpinned
- **Severity**: LOW (test-gap)
- **Dimension**: Sync/Recording (call-site contracts)
- **Location**: `draw.rs` (the `self.build_and_upload_instances(` call, `fog_scale_height_meters, fog_coverage,` on one line) and the matching signature in `context/build_and_upload_instances.rs`; test `record_post_passes_named_arguments_match_parameter_positions` (`context/post_passes.rs`)
- **Status**: NEW
- **Description**: #4773 was a swap of two adjacent `f32` arguments (`fog_coverage` / `fog_scale_height_meters`) that compiled and collapsed the froxel medium to a 1 m layer. The pin it left splits `record_post_passes`'s call one argument per line and checks names against positions. `3500d2737` added `fog_coverage` to the sibling call, which has five adjacent `f32` fog lanes (`fog_single_scatter_albedo`, `fog_scale_height_meters`, `fog_coverage`, `fog_clip`, `fog_power`, `fog_height_reference`) and puts two arguments on one line, which the pin's parser could not read anyway. Today the order matches (checked by hand against the signature).
- **Impact**: a future swap of the new lane with its neighbour feeds `fog_mean_density_scale` a scale height (or the scale height a coverage) and changes the whole beyond-grid fog silently.
- **Suggested Fix**: put one argument per line at both sites and extend the existing test to also walk the `build_and_upload_instances` call (reuse its helper); or pass a `FogParams` struct built by name.

### REN-D8-09: `fog_mean_density_scale` redoes a pure 8 192-iteration f64 loop every frame
- **Severity**: LOW (hot-path CPU; not measured)
- **Dimension**: Volumetrics
- **Location**: `fog_mean_density_scale` (`volumetrics/noise.rs`), called from `build_composite_params` (`context/frame_params.rs`) every frame, including frames where the beyond-grid tail is off
- **Status**: NEW
- **Description**: the histograms are cached in a `OnceLock` but the 256 x 32 mean is recomputed per call, with `smoothstep` and clamps in f64. Its only input is the coverage, which changes on weather transitions.
- **Impact**: on the order of tens of microseconds per frame; the project treats CPU time on the render thread as a defect, so it is worth one cached `(coverage_bits, value)` pair. Measure before claiming a number.
- **Suggested Fix**: memoise on the coverage's bit pattern (in the context, not a global), or skip the call when `height_fog_params.w` (tail enabled) is 0.

### REN-D8-10: misattached doc blocks in `fog_clusters.rs` and a missing tail / air-fade description in the docs
- **Severity**: LOW (doc-rot)
- **Dimension**: Volumetrics
- **Location**: `volumetrics/fog_clusters.rs` (the "whether `transportCombustion`'s RK2 advection block … should run" paragraph sits on `TRANSPORT_EXPIRED_DT`, leaving `combustion_transport_active` undocumented; the `build_fog_volume_clusters` "#2242 … every call resets every entry's `count`" paragraph and the "Conservative world-space box" paragraph sit on `fog_volume_is_shape`); `docs/engine/shader-pipeline.md` (no description of the beyond-grid tail, its mean-density scaling, or the exterior `sky_air_terms` fade added by #6728)
- **Status**: NEW
- **Description / Evidence**: read at the symbols; the shader-pipeline gap is `grep -n -i "beyond-grid\|analytic tail\|sky_air" docs/engine/shader-pipeline.md` returning nothing (the prose lives in `docs/prd/volumetric-lighting.md` and `docs/engine/skyal.md` §2.4 for the cloud half only).
- **Impact**: none at runtime; a reader of `build_fog_volume_clusters` loses its reset-every-call contract, which is the stale-count guard.
- **Suggested Fix**: move the three paragraphs onto the items they describe; add one composite-step bullet to shader-pipeline.md.

## Dimension notes

### Dim 8 — participating media (primary)
- **#6728 (`3500d2737`) read end to end.** `fog_mean_density_scale` mirrors `sampledDensity` line for line (threshold, softness, erosion gain, `mix(0.2, 1.35, ..)`, the `[0.01, 1]` clamp); the host passes the same `fog_coverage` to both the inject (`temporal_params[3]`) and the composite tail, through the same `draw_frame` variable. One approximation: the doc says trilinear filtering "leaves the mean unchanged", true for the texel mean but not for the thresholded density the shader averages, so the tail's scale is calibrated, not exact; the commit's measured residual (1.8 levels, A6 tolerance 2.5) absorbs it. Not a finding. The piecewise integral is right in all four ray / start cases (checked by emulation) except the overflow in REN-D8-07. `sky_air_terms` for below-horizon rays clamps `mu` at 0 in `sky_air_column`, so a downward ray reads as a horizontal one; the fade is gated on `has_surface && depth_params.x > 0.5` and leaves the volumetric debug views untouched (they return first).
- **Local grid (stage 1c).** `LocalBoxSet::update` (source `Vec` taken and handed back, no early return between), eviction / linger / hysteresis, `GpuLocalGridParams::new` (sun, render origin, mask), `begin_frame` stats drain-and-zero, `record_sim` (brick fill → TRANSFER→COMPUTE, history barriers, hand-off barrier), `record_publish_bricks`, `composite_barriers`, destroy order: no issue. The composite compose `volIn + volIn.a*fire + fire.a*((vol - volIn) + vol.a*combined)` is the exact two-segment form. The skip arm (`ran == false` → `record_skipped_frame` → `invalidate_history`, `clear_local_grid_params`) is now pinned.
- **Dispatch and barriers.** Stage order A to F in `VolumetricsPipeline::dispatch`: every `?` precedes the first recorded command, the five history fields get read→write / write→read pairs, `pre_int_write` includes TRANSFER in its source scope (#3647), the publish barrier reaches FRAGMENT. `TransportFieldState::submitted` / `mark_frame_completed` only after submit.
- **Fog-cluster host prep.** Dense packing (#4792), the shared host cluster array against per-slot dirty ranges (empty frames keep the previous range, so stale counts are always rewritten), `u16` volume indices vs `MAX_GPU_FOG_VOLUMES` 512, portal sweep: sound.
- **Integrate pass.** Non-finite scrub writes `vec4(0)` (T = 0, black behind it) rather than T = 1; unreachable because the injection store is already scrubbed (#4782). Not a finding.
- **Resize / teardown.** `recreate_bloom_and_volumetrics` destroys then rebuilds the pass and the composite descriptors (12..=17) are rewritten from `local_fire_views`; a failed rebuild returns `Err`, which the app treats as fatal, so no dangling-descriptor window. `VolumetricsPipeline::destroy` destroys the grid first, every handle null-guarded.

### Dim 3 — struct layout (volumetrics)
`VolumetricsParams` / `CompositeParams` field-order test, `GpuFogVolume` (96 B) offsets and GLSL order, `GpuLocalGridParams` (144 B) in both consumers, the boundary-instance 160 B stride: green. `CompositeParams` has no new lane in this delta (`fog_coverage` is host-only). `wind_gust` lanes z / w now documented live.

### Dim 4 — recording / sync (volumetrics bracket)
Volumetrics → ssao → composite order, skip-clear latches promoted after submit, the infallible post-pass shape, and the sunless / sunlit composite pair selected from `local_fire_sunlit()` after `record_volumetrics_pass` (so a skipped frame, whose `clear_params` sets `sunlit = false`, takes the sunless pipeline with the march already off): unchanged and consistent. `8f95465c4` (#6811, linear capture commit after submit) read: correct, discard / promote hooks paired.

### Dim 5 — memory / teardown
Allocator-backed objects of the pass are destroyed before the allocator `Arc::try_unwrap` (`teardown.rs` order unchanged); the local grid's images, buffers, pool, pipelines, sampler all freed once. `memory_budget_doc_states_the_irradiance_field_and_brick_costs` (new, #6749) green. The 2× irradiance width against the device limit is REN-D8-06.

### Dim 12 — timers
`NESTED_GPU_BRACKETS` now lists `volumetrics_local_sim` (#6754); the sim bracket is recorded inside `Volumetrics` and before `VolumetricsInject`, as the corrected docs say.

### Dims 1, 2, 6, 7, 9, 10, 11 — unchanged since baseline (skimmed)
First steps run against `9409e7b20..HEAD`: Dim 1 one comment commit; Dim 2 `09940503b` (reversed-Z shipped, pinned by `the_shipped_depth_mapping_is_reversed`), `2ed1d91ce` (FO3 / FNV env-mask reflect, NIFAL-owned), comment fixes; Dim 6 byroredux material changes outside this suite; Dim 7 `6926fddab` (TAA silhouettes), not volumetrics; Dims 9 and 11 no commits. Guards for all of them are inside the 1627-test green run.

## Needs-RenderDoc (observations only)

- REN-D8-07's NaN variant (denormal `sigmaStart` times `inf`) depends on the GPU's fp32 denormal mode; the `inf` variant follows from IEEE arithmetic.
- Everything in the baseline's list stays open: sim binding inject set 0 while writing `state[frame]` through set 1; the phase-bake pass's missing trailing barrier (published only by the init submit's fence wait); the sunless composite pipeline's look; local fire march caps under a fast camera (#6531 / #6533).
- The exterior air fade at the real distances (ridge sweep of VLM-AP-001 is unmeasured per the commit message): a look check, no code claim.

## Stale skill premises (for the next `/audit-sync`)

- **Suite brief**: "zero commits on the volumetrics paths since 2026-10-31" is false; see Executive Summary.
- Dim 8 **Guard** line: add `analytic_tail_uses_the_sky_air_terms`, `the_analytic_tail_and_the_grid_share_one_height_profile`, `the_mean_fog_density_scale_follows_coverage`, `the_local_grid_follows_the_skip_and_completion_wiring`, `the_shadow_filter_dispatch_and_the_composite_flag_share_one_predicate`, `the_sunless_composite_pipeline_is_the_same_shader_with_the_constant_false`, `memory_budget_doc_states_the_irradiance_field_and_brick_costs`.
- Dim 8 **Checklist**: the "analytic tail" sentence should say the tail's coefficient carries `fog_mean_density_scale(coverage)` and that its profile is the clamped piecewise form; the local-grid bullet should note the irradiance image is `2 x` froxel width and is not covered by `fit_froxel_divisor_to_device` (REN-D8-06).
- Dim 4 **Guard**: note that `record_post_passes_named_arguments_match_parameter_positions` does not cover the `build_and_upload_instances` call (REN-D4-02).
- Dim 5 / 12 premises from the baseline (REN-D5-01 test, `NESTED_GPU_BRACKETS`) are now true as written.

## Existing / carried (not re-filed)

#6531 / #6533 (local combustion grid, stage 1c not accepted), #6727 / #6729 (volumetric-lighting PRD work), #6776 (authored fog power thickens clear-weather haze about 4.6x; haze-floor re-measure, byroredux-side), #6631 (M39 default-on), #6371 (hand-written pass teardown migration), #6486 (half-fixed, composite per-pixel work).

## Checked, no finding

- `fit_froxel_divisor_to_device` for the six froxel volumes (the #4781 fix itself holds).
- `sunHitIsSolid` / `sunRayBlockedAlphaAware`: material-word reads, bindless index (unmasked, same as `ray_hit.glsl`), alpha chain, 8-layer budget, step past a hole; the boundary instance prefix mirrors the 160 B `GpuInstance`.
- Skipped-frame behaviour, resize rebuild and teardown of the pass.
- Fog-cluster dirty ranges, portal-sweep envelope and slab test, occupancy seeding before the capacity check.

## Prioritized Fix Order

1. REN-D8-06 (HIGH; one divisor bound plus one test assertion).
2. REN-D8-07 and REN-D8-08 together (rewrite the descending segment in end-value form, port the piecewise function to the host with a numeric oracle).
3. REN-D4-02 (one test extension).
4. REN-D8-09, REN-D8-10.

Publish with `/audit-publish docs/audits/AUDIT_RENDERER_2026-11-09.md`.
