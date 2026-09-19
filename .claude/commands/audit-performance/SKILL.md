---
description: "Audit GPU/CPU performance — hot paths, allocations, draw batching, GPU memory pressure, SSBO sizing, telemetry"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Performance Audit

Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` for shared protocol (layout, methodology, dedup, context rules, path-reference convention, finding format, hot-path hashing rule).

Audit ByroRedux for CPU hot-path inefficiency, per-frame allocation churn, draw/instancing waste, GPU memory pressure + eviction thrash, SSBO sizing/upload cost, pass cost and streaming stalls. **Do not re-derive memory ceilings** — `docs/engine/memory-budget.md` owns every size, threshold, reserve floor and VRAM budget; cite it.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

Hardware target: **RTX 4070 Ti (12 GB) + Ryzen 7950X (16c/32t)**; RT VRAM minimum 6 GB. A CPU bottleneck here is a **bug**, not a tuning gap — dimensions are ordered by per-frame impact.

## Parameters

- `--focus <dimensions>`: comma-separated numbers (e.g. `1,3,5`). Default all 8. `--depth shallow|deep`: pattern scan vs traced + quantified. Default `deep`.

## Extra Per-Finding Fields

- **Dimension**: CPU Hot Paths | Draw & Instancing | GPU Memory Pressure | SSBO Sizing & Upload | GPU Pipeline | Skinning & BLAS | Streaming & Cells | NIF Parse | Telemetry & Origin Cost

## Bench discipline (never hard-code numbers)

- ROADMAP.md carries the **Bench-of-record** (flagged stale; gates nothing): read it and the compat table, run the bench it describes, report **observed-vs-ROADMAP deltas** — a regression is a delta. Controls: Prospector (FNV), WhiterunBanneredMare (Skyrim), MedTekResearch01 (FO4).
- Run shape: `--bench-frames N [--bench-hold] [--bench-mode renderer-static|renderer-stepped|system-live]`. Compare only within one mode (baseline TSVs in `.claude/audit-baselines/runtime/` are renderer-static); the mode owns delta-time, so never set `BYROREDUX_FIXED_DT`. Measure the noise floor first (`scripts/bench-variability-envelope.sh`); a delta inside it is not a finding.
- **Harness byte-stability.** `scripts/fsr-bench-matrix.sh` + `scripts/fsr_bench_report.py` must stay byte-stable across commits or cross-commit comparisons mean nothing. Never assert it from memory (#4024): run `scripts/check-bench-harness-provenance.sh [--strict] [<record-commit>]` (full history required) and quote the verdict; a *DIVERGED* verdict → re-bench both sides. An edit to either file that was not itself re-benched is a finding.
- **Adaptive ray budget**: the RT quality tier follows GPU time (`AdaptiveRayBudget`), so a lighting-cost delta is uninterpretable without it — pin with `--rt-test-ray-quality-tier N` for A/B, else report the tier.
- **Hooks** (`byro-dbg` after `--bench-hold`; no `bench-stats` command exists): the once-a-second `gpu_ms:` / `cpu_ms:` lines, `MetricsSnapshot`, console `ctx.scratch`, `skin.coverage`, `rt.integrity`, `depth.stats`. Cite a timer, not a guess.

## Regression-guard posture

Each dimension lists landed fixes as `Guard:` lines — **verify the symbol/test still exists and holds; never re-propose a landed fix** (noise); an eroded guard is a real finding, reported apart from new issues. Std `HashMap`/`HashSet` on a per-frame per-entity keyspace is a fresh regression (the Fx conversion is finished). Falsified by measurement, re-propose only with new numbers: decorate-sort-undecorate for the raster sort (#4194, `manual_bench_draw_sort_decorate_sort_undecorate`, `render/draw_sort_key_tests.rs`).

**Alloc coverage**: NIF parse/import allocation is dhat-testable (`dhat-heap` feature; bounds in `crates/nif/tests/heap_allocation_bounds*.rs`) — propose a bound, don't estimate. Per-frame render/ECS paths have NO dhat coverage: flag "no quantitative guard exists for this site".

## Per-frame pass inventory (verify against code; a pass missing here is itself a finding)

Command-buffer order (`draw_frame` phases, `vulkan/context/`). Timer = `GpuTimerSnapshot` field (19 brackets); a new pass needs a bracket, a row here and a `memory-budget.md` row.

| Pass | Where | Runs when | Scales with | Timer |
|---|---|---|---|---|
| Static-BLAS recovery | `restore_missing_static_blas_for_draws` via `step_static_blas_restore` (`app_step.rs`), *between* frames | missing rigid BLAS; deadline-bounded chunks; skipped if the set cannot fit the budget | missing meshes (~0.2–0.5 ms each, blocking) | CPU only |
| Instance/material/light upload | `build_and_upload_instances.rs`, `scene_buffer/upload.rs` | every frame; hash dirty gates | live draws, unique materials | `ssbo_build` (CPU) |
| Skin palette | `skin_palette.comp` via `plan_palette_dispatch` (`dispatch_skin_and_cluster.rs`) | dirty slot runs only | changed bone slots | `skin_palette_ms` |
| Skin vertices | `skin_vertices.comp` per entity | pose-hash dirty gate | dirty entities × verts | `skin_dispatch_ms` |
| Skinned BLAS refit | `record_skinned_blas_refit` | dirty gate; rebuild after 600 + jitter refits | dirty entities; shared scratch serializes | `skin_blas_refit_ms` |
| TLAS build/refit | `build_tlas` | every frame; UPDATE vs BUILD by address sequence | instances | `tlas_build_ms` |
| Cluster cull | `cluster_cull.comp` | every frame | lights × clusters | `cluster_cull_ms` |
| Ground-cover scatter | `groundcover_interaction.comp` + `groundcover_scatter.comp` (TLAS ray query per candidate); dev bench: `groundcover_bench_ms` | chunks resident (exterior) | chunks × candidate budget | `groundcover_scatter_ms` |
| Sky-cube bake | `SkyCubePipeline::record_bake`: `sky_cube.comp` (6×128², cloud march) + prefilter mips + SH | every frame the pipeline exists (interiors too) | fixed | `sky_cube_ms` |
| Main geometry pass | `triangle.vert/frag` (RT shadows/reflection/GI/glass), water, ground-cover blades | every frame | pixels × lights × rays (adaptive tier) | `main_render_ms` |
| Depth-history copy | `copy_depth_to_history` | only if `scene_has_effect_soft_material` (cached, `SceneEffectSoftCache`) | pixels | `depth_history_copy_ms` |
| SVGF | temporal + 3 à-trous dispatches | every frame | pixels | `svgf_ms` (one bracket) |
| Caustic splat | `caustic_splat.comp` | caustic sources present | refractive pixels | `caustic_splat_ms` |
| Volumetrics | inject + integrate, combustion-moment readback (one frame behind) | `VOLUMETRIC_OUTPUT_CONSUMED` | froxels (resolution-derived) | `volumetrics_ms` |
| SSAO, composite | `ssao.comp`; `composite.frag` (linear HDR) | every frame | render pixels | `ssao_ms`, `composite_ms` |
| Bloom | down/up chain + `bloom_apply.comp` | skipped under raw debug views | pixels | `bloom_ms` |
| TAA | `taa.comp` | only `--upscaler taa` | pixels | `taa_ms` |
| Upscale, presentation | FSR 3.1 or native blit; exposure + ACES (+ UI quad) | every frame | output pixels (presentation does not shrink with FSR presets) | `upscale_ms`, `presentation_ms` |

Outside the loop: lazy blend-pipeline variant compilation (seconds each on a cold driver cache) and the streaming/teardown paths (Dim 7).

## Phase 1: Setup

1. Parse `$ARGUMENTS`. `mkdir -p /tmp/audit/performance`.
2. Dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/performance/issues.json`.
3. Read ROADMAP.md Bench-of-record + compat table, and the newest `docs/audits/AUDIT_PERFORMANCE_*.md`.
4. **Delta scoping**: run each dimension's `First step:` with `--since=<last report date>`; untouched Paths get a guard check + skim.

