**HEAD**: `e16a624e9` · **Baseline**: `AUDIT_RENDERER_2026-10-14.md` (HEAD `adbd468e2`) · **Audited**: Dim 8 (full, volumetrics), Dim 4 (volumetrics sync/barriers), Dim 5 (volumetrics lifecycle/teardown), Dim 3 (volumetrics struct lockstep), Dim 2 (inject-pass ray queries), scope=volumetrics-deep · **AREA-SCOPED (volumetrics only), NOT a full-codebase renderer audit — do not treat as a full baseline; the 2026-10-14 report remains the last full baseline** · **Unchanged since baseline (skimmed)**: Dims 1, 6, 7, 9, 10, 11, 12 — out of area

# Renderer Audit — 2026-10-17 (volumetrics-deep, area-scoped)

Scope: `vulkan/volumetrics.rs`, `volumetrics/{init,fog_clusters,combustion,noise}.rs`, `volumetrics_inject.comp`, `volumetrics_integrate.comp`, `composite.frag` consumer, `context/{post_passes,draw,resize,init,teardown,assemble_camera_and_lights}.rs` call sites, the `byroredux/src/render/mod.rs` fog boundary, and the volumetrics rows of `renderer.md`, `memory-budget.md`, `shader-pipeline.md`.

## Executive Summary

| Severity | NEW | Findings |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 4 | REN-VOL-L-01 … L-04 |

