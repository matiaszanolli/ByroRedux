**HEAD**: `0572bfd5a` · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-09-21.md` (HEAD `f97775ca8`, 132 commits ago). Also deduplicated against today's area-scoped `AUDIT_PERFORMANCE_2026-09-23.md` (volumetrics only, HEAD `17c01a4e5`, issues #4783–#4789). · **Audited**: Dims 1–8 (every dimension's `Paths:` had commits since the baseline) · **Unchanged since baseline (skimmed)**: none at the dimension level. Dim 6's skin files are byte-unchanged; only guards were re-run there.

# Performance Audit — 2026-09-23b (full run)

**Command**: `/audit-performance` (default scope, `--depth deep`). This is the full-dimension run; the same-day `AUDIT_PERFORMANCE_2026-09-23.md` covered only the volumetrics slice, and predates the HEAD commit `0572bfd5a` (interior godrays and sky apertures), which this run reads in full.
**Severity scale**: `.claude/commands/_audit-severity.md`.

## Method

- **Orchestration.** Eight dimension auditors ran as sub-agents, at most three at a time. Their notes are merged here; the orchestrator independently re-verified the CRITICAL, the HIGH and both eroded guards against the code before accepting them.
- **Static audit, no bench.** A `byroredux` test binary and a second coding agent were active in this repo during the run, and the project rule is not to launch the engine in parallel. Every number below is one of:
  - arithmetic from source (marked *est.*);
  - a figure a commit message or code comment records;
  - the checked-in stepped-camera TSVs: `BENCH_stepped-camera_4c9a5b36.tsv` (bench-of-record) and `BENCH_stepped-camera_cb44d99f6.tsv` (2026-09-23).
- **Harness provenance.** `scripts/check-bench-harness-provenance.sh --strict 4c9a5b36` → "harness byte-stable since this record — a re-run is a valid apples-to-apples comparison against it" (exit 0).
- **Guard tests run** (`TMPDIR=/mnt/data/tmp cargo test -j 4 …`), all green:

| Dim | Filters | Result |
|---|---|---|
| 1 | `byroredux-core --lib`: drain_dirty, sweep_contracts, change_detection ×2 | 4 pass |
| 2 | `byroredux`: `draw_sort_key sort_key` | 32 pass, 2 ignored (manual benches) |
| 3 | `byroredux-renderer --lib`: `acceleration static_blas scratch`; bin `static_blas_recovery` | 139 pass (baseline 134, +5 new tests); 1 pass |
| 4 | `byroredux-renderer --lib`: `gpu_instance hash bone_world_slot hash_gpu_material indirect` | 66 pass |
| 5 | `byroredux-renderer --lib`: volumetrics / composite / draw_frame / stale_tlas / gpu_timer / post_passes / sky_cube / svgf / host_readback, then a second targeted set | 144 pass (1 ignored) + 10 pass |
| 6 | `byroredux-renderer --lib`: `skin palette_dispatch skinned_blas refit`; bin `skin_dispatch_ran_rollback` | 87 pass; 1 pass |
| 7 | bin: `streaming unload despawn_batch pre_parse work_budget interior_cell_apply`; `byroredux-nif --features dhat-heap` the three `heap_allocation_bounds*` targets | 78 pass; 4+1+1 pass |
| 8 | renderer lib timer/origin guards; bin telemetry tests | 26 pass; 9 pass |

- **Shipped SPIR-V.** All 35 shipped `.spv` files are byte-identical to fresh `glslangValidator -V` compiles at HEAD. `ENABLE_LEGACY_WRS` is `0`.

## Executive Summary

**22 findings: 1 CRITICAL · 1 HIGH · 4 MEDIUM · 16 LOW.** Two guards eroded (#880, #3062). One existing issue grew in scope (#4610).

1. **CRITICAL, memory corruption: the #4593 fix moved the staging overrun instead of closing it** (PERF-D4-2026-09-23b-01).
   - The terrain ring now releases its previous staging buffer labelled with the *current* upload's size.
   - When exterior streaming grows the terrain-tile count, the pool hands that undersized buffer straight back.
   - `copy_nonoverlapping` then writes past the allocation into neighbouring CpuToGpu sub-allocations in release builds; debug builds hit the `release_to` assert instead.
   - The new guard test pins the defective line.
2. **HIGH, correctness: the player can't swim** in any session that has not loaded an XWCU water-current marker (PERF-D1-2026-09-23b-02).
   - Caused by the #4691 fix: a `?` that used to end an `or_else` closure now returns `None` from the whole `player_water_state`.
   - Owned by physics/gameplay; found by this audit while tracing `0e607cbac`.
3. **`0572bfd5a` (interior godrays) is the biggest cost delta of this run.**
   - **Fog-cluster upload** (PERF-D4-2026-09-23b-02): the fog-cluster record went from 40 B to 784 B, so the per-frame upload is now up to ~3.2 MB. The sun-swept portal pass runs on the CPU every frame, including at night and in open-sky scenes.
   - **Interior sun rays** (PERF-D5-2026-09-23b-02): up to 4 extra closest-hit "rim" rays per escaping interior froxel, which the adaptive ray tier can't shed. All four interior bench controls pay them.
   - **Composite and caps**: the composite pass now loops per pixel × aperture, and the per-cluster caps rose 8×.
4. **NPC spawn regressed on Oblivion/FO3/FNV** (PERF-D7-2026-09-23b-01).
   - `5570c221c` (seam blending) adds ~2.9 MB of zlib inflate per NPC, 4 hand-NIF re-parses and a 1.39 MB EGM copy, on the main thread.
   - *Est.* 5–12 ms per NPC; it erodes the #880 cache guarantee for hands.
5. **Static-BLAS recovery policy changed in a commit titled as docs** (PERF-D3-2026-09-23b-01).
   - `b9e961eeb` removed #3540's fit projection. Over-budget scenes with a moving camera now rebuild meshes every frame, up to the 16 ms deadline, where they used to decline.
   - It is not a regression of the #3540 hang, but it is unbenched.

**Observed vs ROADMAP.** There's no new bench this run. Comparing the two checked-in stepped-camera TSVs (`4c9a5b36` → `cb44d99f6`):
- Draw splits are identical in all 25 rows, and `brd_ms` went down slightly (MedTek 3.65 → 3.15 ms).
- `gpu_main` went up outside the within-record spread: Cornell TAA 8.63 → 10.48 ms, MedTek TAA 34.98 → 46.82 ms. Whiterun TAA is flat.
- ROADMAP **R6a-stale-22** already tracks this delta and deliberately neither accepts nor files it. This audit adds one candidate cause, PERF-D5-2026-09-23b-05: the sky-cube bake landed between the records and is absorbed by the `TOP_OF_PIPE` main-pass timer, and the TSV has no `gpu_sky_cube` column.
- Neither TSV includes `0572bfd5a`. That commit's GPU cost is unmeasured.

## Hot Path Analysis

The RT quality tier is not recorded in either TSV, so no lighting-cost figure below can be attributed to a tier.

**Per-frame CPU (main thread)**

| Site | Cost | Bucket it lands in | Source |
|---|---|---|---|
| Fog-cluster prefix upload | 784 B × `write_hi` clusters: ≥ 1.71 MB with any volume at the camera, up to 3.21 MB (was 87–164 KB). *est.* 0.1–0.6 ms | `cmd_record` (not `brd`) | `volumetrics.rs:193-195,1481-1488` |
| Portal sun-sweep clustering | ≤ 4,096 slab tests per LightShaft/aperture volume, every frame, day or night, open sky included | `cmd_record` | `volumetrics.rs:735-779` |
| `build_render_data` (`brd_ms`, TAA median) | MedTek 3.15 · Dugout 0.58 · Whiterun 0.35 · Prospector 0.22 ms | `rof_pre_draw` | `BENCH_stepped-camera_cb44d99f6.tsv` |
| Interior dust-floor glass scan | O(draws) at a ~480 B stride; *est.* 15–60 µs at MedTek | `rof_pre_draw` | `render/mod.rs:1306-1318` |
| Static working-set rebuilds | 2 Fx set rebuilds over the TLAS draw set; *est.* 50–140 µs at MedTek (13,038 instances) | between frames + `draw_frame` | `tlas.rs:468,548` |
| `CompositeParams` UBO | 12,800 B full write (was 496 B); built twice per interior frame | `rof_draw_call` residual | `composite.rs`, `draw.rs:1031` |
| Draw sort | raster ≤ 283 in the runtime baselines → serial, ≤ ~20 µs; MedTek branch unknown (PERF-D2-2026-09-23b-02) | `rof_pre_draw` | `render/mod.rs:1171-1180` |
| Skin palette plan (#4611) | 2 small allocations when any pose is dirty; < 1 µs | `cmd_record` | `skin_compute.rs:888-918` |
| Static-BLAS recovery, over-budget + moving camera | 0 → up to 16 ms + 1 chunk per frame (*est.*, 0.2–0.5 ms per restore) | `between_frames`/`atw_post` | `resources.rs:395,530-575` |
| HUD refresh (post-#4608) | 8.3 MB @1080p memcpy + staging copy per changed tick | — | `hud.rs:709-712` |

**Per-pass GPU** (brackets from `GpuTimerSnapshot`: 19 brackets, `QUERIES_PER_FRAME` = 38, all consistent with the skill table)

| Pass | Change since baseline | Cost evidence |
|---|---|---|
| Main render | Absorbs the sky-cube bake through its `TOP_OF_PIPE` start (PERF-D5-2026-09-23b-05) | `gpu_main` +21 % Cornell / +34 % MedTek between TSVs (R6a-stale-22) |
| Volumetrics | +1–4 closest-hit rim rays per escaping interior froxel (720p: +0.92–3.7 M rays; 1080p: +2.07–8.3 M); per-cluster loop bound 8 → 64; night interiors still trace for zero radiance | *est.*; one unsplittable bracket (#4789) |
| Composite | + pixels × marked apertures (≤ 256), + ≤ 20 depth fetches per background interior pixel | *est.* 0.1 ms (32 apertures, 1080p) → 0.7 ms (256) |
| Sky-cube bake | Interiors now bake the outdoor palette; the structure is unchanged (it was already marching clouds at coverage 0.35) | `sky_cube_ms`; not in the TSV |
| Exposure meter | Still unbracketed (#4618) | — |

**Load and streaming (main thread, budgeted)**

| Site | Cost | Source |
|---|---|---|
| NPC seam blend (Oblivion/FO3/FNV humanoids) | *est.* 5–12 ms per NPC: ≈ 2.88 MB DDS inflate + 4 hand-NIF parse/imports + 1.39 MB EGM copy + an O(boundary × 2–3k) scan | measured vanilla FO3 sizes |
| Content-sharing fingerprints | 3 SipHashes of each fresh shareable submesh: ≈ 0.44 ms per MB | scratch micro-measure (7.1 GB/s) |
| Beam matchers | ~9 small allocations per placement × sub-mesh; *est.* 2–3 ms per dense interior | `fog.rs:919,1245,1309` |
| `read_pod_vec` | +1 allocation, +1 zero-fill, +1 memcpy per bulk NIF array | `stream.rs:814-861` |

## Findings

### CRITICAL

### PERF-D4-2026-09-23b-01: The #4593 fix labels the terrain ring's previous staging buffer with the *new* upload's size; when the terrain prefix grows, the pool hands that too-small buffer straight back and the copy overruns it
- **Severity**: CRITICAL (host-side memory corruption; Vulkan spec violation)
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:1026-1072` (`upload_terrain_tiles`; the release is at `:1039`); the guard that pins the bug is `crates/renderer/src/vulkan/buffer.rs:2402`.
- **Status**: Regression of #4593. This is a new defect on the same line, introduced by its fix `59c4a01d4`, not the old footprint mislabel returning.
- **Description**:
  - `byte_size` is computed from the **current** call's `tiles.len()` (`:1026`).
  - `previous` is this frame slot's guard from an **earlier** upload, acquired at that upload's size.
  - `previous.release_to(&mut self.terrain_tile_staging_pool, byte_size)` records the old buffer in the pool with `capacity = byte_size`.
  - `StagingPool::acquire` (`buffer.rs:218-223`) returns the first free entry with `capacity >= size`. `release` inserts at `partition_point(capacity < size)`, which is ahead of any equal-capacity entry. So the next line (`:1041`, `acquire(byte_size)`) deterministically takes back the mislabelled buffer.
  - `copy_nonoverlapping(…, byte_size)` (`:1061-1065`) then writes past it:
    - if `byte_size` > the allocation footprint, the write goes into neighbouring sub-allocations of the shared CpuToGpu block, via a pointer derived from a slice of the allocation's length (UB in Rust as well);
    - the `vk::BufferCopy { size: byte_size }` exceeds `srcBuffer`'s create size in either case (the VUID-vkCmdCopyBuffer-srcOffset-00113 family #4593 set out to close);
    - debug builds panic first, on `release_to`'s `debug_assert!(capacity <= alloc.size())` (`buffer.rs:626-631`).
  - Before `59c4a01d4`, the release used `allocation.size()`: the old buffer's real footprint. That was never smaller than the buffer, only over by the rounding slack.
  - Every other `release_to` call site releases in the same scope that acquired, at the size it acquired (`buffer.rs:909,1605,1761`, `texture.rs:248,321`, `texture_registry/upload.rs:613`). Terrain is the only deferred release.