## Phase 2: Dimensions

### Dimension 1: CPU per-frame allocations & hot paths
The highest-yield surface: a 16-core Ryzen must never be the bottleneck.
**Paths**: `byroredux/src/systems/{animation,bounds,billboard,particle,cinematic,locomotion,sandbox,wander,travel,follow,escort,guard,patrol,walk_anim,combat_ai,restoration}.rs`, `crates/core/src/ecs/systems.rs` (`make_transform_propagation_system`), `crates/core/src/ecs/packed.rs`, `byroredux/src/render/{mod,particles,lights,static_meshes,skinned}.rs`, `crates/core/src/ecs/resources/skin_slot_pool.rs`.
**First step**: `git log --since=… --format='%h %s' -- byroredux/src/systems byroredux/src/render crates/core/src/ecs`; scheduler timings in `MetricsSnapshot` name the slow system.
**Method**: per-frame `Vec`/`HashMap`/`String` allocation that should be a persistent scratch; `collect()` where `clear()+extend` works; `mem::take` capacity churn (0→N regrowth); per-entity allocation or component deep-clone inside a per-frame loop; whole-world scans with no change-detection key; sparse-set probes repeated per entity. Ambient locomotion/AI/cinematic systems run every frame on every NPC: no component clones (NavPath and `ScenePlayer` clones were real findings), persistent scratch not fresh `Vec`s.
**Guard**: `drain_dirty_into_preserves_storage_capacity` (`packed.rs`; `take_dirty` zero-capacities — its use in a per-frame system is the regression); `make_animation_system` persistent scratches (`entities_scratch`, `playback_scratch`); `make_billboard_system` skips its loop when `last_cam` is unchanged (re-arming `GlobalTransform` `TRACK_CHANGES` every frame defeats incremental bounds); `make_transform_propagation_system` change-detection fast path (skips when roots, `Parent`/`Children` generations and `Transform` dirty are unchanged); `build_debug_ui_snapshot` clone gated on debug UI visible; `SkinSlotPool` `free_list` contraction so `max_used_slot()` shrinks after a high-NPC unload; `bone_world` not `.clear()`ed per frame (identity-fills new tail slots only); `emit_particles` takes no dead `GlobalTransform` query; `scene_has_effect_soft_material` served from `SceneEffectSoftCache` while storages are structurally unchanged.
**Output**: `/tmp/audit/performance/dim_1.md`

