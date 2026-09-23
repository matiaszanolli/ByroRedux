**HEAD**: `17c01a4e5` (the suite brief named `2237da9c3`; the one commit in between, `17c01a4e5`, touches only water and ECS files, not the renderer) · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-09-21.md` (HEAD `f97775ca8`). The last volumetrics-focused perf run was `AUDIT_PERFORMANCE_2026-09-05.md`. · **Audited**: Dims 1, 3, 4, 5 and 8, volumetrics slice only · **Unchanged since baseline (skimmed)**: all volumetrics paths. Since 2026-09-21 only `2237da9c3` (doc sweep) touched them, but per the `volumetrics-deep` preset the area was re-traced deep. · **Out of suite scope**: Dims 2 (draw/instancing), 6 (skinning/BLAS) and 7 (streaming; only the volumetrics resize/reset path was skimmed).

# Performance Audit — 2026-09-23 (area-scoped: M55 volumetric fog)

**Command**: `/audit-performance`, `--depth deep`, one leg of `/audit-suite --preset volumetrics-deep`. It covers only the performance dimensions that touch the volumetric fog system.

**Scope**:
- `crates/renderer/src/vulkan/volumetrics.rs` + `volumetrics/`
- `volumetrics_inject.comp` / `volumetrics_integrate.comp`
- host plumbing: `context/post_passes.rs` `record_volumetrics_pass`, `assemble_camera_and_lights.rs` (combustion-moment drain), `build_and_upload_instances.rs` (ray-budget feed)
- the timer: `gpu_timers.rs` `cmd_volumetrics_*`
- app-side producers: `byroredux/src/render/fog_volumes.rs`, `byroredux/src/fog.rs`

**Severity scale**: `.claude/commands/_audit-severity.md`.

## Method

- **Static audit.** One auditor worked through the dimensions in order, with no sub-agents. No engine was launched and no source was modified.
- **Where the numbers come from.** Every number below is one of:
  - arithmetic from source (marked *est.*);
  - the checked-in stepped-camera bench TSVs: `BENCH_stepped-camera_4c9a5b36.tsv` (the live record) and `BENCH_stepped-camera_cb44d99f6.tsv` (2026-09-22). Both use harness `1293dfc0`, and `scripts/check-bench-harness-provenance.sh --strict 4c9a5b36` reports "harness byte-stable since this record" (exit 0).
- **Guards run**: `cargo test -j 4 -p byroredux-renderer --lib -- volumetric froxel fog_volume fog_cluster combustion skip_clear` → **51 passed, 0 failed, 0 ignored**. These include:
  - `froxel_grid_cost_matches_the_memory_budget_doc`
  - `fog_volume_upload_bytes_*` and `build_fog_volume_clusters_reports_*` (the #3834 guards)
  - `failed_combustion_moment_drain_leaves_the_slot_latched` (#4535)
  - `combustion_transport_skips_only_once_nothing_is_burning_or_lingering` (#3131)
  - `volumetrics_ubo_sizes_match_host_structs_in_every_shader`
- **Dedup sources**:
  - `gh issue list` keyword searches for volumetric / froxel / fog volume / combustion / soot, about 60 issues, all closed except unrelated #4618 / #4625;
  - `AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D1-01/02, PERF-D3-01 → #3834 / #3835 / #3839, all closed and fixed);
  - #3131 (scene-level transport gate), #2040 (TOP_OF_PIPE caveat);
  - the sibling `AUDIT_RENDERER_2026-09-23.md`, whose six findings are **not** re-filed here. Where one has a performance angle it is cross-referenced (REN-D8-01, REN-D8-03).
- **The Markarth data point** (`volumetrics=3.0` of `main_render=26.4` ms, RTX 4070 Ti) is one slow-frame dump. It is used only as motivation, never as a baseline. See PERF-D8-2026-09-23-01 for why it cannot be broken down as logged.

## Executive Summary

**7 findings, all NEW: 0 CRITICAL · 0 HIGH · 3 MEDIUM · 4 LOW.** No eroded guards: every volumetrics guard the skill and the prior reports name exists and passes.

The froxel grid's allocation, upload and barrier plumbing are sound, and the 2026-09-05 fixes (#3834, #3835, #3839) hold. The cost problem is **what runs per froxel, and what arms it**:

