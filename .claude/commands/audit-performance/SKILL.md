---
description: "Audit GPU/CPU performance — hot paths, allocations, draw batching, GPU memory pressure, SSBO sizing, telemetry"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Performance Audit

Audit ByroRedux for CPU hot-path inefficiency, per-frame allocation churn,
draw-call/instancing waste, GPU memory pressure + eviction thrash, SSBO
sizing, and pipeline overhead.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology,
deduplication, context rules, path-reference convention, and finding format.
See `.claude/commands/_audit-severity.md` for the severity scale.
**Do not re-derive memory ceilings** — `docs/engine/memory-budget.md` is the
authoritative source for every SSBO size, LRU threshold, reserve floor,
deferred-destroy depth, and VRAM budget cited below. Cite it; don't transcribe.

Hardware target: **RTX 4070 Ti (12 GB) + Ryzen 7950X (16c/32t)**. RT VRAM
minimum is 6 GB. A CPU bottleneck on this machine is a **bug**, not a tuning
gap — the dimensions are ordered by per-frame impact accordingly.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3,5`). Default: all 9.
- `--depth shallow|deep`: `shallow` = pattern scan only; `deep` = trace hot paths and quantify. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: CPU Hot Paths | Draw & Instancing | GPU Memory Pressure | SSBO Sizing & Upload | GPU Pipeline | Skinning & BLAS | Streaming & Cells | NIF Parse | Telemetry & Origin Cost

## Bench-of-Record (do NOT hardcode numbers)

ROADMAP.md carries the **Bench-of-record** block (currently `R6a-stale-*`,
flagged in ROADMAP itself as ~100+ commits stale and not gating). **Never
copy FPS / frame-time / fence numbers into this skill** — they rot every
session and ROADMAP marks them stale. Instead: read the current ROADMAP
Bench-of-record + per-game compat table, run the bench described there, and
report **observed-vs-ROADMAP deltas**. A regression is a delta, not an
absolute. The canonical control benches named in ROADMAP are Prospector
(FNV, glass-heavy interior), WhiterunBanneredMare (Skyrim, steady-state),
and MedTekResearch01 (FO4, CSG-precombine-heavy).

## Regression-Guard Posture

The Session 46 perf batch (#1371–#1379) and Session 47 precision work
(#1489–#1498) **already landed**. Treat their invariants as **regression
guards to verify still hold**, not as findings to re-propose. A finding that
re-proposes a landed fix is noise; a finding that the guard has eroded is real.
Each dimension lists its guards inline — confirm the cited symbol is present
before reporting anything in that area.

**dhat alloc-bound coverage is now wired** (#1381). `dhat` is a workspace dep
(`Cargo.toml`); the `byroredux` binary has a `dhat-heap` feature
(`byroredux/Cargo.toml`) installing the profiling allocator, and the NIF crate
ships quantitative bounds at `crates/nif/tests/heap_allocation_bounds.rs` +
`crates/nif/tests/heap_allocation_bounds_geometry.rs`. So allocation findings
on the NIF parse path are now *testable* — propose a bound, don't estimate.
The **gap** that remains: per-frame render/ECS hot paths have NO dhat coverage
(the profiler is a process singleton; the live engine loop is smoke-test
territory). Allocation findings on render/ECS hot paths must still flag
"no quantitative guard exists for this site."

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--depth`
2. `mkdir -p /tmp/audit/performance`
3. Dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/performance/issues.json`
4. Read the current ROADMAP.md Bench-of-record block + compat table
5. Scan `docs/audits/` for prior performance reports

## Phase 2: Launch Dimension Agents

### Dimension 1: CPU Per-Frame Allocations & Hot Paths
The highest-impact surface: a 16-core Ryzen should never be the bottleneck.
**Entry points**: `byroredux/src/systems/animation.rs` (`animation_system`, `transform_propagation_system`), `byroredux/src/systems/bounds.rs` (world-bound propagation), `byroredux/src/systems/billboard.rs`, `byroredux/src/systems/particle.rs` (`apply_emitter_params`), `byroredux/src/render/mod.rs` (`build_render_data`), `crates/core/src/ecs/packed.rs` (dirty-set drain), `crates/core/src/ecs/resources.rs` (`SkinSlotPool` bone pool)
**Checklist**: Per-frame `Vec`/`HashMap`/`String` allocations that should reuse a persistent scratch; `collect()` into a fresh Vec where `clear()+extend` would do; capacity churn from `mem::take`; per-entity allocation inside a per-frame loop.
**Session 46 guards (verify intact, do NOT re-propose)**:
- `PackedStorage::drain_dirty_into(&mut self, out: &mut Vec<EntityId>)` (#1371, `crates/core/src/ecs/packed.rs`) drains into a caller-supplied buffer via `Vec::append`, preserving `self.dirty` capacity. `take_dirty` (the `mem::take` path) zero-capacities and forces 0→N regrowth every frame — its use in `transform_propagation_system` / world-bound propagation is the regression pattern. Guard test: `drain_dirty_into_preserves_storage_capacity`.
- `make_animation_system()` (#1372) captures two persistent scratch Vecs (`entities_scratch`, `playback_scratch`) reused via clear+extend; `make_billboard_system()` (#1374) captures `last_cam` and skips the whole `get_mut` loop when the camera hasn't moved (re-arming `GlobalTransform`'s `TRACK_CHANGES` dirty set every frame was defeating incremental bounds). Re-introducing per-frame `collect()` / unconditional billboard writes is the regression.
- `build_debug_ui_snapshot` deep-clone (2× BTreeMap + Vec<String>) is gated on `debug_ui … visible` (#1376) — boot-default hidden = ~0 cost. A path that clones unconditionally regresses normal play.
- Bone pool `next_slot` contracts after the idle-slot sweep (#1379, `SkinSlotPool` in `crates/core/src/ecs/resources.rs`): `free_list` is sorted and tail-popped while the largest free slot == `next_slot-1`, so `max_used_slot()` (the bone_world copy size + skin dispatch count) shrinks when a high-NPC scene unloads to a low-NPC one. A sweep that never contracts the issued range regresses steady-state cost after cell unload.
**Output**: `/tmp/audit/performance/dim_1.md`

### Dimension 2: Draw-Call & Instancing Efficiency
**Entry points**: `byroredux/src/render/mod.rs` (`build_render_data` @272, `draw_sort_key` @187), `byroredux/src/render/static_meshes.rs` (draw enumeration), `crates/renderer/src/vulkan/context/draw.rs` (the per-instance `GpuInstance` build loop ~1792)
**Checklist**: Sort-key effectiveness — `draw_sort_key` returns a 10-tuple `(u8,u8,u8,u8,u32×6)` ordering by pipeline/material/texture before depth; verify it actually minimizes pipeline + descriptor-set rebinds in the draw loop. Draw-call-count vs entity-count ratio (the compat table reports both per cell). Instanced batching — same mesh + multiple transforms should collapse, not emit N draws. Push-constant / per-draw state churn (`cmd_set_depth_bias` on every draw?). The parallel-sort gate: `draw_commands` switches to `par_sort_unstable_by_key` only at `>= 2000` commands (`render/mod.rs` ~398); verify the threshold matches the typical Bethesda cell range (400–1500) — a too-low gate pays rayon spin-up on small cells, too-high starves big exterior grids. **GT-presence hoist guard (#1377)**: the static-mesh loop does `if tq.get(entity).is_none() { continue; }` before the vis/wb sibling probes — dropping the hoist re-pays two SparseSet gets per GT-less entity.
**Output**: `/tmp/audit/performance/dim_2.md`

### Dimension 3: GPU Memory Pressure & Eviction Thrash
**Entry points**: `crates/renderer/src/vulkan/acceleration/predicates.rs` (`compute_blas_budget` @547), `crates/renderer/src/vulkan/acceleration/blas_static.rs` (LRU eviction), `crates/renderer/src/texture_registry.rs`, `crates/renderer/src/mesh.rs` (vertex/index pool caps), `byroredux/src/asset_provider/material.rs` (BGSM/BGEM cache), `crates/renderer/src/deferred_destroy.rs`
**Checklist** (ceilings: `docs/engine/memory-budget.md`): BLAS budget is **dynamic** — `device_local_bytes / 3` floored at `MIN_BLAS_BUDGET_BYTES` (256 MB) via `blas_budget_bytes`; eviction runs pre-batch + mid-batch at 90% budget, check interval `BATCH_EVICTION_CHECK_INTERVAL` = 64 builds. **Do NOT cite a static "1 GB" figure — that's stale doc.** Verify LRU (smallest last-used tick = victim) evicts smoothly during streaming with no rebuild thrash. Scratch high-water: `shrink_blas_scratch_to_fit` / `shrink_tlas_to_fit` reclaim at cell-unload with `BLAS_REBUILD_SLACK_BYTES`/`TLAS_*_SLACK_BYTES` headroom — verify they don't shrink below the reserve floors (`MIN_TLAS_INSTANCE_RESERVE` / `WORKING_SET_FLOOR` = 8192). `MeshRegistry` soft/hard caps (`VERTEX_POOL_*`, `INDEX_POOL_*`) emit warn/error via `check_pool_growth()` per upload — confirm caps fire, not silent overrun. BGSM/BGEM cache uses **half-eviction** (#1430, drop oldest N/2 via insertion-order `VecDeque`), NOT full flush — a full-flush path reintroduces the cold-restart thundering herd. `NifImportRegistry` 2048-entry LRU (`BYRO_NIF_CACHE_MAX`) bounds scene count. Deferred-destroy countdown = `MAX_FRAMES_IN_FLIGHT` (2); freeing earlier writes in-flight GPU memory (CRITICAL).
**Output**: `/tmp/audit/performance/dim_3.md`

### Dimension 4: SSBO Sizing & Per-Frame Upload
**Entry points**: `crates/renderer/src/vulkan/scene_buffer/constants.rs` (all `MAX_*`), `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`upload_instances` @471, lights/camera/bones/materials/indirect), `crates/renderer/src/vulkan/material.rs` (MaterialBuffer dedup), `byroredux/src/material_translate.rs` (`translate_material` @73), `crates/core/src/ecs/components/material.rs` (`resolve_pbr` @638)
**Checklist** (sizes: `docs/engine/memory-budget.md`): Scene SSBOs are resident, double-buffered (≈140 MB total). `MAX_INSTANCES` = `0x40000` (262 144); `MAX_INDIRECT_DRAWS` is aliased to it; `MAX_MATERIALS` = 16 384. Per-frame upload size must be **O(live data)** not O(capacity) — a full-capacity memcpy of a near-empty buffer is the waste. Host-visible mapped writes: flush correctness + no redundant re-upload of unchanged data. Material dedup ratio — N placements of one material → 1 `GpuMaterial`; report dedup hit-rate per cell; `MaterialTable::intern` must be O(1) amortized; upload size O(unique materials) not O(draws).
**R1 + NIFAL guards (verify, do NOT re-propose)**:
- `GpuInstance` is 112 B (down from ~400 B legacy) carrying per-DRAW data only; per-material fields read through `materials[material_id]` in the shader. Guards: `gpu_instance_is_112_bytes_std430_compatible`, `gpu_instance_field_offsets_match_shader_contract`, `gpu_instance_does_not_re_expand_with_per_material_fields` in `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`. A drift here is HIGH (shader-contract lockstep).
- PBR resolved ONCE at import: `Material::metalness`/`roughness` are plain `f32` (not `Option` + per-draw classify), populated by `Material::resolve_pbr` after `translate_material`. The draw loop must NOT re-enter `classify_pbr_keyword` (`crates/core/src/ecs/components/material.rs` @449) per draw. Regression pattern: per-draw keyword scan, or reading metalness/roughness as `Option`. See `/audit-nifal` for the single-boundary contract.
**Output**: `/tmp/audit/performance/dim_4.md`