### Dimension 2: Draw-call & instancing efficiency
**Paths**: `byroredux/src/render/{mod,static_meshes}.rs` (`build_render_data`, `draw_sort_key`, `sort_draw_commands`), `crates/renderer/src/vulkan/context/{build_and_upload_instances,draw}.rs` (`group_state`, batching).
**First step**: `git log --since=… --format='%h %s' -- byroredux/src/render crates/renderer/src/vulkan/context/draw.rs`; `cargo test -p byroredux draw_sort_key`.
**Guard**: `render/draw_sort_key_tests.rs`, `render/sort_key_tests.rs`.
**Checklist**:
- `draw_sort_key` is a 12-tuple `(u8×4, u32×8)`: slot 0 `rt_only` (off-frustum RT occluders last, so the `MAX_INSTANCES` cap drops them before raster), 1 composition phase, 2 blend class, 3 `no_sorter` (true alpha-over only; opaque/additive write 0 — #4191), then state/mesh/depth. The opaque depth slot is `opaque_depth_bucket` (raw depth = camera-driven sort churn that defeats the SSBO dirty-gate hashes, #3663). Sorted alpha-over is depth-primary on purpose: its higher `bench_draws_*` counts are accepted cost. Every `PipelineKey` axis needs a sort slot (`blend_pipeline_slot`, `pack_depth_state`).
- Verify the sort minimizes pipeline/descriptor rebinds; draw count vs entity count (compat table reports both); same-mesh instances collapse to one indirect draw; no per-draw state churn (`cmd_set_depth_bias`).
- `DRAW_SORT_PARALLEL_THRESHOLD` = 3000 (`sort_draw_commands`; measured crossover 2750–3000). Check it against the `bench_draws_raster_cmds` column of the baseline TSVs — `bench_draws_cmds` also counts RT-only occluders that never enter the sorter; a row with `bench_draws_batches > bench_draws_raster_cmds` cannot come from one capture.
- Static-mesh loop probes `GlobalTransform` before the visibility/bounds siblings (#1377); `build_instance_map` reuses a caller scratch (#4193).
- `needs_two_sided_blend_split` = `is_blend && two_sided && order_dependent_glass` (no `z_write` limb); structurally dormant for engine-classified glass (`collect_static_mesh_draws` clears `two_sided` for `MATERIAL_KIND_GLASS`), so split items cannot explain batch-count movement (#2691); a split on non-glass batches is the regression.
**Output**: `/tmp/audit/performance/dim_2.md`

### Dimension 3: GPU memory pressure & eviction thrash
**Paths**: `crates/renderer/src/vulkan/acceleration/{predicates,blas_static,constants}.rs`, `context/resources.rs`, `crates/renderer/src/texture_registry/`, `crates/renderer/src/mesh.rs`, `byroredux/src/asset_provider/material/provider.rs`, `crates/renderer/src/deferred_destroy.rs`.
**First step**: `git log --since=… --format='%h %s' -- crates/renderer/src/vulkan/acceleration crates/renderer/src/texture_registry crates/renderer/src/vulkan/context/resources.rs`; `rt.integrity` shows BLAS residency, missing/evicted counts and pending-destroy backlog.
**Guard**: `cargo test -p byroredux-renderer acceleration` (admission/eviction/residency tests), `static_blas_recovery_covers_both_sources_and_protects_current_draws` (`resources.rs`), `static_blas_recovery_runs_between_frames_not_in_the_render_driver` (`app_step.rs`).
**Checklist** (ceilings: memory-budget.md):
- BLAS budget is **dynamic**: `blas_budget_for_heap` = (DEVICE_LOCAL heap − `screen_scaled_reservation_bytes`) / 3, clamped to [`MIN_BLAS_BUDGET_BYTES` 256 MB, `MAX_BLAS_BUDGET_BYTES` 1 GB]; the reservation bills every render-extent pass, the upscaler's output-extent images and FSR SDK bytes and is re-derived on resize/upscaler switch. Eviction runs pre- and mid-batch (`blas_over_budget` counts the batch's `pending_bytes`; `BATCH_EVICTION_CHECK_INTERVAL` = 64); admission is bounded on *resident* bytes (`admit_next_static_blas`, `blas_admission_exhausted`) — allocating past residency, or evicting against bytes still in `pending_destroy_blas`, is the regression. LRU must not rebuild-thrash during streaming.
- Static-BLAS recovery stays a deadline-bounded between-frames step; `plan_static_blas_restore` must still skip a set that cannot fit the budget (else evict→rebuild never converges — the ~95 k-draw Starfield city). Documented ceilings, quantify first: `--grid` false-evict burst, shared-scratch serialization (`/audit-renderer` Dim 1).
- Scratch high-water: `shrink_blas_scratch_to_fit` (cell unload), `shrink_tlas_to_fit` / `shrink_tlas_scratch_to_fit` (end of `draw_frame`) respect the reserve floors (`MIN_TLAS_INSTANCE_RESERVE` = `WORKING_SET_FLOOR` = 8192).
- `check_pool_growth()` warns/errors on vertex/index pool caps per upload. Texture staging is bounded by a **byte budget** (`DEFAULT_STAGING_BUDGET_BYTES`), not a texture count. BGSM/BGEM cache half-evicts (a full flush reintroduces the cold-restart herd). `NifImportRegistry` LRU (`BYRO_NIF_CACHE_MAX`, 2048).
- Deferred-destroy countdown = `MAX_FRAMES_IN_FLIGHT`; freeing earlier writes in-flight GPU memory (CRITICAL). Backlog counters must be read by `rt.integrity`.
**Output**: `/tmp/audit/performance/dim_3.md`

### Dimension 4: SSBO sizing & per-frame upload
**Paths**: `crates/renderer/src/vulkan/scene_buffer/{constants,upload,buffers}.rs`, `crates/renderer/src/vulkan/material.rs`, `byroredux/src/material_translate.rs`, `crates/core/src/ecs/components/material.rs`.
**First step**: `git log --since=… --format='%h %s' -- crates/renderer/src/vulkan/scene_buffer crates/renderer/src/vulkan/material.rs`; `ctx.scratch` on a bench cell.
**Guard**: `gpu_instance_is_160_bytes_std430_compatible`, `gpu_instance_does_not_re_expand_with_per_material_fields`; the dirty-gate tests `{instance,indirect,light,material}_hash_tests.rs`; `bone_world_slot_needs_copy` tests (`upload.rs`); `hash_gpu_material_fields_covers_every_gpu_material_field`.
**Checklist** (sizes: memory-budget.md — cite its current total; it moved when instance SSBOs became grow-on-demand):
- Upload cost is **O(live data)**, never O(capacity): instance SSBOs start at `INITIAL_INSTANCE_CAPACITY` and double to `MAX_INSTANCES`; eager `MAX_INSTANCES` allocation or a full-capacity memcpy of a near-empty buffer is the waste. `MAX_MATERIALS` 16 384; `MAX_INDIRECT_DRAWS` = `MAX_INSTANCES`. `GpuInstance` carries per-draw data only (layout drift: `/audit-renderer` Dim 3).
- **Content-hash dirty gates** skip copy + flush on unchanged lights, instances (`hash_instance_slice`, `hash_previous_model_slice`), materials and indirect draws — steady state should skip. A gate that never hits (unstable sort key or material bytes, per-frame jitter in a hashed field) or is not invalidated when buffers are replaced (grow, resize) is the finding. `hash_gpu_material_fields` is one `FxHasher::write` over `GpuMaterial::as_bytes` (#4201, ~0.17 ms at 7 k draws) — check no second build/hash per draw survives (#4442).
- Material dedup: N placements of one material → one `GpuMaterial`; report the ratio per cell (`ctx.scratch`); `MaterialTable::intern` O(1) amortized; upload O(unique) (`min(table.len(), MAX_MATERIALS)`, not `interned_count()`).
- PBR is resolved ONCE at import (`Material::resolve_pbr`); the draw loop must not call `classify_pbr_keyword` (`/audit-nifal` Dim 1).
- Bone-world upload is O(dirty slots): `upload_bone_worlds` takes `dirty_slot_offsets` from `pose_dirty` and stages `vk::BufferCopy` only for changed slots (`bone_world_slot_states`); a full-range copy is the regression.
**Output**: `/tmp/audit/performance/dim_4.md`

### Dimension 5: GPU pipeline & pass efficiency
**Paths**: the inventory table — `context/{draw,geometry_pass,post_passes,dispatch_skin_and_cluster}.rs`, `crates/renderer/shaders/`, `vulkan/{volumetrics,bloom,svgf,taa,composite,ssao,sky_cube,groundcover}.rs`.
**First step**: `git log --since=… --format='%h %s' -- crates/renderer/shaders crates/renderer/src/vulkan/context`; read `gpu_ms:` on a control bench and diff against ROADMAP.
**Checklist**:
- Every pass is O(pixels) / O(froxels) / fixed, never O(meshes). Froxels are resolution-derived (`froxel_extent`; `VolumetricsConfig::default` in `upscaling.rs`). Bloom is a pure O(pixels) pyramid (2×2-box downsample + in-place apply). SVGF = 1 temporal + 3 à-trous dispatches under one bracket.
- The sky-cube bake is a fixed per-frame cost that also runs in interiors that never sample it, and ground-cover scatter traces the TLAS per candidate: quantify with `sky_cube_ms` / `groundcover_scatter_ms` before proposing a gate (a gate interacts with the `exterior_sky_tint.w` ready flag — `/audit-renderer` Dim 2).
- TLAS rebuild-vs-refit frequency; a missing AS build→read barrier is HIGH.
- G-buffer bandwidth: 8 colour attachments × 2 FIF; the two FSR mask attachments are written even under `--upscaler taa` where nothing samples them (known cost).
- Disney ALU is in `include/pbr.glsl` — lobes must not run for fragments that never reach them. Legacy WRS arrays exist only with `ENABLE_LEGACY_WRS` (shipped 0; `NUM_RESERVOIRS` applies only then) — a shipped `triangle.frag.spv` built with it on taxes every frame. `inv_vp` is computed once on the CPU (UBO); a shader-side per-invocation `inverse()` is the regression.
- Depth-history copy runs only when `scene_has_effect_soft_material`; an every-frame copy is the regression. A perf "win" that is only an adaptive-ray-tier drop is not a win (bench discipline).
- Cold blend-variant compile is the multi-second stall: the pipeline cache must keep persisting when variants are created (`save_pipeline_cache_if_grown`).
- **Speculative-Vulkan caveat**: render-pass/barrier/pipeline findings invisible to `cargo test` need RenderDoc evidence or a revert plan — state confidence.
**Output**: `/tmp/audit/performance/dim_5.md`

### Dimension 6: Skinning & BLAS cost
**Paths**: `crates/renderer/src/vulkan/skin_compute.rs`, `shaders/{skin_palette,skin_vertices}.comp`, `acceleration/blas_skinned.rs`, `context/{skinned_blas_refit,dispatch_skin_and_cluster}.rs`, `crates/core/src/ecs/resources/skin_slot_pool.rs` (`pose_dirty`, `try_mark_pose_dirty`, `clear_pose_dirty`), `byroredux/src/render/skinned.rs`.
**First step**: `git log --since=… --format='%h %s' -- crates/renderer/src/vulkan/skin_compute.rs crates/renderer/src/vulkan/context/skinned_blas_refit.rs crates/core/src/ecs/resources/skin_slot_pool.rs`; `skin.coverage` + `skin_*_ms` quantify every dispatch-count claim.
**Guard**: `skin_dispatch_ran_rollback_scope_tests` (`app_frame.rs`), `palette_dispatch_uses_the_dirty_plan_and_rearms_late_bind_inverses` (`dispatch_skin_and_cluster.rs`), `skinned_blas_stays_fx_hashed`.
**Checklist (landed fixes — do not re-propose)**:
- The palette multiply is its own pass and dispatches only dirty slot runs (`plan_palette_dispatch`); a dense full-range dispatch every frame is the regression. `bind_inverses` is persistent, uploaded once at first sight (per-frame upload O(first-sight entities)).
- Dispatch-dirty gate: skip when the bone-pose hash (FNV-1a over the bone-world slice) is unchanged; `pose_dirty` is an `FxHashSet` on `SkinSlotPool`; the gate must never skip before `SkinSlot.has_populated_output` flips.
- BLAS refit gate: `refit_skinned_blas` skips on `built_this_frame` or (`has_populated_output && !is_dirty && has_skinned_blas`); refit dominates, rebuild only on bone-count change or after `SKINNED_BLAS_REFIT_THRESHOLD` (600) + `SKINNED_BLAS_REFIT_JITTER` (60) via `skinned_blas_refit_limit(entity_id)` — a flat shared threshold makes every entity rebuild the same frame. `SKINNED_BLAS_FLAGS` is `FAST_BUILD` on purpose (measured; memory-budget.md).
- Steady state writes 0 descriptors (`SkinSlot.descriptor_bindings[frame]` match skips `vkUpdateDescriptorSets`); a bailed `draw_frame` rolls back the pose-hash commit and requeues first-sight `bind_inverses` (`skin_dispatch_ran`, `bind_inverse_upload_failed`, `app_frame.rs`); morph deltas are shared per mesh (`morph_delta_cache`); `MorphSlot` eviction drains outside the `skin_compute` + `accel_manager` guard.
- Known ceiling, quantify first: all skinned builds/refits share one scratch and serialize (`skin.coverage`, `gpu_skin_blas_refit_ms`).
**Output**: `/tmp/audit/performance/dim_6.md`

### Dimension 7: World streaming, cell transitions & parse cost
**Paths**: `byroredux/src/{streaming,streaming_helpers,app_step}.rs`, `byroredux/src/cell_loader/{unload,transition,load,work_budget,precombined,nif_import_registry,partial}.rs`, `byroredux/src/npc_spawn.rs`, `crates/sfmaterial/src/reader.rs`, `crates/nif/src/{stream.rs,import/,blocks/}`, `crates/plugin/src/esm/records/parse.rs`.
**First step**: `git log --since=… --format='%h %s' -- byroredux/src/streaming.rs byroredux/src/cell_loader crates/nif/src/stream.rs crates/plugin/src/esm`; the `cpu_ms:` split classifies a stall (Dim 8).
**Guard**: `byroredux/src/streaming_tests.rs`; NIF dhat bounds (`crates/nif/tests/heap_allocation_bounds*.rs`).
**Checklist**:
- Boundary-cross stall. `pre_parse_cell` is two-phase (serial header extract → rayon body parse; collapsing it costs ~6–7× stream latency) with a serial fast path below `PRE_PARSE_RAYON_MIN` (8). The worker's cross-request `batch_keys` memo (`recv_next_batch_request`, `worker_batch_duplicate_skips`) prevents duplicate parses in a burst. NIF mesh/collision import runs on the **worker** (`PartialNifImport`); the main-thread drain only re-interns strings (`reintern_imported_meshes`) — import work back on the main thread is the regression.
- Exterior apply and the interior door path are budgeted: `STREAMING_APPLY_BUDGET` seeds a `FrameTimeBudget` that `load_references_budgeted` yields against (placements, NPC sub-NIFs); `InteriorCellApplyJob` keeps the cursor across frames and cancels via `unload_cell`. Boot, save-load restore and debug loads (`load_cell_with_masters`) are unbudgeted by design. NPC spawn cost: `npc_spawn_wall` in the end-of-cell log.
- Batched teardown: a crossing unloads its ring via `unload_cells` (one global `shrink_storages` + `shrink_blas_scratch_to_fit` after the last victim), despawning through `World::despawn_batch` → `PackedStorage::remove_entities_erased` (one sorted-merge pass, in-place swap+truncate); a per-entity `Vec::remove` loop is the quadratic regression. `UnloadPhaseTimings` is wired only on the `app_step.rs` batch path.
- NIF import-cache hit rate across cells; CDB parsed **once** per archive load. Parse allocation (dhat-testable — propose bounds): bulk arrays via `read_pod_vec`, `allocate_vec` results used (`#[must_use]`), block counters via `entry().get_mut()/insert` not `or_insert(name.to_string())`, skin/SSE scratches reserved up front, emitter extraction import-time only.
- ESM parse (`parse_esm_with_load_order`) is single-threaded per plugin on the first-cell path (2026-08-30 release: 1.2–1.7 s per vanilla master, `SeventySix.esm` 3.4 s). Per-plugin parallelism is possible (`FormIdRemap` needs only the header) but unimplemented; a proposal must preserve `merge_from` ordering (#3403, #3384) and add a byte-identical-`EsmIndex` test.
**Output**: `/tmp/audit/performance/dim_7.md`

### Dimension 8: Telemetry & camera-relative origin cost
**Paths**: `crates/renderer/src/vulkan/gpu_timers.rs`, `crates/core/src/ecs/resources/mod.rs` (`ScratchTelemetry`), `byroredux/src/systems/{debug,metrics}.rs`, `byroredux/src/render/camera.rs`, `crates/renderer/src/vulkan/context/{build_and_upload_instances,draw,telemetry}.rs`.
**First step**: `git log --since=… --format='%h %s' -- crates/renderer/src/vulkan/gpu_timers.rs byroredux/src/systems/debug.rs byroredux/src/systems/metrics.rs`.
**Checklist**:
- GPU timers: brackets read back via `read_and_reset` into `GpuTimerSnapshot`, `MAX_FRAMES_IN_FLIGHT` behind (never stalls); readers honour the `_active` flag (skipped pass ⇒ `_ms == 0`); the documented bracket count, `QUERIES_PER_FRAME` and the inventory table must equal the live ones (doc-count rot recurs).
- `cpu_ms:` (`log_stats_system`) splits the frame into `fence_wait` / `acquire` / `submit_present` / `ssbo_build` / `geom_rebuild` / `tlas_build` / `cmd_record` / `rof_*` / `atw_pre` / `atw_scheduler` / `atw_post` / `between_frames`. Buckets **nest** (`atw_post` ⊇ `atw_pre`/`atw_scheduler` work); only `between_frames` lies outside `render_one_frame` (compositor throttle, loop sleep). Large `fence_wait` ⇒ GPU stuck on a prior submit; large `atw_scheduler` with `fence_wait≈0` ⇒ pure CPU; large `atw_post` ⇒ cell-load upload; large `between_frames` alone ⇒ not the engine. Release builds for dense cells.
- `ScratchTelemetry` (`ctx.scratch`): capacity vs used vs wasted for renderer scratches and, after `renderer_row_count`, the engine's `build_render_data` scratches; diff for over-reserved scratch and for a scratch with no row.
- Camera-relative origin: the origin snaps to the 4096-unit cell grid (`RENDER_ORIGIN_SNAP`); `assemble_camera` adds one `look_at_rh` and `rebase_model_matrix` runs inside the existing O(visible-instances) loop — verify no second pass. On a grid crossing `origin_corrected_prev_view_proj` preserves TAA/SVGF/FSR history (#1489); dropping history per crossing is a HIGH perf+quality regression.
**Output**: `/tmp/audit/performance/dim_8.md`

## Phase 3: Merge

1. Read all `/tmp/audit/performance/dim_*.md`.
2. Combine into `docs/audits/AUDIT_PERFORMANCE_<TODAY>.md`: **Executive Summary** (findings by severity; observed-vs-ROADMAP delta, not absolute FPS) · **Hot Path Analysis** (per-frame CPU + per-pass GPU cost table from `gpu_timers` / `ScratchTelemetry`, with the RT quality tier stated) · **Findings** (CRITICAL first, deduplicated; eroded guards separate from new issues) · **Prioritized Fix Order** (quick wins — scratch reuse, preallocation, gate restoration — before architectural changes) · **Stale skill premises**.
3. Remove cross-dimension duplicates.

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/performance`. 2. Tell the user the report is ready. 3. Suggest `/audit-publish docs/audits/AUDIT_PERFORMANCE_<TODAY>.md`.