1. **One flame anywhere in the frustum turns on grid-wide combustion transport** (PERF-D5-01, PERF-D5-02).
   - `has_transport_emitter` scans the frustum-culled volume list with no distance cull, so a fire beyond the 128 m grid still sets `simulationDt > 0` and refreshes the 78 s linger.
   - Once armed, every one of the 0.9–2.1 M froxels runs the RK2 backtrace. That includes the six-neighbour gather #3131 identified. Each quiet froxel pays about 27 trilinear RGBA16F fetches and 2 curl-noise evaluations instead of 3 fetches.
   - #3131's fix gated this per scene, not per region, so every Skyrim interior or exterior with a hearth or brazier pays the full stencil every frame.
   - The bench is consistent with this: Whiterun BanneredMare (hearth → Flame volumes) costs about 5× Prospector per froxel at the same render extent.
2. **The sun shadow ray is traced even when the sun contributes nothing** (PERF-D5-03).
   - At night the host zeroes `sun_color`, but the shader still casts one opaque ray per froxel.
   - The same applies wherever the medium is exactly zero.
3. **These costs do not respond to the adaptive ray budget.** Only `volumetric_light_cap` scales with the tier. The sun ray, the transport stencil and the transmittance marches are tier-invariant. The Markarth dump's `main_render=26.4` ms would already have pushed the controller to tier 0 (light cap 2), so most of that 3.0 ms is exactly the tier-invariant floor these findings target.

**Observed-vs-ROADMAP delta** (checked-in TSVs, 1280×720 output, medians of 3; not re-benched this session):

| Scene | Record `4c9a5b36` TAA / FSR-Q | `cb44d99f6` TAA / FSR-Q | Verdict |
|---|---|---|---|
| Cornell | 0.230 / 0.122 | 0.236 / 0.121 | flat |
| Prospector | 0.259 / 0.155 | 0.262 / 0.155 | flat |
| Dugout Inn | 0.358 / 0.204 | 0.375 / 0.211 | flat |
| Whiterun | 1.030 / 0.537 | 1.128 / 0.828 | FSR-Q +0.29 ms, but the record's own run 3 was 0.824 (runs 0.537 / 0.536 / 0.824). The new runs sit at the old upper mode, so this is **not a finding**. It is the bimodality PERF-D8-01 explains. |
| MedTek | 1.937 / 1.198 | 1.724 / 1.262 | within range |

The 2026-09-21 transport commits (BFECC `10798823b`, multi-scatter `33d253b73`, spectral soot `1e1bca8fb`, forcing retune `f9494d7e1`) show no attributable regression in these rows.

## Hot Path Analysis

**RT quality tier.** It is not recorded in the TSVs or the `gpu_ms` line (PERF-D8-01), so the tier behind each bench row is unknown. The Markarth dump implies tier 0.

Froxel counts use the default `VolumetricsConfig` (`/8`, 64 slices):

| Render extent | Grid | Froxels |
|---|---|---:|
| 853×480 | 107×60×64 | 410,880 |
| 1280×720 | 160×90×64 | 921,600 |
| 1920×1080 | 240×135×64 | 2,073,600 |

