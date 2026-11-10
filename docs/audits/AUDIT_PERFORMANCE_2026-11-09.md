**HEAD**: `dbc815c26` · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-10-31.md` (HEAD `670f81917`; 544 commits since, 98 of them touch the renderer / render seams below) · **Audited**: Dim 5 (volumetrics inject / integrate / local sim / composite fire march, primary), Dim 4 (upload sizing on the volumetrics + composite seam), Dim 3 (reservation billing of the volumetrics images), Dim 1 (per-frame host work on the volumetrics seam: fog-cluster prep, `fog_mean_density_scale`, scratch), Dim 8 (volumetrics timer rows, scratch telemetry) · **Unchanged since baseline (skimmed)**: Dims 2, 6, 7 (one line each, below)

# Performance Audit - 2026-11-09 (volumetrics-deep suite, delta)

**Mode**: static analysis, guard lanes, one standalone `rustc -O` micro-benchmark (scratch, no cargo target). **The engine was not launched** (three audits ran concurrently; a GPU timing taken next to them would be noise, and the sibling renderer audit made the same call). No GPU or frame time in this report was measured by this audit. Every GPU figure is quoted from a checked-in doc and labelled *recorded*, or is labelled *derived* / *unmeasured*. Anything that needs a launch is listed under "Measurement owed".

**Scope correction.** The suite brief says zero commits on the volumetrics paths since 2026-10-31. Wrong. `git log 670f81917..HEAD` over `vulkan/volumetrics*`, the three volumetrics shaders, `composite.{frag,rs}`, `post_passes.rs`, `gpu_timers.rs`, `scene_buffer/`, `render/` returns 98 commits; the performance-relevant ones are the local grid's stages 1b/1c and follow-ups (`626197ae5`, `d3e3acc7b`, `a1a243bc2`, `1d1ebf836`, `15a4ecf10`, `2f3e16891`, `5a5bb3360`), the alpha-aware froxel sun trace (`684bc9161`), interior shaft dust and aperture cap (`fbde91ab5`, `469967ee8`), the haze floor (`000416486`), the underwater god-ray matrix move (`fa9ada406`) and `3500d2737` (#6728, tail scale + exterior sky-air fade). Dims 4, 5, 8 below are read against that whole range, not against 11-07.

## Executive Summary

| Severity | NEW | Existing / tracked |
|---|---:|---:|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 1 | 0 |
| LOW | 6 | 1 (#6831) |

- **The one MEDIUM is a correctness regression found while reading upload sizing, not a cost.** `CompositeParams::view_proj` (added last by `fa9ada406`, #6486) sits after the 256-entry aperture array, and `CompositePipeline::upload_params` (#4805's prefix upload) copies only up to `aperture_count` apertures, so `view_proj` is never uploaded on a normal frame (PERF-2026-11-09-D4-01). The underwater god-ray contact-occlusion taps then project through a matrix the host never wrote.
- **No guard has eroded.** Renderer `--lib` filtered lane (`volumetrics gpu_timers scratch shrink local_grid composite fog sky_aperture timer`): 228 passed, 0 failed. `scripts/check-shader-artifacts.sh`: 40 shaders + opaque early-test variant match glslang 11:16.4.0. Eleven Dim 3/4/5/8 guard names all resolve exactly once (list under "Guards").
- **Recorded GPU cost of the volumetrics passes is small at 1280x720** (inject 0.2-0.77 ms, integrate 0.02 ms, local sim 0.07-0.15 ms, composite fire march +0.35-1.05 ms on its lab scenes; sources in the hot-path table). The open question is **scaling**: the froxel grid and the composite march follow render pixels, every recorded number is 720p, and nothing at 1440p+ is on record (Measurement owed).
- **Observed vs ROADMAP**: no delta can be stated; no bench was run. R6a-regress-23 is unchanged by this window's static reading.
- **Dedup with sibling audits**: REN-D8-09 is filed as #6831 and is measured here (8.7-10.0 us per call, LOW holds). REN-D8-06 (#6827, HIGH) and REN-D8-07 (#6828, MEDIUM) are correctness items and are not re-filed. CONC-2026-11-09-2.1 (#6835, brick-buffer fill sync) and SAFE-2026-11-09-03 (#6834) are not performance and not re-filed.

## Hot Path Analysis

RT quality tier: not applicable (nothing rendered by this audit). The adaptive tier moves inject cost, so every recorded number below is only comparable to its own source run.

### Per-pass GPU cost (recorded by the cited doc, RTX 4070 Ti, 1280x720, `renderer-stepped`; not measured here)

| Pass | Recorded cost | Source | Scales with | Derived at 2560x1440 (4x pixels), unmeasured |
|---|---|---|---|---|
| `volumetrics_inject` (noon) | 0.305-0.768 ms across five fixtures (Skyrim 0.305, FNV 0.418, Oblivion 0.520, FO4 0.768) | `docs/engine/procedural-volumetric-fog.md` "Exterior fixtures, inject and integrate separately" | froxels (160x90x64 = 921,600 at 720p) x rays per froxel | 1.2-3.1 ms if per-froxel cost holds |
| alpha-aware sun re-walk (VLM-SH-009), 07:00 | +0.11 to +0.34 ms inject (FNV +0.34) | same doc | froxels the binary trace calls blocked | 4x the pixels, same shape |
| `volumetrics_integrate` | 0.020-0.022 ms, "constant" | same doc | froxel columns (one thread per column, 64 serial slices) | unmeasured |
| `volumetrics_local_sim` | 0.081 / 0.074 / 0.120 / 0.145 ms (s1 / s2a / s3a / s5); stage 1b (before the overlap fix) 1.252 ms at 272,145 active cells | `docs/engine/fire-volumetrics-plan.md` stage 1b / 1c tables | active cells, not box size | resolution-independent (fixed 64^3 x live boxes) |
| composite fire march | composite 0.36-0.89 ms on the lab scenes (adds +0.41 to +1.05 ms over the pre-march baseline; s1 box covers ~60 % of the screen) | same plan, stage 1c and later cost sections | render pixels x march steps (cap 160 per box, 64 brick jumps) | up to ~4x on a full-screen fire view; a gameplay view covers less |
| composite exterior sky-air fade (`3500d2737`) | none recorded | `sky_air_terms` x2 + `sky_clear_radiance` per exterior surface pixel; ~6 `erfcx` + exps | render pixels | derived tens of microseconds; not a finding |

### Per-frame / per-event CPU on the volumetrics seam

| Site | Cost shape | Cadence | Measured |
|---|---|---|---|
| `fog_mean_density_scale` (`volumetrics/noise.rs`, via `build_composite_params`) | 256 x 32 f64 loop | every frame, also when the tail is off | **8.7 us (target-cpu=native) / 10.0 us (generic) per call**, median of 9 x 200,000 calls, Ryzen 9 7950X under sibling-audit load (scaling 75 %) |
| `filter_fog_volumes_for_grid` + `build_fog_volume_clusters` | O(volumes <= 512) + O(clusters spanned per volume, <= 16^3 each); entry reset 4,096 x 2 stores; occupancy 16 KiB fill + 16 KiB mapped write | every dispatching frame | derived; typical volume spans 1-8 clusters (16 m cell at the default reach). Worst case bound 512 x 4,096 loop iterations, pathological |
| `collect_fog_volumes` (`byroredux/src/render/fog_volumes.rs`) | whole-world `FogVolume` query + per-volume transform + `sort_unstable_by` | every frame | derived; volumes are tens to hundreds, capped at 512 after the sort |
| `LocalBoxSet::update` | O(volumes) filter + sort of <= reach sources; `sources` Vec reused (#6753) | every dispatching frame | derived, trivial |
| 3 descriptor-write calls (`write_tlas`, `write_lights_and_clusters`, `write_boundary_geometry`) | ~10 `vkUpdateDescriptorSets` writes | every dispatching frame | derived <= low microseconds; the TLAS handle changes per frame, so a steady-state skip is only partial |
| host uploads | params 336 B, local params 144 B, fog volumes `16 + n x 96` B prefix, cluster entries `[lo, hi)` range, indices `[..index_len]`, occupancy 16 KiB | every dispatching frame | derived, all O(live data); `fog_volume_buffers` uses `write_mapped_prefix` correctly |

Volumetrics now dispatches on essentially every daytime exterior frame: `with_exterior_haze_floor` (VLM-SH-010, `000416486`) raises the exterior medium above zero whenever the sun is more than ~6 degrees up, so `requires_dispatch`'s `has_global_medium` limb is always true there, including weathers whose authored fog is nil. (Weathers with authored fog already ran it.)

## Findings

### Eroded guards
None.

### New issues

### PERF-2026-11-09-D4-01: `CompositePipeline::upload_params` copies only `offset_of(sky_apertures) + count x 48` bytes, but `view_proj` (added last by `fa9ada406`) sits after all 256 apertures, so it is never uploaded
- **Severity**: MEDIUM (visual regression in submerged views; found under the "upload sizing" focus, owner is the renderer)
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/composite.rs` `upload_params` (the `prefix_bytes` computation, ~1453-1470) and `CompositeParams` (`view_proj` is the last field, `offset_of == 528 + MAX_SKY_APERTURES * 48 = 12,816`); consumer `underwaterSunVisibility` / `underwaterGodRays` in `composite.frag` (`mat4 viewProj = params.view_proj;`); producer `build_composite_params` (`context/frame_params.rs`, `view_proj: view_proj.to_cols_array_2d()`)
- **Status**: NEW. Introduced by `fa9ada406` (2026-11-04, "Refs #6486"). #4805 (closed) added the prefix upload on the assumption that the aperture array is the struct's tail; #6486 (open) is the issue the field was added for. No guard covers the pair: `composite_params_layout_*` asserts offsets and size only, and no test exercises `upload_params`.
- **Description**: `upload_params` writes `write_mapped_prefix(.., prefix_bytes)` with `prefix_bytes = offset_of(sky_apertures) + aperture_count * size_of::<CompositeSkyAperture>()`, to avoid copying the unused 12 KB aperture tail. Before `fa9ada406` nothing followed the array, so that prefix was the whole live struct. `fa9ada406` appended `view_proj` after the array ("appended last so every earlier offset is unchanged"). With `aperture_count = 0` (every exterior, almost every interior) the prefix is 528 bytes; `view_proj` at byte 12,816 is outside it and stays at whatever the per-frame UBO was created with. It is only written on a frame with all 256 apertures populated.
- **Evidence**: `let aperture_count = (params.sky_aperture_count[0] as usize).min(MAX_SKY_APERTURES); let prefix_bytes = offset_of!(CompositeParams, sky_apertures) + aperture_count * size_of::<CompositeSkyAperture>(); self.param_buffers[frame].write_mapped_prefix(device, from_ref(params), prefix_bytes)`; `write_mapped_prefix` does `bytes[..prefix_bytes.min(bytes.len())]`. The only caller is `build_and_upload_instances.rs:1574`. The god-ray taps do `vec4 clip = viewProj * vec4(q, 1.0); if (clip.w <= 1.0e-3) continue;`, so a zero matrix skips every tap and `visible` stays 1.0.
- **Impact**: when the camera is submerged, the underwater god rays lose their contact-shadow occlusion (rocks, hulls, terrain no longer block the shafts) on every frame since 2026-11-04. If the freshly allocated UBO memory is not zero the taps project through garbage instead; either way the host's value never arrives. Visual-only, submerged-only. Confidence: the dropped bytes are CONFIRMED by code reading; the visual consequence is derived and needs one submerged capture (FNV or Skyrim lake) to see. It is also the reason #6486's "half-fixed" state is worse than reported: the per-pixel `inverse()` is gone and so is the matrix.
- **Related**: #6486 (open), #4805 (closed, the prefix upload), `fa9ada406`.
- **Suggested Fix**: make the prefix cover the trailing member: write the prefix and then `view_proj` separately (`write_mapped_at` of the last 64 bytes), or move `view_proj` ahead of `sky_apertures` and shift the offsets in lockstep with `composite.frag` and the layout test. Add a test that builds a `CompositeParams` with a sentinel `view_proj`, runs the prefix-length arithmetic for `aperture_count = 0` and asserts the sentinel's bytes are inside it.