- **Evidence**: Uploads by frame slot with a growing tile prefix (`fill_terrain_tiles` uploads the live high-water prefix; dirty is set at `context/resources.rs:168,185`):
  - upload 1 → slot 0, N tiles;
  - upload 2 → slot 1, N+k tiles;
  - upload 3 → slot 0, N+2k tiles. It releases the N-tile buffer labelled `(N+2k)·160 B`, re-acquires it, and writes `(N+2k)·160 B` into an `N·160 B` (+ alignment) buffer.
- **Impact**:
  - Trigger: any multi-frame exterior stream that allocates terrain slots past the prefix (`--grid … --radius ≥ 1`, the W1 Lake Mead cell-boundary walk).
  - Release builds: silent corruption of other in-flight staging data (texture and mesh uploads sharing the block), a possible fault at the end of a mapped block, and a copy-range spec violation.
  - Debug builds: a panic on the first qualifying stream step.
- **Related**: #4593, #4512 (the overrun class), #3664 (the high-water prefix), SAFE-D2-2026-09-21-01.
- **Suggested Fix**:
  - Store the requested size next to the guard (`terrain_tile_staging_buffers: Vec<Option<(StagingGuard, vk::DeviceSize)>>`) and release the previous guard at *its own* recorded size.
  - Change the `buffer.rs:2402` pin so it forbids releasing at the current call's `byte_size`.
  - Add a unit test that drives two growing uploads into one slot against the pool's best-fit.

### HIGH

### PERF-D1-2026-09-23b-02: The player water sampler's `?` now returns from the whole function, so without a loaded XWCU marker the player never gets a water state
- **Severity**: HIGH (correctness, cross-dimension; route to `/audit-physics` / `/audit-gameplay` owners)
- **Dimension**: CPU Hot Paths (found tracing `0e607cbac`)
- **Location**: `byroredux/src/systems/character.rs:1108-1122` (`player_water_state`); test `:1650-1700`
- **Status**: NEW. It was introduced by `0e607cbac`, the fix for #4691 (itself a follow-up to #3974).
- **Description**:
  - At `f97775ca8` the marker lookup sat inside `.or_else(|| { let cq = world.query::<WaterCurrentVolume>()?; … })`, so the `?` ended only the closure.
  - #4691 rewrote it as a block expression, `let marker_flow = { let cq = world.query::<WaterCurrentVolume>()?; … };`. There the `?` propagates out of `fn player_water_state(...) -> Option<PlayerWaterState>`.
  - `World::query` returns `None` when the storage was never created (`crates/core/src/ecs/world.rs:476`, `self.storages.get(&type_id)?`). Storages are created lazily on first `insert`; the scheduler's `.reads::<WaterCurrentVolume>()` only records an access claim (`crates/core/src/ecs/access.rs:79-82`).
  - The only production insert is the REFR XWCU synth-child path (`cell_loader/references/synth_child.rs:18-36`). Nothing registers the storage at boot.
  - The only `register::<WaterCurrentVolume>` is inside the test (`character.rs:1657`), which is why the suite stays green.
