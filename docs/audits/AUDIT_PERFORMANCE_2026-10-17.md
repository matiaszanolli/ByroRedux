**HEAD**: `e16a624e9` · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-10-12.md` (HEAD `5d31fd637`) · **Audited**: Dim 1 (volumetrics-fed CPU path only), Dim 3 (volumetrics ledger rows only), Dim 4 (volumetrics host-visible buffers only), Dim 5 (volumetrics inject/integrate/composite-tap only), Dim 8 (volumetrics timers, `fog_cluster` bucket, scratch telemetry only); scope = volumetrics-deep · **Unchanged since baseline (skimmed)**: none. The baseline never looked at this area, so nothing here is a delta of a prior look. Dims 2, 6, 7 are out of area.

**THIS REPORT IS AREA-SCOPED (volumetrics), NOT A FULL-CODEBASE PERFORMANCE AUDIT.** The next full run must not treat it as a baseline for anything outside `volumetrics.rs`, `volumetrics/{combustion,fog_clusters,init,noise}.rs`, `volumetrics_inject.comp`, `volumetrics_integrate.comp`, the `composite.frag` froxel tap, and their call sites (`context/post_passes.rs`, `context/assemble_camera_and_lights.rs`, `byroredux/src/render/fog_volumes.rs`).

# Performance Audit — 2026-10-17 (volumetrics-deep)

**Command**: `/audit-performance` as one leg of `--preset volumetrics-deep`, run solo with no sub-agents.

**Mode**: static analysis, unit-test runs, and two throwaway CPU micro-benchmarks. No engine, GPU process or bench was launched (suite rule 4). **Every GPU cost statement below is derived from code or quoted from a dated earlier measurement. None was measured in this run.** The CPU numbers in "Measured here" are measured, but on a scratch copy of the production function, not on the engine.

## Executive Summary

| Severity | NEW | Existing / tracked |
|---|---:|---:|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 1 | 0 |
| LOW | 4 | 1 (#5365, related only) |

- **The volumetrics CPU driver is not a bottleneck.** Per frame it allocates nothing, hashes nothing, and rebuilds the fog-cluster grid in 3–50 µs for realistic scenes. Measured worst cases: 1.3 ms for 512 grid-sized volumes, which no content produces.
- **The one finding with real weight is GPU-side and unmeasured**: the inject shader does 8 device atomics per luminous froxel into a buffer that lives in host system RAM (PERF-VOL-01).
- Every landed volumetrics perf fix I checked is still in place (see "Guard verification"). Nothing regressed.
- Host-visible volumetrics buffers match `docs/engine/memory-budget.md` to the byte (3,285,344 B per slot, 6,570,688 B for 2 FIF), pinned by a passing test. Index lists alone are 3 MiB of that per slot and are fixed worst-case sized.

## Scope and method

Read in full: `volumetrics.rs` production half (lines 1–1620), `volumetrics/fog_clusters.rs`, `volumetrics/combustion.rs`, the buffer and descriptor section of `volumetrics/init.rs`, `volumetrics_integrate.comp`, and in `volumetrics_inject.comp` the header, all bindings, `main()` (2846–3354), the occupancy gate, the transport block, the combustion moment accumulation and the local-light loop. Read the call sites in `context/post_passes.rs` (`record_volumetrics_pass`), `context/assemble_camera_and_lights.rs`, `byroredux/src/render/fog_volumes.rs`, and the `composite.frag` froxel tap.

Delta since the baseline (`git log 5d31fd637..HEAD` over the area): `67476a5d7` (#5772 init cleanup ownership, init-only, no per-frame effect), `1a2c48008` (docs), `5d2679372` (tests). **No functional commit touched `volumetrics.rs`, the two shaders or `composite.frag` since the baseline.** The churn the suite brief lists (#4774, #4780, #4784, #5055, #5094, #5208, #5215, #5593, #5809) landed 10-02 to 10-15, mostly before the baseline report, which did not examine this area. So this is the first perf read of that code.

## Per-frame pass cost model (derived, from `froxel_extent` and the shader header)

Default `froxel_xy_divisor` 8 (`upscaling.rs:146`), 64 slices, 128 m grid far.

| Render extent | Froxels | Worst-case ray queries/frame (22/froxel, shader header) |
|---|---:|---:|
| 1280×720 | 921,600 | 20.3 M (matches the shader header) |
| 1920×1080 | 2,073,600 | 45.6 M |
| 3840×2160 (native, `--upscaler taa`) | 8,294,400 | 182 M |

Dispatch is 8×8×8 groups (`WORKGROUP_X/Y/Z`), `div_ceil` on all three axes, so edge waste is under 6 %. No shared memory is used. The integrate pass is one thread per column doing a serial 64-slice recurrence, about 32 k threads at 1080p; it is a latency chain and not a throughput risk.

**Last measured GPU cost** (`PERFORMANCE_FIX_STATUS_2026-09-26.md`, #4793 A/B, HEAD `55dd8fa7e`, 1280×720, ray tier 3, clock 10:00): `volumetrics_inject_ms` 0.32–0.36 ms on FNV `GSProspectorSaloonInterior` and 0.52–0.80 ms on Skyrim `WhiterunBanneredMare`. Linear scaling by froxel count would put 1080p at roughly 0.8–1.8 ms; that scaling is my extrapolation and is not measured. No measurement exists for exteriors, combustion scenes, or native 4K.

## Findings

### PERF-VOL-01: Combustion light-moment atomics from every luminous froxel target host system RAM over PCIe
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline (Dim 5) / GPU Memory Pressure
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp:2741-2845` (`accumulateCombustionLightMoment`, called from `main()` at `:2967`); buffer created at `crates/renderer/src/vulkan/volumetrics/init.rs:447-459` via `GpuBuffer::create_host_readback`
- **Status**: NEW (searched open and closed issues for `combustion moment atomic`, `GpuToCpu atomicAdd`, `host-cached atomics`; #3835, #4535, #4786 and #5772 concern the CPU drain and init cleanup, not the shader-side target memory)
- **Description**: `combustionLightMoments` (binding 18) is the buffer the host later drains to turn the transported flame field into surface lights. It is allocated with `create_host_readback`, i.e. `MemoryLocation::GpuToCpu`. In gpu-allocator 0.28 that preferred-flag set is `HOST_VISIBLE | HOST_COHERENT | HOST_CACHED` (`gpu-allocator-0.28.0/src/vulkan/mod.rs:810-814`), with no `DEVICE_LOCAL`. On a discrete card that selects the cached system-RAM memory type. The inject shader then performs **8 `atomicAdd`s per luminous froxel** (`weight`, `weighted_x/y/z`, `radiant_r/g/b`, `luminous_volume`, lines 2811-2845) straight into it. Every luminous froxel of every fire hits the same 256 bins × 8 words, so the traffic is also heavily same-address contended.
- **Evidence**: `accumulateCombustionLightMoment` early-outs only when `medium.extinction <= 1e-7`, emission luma is ~0, or the froxel is outside the ±32 m / ±16 m light domain (`:2749-2766`). Inside a flame, plume or explosion every froxel passes. `create_host_readback`'s own doc (`buffer.rs:1119-1137`) says `GpuToCpu` is for host-drained data and contrasts it with BAR-resident `CpuToGpu`; nothing there considers a device-side atomic target. The occupancy mask, by contrast, is `CpuToGpu` (BAR or VRAM) and so does not have this problem.
- **Impact**: Cost scales with luminous froxel count: ~8 PCIe round trips each, 8 × N_lum per frame. A candle is a handful of froxels; a bonfire or explosion is hundreds to tens of thousands. Only combustion frames pay (`transportExpired` skips the call entirely, and a fog-only scene never reaches it). I cannot say how many milliseconds this is; PCIe atomic throughput on this part, and whether the driver coalesces same-address atomics within a warp before they leave the SM, are unmeasured. **Confidence: moderate on the mechanism (memory type and atomic count are read from code), low on the magnitude.**
- **Related**: #3835 (CPU drain gate), #4786 (known-empty transport), `docs/audits/PERFORMANCE_FIX_STATUS_2026-09-26.md` §#4784 (this is the part of the combustion cost the occupancy gate does not touch).
- **Suggested Fix**: Do not restructure on this report alone. First A/B on `--combustion-lab` (renderer-static, pinned camera, tier pinned) with `accumulateCombustionLightMoment` stubbed to `return;` and compare `volumetrics_inject_ms`; same-machine control, read the noise floor first. If it moves: accumulate per-workgroup into a shared-memory 256 × 8 table and flush only touched bins (this alone cuts global atomics by roughly the workgroup's luminous-froxel count), and/or accumulate into a `GpuOnly` 8 KB buffer and `cmd_copy_buffer` it into the readback buffer at the end of the dispatch. Either needs the existing zero/drain protocol (`combustion_moment_dirty`, `latched_drain`) kept intact.

### PERF-VOL-02: `flameSourceSampleRatio` runs for every froxel in a flame-bearing cluster even when the transported medium it scales is zero
- **Severity**: LOW
- **Dimension**: GPU Pipeline (Dim 5)
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp:2975-2983` (call), `:1040-1095` (function)
- **Status**: NEW
- **Description**: The ratio is computed unconditionally inside `if (!transportExpired)` and then multiplied into `transported_combustion.extinction/scattering/emission`. When `transportedMediumActive` is false (`extinction <= 1e-7`), `combustionMedium` has returned an all-zero `LocalMedium` (`:2660-2663`), so the product is zero whatever the ratio is. The function loops up to 64 cluster volumes and runs `evaluateLocalFogVolume` twice for each flame volume.
- **Evidence**: `localFogCluster` returns early only when the cell holds no volumes, so every froxel in a 16 m cluster that contains any flame volume pays the loop, including the empty-air majority. The cost per non-flame volume is the cheap bounds test (`profileRadius >= 1.0` exits before the two noise samples, `:935-936`), so this is a small constant, not a hotspot.
- **Impact**: Wasted ALU in candle- and torch-heavy cells while transport is armed. Not measured; likely well under the inject total. Behaviour-neutral to remove.
- **Related**: #4784 (the stencil gate this complements).
- **Suggested Fix**: Guard the call with `transportedMediumActive`; leave the ratio at 1.0 otherwise.

### PERF-VOL-03: Volumetrics' persistent CPU scratches have no telemetry row and no shrink path
- **Severity**: LOW
- **Dimension**: Telemetry (Dim 8)
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:630` (`fog_cluster_refs: Vec<FogClusterRef>`), `:617` (`combustion_light_candidates`); `crates/renderer/src/vulkan/context/mod.rs:2105-2150` (`scratch_telemetry_coverage_tests`)
- **Status**: NEW
- **Description**: `fog_cluster_refs` is cleared and refilled each frame by `build_fog_volume_clusters` and only ever grows. Its ceiling is 786,432 refs × 6 B ≈ 4.7 MB (4096 clusters × (64 density + 128 portal)); the pathological scene in "Measured here" reached 180,224 refs. `ctx.scratch` cannot show it, and `shrink_frame_scratch` does not release it. The coverage test that is supposed to catch exactly this walks for fields whose name `ends_with("_scratch")` (`mod.rs:2150`); neither field is named that way and both live on `VolumetricsPipeline`, not `VulkanContext`.
- **Evidence**: `grep fog_cluster_refs crates/renderer/src/vulkan/context/{telemetry,shrink_frame_scratch}.rs` returns nothing. `volumetric_fog_scratch` (the sibling Vec) does have a row.
- **Impact**: Observability gap and a one-way high-water mark of a few MB of host RAM after a pathological scene. No per-frame cost.
- **Suggested Fix**: Add a `ScratchRow` for each and let the coverage test key on type or on a shared marker, not only the name suffix.

### PERF-VOL-04: Fog-cluster, volume and moment-drain host-write tail has no cheap "unchanged" gate (informational, cost measured negligible)
- **Severity**: LOW
- **Dimension**: CPU Hot Paths (Dim 1)
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:836-905`; `crates/renderer/src/vulkan/volumetrics/fog_clusters.rs:492-704`
- **Status**: NEW (informational; recorded so the next audit does not re-derive it)
- **Description**: With a parked camera and unchanged volumes, every frame still resets all 4096 cluster entries, refills the 16 KB occupancy mask, re-copies the volumes, re-packs the index list and rewrites the mapped buffers. The grid origin is `camera - far`, so a moving camera legitimately invalidates it every frame; only the parked case is redundant.
- **Evidence**: See "Measured here": 2.9 µs (24 volumes), 4.9 µs (76 volumes), 48 µs (76 volumes with sealed-interior portal sweep), 115 µs (152 volumes with sweep). The sweep is the only super-linear term (slab test per candidate cluster per shaft).
- **Impact**: Negligible on a 7950X. **Do not implement a cache**: it would add an invalidation surface (sun motion, grid rebase, streaming) to save microseconds. Filed only because the dirty-tracking question was asked; the answer is that it is not worth it at the measured cost.
- **Suggested Fix**: None. Re-open only if `fog_cluster` in `cpu_ms:` exceeds ~0.2 ms on a real cell.

### PERF-VOL-05: Docs contradict the shipped default froxel divisor, and the ledger omits the integration UBO
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure (Dim 3), doc rot
- **Location**: `docs/engine/memory-budget.md:620-628` (and `:492`, `:581-604`); `crates/renderer/src/vulkan/volumetrics.rs:189-199` (`volumetrics_host_buffer_bytes`); `.claude/commands/audit-performance/SKILL.md` pass-inventory "Volumetrics" row
- **Status**: NEW
- **Description**: (a) The section's closing paragraph says "The `/4` quality default is denser … a 4× VRAM and 4× inject-dispatch change that has not been through a bench-of-record refresh", in present tense. The default has been 8 since #3611 era (`upscaling.rs:146`, with a comment explaining the retreat from 4), and the same section states 8 four times. (b) `volumetrics_host_buffer_bytes` and its ledger table omit `integration_param_buffers` (2 × 16 B `IntegrationParams` UBO, `volumetrics.rs:659`, `init.rs:739`). 32 B logical, so no budget effect, but the test `volumetrics_host_buffer_ledger_matches_the_code` cannot see it. (c) The audit SKILL's pass-inventory row says Volumetrics "Runs when `VOLUMETRIC_OUTPUT_CONSUMED`". That constant is permanently `true` (`volumetrics.rs:444`); the real gate is `requires_dispatch` plus TLAS, cluster-light and geometry inputs being present (`post_passes.rs:663-782`).
- **Evidence**: As cited. Ledger test passes (79 volumetrics tests green).
- **Impact**: Misleads a reader into thinking 4× the VRAM and dispatch is live.
- **Suggested Fix**: Rewrite the last paragraph of the section as history; add the UBO to the ledger or note it as below the allocation granularity; reword the SKILL row.

## Existing / related, not filed

- **#5365 (OPEN)** — top-of-frame all-slots fence wait, the largest measured CPU-visible stall per its own body. Not re-reported here: the volumetrics occupancy mask is `sync.rs` rider 15 (`sync.rs:107-118`), already covered by the sibling concurrency audit (`AUDIT_CONCURRENCY_2026-10-17.md`). The only perf-side addition is that #5365's body lists the nine original riders and not rider 15, so a narrowing change must migrate the mask first.
- **Concurrency audit C-3 (LOW)** — the `volumetrics_cleared_on_skip` / `caustic_cleared_on_skip` latches are committed at record time and not rolled back on submit failure. Owned by that audit; not re-reported. My "Skipped frames" note below describes the happy path only.
- Out of area, seen in passing: `assemble_camera_and_lights.rs` reads a `BYRO_TMP_JITTER_IN_DEBUG` env var every frame in the FSR-inactive branch, under a comment "TEMP noise-bisection override — remove before commit". A per-frame `getenv`; trivial cost but it is leftover debug code.

## Measured here (CPU only, scratch copy)

`fog_clusters.rs` was copied unmodified apart from stubbing its `use` header into `/tmp/audit/performance/fogbench` and timed in release (2000 iterations, camera moving each frame, 50 warm-up). 7950X, same machine as the engine.

| Scenario | Volumes | Portal sweep | µs/frame | Index refs |
|---|---:|:---:|---:|---:|
| 24 small smoke/flame volumes | 24 | off | 2.85 | 31 |
| + 40 window planes + 12 cones | 76 | off | 4.86 | 150 |
| same, sealed-interior sun sweep | 76 | on | 48.2 | 1,100 |
| 24 small + 128 window planes, sweep | 152 | on | 115 | 2,905 |
| 512 × 32 m smoke boxes | 512 | off | 52.6 | 9,090 |
| 512 grid-sized homogeneous boxes (pathological) | 512 | off | 1,312 | 180,224 |

The same bench file's sibling (`sortbench`) timed `sort_lights_by_priority_with_ids` on an already-sorted array, which `assemble_camera_and_lights` runs every frame after the combustion merge even when nothing was appended: 0.17 µs (64 lights), 0.76 µs (256), 3.1 µs (1,023). Not a finding.

## Examined, no finding

- **Allocation and hashing in the driver.** `grep` of `volumetrics.rs`, `fog_clusters.rs` and `combustion.rs` for `HashMap`, `HashSet`, `format!`, `to_string`, `.to_vec()`, `collect` returns nothing in per-frame code. All staging (`fog_volume_upload`, `fog_cluster_entries`, `fog_cluster_indices`, `fog_cluster_refs`, `fog_cluster_occupancy`) is pre-allocated and reused. `log::` calls fire only on a topology change. The BFECC env var is read once through a `OnceLock`. The FxHash rule is vacuously satisfied.
- **`collect_fog_volumes`** (`render/fog_volumes.rs`) runs on a rayon worker beside the static-mesh collector, writes into a persistent `Vec`, and uses `sort_unstable_by` with a distance comparator recomputed per comparison. At 500 volumes that is ~9 k distance evaluations (tens of µs) and off the critical path. Not worth a decorate-sort.
- **Per-frame descriptor writes.** `write_tlas`, `write_lights_and_clusters` and `write_boundary_geometry` issue 7 `vkUpdateDescriptorSets` writes every dispatched frame without checking whether the handles changed. Roughly a microsecond of driver work; only the TLAS handle plausibly changes. Not worth a cache.
- **Barriers.** Stage B uses ten single-mip GENERAL→GENERAL image barriers with SHADER_READ/WRITE masks only, one `COMPUTE→COMPUTE` stage pair, in one `vkCmdPipelineBarrier`. The inject→integrate barrier carries `FRAGMENT|TRANSFER` in its source scope for the neutral-clear case (#3647). The trailing `COMPUTE→HOST` memory barrier is unconditional but is a visibility edge with no stall. No over-broad `ALL_COMMANDS` stage.
- **Skipped frames (happy path; submit-failure rollback is concurrency C-3).** `requires_dispatch` skips both passes in a fogless frame; `skip_clear_decision` latches the neutral clear to the first skipped frame of a streak and `record_skipped_frame` still resets history on every skipped frame (#4780). `TransportFieldState` sends the expiry sentinel once per slot and `TRANSPORT_KNOWN_EMPTY_DT` afterwards, which skips transport reads and stores outright.
- **Host-write volume.** Per dispatched frame: 336 B params, up to 49 KB volumes (live prefix only), the touched entry range, the live index prefix, and the 16 KB occupancy mask. Largest realistic write is under 100 KB.
- **Composite tap.** `sampleVolumetricColumn` does 8 `texelFetch` plus 4 representative-depth fetches per pixel; the depth taps are shared by each 8×8 pixel block, so they are cache-resident.
- **Light merge.** `append_combustion_surface_lights` is gated by `combustion_moment_dirty`, so fog-only frames skip the invalidate, decode and 8 KB fill (#3835). `combustion_light_is_inside_authored_source` is candidates × volumes with a cheap sphere test; worst case 256 × 512, typical dozens.
- **Index buffers are fixed worst-case sized** (3 MiB per slot × 2) although the list has been densely packed since #4792 and a realistic scene uses a few KB. A smaller allocation would need a total-reference budget, which is a behaviour change (overflow policy), so I am not filing it. It is the dominant row of the 6.27 MiB ledger and the same BAR-pressure class as #4889 if the BAR heap is small.

## Guard verification

All run in this session against HEAD:

| Guard | Result |
|---|---|
| `cargo test -p byroredux-renderer --lib volumetrics` (79 tests) | all pass |
| `volumetrics_host_buffer_ledger_matches_the_code` (ledger = code) | pass |
| `froxel_grid_cost_matches_the_memory_budget_doc` | pass |
| `transport_occupancy_gate_is_wired_into_the_inject_pass`, `the_dispatch_empty_branch_resets_the_occupancy_mask`, `occupancy_marks_reset_between_builds`, `transported_sources_seed_the_occupancy_mask` (#4784, #5216) | pass |
| `known_empty_transport_skips_history_stores_and_source_sampling`, `transport_empty_slots_require_submitted_clears_and_rearm_for_new_sources` (#4786) | pass |
| `distant_transport_emitters_do_not_arm_grid_simulation` (#4783) | pass |
| `sun_visibility_rays_are_gated_on_a_live_sun_term`, `tier_zero_sheds_unmarked_rim_rays_but_preserves_explicit_windows` (#4785, #4793) | pass |
| `cluster_upload_range_excludes_untouched_prefix_and_clears_old_counts`, `packed_segments_tile_the_live_index_prefix_in_admission_order`, `stale_counts_clear_and_offsets_repack_across_frames` (#3834, #4788, #4792) | pass |
| `every_skipped_frame_drops_the_temporal_history_not_just_the_first` (#4780) | pass |
| `-- fog` (22), `-- gpu_timers` (17), `-- draw_frame` (7, includes the `fog_cluster` ordering/subtraction pin, #4810), `-- light_history` (8, includes the shared decorate-sort), `-- scratch_telemetry` (1) | all pass |
| #5055 `GpuLight` is 64 B and matches the inject shader's `GpuLight` struct | holds (`gpu_types.rs:374-385`, `volumetrics_inject.comp:118-125`) |

Not run: `byroredux`-binary tests for `render/fog_volumes.rs` (needs the pinned-toolchain bin build; no change there since the baseline).

## What I could not check

- **Any GPU timing.** No `volumetrics_inject_ms` at HEAD, no exterior number, no combustion number, no 4K-native number. PERF-VOL-01's magnitude is the main open question.
- **Register pressure and occupancy of the inject shader** (512-thread groups, ray queries plus the large transport function). Needs a shader compiler report or Nsight; I did not run one.
- **Whether the driver aggregates same-address `atomicAdd`s within a warp** before they reach host memory.
- **How many `FogVolume` entities a real exterior carries**, so the `collect_fog_volumes` sort and the cluster-build worst case are bounded by code caps (512) rather than by observed content.
- **Real-content `fog_cluster` ns** from a `cpu_ms:` line; the micro-bench above is a model of the function, not of the scene mix.

Next step: `/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-10-17.md`.