### PERF-2026-11-09-D3-01: the BLAS residency reservation and the memory-budget per-preset table bill 44 B per froxel per slot, but the local-grid irradiance field adds 16 B per froxel per slot and the atlas a fixed 48 MiB
- **Severity**: LOW (accounting gap, no reachable effect on the RT-minimum card; see Impact)
- **Dimension**: GPU Memory Pressure
- **Location**: `screen_scaled_reservation_bytes` (`acceleration/predicates.rs`, `froxels * FROXEL_BYTES_PER_SLOT * MAX_FRAMES_IN_FLIGHT`); `FROXEL_BYTES_PER_SLOT = 44` (`volumetrics.rs`); `irradiance_resident_bytes` and `local_grid_resident_bytes` (`volumetrics/local_grid.rs`, called only by the boot log and tests); `docs/engine/memory-budget.md` "Volumetrics (M55)" tables
- **Status**: NEW
- **Description**: `LocalGrid::build` allocates, per frame in flight, an RGBA16F irradiance image of `2 x froxel width` (radiance and mean light direction side by side) and, once, three RGBA16F `64 x 64 x 256` atlas volumes. Both are resident whenever the volumetrics pipeline exists, live boxes or not. Neither is in `screen_scaled_reservation_bytes` (which reads `FROXEL_BYTES_PER_SLOT` only), nor in `render_extent_bytes_per_pixel_x1024`, and `grep -rn -i "irradiance\|local_grid" acceleration/` returns nothing. The doc states the costs in prose ("320x90x64 x 8 B x 2 FIF = 28 MiB at 1280x720") but the per-preset table ("1080p native ~183 MB", "4K native ~730 MB") and the "Fixed floor" column omit them.
- **Evidence** (derived from the constants, default divisor 8, 64 slices): irradiance bytes = froxels x 32 (16 B x 2 FIF). 1280x720: 921,600 froxels = 29.5 MB. 1920x1080: 2,073,600 = 66.4 MB. 2560x1440: 3,686,400 = 118.0 MB. 3840x2160: 8,294,400 = 265.4 MB. Atlas: 48 MiB = 50.3 MB fixed. Missing from the reservation: 36 % of the billed froxel term plus the atlas, i.e. 116.7 MB at 1080p native and 315.7 MB at 4K native. The BLAS budget is `(heap - reservation) / 3` clamped to [256 MiB, 1 GiB], so the overstatement is a third of that: ~39 MB at 1080p, ~105 MB at 4K.
- **Impact**: the budget only moves when `(heap - reservation) / 3 < 1 GiB`, i.e. a heap below ~3 GiB plus the reservation. On the 12 GB dev card and on the 6 GB RT minimum, at 1080p and at 4K native, the clamp wins (6 GiB - 2.3 GB = 3.7 GiB / 3 = 1.2 GiB, still clamped), so nothing changes today. It becomes real on a heap near 5 GiB at 4K native, and it is exactly the "new screen-sized allocation missing from the reservation" regression class #3992 / #6035 closed twice. The doc table is wrong now.
- **Related**: #3992, #6035, REN-D8-06 (#6827, the same image's 3D-dimension limit), #6533.
- **Suggested Fix**: add an `IRRADIANCE_BYTES_PER_FROXEL_PER_SLOT = 16` constant beside `FROXEL_BYTES_PER_SLOT`, bill it (and `local_grid_resident_bytes()`) in `screen_scaled_reservation_bytes`, extend `froxel_grid_cost_matches_the_memory_budget_doc` to pin the new row, and add the irradiance and atlas rows to the per-preset table.