- **Evidence**: `git show f97775ca8:byroredux/src/systems/character.rs` (the `?` inside the `or_else` closure) vs HEAD `:1109-1110` (the `?` in a plain block). The orchestrator re-read both, plus `World::query`, `Access::reads` and every `WaterCurrentVolume` insert site.
- **Impact**:
  - In every session that has not yet loaded an XWCU-bearing reference, `player_water_state` returns `None` for every water plane.
  - The kinematic player never swims: no buoyancy/swim mode, breath or drowning, authored water damage, or player `WaterContact`.
  - Whether the W1 smoke still passes depends on whether Lake Mead's cell set loads an XWCU ref; it has not been re-run since the commit.
  - The perf side is minor: the marker query and linear `.find` now run for every plane every frame, not just planes without a flow.
- **Related**: #4691, #3974; `docs/smoke-tests/w1-water-traversal.sh`
- **Suggested Fix**:
  - Hoist `let current_q = world.query::<WaterCurrentVolume>();` next to `flow_q` (`:1061`), and compute `marker_flow` with `current_q.as_ref().and_then(|cq| cq.iter().find(..).map(..))`.
  - Add a test that samples a plane on a world where the `WaterCurrentVolume` storage is **not** registered.

### MEDIUM

### PERF-D4-2026-09-23b-02: `0572bfd5a` widened each fog cluster from 40 B to 784 B, so the per-frame cluster upload is now ~1.7–3.2 MB. The sun-swept portal pass that fills it runs every frame, at night and in open-sky scenes too
- **Severity**: MEDIUM
- **Dimension**: SSBO Sizing & Upload (with CPU Hot Paths and GPU Pipeline angles). This merges the Dim 1, 4 and 5 reports of the same site: PERF-D1-2026-09-23b-01, PERF-D4-2026-09-23b-02 and PERF-D5-2026-09-23b-01.
- **Location**:
  - `crates/renderer/shaders/include/shader_constants.glsl:342-343` and `crates/renderer/src/shader_constants_data.rs:1287,1291`: `MAX_FOG_VOLUMES_PER_CLUSTER` 8 → 64, new `MAX_FOG_PORTALS_PER_CLUSTER` = 128.
  - `crates/renderer/src/vulkan/volumetrics.rs`:
    - `:177`: `MAX_GPU_FOG_VOLUMES` 128 → 512;
    - `:191-195`: `FOG_CLUSTER_INDEX_STRIDE` = 192;
    - `:313-318`: `GpuFogClusterEntry` 8 → 16 B;
    - `:574-655`: `fog_portal_swept_bounds`;
    - `:735-779`: the portal slab loop;
    - `:1481-1488`: the upload `[..write_hi]` entries plus `[..write_hi * FOG_CLUSTER_INDEX_STRIDE]` indices.
  - `byroredux/src/render/fog_volumes.rs:86-93`: LightShaft/SkyAperture volumes bypass the frustum test.
