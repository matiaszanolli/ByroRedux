**HEAD**: `a2c24b16e` · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-09-29.md` (HEAD `9fcfdc3fc`) · **Audited**: Dims 1–8 (every dimension had commits in `9fcfdc3fc..a2c24b16e`, 313 commits in total) · **Unchanged since baseline (skimmed)**: none. Dims 2, 4, 5 and 6 changed but yielded no new findings; they got a delta read and guard checks.

# Performance Audit — 2026-10-05

**Command**: `/audit-performance`, `--depth deep`, one leg of `/audit-suite --preset comprehensive`. **Mode**: static analysis plus unit-test, dhat and guard runs only. Per the suite constraints no engine, GPU process or benchmark was launched. **Every cost figure below is either derived from code or quoted from an earlier dated measurement; none was measured this run.**

## Executive Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 3 |
| LOW | 2 |

No guard eroded. All six baseline findings are fixed in code:

| Baseline finding | Fixed by |
|---|---|
| PERF-D1-2026-09-29-01 (Talk-arm locks) | #5109 (`83d52707e`) |
| PERF-D3-2026-09-29-01 (residency probe) | #5053 (`8dbe179ad`) |
| PERF-D4-2026-09-29-01 (`GpuLight.history_id`) | #5055 (`c705c310d`) |
| PERF-D5-2026-09-29-01 (early-test kinds) | #5057 (`3c197ed8c`) |
| PERF-D7-2026-09-29-01 (flaky dhat pin) | #5050 (`harness = false`) |
| PERF-D7-2026-09-29-02 (gear-loader archive set) | #5061 (`7ead491d7`) |
| PERF-D7-2026-09-29-03 (stale doc) | #5063 (`3deda6bb7`) |

The new findings are all main-thread stalls that this delta introduced or brought back into play:

- **PERF-D7-2026-10-05-01**: every Skyrim/FO4 door transition now rebuilds the archive providers twice on the main thread. The new LSCR model cover builds one set. The transition itself builds another; #2039 deferred that cost "until interactive doors ship", and they have shipped.
- **PERF-D7-2026-10-05-02**: LOD-water recentering used to translate one Transform. Since #5243/#5244 it rebuilds the mesh at every grid crossing. Each rebuild scans every worldspace cell's 33×33 heightmap and does blocking uploads.
- **PERF-D3-2026-10-05-01**: the Starfield CDB `MaterialIndex` (~2 s and ~470 MB peak per CDB, by the commit's own measurement) is built lazily on the main thread inside the material merge, including during streamed applies. `memory-budget.md` does not account for it.

**Observed vs ROADMAP.** Nothing was measured, so no bench delta is reported. The Bench-of-record is still `a37fcba3c`, 255+ commits stale (ROADMAP **R6a-stale-24**). The R6a-regress-22 residual (+0.7 ms, fence +2.6 ms, from the old pose) stands. `scripts/check-bench-harness-provenance.sh a37fcba3c` now returns **DIVERGED**, citing one commit (`8fdd9ae56`, #5128). That edit appends `camera_pos`/`camera_forward` as the last TSV columns and adds pose reporting to `fsr_bench_report.py`. By inspection it does not change what is measured, but it was not re-benched. Per bench discipline, the next comparison against the record must re-bench both sides. R6a-stale-24 already requires that re-run, so this is not filed as a separate finding.

## Hot Path Analysis

No new timings. The latest in-repo per-pass numbers are still MedTekResearch01 from `AUDIT_PERFORMANCE_2026-09-26b.md`: renderer-stepped/pan, Native AA 1280×720, **RT quality tier pinned to 0**, HEAD `b7491072f`. The baseline report reproduces that table; it is not repeated here because nothing was re-measured.

CPU-side changes since the baseline (derived from code, not measured):

- **Lower cost**:
  - Interaction Talk arm (#5109): per-root `World::get` chains became one bulk set build per storage.
  - `has_any_view_of_path` (#5053): one normalize and an in-place key rewrite instead of up to 16 Strings.
  - Mid-life gear loader (#5061): reuses the corpse loader's archives.
- **Higher cost, event-driven**: the door-transition provider rebuilds (finding D7-01) and the per-crossing LOD-water rebuild (finding D7-02).
- **Per frame, small**: the Talk arm now allocates one Vec per alias-bearing quest plus four entity sets every frame (finding D1-01).

GPU-side changes (derived):

- #4784 adds a coarse occupancy skip to `volumetrics_inject.comp`. Quiet froxels no longer pay the RK2 backtrace and history gather while transport is armed.
- #5057 moves lighting-shader kinds 1–16 onto the early-Z pipeline.
- #5192 splits the reflection and transmission lobes with no extra rays.
- The blade fragment shader adds one BTXT base fetch and skips zero-weight layers.
- No pass was added. The timer inventory holds 28 brackets (23 named + 5 main-render sub-phases) in 56 query slots, matching the skill.
- All 36 shaders and the opaque early-test variant are byte-reproducible from GLSL.
- `ENABLE_LEGACY_WRS` = 0.

## Findings

### Eroded guards

None.

### New issues

### PERF-D7-2026-10-05-01: Every Skyrim/FO4 door transition opens the full archive set twice on the main thread, once for the new LSCR model cover and once in `step_cell_transition`, and the cover re-imports its model each time
- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**:
  - `byroredux/src/loading_screen.rs:312-322` (`spawn_model_stage`) and `:191-231` (`begin_artwork`, called from `begin` and `begin_save`)
  - `byroredux/src/app_step.rs:1041-1042` (Ext→Int) and `:1141-1142` (→Ext)
  - the stale doc at `byroredux/src/app_step.rs:887-933`
  - `byroredux/src/asset_provider/texture.rs:479-535` (`build_texture_provider`)
- **Status**:
  - The cover half is NEW (`e60911864`, 2026-10-02).
  - The transition half is the cost **#2039** (PERF-D7-02, 2026-07-16) recorded. That issue was closed on 2026-07-17 as a design note: "not urgent before Stage 4 interactive door activation". Interactive doors have since shipped (`InteractionKind::Door`, `interaction.rs:1281-1286`, and the `p5-door-transition.sh` smoke), so the deferral's trigger has been met.
  - No open issue covers either half.
- **Description**: `spawn_model_stage` calls `build_texture_provider(&args)` and `build_material_provider(&args)`. That re-opens every `--bsa`/`--textures-bsa` archive (headers and file tables) and builds a cold BGSM/BGEM cache. It then extracts the NNAM model, runs `peek_or_parse_scene` and `load_nif_bytes`, which registers and uploads meshes and textures and builds the BLAS. All of this happens on the main thread, before the cover's first presented frame (`Phase::AwaitingPresentation`).
  - The model path has no key check. The image path at `:249-254` does have one, and reuses its texture when the key matches.
  - The stage is released at cover end (`retire_stage` → `release_entities`), so the next door repeats the whole spawn.
  - `step_cell_transition` then builds a second fresh provider pair for the destination apply (#2039).
  - The doc comment at `app_step.rs:890-893` still says this path is "reachable today only via the `door.teleport` console command".
- **Evidence**: `loading_screen.rs:321-322`; `app_step.rs:1002` (`self.loading_screen.begin(...)` runs ahead of the `:1041` rebuild on the same transition); `asset_provider/texture.rs:479` (no caching: every call opens archives through `open_with_numeric_siblings`).
- **Impact**: each door use pays two full archive-index opens, a model import and GPU upload, and the BLAS build, all on the main thread. The cover was added to hide the transition stall, but it delays its own first frame. #2039 estimated "a few-hundred-ms BSA re-open" per rebuild; FO4's dozens of BA2s are the heavy case. Unmeasured this run. No quantitative guard exists for this site.
- **Related**: #2039 (design note with the cache shape), #5061 (the same "loader opens its own archive set" pattern, fixed for the gear and corpse loaders), #5193 (cover teardown), `save_io.rs:1388/1543` (save-load also rebuilds, but that path is unbudgeted by design).
- **Suggested Fix**: implement #2039's App-owned provider slot, keyed on the plugin-set identity, and lend it to `LoadingScreen::begin*` and `step_cell_transition`. Key the model stage the way the image artwork is keyed, so a repeat cover reuses the registered stage instead of re-importing it.

### PERF-D7-2026-10-05-02: LOD-water recentering became a full mesh rebuild with blocking uploads at every grid crossing, re-folding every worldspace cell's 33×33 heightmap
- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**:
  - `byroredux/src/streaming.rs:989-1020` (`recenter_lod_water`), called from `byroredux/src/app_step.rs:114-129` on every `grid_changed`
  - `byroredux/src/cell_loader/water.rs:1202-1276` (`rebuild_lod_water_mesh`), `:982-997` (`distant_water_cells`), `:911` (`build_distant_water_mesh`)
  - `crates/renderer/src/mesh.rs:804-858` (`upload_scene_mesh`)
- **Status**: NEW. It arrived with `98061ec58` (#5243) and `3e258fa36` (#5244), both closed correctness fixes. Before them, recentering translated the plane's `Transform` by the grid delta.
- **Description**: at each boundary crossing, `rebuild_lod_water_mesh`:
  1. Calls `distant_water_cells(cells)`. That walks **every** exterior cell of the worldspace (`record_index.cells.exterior_cells[worldspace]`) and folds each cell's `LandscapeData.heights` (1089 f32) for `land_min`, collecting a fresh Vec. The projection depends only on session-invariant record data.
  2. Rebuilds the ring geometry.
  3. Uploads it with `upload_scene_mesh`. Its per-mesh buffers go through `create_device_local_buffer` → `with_one_time_commands`: a blocking submit and fence wait, about "2 synchronous fence-waits" per upload according to `upload_scene_mesh_global_only`'s own doc at `mesh.rs:850-858`.
  4. Appends to the global vertex/index pool and drops the previous mesh.
- **Evidence**: `water.rs:1214-1216` and `:1246-1248`; `streaming.rs:1010-1019`.
- **Impact**: main-thread work at every crossing, outside the `FrameTimeBudget` apply slices:
  - O(worldspace cells × 1089) min-folds, about 11 M f32 for Tamriel's roughly 10 k LAND cells (estimated, unmeasured);
  - one Vec of every cell;
  - about two GPU round-trip waits;
  - global-pool churn that waits for compaction.
  
  It falls on exactly the boundary-cross frame that the streaming work (#3659, #4810 and the prefetch pipeline) was built to keep flat. No quantitative guard exists for this site.
- **Related**: #5243, #5244; skill Dim 7 boundary-cross stall checklist.
- **Suggested Fix**:
  - Compute the per-cell projection (grid, effective height, `land_min`) once when the plane spawns and store it on `LodWaterPlane`.
  - Rebuild only the ring geometry per crossing, through the batched/staging-pool upload. Alternatively, upload one full-reach mesh and cut the streaming-boundary hole in the shader, since the hole radius fits in a uniform.

### PERF-D3-2026-10-05-01: The Starfield CDB `MaterialIndex` is built lazily on the main thread inside the material merge (~2 s and ~470 MB peak per CDB), unbudgeted and missing from `memory-budget.md`
- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure (CPU-side material memory) / Streaming
- **Location**: `byroredux/src/asset_provider/material/cdb.rs:102-167` (`cdb_material_index`, `lookup_cdb_material`); callers `asset_provider/material/merge.rs:482,589,700,1372`; `docs/engine/memory-budget.md:9-19`
- **Status**: NEW. It arrived with `224a19372` (#3398 Phase 2) and `18fce7e43`/`978d25c19`. #5210 covers the CDB Phase-2 *spec* rot in `nifal.md`, not cost or memory. #3398 is the feature tracker.
- **Description**: on the first `.mat` lookup that reaches a given CDB, `cdb_material_index` does three things:
  1. re-opens the archive (`Archive::open(source)`);
  2. extracts the 105 MB CDB;
  3. runs `MaterialIndex::build`. `224a19372` measured this at about 2 s and about 470 MB peak.
  
  Every caller holds `&mut MaterialProvider`, so the build runs on the main thread:
  - `merge_external_material` ← `nif_import_registry::merge_external_materials` (in `finish_partial_import`, the streamed apply);
  - `spawn/mesh_instance.rs`;
  - `scene/nif_loader.rs`.
  
  `lookup_cdb_material` walks sources `.rev()` (DLC/Creation CDBs first) and builds each index lazily. A base-only material, or a path in no CDB, first seen during a streamed apply therefore builds the next CDB inside the apply slice. `FrameTimeBudget` cannot preempt that. With the SFBGS007 near-copy (500,385 keys), two indices can end up resident. Their resident size is unmeasured: the index is std `HashMap`s of per-object `Vec`/`String` (`crates/sfmaterial/src/index.rs:120-139`). `memory-budget.md` still says production "does not retain inflated CDB blobs" and documents only the `probe_header` cache.
- **Evidence**: `cdb.rs:111-129` (build outside any worker); `cdb.rs:155-167` (lazy per-source walk).
- **Impact**:
  - A multi-second main-thread stall on the first Starfield `.mat` merge. At boot it lands on top of the ~7.7 s ESM parse, which it could overlap.
  - A further stall at whichever later streamed apply first reaches an un-built CDB.
  - Resident CPU memory missing from the budget the RT VRAM/RAM plan is checked against.
- **Related**: #3398, #5210, the `870ea1d07` concurrent-build race fix (now only a test-harness concern, since production is main-thread only), #2705.
- **Suggested Fix**: start each discovered CDB's index build on a worker at discovery time (the stream pool or a rayon task, overlapping the ESM parse) and publish it through the existing cache. Measure peak and resident bytes with the real-data gate and add a row to `memory-budget.md`.

### PERF-D1-2026-10-05-01: #5109's bulk Talk filter rebuilds the whole alias-definition table and four entity sets every frame
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/interaction.rs:1302-1331` (inside `populate_candidates`, called every frame via `interaction_system` → `select_interaction_target` → `collect_candidates`); `crates/scripting/src/scene/quest_alias.rs:960-1010` (`installed_alias_ids_by_quest`, `running_quest_bound_entities`)
- **Status**: NEW. This is the residual of a landed fix: #5109 replaced the per-root `World::get` chains with bulk set builds, as the baseline asked.
- **Description**: on every frame, ungated, `running_quest_bound_entities` calls `installed_alias_ids_by_quest`. That builds an `FxHashMap<QuestFormId, Vec<i32>>` with one Vec per installed quest. The registry is filled with every QUST in the load order (`asset_provider/script.rs:611-612`, `index.quests.values()`). It also builds a `running` set over all of them. The arm then collects, per frame:
  - an `FxHashSet` of every `ActorValues` entity;
  - a Vec of every `SceneAliasCandidate` root (every placement root);
  - the refusal sets;
  - the `withheld` set.
  
  The answer changes only on a binding refresh, a quest start or stop, a death, or an AV-membership change.