### PERF-2026-11-09-D5-01: the composite pipeline pair is specialised on sun radiance only, so every daytime exterior frame uses the 126-register variant although no fire march runs without a live box
- **Severity**: LOW (**unmeasured**)
- **Dimension**: GPU Pipeline
- **Location**: `CompositePipeline::dispatch` / `pipeline_sunless` (`composite.rs`), `GpuLocalGridParams::sunlit` and `new` (`volumetrics/local_grid.rs`, `sun_radiance` is filled outside the `if simulated` block), `LOCAL_FIRE_SUNLIT` (`composite.frag`), `record_post_passes`' pipeline pick from `local_fire_sunlit()`
- **Status**: NEW
- **Description**: `15a4ecf10` made the march loop cheaper by specialising the pipeline on `LOCAL_FIRE_SUNLIT`. The recorded reason is register pressure: 96 registers per thread sun-less, 126 sun-lit (`VK_KHR_pipeline_executable_properties`, 4070 Ti, plan doc), costing ~0.1 ms of composite per scene "merely by existing in the loop". The selector is `sunlit() = any(sun_radiance > 0)`, and `sun_radiance` is set whether or not a box is live. So an exterior by day, with no fire anywhere in reach (`localGrid.info.z == 0`, the march branch never taken), still binds the 126-register pipeline for the whole fullscreen composite. A register count is a property of the pipeline, not of the path taken.
- **Evidence**: `fn sunlit(&self) -> bool { self.sun_radiance[..3].iter().any(|&c| c > 0.0) }`; `params.sun_radiance = [frame.sun_color[0..3], 0.0]` is assigned after, outside, `if simulated { .. }`. The doc's measurements are all scenes with fire in view; no no-fire frame is on record against a march-free build.
- **Impact**: not quantified. Potential is the occupancy of the non-fire composite work (sky, fog compose, caustics, the underwater branch) on the commonest frame type. Upper bound by the doc's own figure: ~0.1 ms per scene at 720p for the loop's presence, scaling with render pixels if it is occupancy-bound.
- **Measure first**: A/B composite_ms on a no-fire exterior (FNV Goodsprings noon, Skyrim Whiterun exterior) between the shipped build and one with the march folded out by a third specialization constant (`LOCAL_FIRE_ENABLED`, false when `info.z == 0`); read the register counts with `VK_KHR_pipeline_executable_properties`. Interleave the runs, pin `--rt-test-ray-quality-tier`. The existing `BYRO_LOCAL_GRID_DEBUG=no_march` does not answer it: it is a runtime branch and leaves the register count alone.
- **Suggested Fix** (only if the A/B shows a gain): a second constant, three pipelines (or two, selected by `live_mask != 0 && sunlit`), all in the same `create_graphics_pipelines` batch.