The volumetrics area is in good shape. The newest commits (`67476a5d7` #5772, `1a2c48008` #5773, `5d2679372` #5771, the #5593/#5809 `clamp_sampler` and layout-builder refactor) are behaviour-preserving and correct. No sync, lifetime, struct-lockstep or ray-query defect was found. The four LOW items are one init-ownership sibling gap, one failure-path policy gap, doc rot, and a missing call-site pin.

**Delta** `adbd468e2..e16a624e9` over the area: `volumetrics.rs` (doc wording, `pub(super)` visibility), `volumetrics/init.rs` (layout-binding builders, `clamp_sampler`, #5772 push-before-init), `composite.frag` (`shadowCorrectionTex`, not volumetrics), `bindings.glsl` / `cluster_cull.comp` (fog wording), `shader_constants.glsl` (ReSTIR/decal constants, unrelated). `draw.rs`, `post_passes.rs`, `resize.rs`, `fog_clusters.rs`, `noise.rs`, `combustion.rs`, both volumetrics shaders and `froxel_slices.glsl` are unchanged in the window; they were still read in full for the deep audit.

**Method.** Static review plus guard runs. The engine was not launched.
- `cargo test -p byroredux-renderer --lib volumetrics`: 79 passed.
- Filtered run (`composite_and_volumetrics`, `gpu_boundary_instance`, `gpu_instance_glsl`, `record_volumetrics_pass`, `volumetric_sun`, `reflect`, `fog`, `caustic`, `water_fragment`): 122 passed. This includes `volumetrics_params_member_offsets_match_host_struct`, `volumetrics_ubo_sizes_match_host_structs_in_every_shader`, `refactored_pipeline_layouts_match_their_committed_shaders`, `gpu_boundary_instance_stride_matches_gpu_instance` and `gpu_instance_glsl_copies_stay_in_lockstep`.
- `-p byroredux --bin byroredux fog`: 77 passed.
- `scripts/check-shader-artifacts.sh`: clean (37 shaders plus the early-test variant byte-match glslang).
- Not run: anything needing a GPU or a RenderDoc / `BYRO_VALIDATION=1` capture; game-data tests.

## Findings

### REN-VOL-L-01: #5772 left the identical gap one loop below — `integration_param_buffers` local is outside `partial` when `write_mapped` fails
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle (Volumetrics)
- **Location**: `volumetrics/init.rs`, `VolumetricsPipeline::new_inner` §7 "Per-FIF integration parameters" (the `for _ in 0..MAX_FRAMES_IN_FLIGHT` loop)
- **Status**: NEW (sibling of closed #5772 / baseline REN-L-06, which fixed only the moment and occupancy buffers)
- **Description**: `let mut buffer = try_or_cleanup!(… create_host_visible …)`, then `try_or_cleanup!(… buffer.write_mapped(…))`, and only afterwards `partial.integration_param_buffers.push(buffer)`. If `write_mapped` fails, `partial.destroy` does not know the buffer. Same effect class as baseline REN-L-06: not a leak, since `GpuBuffer::Drop` (#656) frees the buffer, but a debug build hits `debug_assert!(false, "GpuBuffer leaked into Drop")`, which masks the `Err` with a panic. Effectively unreachable (a mapped host buffer of exactly `size_of::<IntegrationParams>()`).
- **Evidence**: moment/occupancy sites now `push` first and fill via `.last_mut()`; this loop still fills first.
- **Suggested Fix**: push, then `write_mapped` through `.last_mut()` (the #5772 shape). #5772 shipped without a pin (`init.rs` has no tests); add a source-shape test that no `GpuBuffer` local in `new_inner` is filled before being pushed.

### REN-VOL-L-02: `initialize_layouts` destroys the noise staging buffers unconditionally, ignoring the #4891 `MaybeInFlight` policy
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle (Volumetrics)
- **Location**: `volumetrics/init.rs`, `initialize_layouts` tail (`for buffer in staging.iter_mut() { buffer.destroy(…) }` before `upload_result?`); same pattern in `cloud_noise.rs` (SKYAL, shared `noise::create_density_noise_staging`). The callers then destroy the whole pipeline on that error (`context/init.rs` volumetrics init arm, `context/resize.rs` `recreate_bloom_and_volumetrics`).
- **Status**: NEW
- **Description**: the comment says the one-time submit "waits for the queue before returning, so neither staging buffer can still be referenced". That is false for `OneTimeCommandError::MaybeInFlight`: a failed `queue_submit`, or a `vkWaitForFences` failure other than DEVICE_LOST (#5270), leaves the copies possibly pending. The repo's rule (#4891; `buffer.rs`, `blas_static.rs`) is "destroy on `NotSubmitted`, leak on `may_be_in_flight`". The helper itself leaks `cmd` and the fence on that class, so the staging free here is the one remaining non-conforming disposal. Reachable only on device loss / OOM during init or resize.
- **Suggested Fix**: `if OneTimeCommandError::may_be_in_flight(&e) { mem::forget(staging) }` else destroy; decide the same for the froxel images the caller destroys. Apply to both consumers of the shared helper. Failure-path only and not testable on a device-free run, so frame it as policy conformance.

### REN-VOL-L-03: doc rot — the per-volume field descriptions of the combustion textures disagree with the shader
- **Severity**: LOW (doc rot)
- **Dimension**: Volumetrics
- **Location**: `docs/engine/memory-budget.md` Volumetrics table rows `combustion_state_volumes` ("σ_a") and `combustion_dynamics_volumes` ("velocity xyz + σ_s"); `volumetrics.rs` doc on `COMBUSTION_FIELD_FORMAT` ("absorption sigma_a", "scattering sigma_s in w"); `memory-budget.md` host-buffer table (omits the two 16 B `integration_param_buffers`, 32 B total — negligible)
- **Status**: NEW
- **Evidence**: `volumetrics_inject.comp` header, `combustionMedium` and `sanitizedChemistry` store total extinction σ_t in `chemistry.z`; dynamics `.w` is specific overpressure (m²/s²); spectral σ_s lives in the separate `combustionOptical` volume. `shader-pipeline.md` rows 14–23 and the `VolumetricsPipeline` field docs are already correct.
- **Suggested Fix**: correct the two table rows and the const doc to match `shader-pipeline.md`.

### REN-VOL-L-04: the fog-colour render-boundary decode has no call-site pin
- **Severity**: LOW (test gap)
- **Dimension**: Volumetrics / fog colour
- **Location**: `byroredux/src/render/mod.rs` (`let fog_color = crate::fog::fog_colour_radiance(…)`); tests in `render/fog_curve_propagation_tests.rs`
- **Status**: NEW
- **Description**: #5773 documented the convention "decoded exactly once at the boundary, `GpuCamera.fog.xyz` and the volumetric `fog_tint` carry linear light", but the only test is `fog_colour_radiance_encodes_back_to_the_authored_display_value`, which pins the function. The existing `run_view` fixtures feed `fog_color: [0.5, 0.4, 0.3]` and never assert `view.fog_color`. Removing the call (or adding a second decode downstream) keeps every test green and would silently shift every fog-lit interior by ~2.7x.
- **Verified clean today**: single decode; composite `fog_color`, `pack_fog_tint` input and `normalize_fog_tint` all consume the decoded value; no shader decodes again; `CellLightingRes` keeps the authored value.
- **Suggested Fix**: add one assertion in the `run_view` tests: `view.fog_color == fog_colour_radiance([0.5, 0.4, 0.3])`.

## Existing / forwarded items (not re-reported)

- **Concurrency audit C-3** (`AUDIT_CONCURRENCY_2026-10-17.md`, LOW): `volumetrics_cleared_on_skip[frame]` / `caustic_cleared_on_skip[frame]` are committed at record time and not rolled back when `end_command_buffer` / `reset_fences` / `queue_submit` fail (#3991 class). I confirmed the latch is set in `record_volumetrics_pass` through `skip_clear_decision` before submit. Skip-latch rollback belongs to that finding.
- Baseline REN-L-06 → fixed by `67476a5d7` (#5772) for the moment and occupancy buffers; see REN-VOL-L-01 for the sibling.
- Baseline REN-L-07 → fixed by `1a2c48008` (#5773): `bindings.glsl`, `cluster_cull.comp`, the `pack_fog_tint` / `normalize_fog_tint` docs and `shader-pipeline.md` now state linear-light fog radiance.
- Baseline REN-L-05 → fixed by `5d2679372` (#5771): `volumetrics_inject_layout_bindings` / `volumetrics_integrate_layout_bindings` are shared between pipeline construction and the reflection test.

## Verified clean (focus areas)

- **Image layouts and barriers.** All six per-FIF volumes plus the two noise images are created UNDEFINED. `initialize_layouts` moves froxels to GENERAL (dst `SHADER_READ|WRITE|TRANSFER_WRITE`, stages COMPUTE|TRANSFER), clears them, uploads the noise, then publishes with a TRANSFER→COMPUTE memory barrier and a TRANSFER_DST→SHADER_READ image barrier. This runs in a fence-waited one-time submit, and runs again on every resize because the whole pipeline is rebuilt. No layout transition occurs afterwards; every sampled use declares GENERAL.
- **Per-frame chain.**
  - Host → compute memory barrier (HOST_WRITE → UNIFORM/SHADER read+write).
  - Stage B: this slot READ→WRITE plus previous slot WRITE→READ on all five history fields, COMPUTE→COMPUTE.
  - Stage D: lighting WRITE→READ plus the integrated slot `SHADER_READ|TRANSFER_WRITE`→`SHADER_WRITE`. The source stages include FRAGMENT and TRANSFER, so a prior composite read or a `record_neutral_frame` clear is covered (#3647).
  - Stage F: integrated WRITE→READ, dst FRAGMENT.
  - Compute→HOST barrier for the moment readback.
  - Cluster/light-buffer COMPUTE W→R barrier before `write_lights_and_clusters`; the same barrier carries the occupancy-mask RAW (#5215).
  - TLAS: the frame's single unconditional `AS_BUILD` → `FRAGMENT|COMPUTE` `AS_READ` barrier, on both arms, with `ray_query_tlas` withholding a stale TLAS (#4779).
- **Cross-frame hazards are moot.** `sync_and_acquire_frame` waits all `in_flight` fences, so a previous frame is fully retired before any host rewrite of the occupancy mask, moment buffer, param UBOs or fog buffers (rider 15 in `sync.rs`, pinned by `the_all_slots_wait_argument_is_pinned`). Barrier correctness does not depend on that wait.
- **History ping-pong, first-frame, skip-frame.** `history_valid` is promoted only by `mark_frame_completed` after a successful `queue_submit`. `signal_history_reset`, `record_skipped_frame` (runs on every skip, #4780) and `dispatch` failure all clear it. A failed submit leaves `current_frame` un-advanced, so the retry reuses the same slot against the correct previous slot. `TransportFieldState` (`empty` / `pending_clear`) walks correctly through active → expired → known-empty, and any new emitter resets `empty`. `prev_camera_pos.w` carries the host validity flag, and the shader gates every history and transport read on it.
- **Descriptors.** Static bindings are written at construction. Bindings 2, 3–5 and 19–21 are rewritten per frame on an idle slot, with `tlas_written` / `lights_written` / `boundary_geometry_written` debug-assert latches. On resize the pipeline is destroyed and rebuilt under `device_wait_idle`, then composite's binding 6 is rebound from `integrated_views()`. A failed volumetrics re-creation is fatal, matching the init policy (composite requires the 3D view). Integration pool/sets are sized from the layout builder.
- **#5772 ordering and teardown.** `partial.destroy` drains images, buffers, then pipelines, layouts, pool and samplers, and is idempotent. `Drop` destroys volumetrics after composite and caustic and before bloom, all under `device_wait_idle`.
- **Struct lockstep.** `VolumetricsParams` (15 members, offsets) and `IntegrationParams` are reflected against the SPIR-V. The `GpuFogVolume` field order, std430 layout and `GpuFogClusterEntry` (4×u32) are pinned. `GpuBoundaryInstance` stride is pinned against `GpuInstance`. `CombustionLightMoment` is eight u32 words. The light-buffer prefix (count, 3 pad, `previousLightToCurrent[MAX_LIGHTS+1]`) matches `bindings.glsl`. The host-buffer ledger test and `froxel_grid_cost_matches_the_memory_budget_doc` pass.
- **Inject-pass ray queries.**
  - The sun, local-light and glass queries use OPAQUE plus terminate-on-first-hit with layer masks, and `tMax <= tMin` returns "no hit".
  - Only the architectural-glass candidate loop uses `NoOpaque` (#4945, #4858).
  - The rim probe takes the nearest hit without terminate.
  - Absolute TLAS space is restored via `render_origin`.
  - Boundary-normal reads are bounds-checked against index and vertex capacity and skip skinned instances.
  - The directional trace is gated on `sunTermLive`.
  - The fog-cluster, occupancy and light-moment indices are clamped or bounds-checked.
  - The history guard and store clamp (#4782) are in place.
- **Fog colour.** Authored sRGB fog is decoded once at `render/mod.rs` (`fog_colour_radiance`); `pack_fog_tint` splits it into chromaticity plus peak; nothing re-decodes (see REN-VOL-L-04 for the missing pin).
- **Closed-issue fixes spot-verified in place**: #4774 (texel-centre write), #4775 (expiry sentinel), #4776 (R3 integer rotation, `froxel_ray_dir`), #4779, #4780, #4781 (`fit_froxel_divisor_to_device`), #4782, #4784 (occupancy plus rider), #4858, #4968, #4971, #5208, #5214, #5215, #5772, #5773.

## Considered and dropped

- **`normalize(params.sun_dir.xyz)` is unguarded.** A zero vector would NaN the whole froxel; the store clamp would then zero `extinction_coef` too. No production path produces a zero sun direction (defaults are `[0,-1,0]`; the zero seed exists only in a weather test fixture). Not filed.
- **`combustionPathBlocked` uses terminate-on-first-hit and then reads the committed triangle's normal**, so the normal can come from a farther surface along a short step. The normal is flipped against the step direction and the distance is a fraction of a froxel; accuracy only, no hazard.
- **`validate_set_layout(…).expect(…)` panics mid-construction after partial allocation.** Dev-time drift assert, now also covered by the offline reflection test. The same unconditional init-failure leak pattern exists across all of `VulkanContext::new`, so it is not volumetrics-specific.
- **BLAS-budget reservation uses the requested, not the device-fitted, froxel divisor.** Over-reserves only on exotic devices; conservative.
- **Stale `volumetrics_state` telemetry on skipped frames.** Debug display only.

## Needs-RenderDoc / validation

1. `BYRO_VALIDATION=1` sync-validation on a fire/smoke cell for the Stage B/D/F chain and the occupancy RAW (no change proposed; static analysis finds none).
2. Failed-submit behaviour of the skip latch (concurrency audit C-3) is observable only with fault injection.

## Stale skill premises (for the next `/audit-renderer` sync)

- **Dim 8**: the inject-layout builders now live in `volumetrics/init.rs` (`volumetrics_inject_layout_bindings`, `volumetrics_integrate_layout_bindings`); the Guard list should add `volumetrics_params_member_offsets_match_host_struct`, `volumetrics_ubo_sizes_match_host_structs_in_every_shader`, `refactored_pipeline_layouts_match_their_committed_shaders`, `gpu_boundary_instance_stride_matches_gpu_instance`, `froxel_grid_cost_matches_the_memory_budget_doc` and `volumetrics_host_buffer_ledger_matches_the_code`. Add `context/{resize,init,teardown}.rs` volumetrics arms and `byroredux/src/fog.rs` / `render/mod.rs` (`fog_colour_radiance`) to Paths. The checklist should state the fog-decode convention and the all-slots fence dependence of the host-visible occupancy/moment buffers.
- **Dim 5**: add the `new_inner` push-before-initialize rule (#5772) and the staging `MaybeInFlight` disposal rule for one-time uploads.