| Site | Cost | Class | Finding |
|---|---|---|---|
| Inject pass, transport armed (`dt > 0`) | Quiet froxels:<br>• 27 trilinear 3D RGBA16F fetches vs 3<br>• 9 reprojections<br>• 2 `curlField` + 2–3 wind `sin`<br>*est.* +22 M fetches/frame at 720p render, +50 M at 1080p | Grid-wide stencil, scene-level gate | PERF-D5-02 |
| Transport arming | Any Flame/Smoke/Explosion volume in the frustum, at any distance → `dt > 0` plus a 78 s linger | Gate too wide | PERF-D5-01 |
| Sun visibility ray | 1 ray/froxel (2 in interiors on an opaque miss): 0.92–2.07 M rays/frame, spent even when `sun_color == 0` (exterior night) or the froxel's medium is 0 | Wasted RT | PERF-D5-03 |
| Transport fields with nothing burning | 3 fetches + 3 × 8 B stores per froxel: *est.* 44 MB/frame at 720p, 100 MB at 1080p, 400 MB at 4K | All-zero field traffic | PERF-D5-04 |
| `sampleLocalMedium` | Full profile (2 noise fetches) evaluated for transported volumes, then discarded | Wasted ALU/texture | PERF-D5-05 |
| Fog cluster upload (CPU, WC) | Prefix up to the max touched index. The major axis is world Z (10 KB/layer), so a volume ≥ 16 m in +Z uploads ≥ 90 of 160 KB | Upload amplification (partial #3834) | PERF-D4-01 |
| Combustion-moment drain (CPU) | 8 KB decode + fill + flush on every dispatching frame, fire or not (`combustion_moment_dirty` set unconditionally, `volumetrics.rs:1175`) | µs-scale; noted under PERF-D5-04 | — |
| Integrate pass | 64-slice serial march per column; 14,400 threads at 720p render; *est.* < 0.05 ms | Latency-bound but small | none |
| Per-frame descriptor rewrites | 3 `vkUpdateDescriptorSets` calls (7 writes) per dispatching frame (`write_tlas` / `write_boundary_geometry` / `write_lights_and_clusters`) | µs-scale | none |
| `volumetrics_ms` bracket | Inject + integrate in one bracket. The TOP_OF_PIPE start absorbs the SVGF/caustic tail. No tier or transport state is logged | Attribution gap | PERF-D8-01 |

**VRAM** (Dim 3; cite `docs/engine/memory-budget.md` § Volumetrics). The grid costs 44 B/froxel/slot × 2 FIF, i.e. about 81 MB at 720p render, 183 MB at 1080p, 324 MB at 1440p and 730 MB at 4K. Of the 44 B, 20 B are the combustion fields, which are allocated unconditionally. That page records them as "Runtime work, tracked separately", but no issue tracks the move to local volumes. That is a note, not a finding. Allocation is once per resize, under `device_wait_idle`, and there is no per-frame reallocation.

## Findings

### MEDIUM

#### PERF-D5-2026-09-23-01: Transport emitters outside the 128 m froxel grid arm the grid-wide combustion stencil and its 78 s linger
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**:
  - `byroredux/src/render/fog_volumes.rs:85` (frustum-only cull), `:105` (truncate after a near-to-far sort, no distance cull);
  - `crates/renderer/src/vulkan/volumetrics.rs:234-241` (`has_transport_emitter`), `:253-259` (`combustion_transport_active`), `:1155-1164` (`fog_reference[3] = simulation_dt`), `:1598-1615` (`requires_dispatch`).
- **Status**: NEW. This is a gap in #3131's gate, which keys on the list rather than the grid.
- **Description**:
  - `collect_fog_volumes` keeps any `FogVolume` whose bounding sphere passes the frustum test, at any distance.
  - `has_transport_emitter` then returns true for any Smoke/Flame/Explosion profile in that list. It is used both to arm `simulationDt > 0` and to push `combustion_active_until_seconds` 78 s ahead.
  - The shader's source injection only ever sees volumes inside the camera-centred 16³ × 16 m cluster cube. `build_fog_volume_clusters` drops the rest (`:559-574`), and `sampleLocalMedium` / `applyCombustionSources` are cluster-driven. So a flame beyond the grid far plane contributes nothing, yet it still switches the whole grid onto the RK2 path (PERF-D5-02).
  - `requires_dispatch` also returns true for `!fog_volumes.is_empty()`. In zero-extinction weather (`FogMedium::DISABLED`, `extinction_per_meter: 0.0`), one distant fire in view therefore dispatches the entire pass.
- **Evidence**:
  ```rust
  // fog_volumes.rs:85 — the only spatial test
  if frustum.contains_sphere(center, extents.length()) { out.push(gpu); }
  // volumetrics.rs:1609-1614
  if has_transport_emitter(fog_volumes) { self.combustion_active_until_seconds = now + COMBUSTION_HISTORY_LINGER_SECONDS; }
  has_global_medium || !fog_volumes.is_empty() || (self.history_valid && now <= self.combustion_active_until_seconds)
  ```
  Particle fires become Flame volumes by default (`fire_volume_from_particle`, `fog.rs:544`, `BYRO_FIRE_VOLUMES` default on), so every lit brazier, campfire or burning barrel in view qualifies.
- **Impact**: This is the full PERF-D5-02 cost (*est.* 0.3–0.6 ms at 1080p render; see that finding) in open exteriors whose only fires are distant, plus 78 s of the same cost after the last one leaves view. It is tier-invariant.
- **Related**: #3131 (closed; introduced the scene-level gate), PERF-D5-02.
- **Suggested Fix**: Filter once on the renderer side before `requires_dispatch` / `dispatch`. Drop volumes with `distance(center, camera) − radius > far_distance_world()`, reusing the same sphere test `build_fog_volume_clusters` already does, and feed the filtered slice to `has_transport_emitter`, `requires_dispatch` and the cluster build. Pin it with a unit test: an off-grid Flame must leave `combustion_transport_active` false.

#### PERF-D5-2026-09-23-02: When any transport emitter is active, every froxel in the grid runs the RK2 backtrace, the six-neighbour gather and the curl forcing, including the empty majority
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp`:
  - `main` → `transportCombustion` (`:2594`);
  - the RK2 block `:2005-2078`;
  - the `combustionActivity < 0.08` neighbour gather `:2007-2018` → `incomingDynamicsFromNeighbors` `:1770-1828`;
  - the midpoint/source probes `:2026`, `:2039`;
  - the `dt > 0` curl/wind forcing `:2309-2337`, computed and then scaled by `activity` (0 on empty froxels).
- **Status**: NEW. This is the residual of #3131: that fix zeroes `dt` only when *nothing* in the scene transports.
- **Description**:
  - `simulationDt` is one scalar for the whole dispatch. With `dt > 0`, an empty froxel (the overwhelming majority) does all of the following:
    - samples its own history (3 fetches);
    - fails the `< 0.08` activity test the "wrong" way, so it runs the six-neighbour gather (6 × `samplePreviousTransport` = 18 fetches; `carriesCombustion` rejects each neighbour only *after* its three samples);
    - takes a midpoint and a source sample (6 fetches);
    - evaluates two `curlField`s (12 `sin`/`cos`) and 2–3 `atmosphericWindVelocity` `sin` calls, all multiplied by `activity = 0`.
  - Total: 27 trilinear RGBA16F 3D fetches and 9 reprojections (mat4·vec4 + `length` + log slice mapping) per quiet froxel, against 3 fetches at `dt = 0`.
  - A plume occupies a tiny fraction of the grid, and its transport support is bounded: `MAX_COMBUSTION_SPEED_MPS` 28 m/s × the `dt` clamp of 1/15 s is 1.87 m per step, far below one 16 m fog cluster.
- **Evidence**:
  - *est.*: +24 fetches × 921,600 froxels = +22 M trilinear fetches/frame at a 720p render extent (FSR Quality at 1080p output), and +50 M at 1080p render.
  - At the 4070 Ti's texture rate that is ≥ 0.16 ms ideal at 1080p. Latency-bound reality is plausibly 0.3–0.6 ms.
  - Bench corroboration (1280×720 output, FSR-Q, 410,880 froxels): Whiterun BanneredMare (hearth → Flame volumes; `fog.rs:595` names its `FlamesSmall03-Emitter`) is 0.83 ms (≈ 2.0 ns/froxel). Prospector is 0.155 ms (≈ 0.38 ns/froxel). The scenes differ in more than transport, so this is corroboration, not attribution. PERF-D8-01's split bracket would make it attributable.
- **Impact**:
  - A tier-invariant cost on every frame of every cell with a fire in view, which covers most Skyrim interiors and many exteriors (Markarth's braziers).
  - It scales linearly with render resolution: roughly 4× at native 4K versus 1080p.
  - The adaptive controller cannot shed it, because only `volumetric_light_cap` is tiered.
- **Related**: #3131, PERF-D5-01, REN-D8-2026-09-23-03 (the frozen residual keeps `transportedMediumActive` froxels paying `transportedCombustionTransmittance`: 8 steps × (1 + lights) fetches).
- **Suggested Fix**:
  1. Add a world-space coarse occupancy mask on the existing 16³ fog-cluster grid. Mark a cell when the inject pass writes `carriesCombustion` (atomicOr into a small SSBO read next frame, one frame behind like the moment buffer), and OR in the clusters that hold a transported source volume (the CPU already knows them).
  2. Dilate by one cell (16 m ≫ 1.87 m/step).
  3. In the shader, skip the whole `hadHistory && dt > 0` block for froxels whose cell and its 26 neighbours are empty, falling through to the `dt = 0` carry.
  4. Hoist the curl/wind forcing under `activity > 0` in any case.
  5. Measure with the PERF-D8-01 split bracket before and after, at a pinned `--rt-test-ray-quality-tier`.
- **Confidence**: High on the structure (read from GLSL). Medium on the magnitude (arithmetic plus cross-scene bench, not an A/B).

#### PERF-D5-2026-09-23-03: The sun visibility ray (and its transported-soot march) is traced for every froxel even when the sun radiance is zero, or the froxel's medium is exactly zero
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**:
  - host: `crates/renderer/src/vulkan/context/post_passes.rs:669-678` (`sun_radiance = [0,0,0,fog_far]` when `sun_intensity <= 0`);
  - source of the zero: `byroredux/src/systems/weather.rs:146-155` (`sun_intensity = 0.0` outside `[sunrise_begin, sunset_end]`);
  - shader: `volumetrics_inject.comp:2738-2762` (unconditional `traceShadowBinary` to `DIRECTIONAL_SHADOW_TRACE_DISTANCE`, a glass-only second ray on interior misses, and the `transportedCombustionTransmittance` march) and the local-light loop `:2804-2888`.
- **Status**: NEW
- **Description**:
  - `inscatter += scattering_coef * phase * params.sun_color.rgb * visibility`. When `sun_color.rgb == 0` (every exterior night frame) the visibility term is multiplied by zero, but it is still computed: one ray query per froxel, and in interiors a second glass ray wherever the opaque ray escapes.
  - Froxels whose `scattering_coef` is exactly zero also trace the sun ray and every admitted local light's rays. Examples: `FogMedium::DISABLED` weather with the pass running only for fog volumes, or froxels outside every local medium.
- **Evidence**: `sun_color` is a per-dispatch uniform, so a `if (any(greaterThan(params.sun_color.rgb, vec3(0.0))))` guard is warp-uniform and free. `scattering_coef` is a per-froxel value that is already computed before the traces (`:2691`).
- **Impact**:
  - *est.* 0.92 M (720p render) to 2.07 M (1080p) wasted ray queries per frame on every exterior night frame, which is roughly 35–40 % of the in-game day in FNV/FO3/Skyrim time-of-day tables, plus the 8-step soot march in transported froxels.
  - A coherent terminate-on-first-hit shadow ray against an exterior TLAS is on the order of 0.1–0.4 ms per 2 M rays on this GPU (*est.*, not measured).
  - It is tier-invariant.
  - Until REN-D8-2026-09-23-01 is fixed, the medium above about 1 m is nearly zero, so most daytime sun and light rays are also near-worthless. Fixing that swap makes those rays productive again, so this finding is about the zero-radiance and zero-medium cases, not the swap.
- **Related**: REN-D8-2026-09-23-01 (argument swap), #1022 (interior sun policy; interiors use `SkyParams::default()`, `sun_intensity` 5.0, deliberately).
- **Suggested Fix**:
  - Wrap the sun block (opaque ray, glass ray and transmittance march) in a uniform `sun_color.rgb > 0` test, or pass a host flag in a spare UBO lane.
  - Skip the sun and local-light visibility work for a froxel when `max3(scattering_coef) == 0.0` exactly. Emission and authored inscatter do not need visibility, so the result is bit-identical.

### LOW

#### PERF-D5-2026-09-23-04: With nothing burning, every dispatching frame still reads and rewrites all three transport fields, and drains the moment buffer, for an all-zero field
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**:
  - `volumetrics_inject.comp:2594-2612` (unconditional `transportCombustion` + 3 `imageStore`), `:2639-2658` (`combustionMedium`, `accumulateCombustionLightMoment`, `flameSourceSampleRatio`);
  - `volumetrics.rs:1175` (`combustion_moment_dirty[frame] = true` on every dispatch) → drain `:1444-1467`.
- **Status**: NEW. This is the `dt = 0` residue of #3131. #3835 removed the drain only for fog-free frames.
- **Description**:
  - In a fogged exterior or dusty interior with no transport emitter and no linger, `dt = 0`, and each froxel still issues 3 trilinear history fetches and 3 RGBA16F stores (48 B/froxel of traffic) to carry a field that is identically empty.
  - The host still decodes and zeroes the 8 KB moment buffer every frame, because `dispatch` marks it dirty unconditionally even though the atomics cannot fire without emissive transported medium.
- **Impact**: *est.* 44 MB/frame at a 720p render extent, 100 MB at 1080p and 400 MB at native 4K (≈ 0.1 / 0.2 / 0.8 ms of bandwidth at a 500 GB/s effective rate), plus a few µs of CPU. It is bounded, but it is the common case in every fogged cell.
- **Related**: #3131, #3835, REN-D8-2026-09-23-03. That finding's option (b), zeroing the expired residual, makes a "field known-empty" latch trivially sound.
- **Suggested Fix**: Add a CPU "transport field empty" latch, armed once both FIF slots have been written with `dt = 0`, no emitter, and the linger expired (after REN-D8-03's residual clear). While it is set, pass a UBO flag that makes `main` skip `transportCombustion` / the stores / the moment and flame-ratio calls, and skip setting `combustion_moment_dirty`. Re-arm on the first transport emitter.

#### PERF-D5-2026-09-23-05: `sampleLocalMedium` evaluates the full density profile of transported volumes before discarding them
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `volumetrics_inject.comp:731-741`
- **Status**: NEW
- **Description**: The discard test is `!evaluateLocalFogVolume(...) || isTransportedProfile(profileKind)`. The profile kind is `volume.profile_params.x`, which is known before any evaluation. For every froxel inside a Smoke/Flame/Explosion primitive, the function runs `localProfileRadius`, `sampleLocalTurbulence` (2 noise texture fetches) and `localDensityProfile`, then throws the result away, because transported media are owned by the transport field.
- **Impact**: This affects only froxels inside transported primitives. That is small for torches, but it covers large regions for explosions and nuclear clouds and for near-camera fires, where froxels are densest. Fixing it is a one-line reorder.
- **Suggested Fix**: `if (isTransportedProfile(clamp(volume.profile_params.x, …))) continue;` before `evaluateLocalFogVolume`. Even better, have the CPU cluster build skip transported volumes in a separate "homogeneous-only" index list.

#### PERF-D4-2026-09-23-01: #3834's cluster "prefix" upload is keyed on the highest touched cluster index, so any volume in the camera's +Z half uploads most of the 160 KB set
- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**: `volumetrics.rs:579-588` (`cluster_index = x + 16y + 256z`, `cluster_hi = max`), `:1228-1235` (upload `[..write_hi]` entries and `[..write_hi*8]` indices, unioned with the slot's previous hi).
- **Status**: NEW (partial effectiveness of #3834, closed). The original report proposed a min/max *range*; the fix shipped a prefix.
- **Description**: The index's major axis is the world-Z cluster layer, and each layer is 256 clusters × (8 B entry + 32 B indices) = 10 KB. One volume whose cluster range reaches layer *k* forces (k+1) × 10 KB. A fire one cell (16 m) in +Z of the camera already uploads ≥ 90 KB of the 160 KB, and a fire-rich cell (Markarth) sits near the full amount.
- **Impact**: About 90–160 KB of write-combined host writes per frame in fire-bearing cells (*est.* 20–60 µs CPU), which is the case #3834 targeted. It is CPU-only, and the GPU is unaffected.
- **Suggested Fix**: Track `cluster_lo` alongside `cluster_hi`, and upload `[min(lo, prev_lo), max(hi, prev_hi))` with the existing per-slot union. Alternatively, keep a per-Z-layer dirty mask and upload the touched layers.

#### PERF-D8-2026-09-23-01: `volumetrics_ms` cannot attribute its cost: inject and integrate share one TOP_OF_PIPE-started bracket, and no volumetrics state reaches the logs or bench
- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost
- **Location**:
  - `crates/renderer/src/vulkan/gpu_timers.rs:1007-1040` (START `TOP_OF_PIPE`, END `COMPUTE_SHADER`), `:158-171` (upper-bound doc);
  - `context/post_passes.rs:790-797` (bracket around `dispatch`);
  - `volumetrics.rs:1279-1287` (the first in-bracket barrier, COMPUTE→COMPUTE, waits on the preceding SVGF à-trous + caustic dispatches);
  - `context/build_and_upload_instances.rs:730-737` (controller input `max(main_render, volumetrics)`);
  - `byroredux/src/systems/debug.rs:67-100` (the `gpu_ms` / SLOW FRAME format).
- **Status**: NEW. #2040 (closed) documented the generic TOP_OF_PIPE upper-bound caveat. This finding is the volumetrics-specific attribution gap plus the missing state.
- **Description**:
  - The START timestamp is written before the pass's first barrier. That barrier serialises against the still-draining SVGF/caustic compute, so their tail lands inside `volumetrics_ms`.
  - Inject (ray-query and transport heavy) and integrate (a cheap column march) share one bracket.
  - Neither the once-a-second `gpu_ms:` line, the SLOW FRAME warning nor the bench TSV carries:
    - the adaptive RT tier, and hence `volumetric_light_cap`;
    - the froxel extent;
    - whether transport was armed (`dt > 0`);
    - the fog-volume count.
  - A number like Markarth's `volumetrics=3.0` therefore cannot be split between sun rays, light rays, the transport stencil and absorbed queue drain.
- **Evidence**:
  - Within-config bimodality in the checked-in TSVs: Whiterun FSR-Q 0.537 / 0.536 / 0.824 ms and balanced 0.485 / 0.742 / 0.485 (record `4c9a5b36`); MedTek FSR-Q 1.198 / 1.270 / 0.750.
  - The steps are about 0.25–0.3 ms, consistent with either a tier flip or tail absorption. The telemetry cannot tell which.
- **Impact**: Every volumetrics perf claim, including this report's, rests on cross-scene inference rather than attribution. The ray-budget controller also consumes the inflated value.
- **Related**: #2040, #4618 (open; exposure meter has no bracket), the skill's bench-discipline rule "report the tier".
- **Suggested Fix**:
  - Split into `volumetrics_inject_ms` and `volumetrics_integrate_ms`.
  - Write the START timestamp at `COMPUTE_SHADER` stage (it waits for prior compute to finish that stage), so the SVGF/caustic tail is excluded.
  - Add the RT tier, `volumetric_light_cap`, the froxel extent, `transport_armed` and the fog-volume count to the `gpu_ms` line and as TSV columns.

## Prioritized Fix Order

1. **PERF-D8-01 (measure first)**: split the bracket, fix the START stage, and log tier and transport state. Everything below then becomes A/B-able at a pinned `--rt-test-ray-quality-tier`.
2. **PERF-D5-03**: the uniform `sun_color > 0` guard plus the exact zero-medium skip. Tiny and shader-local, with a bit-identical result.
3. **PERF-D5-01**: a distance cull to the grid before `has_transport_emitter` / `requires_dispatch`, plus one unit test.
4. **PERF-D5-05**: a one-line reorder in `sampleLocalMedium`.
5. **PERF-D4-01**: a min/max range for the cluster upload (CPU).
6. **PERF-D5-02**: the coarse occupancy gate for the RK2 block. This is the largest win and needs the split timer to validate.
7. **PERF-D5-04**: the "field empty" latch. Land it after REN-D8-2026-09-23-03's residual fix, which makes it sound.

Cross-suite dependency: fix **REN-D8-2026-09-23-01** (the argument swap) before judging sun or light ray value. Today it zeroes the medium above about 1 m, so most traced rays contribute almost nothing.

## Stale skill premises (for the next `/audit-performance` sync)

1. **Per-frame pass inventory, Volumetrics row**:
   - "Runs when `VOLUMETRIC_OUTPUT_CONSUMED`" is stale. That is a `const true`. The live gate is `requires_dispatch` (global medium, any frustum fog volume, or combustion linger) **and** TLAS + cluster-cull + global-geometry availability; otherwise `record_neutral_frame` runs through `skip_clear_decision`.
   - "Scales with froxels" omits the transport multiplier: roughly 9× texture fetches per froxel while any transport emitter is armed (PERF-D5-02).
2. **Dim 5 checklist**: add "tier-invariant volumetrics work". Only `volumetric_light_cap` is tiered; the sun ray, transport stencil and transmittance marches are not. Also add the transport arming gate (`has_transport_emitter`, `combustion_active_until_seconds`) and the sun-radiance-zero case.
3. **Dim 8 checklist**: `volumetrics_ms` is a combined inject+integrate bracket whose START precedes a COMPUTE→COMPUTE barrier. Name it as a known attribution gap until PERF-D8-01 lands, and require the tier to be recorded alongside any volumetrics figure.
4. **Dim 3**: `memory-budget.md` says the combustion-field relocation is "tracked separately", but no issue exists. Either open one or reword the doc.