### PERF-2026-11-09-D5-02: the irradiance field is zero-stored for every froxel every frame, with its barrier pair, even when no local-grid box is live
- **Severity**: LOW (**unmeasured**)
- **Dimension**: GPU Pipeline
- **Location**: tail of `volumetrics_inject.comp` (`imageStore(localIrradiance, coord, irradiance); imageStore(localIrradiance, coord + ivec3(size.x, 0, 0), lightDirection);`), `LocalGrid::record_frame` (the irradiance read-to-write / write-to-read barriers are "unconditional", comment in code), `composite_barriers` in the Stage F publish
- **Status**: NEW
- **Description**: the light-field math is gated (`needsLightField` is true only for froxels within a box plus one froxel of margin, "rays are paid only there"), but the two stores are not: with no box live every froxel writes `vec4(0)` twice (16 B), the history barriers on the image run, and Stage F publishes it to the fragment stage. Composite never reads it unless `localGrid.info.z != 0`, and the inject reads the previous slot only inside `needsLightField`.
- **Evidence**: derived store traffic = froxels x 16 B per frame: 14.7 MB at 720p, 33.2 MB at 1080p, 59.0 MB at 1440p, 132.7 MB at 4K native. Next to the 44 B of froxel state the same shader already writes per froxel per frame (lighting, three combustion volumes, emission), that is +36 %. Resident cost is D3-01.
- **Impact**: not quantified. At the 4070 Ti's bandwidth the 1440p figure is ~0.12 ms if the stores were bandwidth-bound, but they overlap the shader's ray work, so the real number is probably a fraction of that; it is also a fixed cost on the commonest frame type. The fire cost-tables in the plan doc all have fire in view, so the cost of the field with no fire is not on record.
- **Measure first**: `volumetrics_inject_ms` on a no-fire exterior, shipped vs a build whose stores sit behind `localGrid.info.z != 0u`. The gate needs a history rule: on the first frame a box goes live the previous slot's field must not be blended (`prev_camera_pos.w` plus a host "field valid" bit), or stale values from an earlier fire leak in; that is why it is not a one-line change.
- **Related**: #5950 (the inject's other atomics / stores were measured free; this site is a different write).