### Dimension 5: GPU Pipeline & Pass Efficiency
**Entry points**: `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`), `crates/renderer/shaders/triangle.frag`, `crates/renderer/src/vulkan/volumetrics.rs` (M55), `crates/renderer/src/vulkan/bloom.rs` (M58), `crates/renderer/src/vulkan/svgf.rs`, `crates/renderer/src/vulkan/taa.rs`, `crates/renderer/src/vulkan/composite.rs`, `crates/renderer/src/vulkan/ssao.rs`
**Checklist**: Per-pass dispatches must be O(pixels) / O(froxels), never O(meshes). Volumetrics froxel grid (`volumetrics.rs`: 160×90×128, inject+integrate, single-ray TLAS shadow per froxel) must not scale with mesh count. Bloom pyramid (`bloom.rs`: 5 down + 4 up mips, 4-tap bilinear box) pure O(pixels). TLAS rebuild-vs-refit frequency + AS barrier placement (missing build→read barrier is HIGH). G-buffer bandwidth (6 attachments × 2 FIF; #1583/#1590 retired the ReSTIR reservoir attachment, 7→6). Disney BSDF lobe ALU now lives in `crates/renderer/shaders/include/pbr.glsl` (split out of `triangle.frag`) — verify lobes aren't evaluated for fragments that never reach them. Streaming-RIS shadow reservoir loop count — verify `NUM_RESERVOIRS` in `crates/renderer/shaders/include/lighting.glsl` against its `GpuLight` cost; a bumped count doubles divergence. `inv_vp` is computed once on the CPU and passed via UBO (cluster cull + SSAO read it) — a shader-side per-invocation `inverse()` (~100 ALU) is the regression. **Speculative-Vulkan caveat**: render-pass / barrier / pipeline-state findings whose failure mode is invisible to `cargo test` need RenderDoc evidence or a revert plan, not speculation — flag confidence explicitly.
**Output**: `/tmp/audit/performance/dim_5.md`

### Dimension 6: Skinning & BLAS Cost (M29.x)
**Entry points**: `crates/renderer/src/vulkan/skin_compute.rs`, `crates/renderer/shaders/skin_palette.comp`, `crates/renderer/shaders/skin_vertices.comp`, `crates/renderer/src/vulkan/acceleration/blas_skinned.rs` (refit), `crates/core/src/ecs/resources.rs` (`SkinSlotPool` @654: `pose_dirty` HashSet @688, `try_mark_pose_dirty` @913, `clear_pose_dirty` @929), `byroredux/src/render/skinned.rs` (per-frame mark/clear call sites)
**Checklist + guards (verify, do NOT re-propose)**:
- Bone-palette multiply runs as a dedicated compute pass (`SkinPaletteComputePipeline`, M29.5), not inline per skinned-vertex. `bind_inverses` is a **persistent** SSBO with a per-entity slot pool (M29.6) — uploaded ONCE at first-sight; per-frame upload size must be O(first-sight entities), not O(all skinned).
- **Dispatch-dirty gate (#1195)**: skin dispatch is skipped when the bone-pose hash (FNV-1a over the bone-world slice) matches last frame. The dirty set is `pose_dirty: HashSet<EntityId>` on `SkinSlotPool` in `crates/core/src/ecs/resources.rs` (NOT the renderer layer); marked/drained via `try_mark_pose_dirty`/`clear_pose_dirty`; call sites in `byroredux/src/render/skinned.rs` (`clear_pose_dirty` before the dispatch loop, `try_mark_pose_dirty` inside). Regression: a path that bumps `dispatches_total` without consulting `pose_dirty.contains(entity)`. First-sight invariant: `SkinSlot.has_populated_output` is false until the first dispatch flips it — the gate MUST NOT skip before that flip.
- **BLAS refit gate (#1196)**: `refit_skinned_blas` skips on `has_populated_output && !is_dirty && accel.has_skinned_blas(entity_id)` — all three live or first-sight/skip-safety breaks. Refit must dominate; full rebuild only on bone-count change or after `SKINNED_BLAS_REFIT_THRESHOLD` (600 frames). `SKINNED_BLAS_FLAGS` is deliberately `FAST_BUILD` not `FAST_TRACE` (memory-budget.md: +15.8 FPS on Prospector) — flipping it back is a regression.
- **Descriptor-rewrite skip (#1197)**: `vkUpdateDescriptorSets` for the skin dispatch is skipped when the (input, palette) pair matches `SkinSlot.descriptor_bindings[frame]`; steady-state target is 0 writes after warm-up.
- **Instrumentation**: per-pass GPU timer + `dispatches_skipped` on `SkinCoverageFrame` (#1194) — reachable via `bench-stats --break-down skin` / `skin.coverage` (byro-dbg). Dispatch-count findings can be **quantified, not estimated**.
**Output**: `/tmp/audit/performance/dim_6.md`

### Dimension 7: World Streaming & Cell Transitions (M40)
**Entry points**: `byroredux/src/streaming.rs` (+ `streaming_tests.rs`), `byroredux/src/streaming_helpers.rs` (`drain_streaming_state` + `consume_streaming_payload`), `byroredux/src/cell_loader/transition.rs` (boundary cross), `byroredux/src/cell_loader/precombined.rs` (FO4 XCRI/XPRI), `byroredux/src/cell_loader/nif_import_registry.rs`, `byroredux/src/npc_spawn.rs`, `crates/sfmaterial/src/reader.rs` (Starfield CDB)
**Checklist + guards**: Cell-transition stall budget (frame-time spike at boundary cross). `pre_parse_cell` is a two-phase pipeline: serial header extract feeds a rayon-parallel body parse (#877) — collapsing the phases regresses cell-stream latency ~6–7× on FNV/SE exterior grids. Small-model serial fast-path (#1262) skips rayon overhead below a size threshold inside `pre_parse_cell` — verify intact. NIF import cache hit-rate during streaming (cached across cells, not per-cell churn). BLAS LRU evicts smoothly under the dynamic budget (Dim 3) with no thrash. Shutdown drain joins the worker without leak. CDB parsed/indexed **once** per archive load (O(1) lookup), never per-cell or per-material. Multi-cell exterior grid is M40 follow-up; the single-cell baseline must not regress.
**Output**: `/tmp/audit/performance/dim_7.md`

### Dimension 8: NIF Parse Performance
**Entry points**: `crates/nif/src/stream.rs` (`allocate_vec`, `read_pod_vec`), `crates/nif/src/import/`, `crates/nif/src/blocks/`, `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params`, `extract_emitter_rate`), `byroredux/src/streaming.rs` (`pre_parse_cell`)
**Checklist + guards (now dhat-testable — propose bounds)**:
- Bulk-array readers go through `read_pod_vec<T>` to collapse double-allocation (#833); direct allocate-then-loop-fill is the regression. The helper has a big-endian compile-error gate (bytemuck is NOT a workspace dep, despite some audits claiming it).
- `stream.allocate_vec::<T>(n)?` is `#[must_use]` (#831); bound-check call sites that discard the empty Vec are the no-op pattern it fixed.
- Per-block parse counters use `entry().get_mut()/insert` split, NOT `or_insert(name.to_string())` (#832) — the to_string path leaked ~150 KB/cell of throwaway short strings on Oblivion.
- Verify typed particle blocks (`NiPSysEmitter*`) parse during import only — `extract_emitter_params`/`extract_emitter_rate` are a one-time import-side walk, not re-walked per frame (the per-frame side lives in Dim 1). These feed the NIFAL canonical tier — see `/audit-nifal`.
- Allocation findings here CAN be bounded by a dhat test (see Regression-Guard Posture); the existing bounds cover the node + geometry + particle parse paths (`crates/nif/tests/heap_allocation_bounds.rs`, `heap_allocation_bounds_geometry.rs`). Propose extending them for new alloc sites rather than estimating.
**Output**: `/tmp/audit/performance/dim_8.md`

### Dimension 9: Telemetry & Camera-Relative Origin Cost
**Entry points**: `crates/renderer/src/vulkan/gpu_timers.rs` (`GpuPerFrameTimers`, `GpuTimerSnapshot`), `crates/core/src/ecs/resources.rs` (`ScratchTelemetry` @392), `byroredux/src/render/camera.rs` (`assemble_camera`, render-origin snap), `crates/renderer/src/vulkan/context/draw.rs` (per-instance model rebase ~1805, `origin_corrected_prev_view_proj`)
**Checklist**:
- **GPU-side timing**: `gpu_timers.rs` exposes per-pass `cmd_*_start/end` timestamp pairs (main render, skin dispatch, BLAS refit, TLAS build, TAA, SVGF, SSAO, bloom, composite, cluster cull, caustic splat, volumetrics) read back via `read_and_reset` into a `GpuTimerSnapshot`. Findings that claim a pass is "expensive" should cite these timers (reachable via `bench-stats`), not guess. Verify timers don't introduce a per-frame stall (timestamp query readback must use the prior frame's results, not block on the current).
- **CPU per-phase wall-clock breakdown (#ec81f233)**: `log_stats_system` (`byroredux/src/systems/debug.rs`) emits a once-a-second `cpu_ms:` line splitting the frame into `fence_wait` / `acquire` / `submit_present` / `ssbo_build` / `rof_*` / `atw_pre` / `atw_scheduler` / `atw_post`. This is the **measurement that classifies a multi-second frame** instead of guessing: large `fence_wait` ⇒ GPU hung on a prior submit; large `atw_scheduler` with `fence_wait≈0` ⇒ pure CPU stall; large `atw_post` ⇒ cell-load upload cost (`step_streaming` + uploads). Cite this split when attributing a CPU stall to a phase; the debug build is too slow to stream the dense cells that fault, so run release with the breakdown to localize.
- **CPU-side scratch telemetry**: `ScratchTelemetry` (refreshed per-frame, surfaced via `ctx.scratch`) tracks `gpu_instances` / `batches` / `indirect_draws` / `terrain_tile` / `tlas_instances` capacity-vs-used + wasted bytes. Diff against it for over-reserved scratch.
- **Camera-relative origin cost (#1492–#1496, #markarth-precision)**: the render origin is snapped to the 4096-unit cell grid (`RENDER_ORIGIN_SNAP`), so it only moves on a cell-boundary crossing. CPU cost: `assemble_camera` builds an extra relative view-proj (one `look_at_rh` per frame), and the per-instance build loop in `draw_frame` subtracts the origin from each model translation column (`m[12..14] - render_origin`, 3 subtractions inside the already-O(visible-instances) loop). This is **negligible added CPU** — verify it stays inside the existing loop and is not refactored into a second full pass over instances. On a grid crossing, `origin_corrected_prev_view_proj` right-multiplies the previous VP by `translation(O₂−O₁)` so TAA/SVGF history is NOT reset (#1489) — a path that drops history on every crossing is a HIGH perf+quality regression (full-screen flash per cell edge).
**Output**: `/tmp/audit/performance/dim_9.md`

## Phase 3: Merge

1. Read all `/tmp/audit/performance/dim_*.md`
2. Combine into `docs/audits/AUDIT_PERFORMANCE_<TODAY>.md`:
   - **Executive Summary** — findings by severity; observed-vs-ROADMAP bench delta (not absolute FPS)
   - **Hot Path Analysis** — per-frame CPU + per-pass GPU cost table, sourced from `gpu_timers` / `ScratchTelemetry` where available
   - **Findings** — grouped by severity (CRITICAL first), deduplicated, eroded-guards called out separately from new issues
   - **Prioritized Fix Order** — quick wins (scratch reuse, preallocation, gate restoration) before architectural changes
3. Remove cross-dimension duplicates

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/performance`
2. Inform user the report is ready
3. Suggest: `/audit-publish docs/audits/AUDIT_PERFORMANCE_<TODAY>.md`