- **Status**: NEW. It amplifies Existing #4788 (written against `17c01a4e5`, before this commit). #4788's `[lo, hi)` range fix does not bound it, for the reasons in the last bullet of the Description.
- **Description**:
  - #3834's layout gives each cluster a permanent offset (`cluster_index * FOG_CLUSTER_INDEX_STRIDE`). The upload is therefore O(touched clusters × capacity), not O(live references).
  - **Before**: 8 B entry + 8 × 4 B indices = 40 B per cluster; 10 KB per Z layer; 160 KB for the full set (#4788's figure).
  - **Now**: 16 B + 192 × 4 B = 784 B per cluster; ~200 KB per layer; 4,096 × 784 = **3.21 MB** for the full set.
  - The grid is camera-centred, so a single volume in the camera's cluster (8,8,8) gives `write_hi = 2185`, which uploads **1.71 MB/frame**.
  - `fog_portal_swept_bounds` sweeps every LightShaft/SkyAperture source along −sun to the grid boundary. Any sun azimuth with a +Z component pushes `write_hi` toward 4,096, and a diagonal sun makes the swept AABB span up to 4,096 clusters, each slab-tested on the CPU.
  - None of these gates the sweep:
    - **frustum**: portal volumes skip it (`fog_volumes.rs:89`), so one converted beam anywhere in the cell keeps the list non-empty and the pass dispatching;
    - **radiance**: the sweep checks only that the sun direction is finite and non-zero (`:579-586`), so at night (`portal_sun` radiance 0, `render/sky.rs`) it still fills portal lists and inflates the upload;
    - **open sky**: exteriors and Show-Sky interiors run it too, although the shader reads portal lists only behind `!hasOpenSky && localSkyAperture(...)` (`volumetrics_inject.comp:2936-2937`).
  - Why #4788's fix does not help: the unit stays 784 B however few references a cluster holds (usually 1–3), and a portal sweep crosses the grid, so even a tight `[lo, hi)` spans most layers.
- **Evidence**:
  - `git show f97775ca8:crates/renderer/shaders/include/shader_constants.glsl` has `MAX_FOG_VOLUMES_PER_CLUSTER 8u` and no portal constant. The orchestrator confirmed the HEAD constants 64/128 and the 192 stride.
  - The buffers are host-visible CpuToGpu and written with `write_mapped`.
  - `memory-budget.md:889` ledgers the ~6.5 MB of residency correctly. No doc or telemetry counter records the per-frame upload bytes.
  - Base-game beam placement counts from `docs/engine/interior-godrays-status.md`: FO3 6,363, FNV 3,051, Oblivion 1,249.
- **Impact**:
  - *est.* 0.1–0.6 ms of main-thread write-combined memcpy per frame, plus µs to sub-ms of slab tests per portal.
  - Paid in essentially every interior with a hearth or a converted beam, and in exteriors with a fire in view.
  - It runs in `record_post_passes`, after the all-slots fence wait, so on GPU-bound frames the GPU idles through it (the #4606 mechanism). It is reported under `cmd_record`, not `brd` (PERF-D8-2026-09-23b-01).
  - No quantitative guard exists for this site.
- **Related**: #4788, #3834, #4606, #4783; PERF-D8-2026-09-23b-01
- **Suggested Fix**:
  - Compact the cluster lists to CSR (count pass, prefix-sum, fill) and upload `entries[lo..hi]` plus the live index count only. This also subsumes #4788.
  - Skip the portal sweep when portal radiance is zero or the scene has open sky.
  - Replace the unconditional frustum bypass for LightShaft volumes with a reach cull.
  - Pin the upload size with a unit test (one camera-cluster volume ≤ N KB), and add an upload-bytes counter to the volumetrics telemetry.

### PERF-D5-2026-09-23b-02: In sealed interiors, every daytime froxel whose sun ray escapes now pays up to 4 extra closest-hit "architecture rim" rays. They are recomputed every frame, the ray tier can't shed them, and their scattering gate always passes because of the forced dust floor
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**:
  - `crates/renderer/shaders/volumetrics_inject.comp`:
    - `:316-338` `traceArchitectureDistance` (opaque flag only; `while (rayQueryProceedEXT(rq)) {}` is a closest-hit traversal);
    - `:351-380` `hasArchitectureRimAroundSkyRay` (4 probes, 192 BU apart);
    - `:2927-2962`: the sun block;
    - `:2955-2957`: the gate.
  - `crates/renderer/src/vulkan/context/post_passes.rs:584-595`: the interior dust floor.
  - `byroredux/src/render/mod.rs:1306`: `with_sky_aperture_dust_floor`.
  - `byroredux/src/render/sky.rs:73-98,122-160`: the portal sun.
- **Status**: NEW (`0572bfd5a`)
- **Description**:
  - **Before**: a sealed-interior froxel traced one terminate-on-first opaque sun ray, then on a miss one glass ray.
  - **Now**: on an opaque miss with no authored aperture and no architectural glass, it runs up to 4 closest-hit traversals out to the grid far plane, each followed by a `rigidBoundaryNormal` vertex fetch.
  - Opaque misses are the common interior case: Bethesda interiors routinely omit ceilings (per the code comment and `interior-godrays-status.md`). A missing ceiling costs +1 ray per froxel (probe 1 misses and exits); partial shells cost 2–4.
  - The gate doesn't bound it:
    - `dot(scattering_coef,1) > 1e-7` is always true in interiors, because the dust floor forces a uniform medium;
    - `sun_color > 1e-5` holds from sunrise to sunset.
  - The verdict depends only on froxel position, sun direction and static architecture, yet it is recomputed for ~1–2 M froxels every frame.
  - Only `volumetric_light_cap` follows `AdaptiveRayBudget`, so the controller can't shed these rays.
- **Evidence**: The shader header's own worst-case count rose from 10 to 14 rays per froxel (`:19-28`). *est.* extra rays per frame:

  | Render extent | Froxels | Missing ceiling (+1) | Partial rims (+4) |
  |---|---:|---:|---:|
  | 720p | 921,600 | +0.92 M | +3.7 M |
  | 1080p | 2.07 M | +2.07 M | +8.3 M |

- **Impact**:
  - A new GPU cost in daytime interiors that no tier drop can remove. All four interior bench controls (Prospector, BanneredMare, MedTek, Dugout) are affected.
  - It is invisible inside the single `volumetrics_ms` bracket (#4789, worsened).
  - It also worsens #4785: interiors now take a zero-radiance sun at night, yet still trace the opaque ray, the aperture scan and the glass ray.
- **Related**: #4785, #4789, #4787, #2509
- **Suggested Fix**:
  - Wrap the whole interior sun block in a warp-uniform `sun_color.rgb > 0` test (this also closes the interior half of #4785).
  - Amortise the rim verdict per coarse block or per fog cluster, rebuilt only when the sun moves or the grid re-centres.
  - Skip unmarked-opening detection at ray tier 0.
  - Measure noon against midnight in a missing-ceiling interior at a pinned `--rt-test-ray-quality-tier`.
- **Confidence**: High on structure and gating; low-to-medium on magnitude (it depends on the per-cell escape fraction).

### PERF-D7-2026-09-23b-01: Seam blending (`5570c221c`) adds ~2.9 MB of archive inflate and four hand-NIF re-parses to every Oblivion/FO3/FNV NPC spawn on the main thread, with nothing reused across NPCs
- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells (NPC spawn)
- **Location**:
  - `byroredux/src/npc_spawn/seam_blend.rs`: `:84-118` (`ToneSampler`), `:281-307` (brute-force match), `:365-385` (fade).
  - `byroredux/src/npc_spawn/resumable.rs`: `:846-852`, `:863-913` (hand hook), `:1313-1336` (head hook), `:1510-1560` (`build_seam_context`), `:1708-1720` (`yup_egm_morphs`).
  - `byroredux/src/scene/nif_loader.rs`: `:201-214` (`peek_or_parse_scene`), `:440-471` (the hook bypass; the comment at `:446-447` is now false).
- **Status**: NEW. Partial Regression of #880 (the hand-NIF cache hit; see Eroded guards).
- **Description**: For every Oblivion or FO3/FNV humanoid (`resumable.rs:677`):
  - **(a) Skin textures are re-extracted per part.** Each part builds its own `ToneSampler`, whose whole-DDS cache dies with the part, and `TextureProvider::extract` has no cache.
    - Vanilla FO3 sizes: head + body-skin neighbour 1.40 MB; each hand + neighbour 0.74 MB; ≈ **2.88 MB per NPC**. The same bytes repeat for every NPC of that race and gender.
    - `Fallout - Textures.bsa` has flags 0x107, so these are zlib inflates.
  - **(b) Hands lost the import cache.** The hook path parses and imports without inserting into `SceneImportCache`, and `peek_or_parse_scene` only *peeks*. Each 86.7 KB hand NIF is therefore parsed and imported twice per NPC: 4 parses, where it used to be 0 after the first NPC. Peek-miss parses are also missing from the cache's `parses` counter.
  - **(c) Matching is brute force.** Head boundary vertices × ~2,000–3,000 neighbour vertices, with no spatial index.
  - **(d) The EGM is copied whole.** `yup_egm_morphs` makes a Y-up copy of the full EGM per NPC: 80 morphs × 1,449 verts × 12 B = 1.39 MB in 82 allocations, although the EGM is shared per head mesh.
- **Evidence**:
  - `resumable.rs:873` creates `ToneSampler::new(tex_provider)` per hand part.
  - `nif_loader.rs:206-213`: peek, then `extract_mesh` + `parse_import_and_merge`, with no insert. The orchestrator confirmed this.
- **Impact**:
  - *est.* 5–12 ms per NPC in release, inside `FrameTimeBudget` units. A Megaton-scale cell (~30 humanoids) adds ~150–350 ms of apply work, about 10–20 more budgeted frames.
  - Boot, save-load and debug loads are unbudgeted and take all of it in one frame. Debug builds are several times worse.
- **Related**: #880 / CELL-PERF-02, #4617
- **Suggested Fix**:
  - Add a shared per-texture tone cache resource (the 256-wide mip is enough).
  - Have the hook path clone the cached `Arc<ImportedScene>`, inserting on a miss, and make `peek_or_parse_scene` insert too.
  - Convert the EGM in place, or cache it Y-up per head path.
  - Put a bucketed grid over the neighbour vertices.
  - Pin "a second NPC with the same hand path performs 0 hand parses".

### PERF-D3-2026-09-23b-01: #3540's fit projection was removed in a docs-titled commit; over-budget scenes now rebuild-churn every frame of camera travel where they used to decline
- **Severity**: MEDIUM (measure first)
- **Dimension**: GPU Memory Pressure
- **Location**:
  - `crates/renderer/src/vulkan/acceleration/predicates.rs:1105-1131` (`plan_static_blas_restore`);
  - `acceleration/static_working_set.rs:1-28`;
  - `context/resources.rs:424-475,530-575`;
  - `acceleration/blas_static.rs:86-112,1258-1270`;
  - `docs/engine/memory-budget.md:596-601`.
- **Status**: NEW. This is a policy change to #3540, not a regression of its hang.
- **Description**: `b9e961eeb` ("Enhance physical lighting documentation…") also rewrote the recovery policy.
  - **Before**: the planner projected `mean_resident_size × visible` and declined the whole set when the projection exceeded the budget.
  - **After**: it declines only when `required_resident_bytes >= budget` (`:1127`). Convergence now comes from `StaticBlasWorkingSet`, which protects the upcoming TLAS handles from `evict_unused_blas`. The doc comment explains the intent: a large unused mesh should no longer strand a small shadow set.
  - A static camera converges.
  - An over-budget scene with a moving camera is where it changes:
    - the TLAS set is distance-gated (`render/static_meshes.rs:461-466`), so meshes leave the ring every frame and become evictable after `MIN_IDLE_FRAMES`;
    - meshes re-entering the ring are missing, and required residency is now below budget, so the planner runs;
    - each restore evicts the just-departed meshes, up to the 16 ms deadline plus one guaranteed chunk.
  - The old code declined this regime outright. No bench accompanies the change.
- **Evidence**: the orchestrator re-read `plan_static_blas_restore` at HEAD:
  ```rust
  if required_resident_bytes >= budget_bytes { return 0; }
  missing.min(per_frame_cap)
  ```
- **Impact**:
  - *est.* 0 → up to ~16 ms per frame of between-frames rebuild while travelling in over-budget scenes (FO4 downtown, Starfield `citycydoniamainlevel`, or any GPU clamped to the 256 MB floor).
  - In exchange, RT completeness improves. There is no VRAM or correctness risk, since admission still bounds residency.
  - `memory-budget.md:600` still documents the removed "Fit projection".
- **Related**: #3540, #4180, #4196; AUDIT_CONCURRENCY_2026-09-23 (stale TLAS vs working set)
- **Suggested Fix**:
  - Bench old against new on an over-budget cell with a moving camera (`override_blas_budget_for_test` set low).
  - If churn shows, add hysteresis: don't re-admit a handle evicted within N frames while at budget.
  - Update `memory-budget.md` § per-frame BLAS recovery.

### LOW

### PERF-D7-2026-09-23b-02: `read_pod_vec_from` now zero-fills a scratch buffer and copies every bulk NIF array twice, the opposite of its "#3062 win kept" claim
- **Severity**: LOW
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/stream.rs:814-861`; stale comment `:805-813`
- **Status**: Regression of #3062 (introduced by `3dc127d0b`, the #4594 soundness fix)
- **Description**:
  - Every bulk array (points, vec2/3/4, u16/u32/i16/f32, triangles, colours, BSGeometry weights and meshlets, header block tables) now goes through four steps: `vec![0u8; byte_count]`, a `read` into it, a `copy_nonoverlapping` into `out`, then a free.
  - Compared with #3062's single copy, that is one more allocation, one more zero-fill and one more memcpy, and transient peak bytes double.
  - The comment says the fix copies "from that slice directly". It doesn't.
- **Evidence**: orchestrator-verified at `stream.rs:824-860`.
- **Impact**: *est.* tens of µs per 100 KB of geometry on the stream worker, and on the main thread for the NPC hook-path parses (PERF-D7-2026-09-23b-01). The dhat bounds assert peak bytes on tiny fixtures and are blind to it.
- **Related**: #3062, #4594
- **Suggested Fix**:
  - Both instantiations are `Cursor<&[u8]>`, so take that type directly: bounds-check `get_ref()[pos..pos+byte_count]`, `copy_nonoverlapping` into spare capacity, `set_len`, `set_position`. This is sound with one copy. `bytemuck::pod_collect_to_vec` is an alternative.
  - Add an allocation-count bound.

### PERF-D1-2026-09-23b-03: `WaterSurfaceMesh::surface_y_at` scans every surface triangle linearly, per frame for the camera and player and per dynamic body per physics step
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**:
  - `crates/core/src/ecs/components/water.rs:599-661`;
  - callers `byroredux/src/systems/water.rs:198-205`, `systems/character.rs:1062-1071`, `crates/physics/src/water.rs:880-885`;
  - producer `byroredux/src/material_translate.rs:437-475`.
- **Status**: NEW (`17c01a4e5`)
- **Description**: The whole placed mesh's world-space triangles are attached, and each query runs a barycentric test over all of them. There is no XZ grid or BVH, and the AABB reject passes everything over the footprint.
- **Impact**: (camera + player + bodies in footprint) × triangles, per frame or per step. Stream-mesh triangle counts are uncensused, so there is no estimate.
- **Suggested Fix**: Build a coarse XZ grid (cell → triangle indices) at component construction.

### PERF-D1-2026-09-23b-05: #4607's fix left one fresh per-frame Vec and the redundant `wanted` set in `GroundCoverResidency::reconcile`
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/render/groundcover.rs:118-203` (the return `.collect()` at `:190-201`; `wanted` at `:90,127-128`)
- **Status**: NEW (a residual of CLOSED #4607, not a regression)
- **Description**: A ~5 KB `Vec` is allocated every exterior frame. `wanted` duplicates `desired.contains_key`, adding ~135 inserts and ~135 lookups per frame.
- **Suggested Fix**: Return into a persistent field, and replace `wanted` with `desired.contains_key`.

### PERF-D2-2026-09-23b-01: The sky-aperture dust floor scans the whole draw list every interior frame to find architecture glass
- **Severity**: LOW
- **Dimension**: Draw & Instancing (per-frame CPU in `build_render_data`). Merges PERF-D1-2026-09-23b-04.
- **Location**: `byroredux/src/render/mod.rs:1306-1319` (`0572bfd5a`)
- **Status**: NEW
- **Description**:
  - `draw_commands.iter().any(|d| d.material_kind == MATERIAL_KIND_GLASS && d.render_layer == Architecture)` walks the whole post-sort list at a ~480 B stride.
  - It runs on every interior frame without a fog override, sky exposure or light-shaft volume.
  - The answer is effectively cell-static: glass is never TLAS-excluded.
  - `collect_static_mesh_draws` already reads both fields.
- **Impact**: *est.* 15–60 µs/frame at MedTek (13,545 commands), 1–15 µs in typical interiors, 0 on exteriors.
- **Suggested Fix**: Return `saw_architecture_glass` from `collect_static_mesh_draws`, or cache the flag per cell.

### PERF-D2-2026-09-23b-02: The bench-of-record harness drops `bench_draws_raster_cmds`, so no capture shows which sort branch the only >3000-command scene takes
- **Severity**: LOW
- **Dimension**: Draw & Instancing (threshold verification)
- **Location**: `scripts/fsr-bench-matrix.sh:265`; emitter `byroredux/src/app_events.rs:1154`
- **Status**: NEW
- **Description**:
  - The runtime baselines carry the column but peak at 283.
  - MedTek (13,545 cmds, 1,281 batches) is the only checked-in scene above 3,000. The stepped-camera TSVs record only `draws=N/Mb/Kc`, so its raster count lies somewhere in [1,281, 13,545], on either side of `DRAW_SORT_PARALLEL_THRESHOLD`.
  - The difference at stake is ~0.15 ms at N=5k and ~0.56 ms at N=10k (the in-code table).
- **Suggested Fix**: At the next deliberate harness bump, capture `bench_draws_raster_cmds` into a column, then re-bench both sides.

### PERF-D2-2026-09-23b-03: The #4580 comment says two-sided intent rides on `PipelineKey`; it is dynamic cull state, deliberately not a key axis
- **Severity**: LOW (doc rot on a batching invariant)
- **Dimension**: Draw & Instancing
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:598-600` (`f1fadbf9f`)
- **Status**: NEW
- **Description**: Two-sided is not a `PipelineKey` axis (#930; `pipeline.rs:114-123`). It is a `group_state` batch-merge axis and sort slot 5. The comment invites an unnecessary pipeline variant or sort slot.
- **Suggested Fix**: Reword to "…applied through dynamic `cmd_set_cull_mode` (#930; a `group_state` axis, not a `PipelineKey` axis)."

### PERF-D3-2026-09-23b-02: Census-only mesh provenance is recorded on every upload and never pruned on free
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure (host-side retention). Merges PERF-D7-2026-09-23b-04.
- **Location**:
  - `crates/renderer/src/mesh.rs:372-382` (field), `:986-1006` (`note_mesh_provenance`), `:1071-1110` (`release_mesh_ref`, no `remove`), `:1178` (cleared only in `destroy_all`);
  - consumer `mesh/geometry_residency.rs:116,225-228` (gated on `BYROREDUX_GEOMETRY_CENSUS=1`).
- **Status**: NEW (`ff1b48d7c`)
- **Description**:
  - Seven production sites write an owned label into a std `HashMap<u32, MeshProvenance>` on every upload; the cell path uses `format!` + `to_owned`.
  - Handles are never reused, and `release_mesh_ref` frees the GPU buffers but leaves the entry.
  - The census reads only live handles.
  - The sibling `geometry_cache` *is* pruned on free, which shows the intended pattern.
- **Impact**: *est.* ~150 B of host RAM per mesh ever uploaded; tens of MB over a long exterior soak. Nothing per frame, nothing on the GPU.
- **Suggested Fix**: Remove the entry in `release_mesh_ref`'s last-holder arm, or record provenance only when the census env var is set.

### PERF-D3-2026-09-23b-03: Content-shared geometry hashes each fresh shareable submesh's full vertex and index bytes three times
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure (the cost lands on cell load)
- **Location**: `crates/renderer/src/mesh/geometry_sharing.rs:12-18,51,82`; `byroredux/src/cell_loader/spawn/mesh_instance.rs:767,775,842`; `byroredux/src/scene/nif_loader.rs:1179,1217`
- **Status**: NEW (`b9e961eeb`, extended to the NPC path by `ff1b48d7c`)
- **Description**: The same bytes are SipHashed three times: in `acquire_matching_scene_mesh`, in `fresh_by_content.entry(...)`, and in `register_scene_geometry_for_sharing` (twice on the NPC path). `same_geometry` verifies every candidate anyway, so one fingerprint would do.
- **Impact**: *est.* 0.44 ms per MB of fresh shareable geometry on the main thread (~9 ms for a cell with 20 MB of fresh geometry). The sharing itself is a memory win.
- **Suggested Fix**: Compute the fingerprint once and thread it through; optionally use a faster byte hasher.

### PERF-D3-2026-09-23b-04: The static working set rebuilds a hash set twice per frame over the TLAS draw set, where a dense handle-indexed stamp would do
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:468,548`; `blas_static.rs:105-112`; `static_working_set.rs:19-27`
- **Status**: NEW (`b9e961eeb`)
- **Description**:
  - Two rebuilds run each frame: `mark_static_blas_used` → `replace(handles)`, and `build_tlas_instances` → `clear()` plus one `insert` per eligible rigid draw.
  - The set is Fx, so this is not a #2923 regression. But mesh handles are dense slot ids.
- **Impact**: *est.* 50–140 µs/frame at MedTek (13,038 instances).
- **Suggested Fix**: Use a `Vec<u32>` generation stamp indexed by handle, advanced once per real frame.

### PERF-D4-2026-09-23b-03: `CompositeParams` grew from 496 B to 12,800 B, is written in full every frame, and is built twice per frame in interiors
- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**:
  - `crates/renderer/src/vulkan/composite.rs:1280-1287` (`upload_params` → a full-struct `write_mapped`);
  - `context/draw.rs:807-825` (a zero-initialised 12 KB aperture array per build) and `:1031-1044` (`build_sky_cube_params` calls `build_composite_params` a second time);
  - `byroredux/src/render/sky.rs:160` (`portal_outdoor_sky: Some(Box::new(..))`, one heap allocation per interior frame).
- **Status**: NEW (`0572bfd5a`)
- **Description**: The full 12.8 KB is written even with 0 apertures, although `write_mapped_prefix` exists. Interiors build a second full `CompositeParams` only to read its sky fields.
- **Impact**: ~25–40 KB of stack and write-combined traffic plus one heap allocation per interior frame: µs-scale. No quantitative guard exists for this site.
- **Suggested Fix**: Upload a prefix over the populated aperture count; derive `SkyCubeParams` from the outdoor `SkyParams` directly; hold the outdoor palette by value or in a persistent resource.

### PERF-D5-2026-09-23b-03: The composite sky-aperture mask costs render pixels × marked apertures with no screen-space cull, and every interior runs the opening depth probe on its background pixels
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/composite.frag:420-440` (`boundedInteriorOpening`, ≤ 20 `texelFetch`), `:448-474` (≤ 256-aperture loop, surface pixels included), `:518-525`; `context/draw.rs:801-837` (packing, no cull); `render/fog_volumes.rs:89`
- **Status**: NEW (`0572bfd5a`)
- **Description**:
  - Each pixel loops over every uploaded aperture, off-screen ones included, with 2 quaternion rotations plus a plane intersection each. The first rotation is uniform per aperture but recomputed per pixel.
  - The pass-inventory invariant is "O(pixels), never O(scene content)".
  - Every interior (`sky_lower.w = 2` is always set in interiors) runs the depth probe on background pixels.
- **Impact**: *est.* 0.1 ms (32 apertures, 1080p) to 0.7 ms (the 256 cap). Today only the Nellis converter emits marked apertures, so the trigger is narrow.
- **Suggested Fix**: Frustum-cull apertures, precompute each one's camera-local origin and clip-space rect on the CPU, and reject pixels outside the rect before any rotation.

### PERF-D5-2026-09-23b-04: The per-cluster density cap rose 8× (8 → 64) and the volume cap 4× (128 → 512) with no cost characterisation
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `shaders/include/shader_constants.glsl:342`; `volumetrics.rs:177`; the loops at `volumetrics_inject.comp:879` (`sampleLocalMedium`), `:942`, `:1145` (`applyCombustionSources`), `:751`
- **Status**: NEW (worsens #4787, #4784)
- **Description**:
  - Each admitted volume costs a profile evaluation with 2 noise fetches per sample, and transported volumes are still fully evaluated before being discarded (#4787), now up to 64 per cluster.
  - Off-screen shafts bypass the frustum and occupy density slots.
- **Impact**: Up to 8× the per-froxel loop work in dense fire or beam clusters. The ray tier can't reduce it, and it is unmeasured.
- **Suggested Fix**: Land #4787's reorder first; keep low-density LightShaft candidates out of the density list; record max `count`/`portal_count` in telemetry so the caps can be sized from data.

### PERF-D5-2026-09-23b-05: `gpu_main` absorbs the sky-cube bake: the main-render timer starts at `TOP_OF_PIPE` right after `record_bake`, and the bench TSV does not extract `sky_cube_ms`
- **Severity**: LOW (attribution; confirmed independently by the Dim 8 auditor)
- **Dimension**: Telemetry & Origin Cost
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:694-708`; `context/geometry_pass.rs:33`; `context/build_and_upload_instances.rs:969-979` (the bake), `:731-738` (the controller input `max(main_render_ms, volumetrics_ms)`); `scripts/fsr-bench-matrix.sh:207,270-272`
- **Status**: NEW (the main-pass analogue of #4789; #2040 caveat class)
- **Description**:
  - A `TOP_OF_PIPE` start doesn't wait for prior compute, so the bake's drain lands in `main_render_ms` and is double-counted against `sky_cube_ms` (END at `BOTTOM_OF_PIPE`).
  - The adaptive ray-budget controller consumes the inflated value.
  - The bake landed between the two bench records (2026-09-13…14).
  - The TSV extracts 8 of the 19 brackets, and not `gpu_sky_cube`, although the `bench:` line prints it.
- **Impact**: Doesn't make the engine slower, but makes `gpu_main` and the controller input unattributable across records. It is a candidate partial cause of R6a-stale-22.
- **Suggested Fix**: Write the main-render START at a stage that waits for the bake. Add `gpu_sky_cube`, `gpu_tlas`, `gpu_cluster_cull` and the RT tier to the TSV, then re-bench both sides, since the harness changes.
- **Confidence**: Medium; `TOP_OF_PIPE` timestamp placement is implementation-defined, and this is not verified on NVIDIA.

### PERF-D7-2026-09-23b-03: Beam-card matchers run per placement × sub-mesh on the main thread, allocating ~6 Strings each to recompute a per-model result
- **Severity**: LOW
- **Dimension**: Streaming & Cells (placement spawn)
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:702-730` (called per REFR from `cell_loader/spawn.rs:773`); `byroredux/src/fog.rs:919,1245,1309,1380`; pre-existing eager `fog_semantics` at `mesh_instance.rs:563-565` (3 lowercases feeding only a `log::debug!`)
- **Status**: NEW (`0572bfd5a`)
- **Description**: `prepare_mesh_uploads` runs inside the budgeted per-REFR loop on the main thread. Three of the seven matchers allocate a `replace` and a `to_ascii_lowercase` before they can reject. The verdict depends only on the model path.
- **Impact**: *est.* 2–3 ms per 2–3k-placement interior, load-time only.
- **Suggested Fix**: Classify once per unique model when the `CachedNifImport` is filled; gate `fog_semantics` behind `log_enabled!(Debug)`.

### PERF-D8-2026-09-23b-01: Three `draw_frame` CPU spans fall outside every `cpu_ms:` sub-bucket or into the wrong one; the lazy blend-pipeline compile is the largest
- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost
- **Location**:
  - `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:802` (`ssbo_build` closes) → `:808-897` (blend-variant compile + `save_pipeline_cache_if_grown`) and `:900-1189` (UBOs, composite ×2, terrain re-upload), before `cmd_t0` (`draw.rs:2109`);
  - `draw.rs:2109-2312` (`cmd_record` contains `build_fog_volume_clusters` + the fog upload);
  - `draw.rs:2523+` (post-present shrinks);
  - bucket docs `crates/core/src/ecs/resources/mod.rs:820,870-874`.
- **Status**: NEW
- **Description**:
  - The multi-second first-sight blend compile (the code comment records "26 s single frames") lands only in the `rof_draw_call` residual, which the doc tells readers to treat as driver or host wait.
  - The fog-cluster build and upload (PERF-D4-2026-09-23b-02) reads as command recording.
  - #4767 budgeted `draw_frame`'s *line count*, not its time. The extracted shrinks still sit in the undocumented residual.
- **Impact**: Hitch triage blames the driver for a pipeline compile. No runtime cost.
- **Related**: #4208
- **Suggested Fix**: Add `pipeline_compile_ms` and `fog_cluster_ms` sub-buckets (and log each variant compile with its duration), and rewrite the `cmd_record_ms` / `rof_draw_call_ms` docs.

### PERF-D8-2026-09-23b-02: `GpuTimerSnapshot::composite_ms` documents a pre-#4202 / pre-#2796 composite
- **Severity**: LOW (doc)
- **Dimension**: Telemetry & Origin Cost
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:203-205`
- **Status**: NEW
- **Description**:
  - The doc says composite writes "into the swapchain image with ACES tone-mapping" with bloom as input. In fact composite writes linear HDR; bloom runs after it (#2796); tone mapping (ACES|AgX) is in presentation.
  - The new aperture mask (PERF-D5-2026-09-23b-03) is now also part of `composite_ms`.
- **Suggested Fix**: Rewrite the doc, and fold in the stale "exposure + ACES" presentation label tracked under #4618.

### Eroded guards (reported apart from new issues)

| Guard | Eroded by | Where it is reported |
|---|---|---|
| #880: NPC skeleton/body/**hand** NIFs hit `SceneImportCache` after the first NPC. The comment at `nif_loader.rs:446-447` is now false | `5570c221c` (seam-blend `pre_spawn_hook` on hands; `peek_or_parse_scene` never inserts) | PERF-D7-2026-09-23b-01 (b) |
| #3062: `read_pod_vec` does a single copy with no pre-fill | `3dc127d0b` (#4594) | PERF-D7-2026-09-23b-02 |

Every other `Guard:` named by the skill is intact:

| Dim | Guards checked | Result |
|---|---|---|
| 1 | `drain_dirty_into` (`take_dirty` is test-only), animation/billboard scratches, propagation fast path, debug-UI gate, `SkinSlotPool` contraction, `bone_world` no-clear, `emit_particles`, `SceneEffectSoftCache`, #2923 Fx in `render/` | intact |
| 2 | 12-tuple key, #4191 slot 3, #3663 bucket, #4192 `blend_pipeline_slot`, raster-prefix sort @3000, #1377, #4193, dormant split predicate, state-on-change | pass |
| 3 | budget floor/ceiling, pre/mid-batch eviction, residency-bounded admission, between-frames deadline recovery, reserve floors after the #4767 move, pool growth, 128 MiB staging, half-eviction, NIF LRU 2048, `DEFAULT_COUNTDOWN` = FIF, `rt.integrity` backlog | intact. The "skip a set that cannot fit" premise changed by design; see PERF-D3-2026-09-23b-01 |
| 4 | `GpuInstance` 160 B and no re-expansion, hash-gate tests, grow invalidates both slot hashes, O(live) upload, O(unique) materials, single hash per draw, no draw-loop PBR classification | pass |
| 5 | `ENABLE_LEGACY_WRS 0` + `.spv` recompile match, CPU `inv_vp`, depth-history gate, `save_pipeline_cache_if_grown`, AS build→read barrier, #4779 TLAS gate, #4602 flush edge, froxel extent | intact |
| 6 | dirty palette plan + late bind-inverse re-arm, FX-hashed skinned BLAS, refit 600 + 60 jitter, rollback scope (now `skin_state_submitted`), 0 steady-state descriptor writes, one pipeline bind, `morph_delta_cache` | pass |
| 7 | streaming/unload suite, two-phase pre-parse, batch memo, worker-side import, budgeted exterior and interior apply, batched teardown, CDB probe-only | pass (except the two above) |
| 8 | 19 brackets = 38 queries = table, `_active` readers, `between_frames` ordering, `RENDER_ORIGIN_SNAP` 4096, single-pass rebase, `origin_corrected_prev_view_proj` (#1489) | intact. The godray/aperture rebase is consistent |

### Prior report (2026-09-21) and open issues

| Issue | State at HEAD |
|---|---|
| #4607 (ground-cover std hashing + allocations) | Closed, **mostly fixed** by `c010c9fb9`; residual in PERF-D1-2026-09-23b-05 |
| #4608 (HUD full-frame copy) | Closed, **partly fixed**: the allocation and staging churn are gone, but a full-frame 8.3 MB memcpy per changed tick remains (`hud.rs:709-712`; tracked with #3429) |
| #4606 (all-slots fence wait idles the GPU) | Closed for its **doc half only**. The mechanism is unchanged (`sync_and_acquire_frame.rs:81`), and the throughput cost now has **no open issue**. PERF-D4-2026-09-23b-02 adds CPU work inside that GPU-idle window |
| #4609, #4612, #4614, #4611, #4615, #4616, #4617, #4618, #4208, #4209, #3813, #3142, #3477 | Open, unchanged |
| #4613 (P2 combat per-frame allocations) | Open and **slightly worse**: `fc825a6cd` added a third fresh `Vec` (`pending`) to `combat_ai` |
| #4610 (scratches with no `ScratchTelemetry` row) | Open, **scope grew** (PERF-D8-2026-09-23b-03): #4607 added 8 persistent ground-cover scratches with no row, making 14 missing rows. Publish as a comment on #4610, not a new issue |
| #4783–#4789 (today's volumetrics run) | Not re-filed. `0572bfd5a` makes #4785, #4787, #4788 and #4789 worse and #4784 slightly worse (see the findings above); #4783 and #4786 are unaffected |
| #2764, #4722, #4726 | Open; re-confirmed at HEAD by Dim 2 |

### Cross-audit notes (not scored here)
- **NIFAL**: the comment at `byroredux/src/helpers.rs:115-149` says the lit-carrier gate reads `from_bgsm`, but the code reads `external_material_resolved` (from `d3e043d3e`). Route to `/audit-nifal`.
- **Physics**: `c9843d5d1` (#4572) allocates two std `HashSet`s per frame in ragdoll writeback while a ragdoll is active (`ragdoll.rs:555-556`). This is not on the render/skinning path, so it is not a #2923 violation.
- **Fog-cluster host arrays**: 3 MiB of host RAM (was 128 KiB) that `memory-budget.md` doesn't mention; its new row counts only the GPU side.

## Prioritized Fix Order

**Tier 0: correctness, fix now**
1. PERF-D4-2026-09-23b-01: record each terrain staging guard's own size and fix the pin test. This is a one-struct change, and it is memory corruption on every growing exterior stream.
2. PERF-D1-2026-09-23b-02: hoist the `WaterCurrentVolume` query out of the per-plane block. One line plus a test; the player can't swim without it.

**Tier 1: quick, local wins**
3. PERF-D4-2026-09-23b-02 (cheap half): skip the portal sweep when portal radiance is 0 or the scene has open sky; replace the frustum bypass with a reach cull.
4. PERF-D5-2026-09-23b-02 (cheap half): a warp-uniform `sun_color > 0` gate around the whole interior sun block. This also closes the interior half of #4785.
5. PERF-D7-2026-09-23b-01 (b): make the NPC hook path clone and insert into `SceneImportCache`. That restores the #880 guard; add the "second NPC performs 0 hand parses" pin.
6. PERF-D7-2026-09-23b-02: restore the single-copy `read_pod_vec` from the `Cursor` slice.
7. Small hygiene items: PERF-D2-01, D1-05, D3-02, D3-03, D4-03, D7-03, D2-03, D8-02.

**Tier 2: architectural; measure first**
8. PERF-D4-2026-09-23b-02 (full): CSR-compacted fog-cluster lists, which subsumes #4788.
9. PERF-D5-2026-09-23b-02 (full): amortise the rim verdict per cluster or tile, and tie it to the ray tier.
10. PERF-D7-2026-09-23b-01 (a, c, d): a shared tone cache resource, the EGM converted once, a spatial grid for seam matching.
11. PERF-D3-2026-09-23b-01: bench the new BLAS restore policy on an over-budget moving camera; add hysteresis if it churns.
12. PERF-D5-2026-09-23b-05 + PERF-D2-2026-09-23b-02: one deliberate harness bump (add the `gpu_sky_cube`, `gpu_tlas`, `gpu_cluster_cull`, raster-cmds and RT-tier columns; move the main-render START stage), then re-bench both sides. That also answers R6a-stale-22.
13. PERF-D8-2026-09-23b-01: `pipeline_compile_ms` / `fog_cluster_ms` sub-buckets.
14. PERF-D5-03, D5-04, D1-03, D3-04 when their scenes become common.

## Capture items for the runtime audit

1. **Terrain staging overrun (debug build)**: `--grid 0,0 --radius 3`, or the W1 cell-boundary walk. Expect the `release_to` debug_assert. A release build under `BYRO_VALIDATION=1` should report a vkCmdCopyBuffer src-range VUID.
2. **Swim regression**: a water cell with no XWCU references. Confirm `WaterContact` never populates for the player, and re-run `docs/smoke-tests/w1-water-traversal.sh`.
3. **Godray cost**: rebench `cb44d99f6` against `0572bfd5a` on the four interior controls at a pinned `--rt-test-ray-quality-tier`. Record:
   - `volumetrics_ms`, `composite_ms`, `sky_cube_ms`;
   - `cpu_ms` `cmd_record` (not `brd`) for the fog-cluster upload;
   - noon against midnight in a missing-ceiling interior.
4. **Fog-cluster instrumentation**: `write_hi`, uploaded bytes, LightShaft count, and max `count`/`portal_count` per frame on FNV Nellis hangar and an FO3 beam-fan interior.
5. **NPC spawn**: FO3 Megaton, before and after `5570c221c`. Record `npc_spawn_wall`, the apply-frame count, and the `SceneImportCache` parse counters plus the total `parse_import_and_merge` calls.
6. **Over-budget BLAS churn**: FO4 downtown or Starfield `citycydoniamainlevel` (or `override_blas_budget_for_test` set low) with a moving camera. Log per-frame "Restored N/M missing static shadow BLAS … in X ms", before and after `b9e961eeb`.
7. **R6a-stale-22**: a same-machine control `4c9a5b36` against HEAD, with a `gpu_sky_cube` column and the tier recorded. As a one-off A/B, move the main-render START stage.
8. **MedTek `bench:` line**: `bench_draws_raster_cmds` with `BYRO_PROFILE=1` `sort=` ms, to settle which side of the 3000 threshold MedTek is on.
9. **Blend-compile attribution**: a cold driver cache plus new blend combos. Confirm the SLOW FRAME line shows the hitch only as `rof_draw_call`.
10. **Mesh water**: `WaterSurfaceMesh` triangle counts at load for Markarth and FO4 mesh-water cells, and the per-frame submersion/buoyancy cost.
11. **Provenance retention**: a long `grid-soak` with `BYROREDUX_GEOMETRY_CENSUS=1`; RSS trend against live `meshes`.

## Stale skill premises (for the next `/audit-performance` sync)

1. **Pass inventory**:
   - It has no **exposure-meter** row (`record_exposure_meter_pass`, `post_passes.rs:330`, unbracketed: #4618). The "Upscale, presentation" row still says "exposure + ACES", but AgX exists.
   - The **Sky-cube** row's "interiors … never sample it" (in Dim 5's checklist) is now false: interiors bake the outdoor palette, and `triangle.frag`'s window-portal escape samples it. Gating the bake off in interiors is no longer a valid proposal.
   - **Volumetrics** "scales with froxels" should add: portal candidates per cluster (≤ 128), up to 4 rim probes per escaping interior froxel, and a 14-ray worst case.
   - **Composite** "scales with render pixels" should add: × marked apertures (≤ 256) in interiors; `CompositeParams` is 12.8 KB.
2. **Dim 1 Paths and First step** omit the sites that produced the baseline's and this run's findings: `byroredux/src/render/{groundcover,fog_volumes,sky}.rs`, `app_frame.rs`, `hud.rs`, `objectives.rs`, `systems/{combat_anim,combat_ai,character,water}.rs`. There is no Guard entry for the #4607 ground-cover scratches. The CPU cluster builders in `volumetrics.rs` are covered by no dimension.
3. **Dim 2**:
   - Paths omit `context/geometry_pass.rs`, where the binds, depth bias and indirect merge live, and the First-step log misses `build_and_upload_instances.rs`.
   - "`bench_draws_raster_cmds` in the baseline TSVs" is true only of `.claude/audit-baselines/runtime/` (max 283); the bench-of-record TSVs don't carry it.
   - The in-code threshold note at `render/mod.rs:1165` still says the key is 11-tuple (it's 12).
4. **Dim 3**:
   - "`plan_static_blas_restore` must still skip a set that cannot fit" no longer describes the code; name `StaticBlasWorkingSet` + `required_static_blas_bytes`.
   - Paths omit `acceleration/static_working_set.rs`, `mesh/geometry_sharing.rs` and `mesh/geometry_residency.rs`, and should mention `context/shrink_frame_scratch.rs`.
   - The guard test count is now 139.
   - `TemplateCache` evicts one entry at a time (not by halves).
5. **Dim 4**:
   - Paths omit `volumetrics.rs` and `composite.rs`, which now carry the largest per-frame uploads.
   - The `bone_world_slot_needs_copy` filter selects 0 tests (use `bone_world_slot`), and `hash_gpu_material_fields_covers_every_gpu_material_field` lives in `scene_buffer/shader_contract_tests.rs`.
   - It should cite #4615 as the open residency waste.
6. **Dim 5**:
   - It has no `Guard:` line. Suggested: `ENABLE_LEGACY_WRS 0` + `.spv` recompile match, the depth-history gate, `save_pipeline_cache_if_grown`, `compute_ray_query_passes_take_the_build_gated_tlas`, `draw_frame_stays_within_its_line_budget`, `host_readback_flush_edge_tests`.
   - Bench discipline names `gpu_ms` hooks, but the TSV extracts only 8 of the 19 brackets.
   - Add a checklist item: the all-slots fence wait's throughput cost (#4606 closed doc-only) has no open issue.
7. **Dim 6**:
   - Since #3991 the rollback reads `ctx.skin_state_submitted`, not `skin_dispatch_ran`, and `has_populated_output` is promoted only after submit.
   - The guard cites the test module name rather than the test.
   - The First-step log omits `blas_skinned.rs`, `dispatch_skin_and_cluster.rs` and `render/skinned.rs`.
8. **Dim 7**:
   - "Bulk arrays via `read_pod_vec`" should say "single copy, no scratch".
   - Add a checklist item for NPC `pre_spawn_hook` cache bypass and per-NPC texture/EGM work repeated across NPCs of one race.
9. **Dim 8**:
   - The checklist line "Buckets **nest** (`atw_post` ⊇ `atw_pre`/`atw_scheduler` work)" repeats the wrong contract #4208 describes. The three `atw_*` brackets are disjoint siblings, and `atw_post` ⊇ the `rof_*` buckets.
   - The bucket list omits the `rof_draw_call` residual's contents (blend compile, UBO uploads, post-present shrinks), and that `cmd_record` now contains the fog-cluster build and upload.
10. **Brief**: the skill still names `4c9a5b36` as the bench-of-record; a newer `BENCH_stepped-camera_cb44d99f6.tsv` exists (the ROADMAP's R6a-stale-22 context).

## Process notes
- Dimension scratch notes and test logs were in `/tmp/audit/performance/` during the run and are removed per Phase 4. Every finding above carries its evidence inline.
- Merged duplicates:
  - PERF-D1-01, D4-02 and D5-01 → **PERF-D4-2026-09-23b-02**;
  - D1-04 → **D2-01**;
  - D7-04 → **D3-02**;
  - D8-03 → comment on **#4610**.
- Orchestrator re-verification: D4-01 (pool best-fit and insertion order, and all `release_to` sites), D1-02 (`World::query`, `Access::reads`, every insert site), D3-01 (the HEAD planner), D7-01 (b) and D7-02 (both eroded guards), and the fog-cluster constants.