- **Evidence**: see Location.
- **Impact**: O(alias-bearing quests) small allocations plus O(placement roots + actors) set and Vec building, every frame, on the `Stage::Update` exclusive head. FO3/FNV QUST records carry no aliases, so they pay only the root-scaled part. Skyrim, FO4 and Starfield pay the quest-scaled part too. Small but permanent. No quantitative guard exists for this site.
- **Related**: #5109, #3475, #5025 (the lock-order reason the guards are taken one at a time).
- **Suggested Fix**: cache the bound set, keyed on the `SceneActorBindings` refresh generation plus a `QuestStageState` running-set generation. Then iterate the small `bound` set and probe `SceneAliasCandidate`, `ActorValues` and the refusal storages, instead of materializing every root.

### PERF-D8-2026-10-05-01: The ground-cover collect scratch's new `layout_order` Vec has no ScratchTelemetry row
- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost
- **Location**: `byroredux/src/render/groundcover.rs:272-279` (`GroundCoverCollectScratch.layout_order`, added by `226f2dd4c` / #5176); engine rows in `byroredux/src/app_events.rs:876`
- **Status**: NEW
- **Description**: the scratch is cleared, refilled with `0..slot_count` and sorted every frame (`assign_nearest_first_layout_order`). `ctx.scratch` lists its `candidates` and `emitted` siblings but not `layout_order`. By contrast, the new renderer-side light scratches (`frame_light_ids_scratch`, `light_resort_scratch`) and the engine's `light_ids` have rows.
- **Impact**: coverage only. The scratch is bounded by `GROUNDCOVER_MAX_CHUNKS` × 8 B.
- **Suggested Fix**: add `vec_row("groundcover_collect_scratch.layout_order", &gc.layout_order)` next to the existing two.

## Guard verification (all held)

| Guard | Result |
|---|---|
| Renderer lib, filters `acceleration static_blas_recovery scene_buffer skin_compute dispatch_skin gpu_timers material_tests hash_ bone_world has_any_view_of_path early_fragment light_history` | 403 passed |
| Bin (toolchain 1.96.0), filters `draw_sort_key static_blas_recovery skin_dispatch_ran bench_gpu_keys atw_bracket sort_key interaction streaming:: load_order::parallel groundcover_hasher light` | 229 passed, 3 ignored (manual benches and real-order digest) |
| NIF dhat `heap_allocation_bounds` (now `harness = false`) | 3/3 runs, 7 tests passed each |
| NIF dhat `heap_allocation_bounds_geometry` / `_import` | passed |
| `scripts/check-shader-artifacts.sh` | 36 shaders + opaque early-test variant match glslang 11:16.2.0 |
| `scripts/check-bench-harness-provenance.sh a37fcba3c` | **DIVERGED** (1 commit, `8fdd9ae56`, append-only pose columns; see Executive Summary) |

Symbol checks also held (grep and read):

- **Dim 1**:
  - `drain_dirty_into_preserves_storage_capacity`;
  - `take_dirty` appears only in tests (`billboard.rs:567+`, `systems.rs:1158+`);
  - animation `entities_scratch`/`playback_scratch`;
  - billboard `last_cam`;
  - `SceneEffectSoftCache`;
  - `SkinSlotPool.free_list`.
- **Dim 2**:
  - `DRAW_SORT_PARALLEL_THRESHOLD` = 3000;
  - `needs_two_sided_blend_split` = blend && two_sided && order_dependent_glass (`frame_params.rs:1872`);
  - `opaque_depth_bucket`;
  - `build_instance_map`;
  - the #4726 batch clamp is O(batches) and allocation-free unless it clamps.
- **Dim 3**:
  - `DEFAULT_STAGING_BUDGET_BYTES` 128 MiB;
  - `DEFAULT_COUNTDOWN = MAX_FRAMES_IN_FLIGHT`;
  - `check_pool_growth`;
  - BLAS budget clamps 256 MB / 1 GB;
  - `BATCH_EVICTION_CHECK_INTERVAL` 64;
  - `MIN_TLAS_INSTANCE_RESERVE` = `WORKING_SET_FLOOR` = 8192.
- **Dim 4**:
  - `gpu_light_is_64_bytes`;
  - `hash_light_upload` hashes the lights plus the `previous_to_current` remap, so the gate still covers the remap after #5055;
  - `upload_materials` is bounded by `min(len, MAX_MATERIALS)`;
  - no `classify_pbr_keyword` call in render paths.
- **Dim 5**:
  - depth-history copy gated (`depth_capture.rs:460-478` pin);
  - `save_pipeline_cache_if_grown` on variant creation;
  - `QUERIES_PER_FRAME` 56.
- **Dim 6**:
  - `SKINNED_BLAS_REFIT_THRESHOLD` 600 plus jitter;
  - `plan_palette_dispatch`;
  - `morph_delta_cache` is an `FxHashMap`;
  - the #5187 residency gate is O(1) per draw;
  - #5194's swap-on-success is present in `skinned_blas_refit.rs:744-764`. The commit titled "Fix #5194" (`0df2f88e0`) only touches `loot_appearance.rs`, a #5083-class mis-title rather than a perf issue.
- **Dim 7**:
  - `PRE_PARSE_RAYON_MIN` 8;
  - `STREAM_PARSE_INPUT_BYTES` 64 MiB;
  - `STAGED_BYTE_CAP` 256 MiB;
  - `unload_cells` + `UnloadPhaseTimings`;
  - `World::despawn_batch` → `remove_entities_erased`;
  - `AppearanceProviders` shared by the corpse and gear loaders (#5061).
- **Dim 8**:
  - `render/camera.rs` unchanged since the baseline;
  - the #5220 counters are zeroed after the harvest, and the harvest consumes a pipelined readback with no wait.

## Prioritized Fix Order

1. **PERF-D8-2026-10-05-01**: one line, restores `ctx.scratch` coverage.
2. **PERF-D7-2026-10-05-02**: cache the per-cell water projection on `LodWaterPlane`. That is a local change and removes the O(worldspace) fold per crossing. Moving the upload to the batched/staging path, or a shader-side hole, is the second step.
3. **PERF-D7-2026-10-05-01**: the #2039 App-owned provider slot, already designed in `app_step.rs`, plus a keyed, reusable cover stage. It removes two archive-set opens per door.
4. **PERF-D3-2026-10-05-01**: build CDB indices off-thread at discovery, then measure and document the resident bytes.
5. **PERF-D1-2026-10-05-01**: a generation-keyed bound-entity cache for the Talk arm.

The bench-of-record refresh (R6a-stale-24, now also needed because the harness diverged) remains the top **measurement** item. None of the findings above is claimed to move it, since the default scene set has no door transition, no exterior crossing and no Starfield scene.

## Stale skill premises

- **Dim 7 Paths**: they omit `byroredux/src/loading_screen.rs` (LSCR cover, new archive-set consumer) and `byroredux/src/cell_loader/water.rs` (the per-crossing LOD-water rebuild). Both carried this run's Dim 7 findings. The Guard line should also name the loading cover and `step_cell_transition` (#2039) as archive-set consumers beside the corpse and gear loaders.
- **Dim 3 Paths**: they list `asset_provider/material/provider.rs` but not `asset_provider/material/cdb.rs`, where the Phase-2 index is built and cached. `docs/engine/memory-budget.md` §"Starfield Component Database" predates Phase 2, so the skill's "cite memory-budget.md, don't re-derive" rule has no row to cite for it.
- **Bench discipline**: `check-bench-harness-provenance.sh` reports DIVERGED for `8fdd9ae56`, whose harness edit only appends output columns. The skill should say whether an append-only, measurement-neutral edit still forces a re-bench. By the letter it does; R6a-stale-24 needs the re-run anyway.
- **Phase 4**: the skill says `rm -rf /tmp/audit/performance`. The scratch files were kept for the `/audit-suite` orchestrator's reconciliation.
- **Checked and still true**:
  - the 28-bracket / 56-query inventory;
  - `DRAW_SORT_PARALLEL_THRESHOLD` = 3000;
  - the `STREAM_PARSE_INPUT_BYTES` / `PRE_PARSE_RAYON_MIN` / `STAGED_BYTE_CAP` values;
  - the `needs_two_sided_blend_split` predicate;
  - early-test eligibility 0..=`MATERIAL_KIND_MAX_LIGHTING_SHADER` (16);
  - `GpuLight` at 64 B.

Next step: `/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-10-05.md`