### PERF-2026-11-09-D5-03: the local sim hashes twelve words and issues three same-address global atomics per active cell, for a number only the debug log and the bench line read
- **Severity**: LOW (**unmeasured**)
- **Dimension**: GPU Pipeline / Telemetry & Origin Cost
- **Location**: `volumetrics_local_sim.comp` (the `if (carriesCombustion(..))` block: 12 x `hashWord`, `atomicAdd(localGridStats[base+0], 1)`, `atomicXor(.., h)`, `atomicAdd(.., h)`); consumers `LocalGrid::begin_frame` / `fold_stats`, `note_volumetrics_state`, `app_events.rs` and `systems/debug.rs` log lines
- **Status**: NEW
- **Description**: `local_grid_cells` and `local_grid_hash` exist to verify determinism and to print on the bench / stats line (`grep -rn local_grid_hash crates byroredux` finds only those logging sites and the struct). They are computed by every active cell of every box on every frame in release builds. The brick stores in the same block are real (composite reads them); the hash and the atomics are not.
- **Evidence**: the atomics target three fixed words per box. Their values are divergent (`h` differs per cell), so they cannot be warp-aggregated at compile time the way a constant `atomicAdd(.., 1)` can. Recorded sim cost per active cell falls as the cell count rises (s1: 0.081 ms / 7,228 cells = 11 ns; s2a: 0.262 / 37,277 = 7 ns; stage 1b s3a: 1.252 / 272,145 = 4.6 ns), which fits a fixed serialised component but does not prove one. #5950 measured a different site (the inject's per-bin combustion-moment atomics, spread over 256 bins) as free; do not read this as re-opening that.
- **Impact**: not quantified; bounded by the sim's recorded cost (0.07-0.15 ms on the current scenes, so at most a fraction of that).
- **Measure first**: `volumetrics_local_sim_ms` on s3a and s2a with the hash block stripped (keep the bricks), interleaved medians of 5. If it is below noise, close it with the number.
- **Suggested Fix** (only if it matters): move the hash behind a `LOCAL_GRID_DEBUG` bit already carried in `localGrid.info.w`, or compute `active_cells` from the brick buffer on the host.

### PERF-2026-11-09-D1-01: `fog_mean_density_scale` is 9-10 microseconds per call, every frame (measurement of the sibling's REN-D8-09)
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/renderer/src/vulkan/volumetrics/noise.rs` `fog_mean_density_scale`, called from `build_composite_params` (`context/frame_params.rs`) every frame
- **Status**: Existing: #6831 (REN-D8-09). Measured here; no change to its severity.
- **Description / Evidence**: standalone replica of the loop (`/tmp/audit/performance/bench/fmds.rs`, same arithmetic, all 256 base bins non-zero so the `p_base == 0.0` skip never fires, `black_box` on the inputs, 9 runs x 200,000 calls): **8.67 us median with `-C target-cpu=native`, 9.98 us generic**, min-max spread under 2 %, on a Ryzen 9 7950X at 75 % scaling frequency while two other audits were building. That is 0.03 % of a 33 ms frame, 0.2 % of a 5 ms one. The sibling report's "tens of microseconds" overstates it by ~3x.
- **Impact**: the project treats render-thread CPU as a defect, and it is a pure function of one scalar that changes on weather transitions, so the cost is avoidable, but it is not a hot-path problem. Not in the top of the fix order.
- **Suggested Fix**: as #6831: memoise on the coverage's bit pattern in the context.

### PERF-2026-11-09-D1-02: two small heap allocations per frame on the volumetrics seam
- **Severity**: LOW (derived; no quantitative guard exists for either site)
- **Dimension**: CPU Hot Paths
- **Location**: (a) `VolumetricsPipeline::dispatch` Stage F: `let published: Vec<vk::ImageMemoryBarrier> = std::iter::once(post_int).chain(local_published.into_iter().flatten()).collect();` (`volumetrics.rs`); (b) `build_composite_params`: `let mut candidates: Vec<(f32, CompositeSkyAperture)> = Vec::new();` in the interior portal-sky branch (`context/frame_params.rs`)
- **Status**: NEW
- **Description**: (a) the barrier list is always exactly four entries (one + the grid's `[_; 3]`, or one when the grid is absent) and is built into a fresh `Vec` on every dispatching frame. (b) `candidates` is a new `Vec` per frame in cells with a portal-outdoor sky, grown by doubling to the aperture count (#6753 removed the *second* vector there and left this one).
- **Impact**: roughly a handful of allocator calls per frame, well under a microsecond each; it matters only because its neighbours (`sources`, `fog_cluster_refs_scratch`, the moved-surface list) were each given a persistent scratch for exactly this reason. Hot-path rule: per-frame allocations that should be persistent.
- **Suggested Fix**: (a) a `[vk::ImageMemoryBarrier; 4]` with a slice length; (b) a `ScratchBuffers` Vec cleared and refilled, with a `ctx.scratch` row and the existing shrink.

### Existing / still present
- **#6831** (REN-D8-09): see D1-01.
- **#6486** (PERF-D5-01, open): `composite.frag` no longer inverts per pixel (`fa9ada406`), but see D4-01: the replacement matrix does not reach the shader. The other half (144 depth taps per submerged pixel, no submerged `composite_ms`) is unchanged and still owed.
- **#6827 / #6828** (REN-D8-06 HIGH, REN-D8-07 MEDIUM): correctness, covered by the renderer report; D3-01 and D5-02 concern the same irradiance image from the memory and cost side.
- **#5959, #6248** (MEDIUM), **#6313, #6315, #6316, #6341, #6485 (fixed in code by `59e376a90`, issue still open in the snapshot)**: not volumetrics, not re-read this run beyond the Guard line for #6485 (`moved_surface_ids_scratch` is in `shrink_frame_scratch`).

### Checked, no finding
- **Dispatch gating**: `requires_dispatch` runs on the grid-filtered volume list, so a distant source neither arms the grid nor its linger; `combustion_transport_active` and `TRANSPORT_KNOWN_EMPTY_DT` skip the transport stencil when nothing burns; the combustion moment drain is latched by `combustion_moment_dirty` and a fog-free frame never invalidates, decodes or flushes; the sim dispatch is skipped with no live box (`has_work`), including its brick fill and its three barriers. `local_visibility_work_is_gated_on_nonzero_scattering`, `transport_occupancy_gate_is_wired_into_the_inject_pass` and `the_dispatch_empty_branch_resets_the_occupancy_mask` hold.
- **Host uploads are O(live data)**: fog volume buffer prefix (`16 + n x 96`), cluster entries by `[lo, hi)` with the stale-range union, indices by `index_len`, local params 144 B. The 16 KiB occupancy mask rewrite is unconditional on purpose (#4784) and costs one memcpy.
- **Scratch**: `fog_cluster_refs_scratch` and `combustion_light_candidates_scratch` are shrunk through `VolumetricsPipeline::shrink_scratch` and have rows in `ctx.scratch`; `volumetric_fog_scratch` has a row, is bounded by the 512-volume truncation in `collect_fog_volumes` (<= 49 KB) and needs no shrink. One cosmetic point: `fog_cluster_refs_scratch.len()` is the working-set signal and is only reset by a cluster build, so after a heavy cell it stays pinned (<= 4.7 MB host RAM) through cells that dispatch with zero volumes. Not filed.
- **Env switches** (`BYRO_BFECC`, `BYRO_SUN_ALPHA`, `BYRO_SUN_DUST`, `BYRO_LOCAL_GRID_DEBUG`): all `OnceLock`, none reads the environment per frame, including the ones called from `record_volumetrics_pass` every frame.
- **Load-time vs per-frame fog fitting**: `fit_legacy_fog_extinction_with_power` (80 iterations x 256 samples, ~20 k `exp`) is called from `FogMedium::from_authored_fog` / `from_legacy_ramp`, which run at cell / weather translation (`env_translate`, `CellLightingRes`), not per frame; the only per-frame call is behind an explicit `BYRO_FOG_*` override.
- **Composite exterior sky-air fade** (`3500d2737`): no loop; the Chapman column is closed-form. Not a cost concern at any resolution the engine runs.
- **Timers**: `TimerPass` has 26 rows, `QUERIES_PER_FRAME` is 62 (31 brackets x 2), `BENCH_GPU_KEYS` has 25 entries (24 + `volumetrics_local_sim`), `every_named_bracket_has_exactly_one_pass_row` resolves, `volumetrics_local_sim` is recorded inside `Volumetrics` and listed in `NESTED_GPU_BRACKETS` of the studio panel, so no sum double counts it. `VolumetricsLocalSim` has no rows of its own for the brick `cmd_fill_buffer` (it is inside `volumetrics_ms` only). Fine.
- **Integrate pass**: `sliceDistance` is evaluated twice per slice per thread (front and back of the same slab) and the slab widths are identical across all columns. Derived ALU saving is tiny against the recorded 0.02 ms; not filed.
- **Stage D barrier** (`COMPUTE | FRAGMENT | TRANSFER -> COMPUTE` on the integrated volume): the `FRAGMENT` source scope can order integrate behind this frame's main-pass fragment work. Needs Nsight / a timeline to show a bubble; speculative-Vulkan caveat applies, no claim made.

### Guards verified (all resolve exactly once; renderer lane green)
`the_volumetrics_scratches_are_shrunk_through_the_pipeline`, `fill_scratch_telemetry_covers_every_scratch_field_in_the_crate`, `volumetrics_host_buffer_ledger_matches_the_code`, `froxel_grid_cost_matches_the_memory_budget_doc`, `the_local_grid_follows_the_skip_and_completion_wiring`, `every_named_bracket_has_exactly_one_pass_row`, `the_dispatch_empty_branch_resets_the_occupancy_mask`, `transport_occupancy_gate_is_wired_into_the_inject_pass`, `local_visibility_work_is_gated_on_nonzero_scattering`, `the_sunless_composite_pipeline_is_the_same_shader_with_the_constant_false`, `overflow_warns_once_per_episode_and_rearms_under_the_cap`; `bench_gpu_keys_match_the_reported_bracket_order` exists in the bin (not run: bin build deferred to the runtime audit).

### Dims 2, 6, 7 (unchanged since baseline, skimmed)
Dim 2: `09940503b` (reversed-Z shipped), `2ed1d91ce` (FO3/FNV env-mask, NIFAL-owned) and comment-only commits; no sort-key or batching change. Dim 6: four commits (`27dcf26db`, `0ae4da4ee`, `ac3e246dd`, `4f6d738d7`), all structural or comment moves (SkinChain methods, SkinningState grouping, buffer_reference declaration); not read line by line for behaviour. Dim 7: `ea5417dc7` region-weather resolution runs at the EXAL boundary per crossing, not per frame; nothing in the volumetrics path.

## Measurement owed (needs an engine launch; none was made)
1. `volumetrics_inject_ms`, `volumetrics_integrate_ms`, `composite_ms` at 2560x1440 TAA-native and 3840x2160 FSR-Quality (1440p render) on FNV Goodsprings noon, Skyrim Whiterun exterior and FO4 Commonwealth: every recorded figure is 1280x720 and the froxel grid is four times larger at 1440p.
2. D5-01 and D5-02 as described (no-fire exterior, interleaved, tier pinned).
3. D5-03 (strip the hash, s2a / s3a).
4. Submerged `composite_ms` (carried from #6486), plus a submerged capture to see D4-01's missing occlusion.
5. `scripts/check-bench-harness-provenance.sh` before any cross-commit comparison.

## Prioritized Fix Order
1. **PERF-2026-11-09-D4-01**: upload the trailing `view_proj` (one extra 64-byte write or a field move) and add the sentinel test. Quick win, user-visible.
2. **PERF-2026-11-09-D3-01**: bill the irradiance field and the atlas in `screen_scaled_reservation_bytes` and correct the table. Quick win, no behaviour change on current hardware.
3. **Measurement owed 1**, then decide whether D5-01 / D5-02 / D5-03 are worth anything; each is a one-run A/B.
4. **PERF-2026-11-09-D1-02** with D1-01 (#6831): persistent barrier array and aperture scratch, memoised mean-density scale.

## Stale skill premises
- Suite brief: "zero commits on the volumetrics paths since 2026-10-31" is false (98 commits against `670f81917` over the wider seam list; 10 against `9409e7b20` on the volumetrics + composite paths alone).
- Dim 5 inventory row "Volumetrics ... Runs when `requires_dispatch` (fog / emitters / combustion live)": by day in an exterior the haze floor makes the global-medium limb always true.
- Dim 3 checklist: the reservation list should say that `screen_scaled_reservation_bytes` bills 44 B/froxel/slot only and does not include the local-grid irradiance field (16 B/froxel/slot) or the 48 MiB atlas (D3-01).
- Dim 4: add a guard line for `CompositePipeline::upload_params`: the aperture prefix must cover every member that follows the array (D4-01).
- Dim 8 / inventory: `BENCH_GPU_KEYS` is 25 (the 10-31 report said 24); `QUERIES_PER_FRAME` 62 and 31 brackets match.
- Dim 5 composite row: the fire march pipeline pair is chosen by sun radiance alone (D5-01).

Next step: `/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-11-09.md`
