# Performance Audit — 2026-08-27b

> **Same-day sibling.** `docs/audits/AUDIT_PERFORMANCE_2026-08-27.md` is a
> **Dimension-7-only** run (preset `streaming-deep`) committed earlier today.
> This file is the **full nine-dimension** sweep and does not supersede it —
> both are current. The `b` suffix follows the repo's existing same-day
> convention (`AUDIT_PERFORMANCE_2026-04-13b.md`,
> `AUDIT_RENDERER_2026-08-12b.md`).

- **Scope**: `/audit-performance`, all 9 dimensions, `--depth deep`, no
  `--focus` filter. Part of a `--preset comprehensive` audit-suite run.
- **Executed solo** (no sub-agent dispatch), per explicit instruction — the
  skill's own "Task agent (max 3 concurrent)" orchestration was not followed.
- **HEAD**: `969d81c8`, branch `main`. **142 commits** since the previous full
  sweep's baseline `048a8bd8` (2026-08-24) — session-72 exterior work
  (FNV/FO3 distant-object LOD `#3321`, `.btr` terrain LOD), the resumable
  global-geometry SSBO rebuild (`#3298` + `#3372`), Bethesda authored
  lighting response in `triangle.frag` (`GpuMaterial` 364→432 B), a large
  Skyrim/FNV ESM correctness batch, and the `#3319` NavPath-storage
  registration that finally made the AI path cache live.
- **Dedup baseline**: 400-issue all-state fetch at
  `/tmp/audit/performance/issues.json`, plus every prior
  `docs/audits/AUDIT_PERFORMANCE_*` report, with the two most recent
  (`AUDIT_PERFORMANCE_2026-08-27.md` Dim-7-only and
  `AUDIT_PERFORMANCE_2026-08-24.md`) triaged finding-by-finding below.
- **Method**: static analysis + read-only `git`. **No engine process was
  launched** and **no `cargo` command was run** — the user may have a live
  instance and parallel launch is forbidden by project policy
  (the *feedback_no_parallel_engine_launch* project note). Every magnitude below is *derived
  from checked-in constants and struct layouts* and is labelled as such.
  No FPS figure is manufactured.
- **Cross-audit deconfliction**: the concurrent `/audit-renderer` run already
  filed a **HIGH** on `crates/renderer/src/mesh.rs:1244-1288` routing around
  `GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES`. That defect is **not re-filed
  here**; `PERF-2026-08-27b-01` and `-02` are the VRAM-ledger and
  chunk-pacing consequences of the same code, which survive that fix.
- **Un-owned subsystems touched** (per `_audit-common.md`'s coverage note):
  the **gameplay slice** (`byroredux/src/interaction.rs`) was examined and
  produced one finding. `crates/sdk`, `crates/mod-runtime`, `crates/facegen`
  and `crates/hkx` were **not** examined — they carry no per-frame path.

---

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 3 |
| LOW | 2 |
| **Total** | **5** |

| ID | Sev | Dimension | Title |
|---|---|---|---|
| PERF-2026-08-27b-01 | MEDIUM | GPU Memory Pressure | `memory-budget.md`'s vertex/index-pool row does not carry `#3298`'s deliberate **two-generation** peak, so the ledger this skill is told not to re-derive under-reports the geometry pool's high-water mark by up to 2× plus a session-retained 64 MiB staging buffer |
| PERF-2026-08-27b-02 | MEDIUM | GPU Pipeline / Telemetry | `GEOMETRY_REBUILD_CHUNK_BYTES` = 64 MiB per **blocking** staged copy makes each advancing rebuild frame's "bounded slice" roughly a whole 60 Hz frame — and its own doc defers tuning to a measurement no timer in the engine can produce |
| PERF-2026-08-27b-03 | MEDIUM | Skinning & BLAS | `vkGetBufferDeviceAddress` is called per skinned draw per frame inside the `GpuInstance` build loop for an address that is immutable for the buffer's lifetime — the sibling `MorphSlot` landed days later already caches it |
| PERF-2026-08-27b-04 | LOW | CPU Hot Paths | `select_interaction_target` re-acquires 2–5 per-component storage read locks *per candidate per frame* through `World::get`, in the first `Stage::Update` exclusive, after `#3059` already scratch-pooled the candidate map itself |
| PERF-2026-08-27b-05 | LOW | CPU Hot Paths | `collect_newcomers` re-derives the physics newcomer set by rescanning **every** `CollisionShape` row every tick; there is no pending-registration set, so the steady-state answer (empty) costs O(all colliders) |

### Observed-vs-ROADMAP delta

**None, and the reason is unchanged from 08-24 and 08-20.** ROADMAP's
Bench-of-record is still the 2026-08-14 `34074b93` stepped-camera matrix
(Prospector / WhiterunBanneredMare / MedTekResearch01 / FO4 Dugout Inn /
Cornell). It is now **608 commits stale** against its own 30-commit gate,
which ROADMAP itself tracks as `R6a-stale-20` — not re-filed here. No bench
was run this cycle.

Two structural gaps in the bench-of-record are worth restating because this
cycle's largest perf-shaped change lands squarely in both:

1. **No exterior/streaming scene is in the matrix** (carried verbatim from
   `AUDIT_PERFORMANCE_2026-08-27.md`'s Dim-7 finding — four interiors and one
   synthetic control). The `#3298` chunked geometry rebuild and the `#3321`
   FNV/FO3 object-LOD ring are *both* exterior-traversal features, so neither
   can produce a bench delta against anything.
2. **No timer isolates the geometry rebuild** — see `PERF-2026-08-27b-02`.

### Hot-path cost table (derived from checked-in constants — not sampled)

| Signal | Source | Value | Movement since 08-24 |
|---|---|---|---|
| `GpuMaterial` size | `crates/renderer/src/vulkan/material.rs:1495` | **432 B** | 364 → 432 B (BGEM glass optics + Bethesda lighting response). Docs now agree — see "prior findings" |
| Material SSBO total | `docs/engine/memory-budget.md:34` | 6.75 MB × 2 FIF = **13.5 MB** | 11.4 → 13.5 MB |
| `GpuInstance` size | `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` | **160 B** | unchanged |
| `DRAW_SORT_PARALLEL_THRESHOLD` | `byroredux/src/render/mod.rs:564` | 3000 | unchanged; 1 of 5 runtime baselines still crosses it (fo4 @ 3949 cmds) |
| `bench_draws_cmds` (5 baselines) | `.claude/audit-baselines/runtime/*.tsv` | 325 / 1581 / 2110 / 2342 / 3949 | fo3 re-baselined 2026-08-27 |
| `GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES` | `crates/renderer/src/mesh.rs:42` | 256 MiB | unchanged — but see the renderer audit's HIGH |
| `GEOMETRY_REBUILD_CHUNK_BYTES` | `crates/renderer/src/mesh.rs:55` | **64 MiB** | **new this cycle** (`#3298`) |
| `DEFAULT_STAGING_BUDGET_BYTES` | `crates/renderer/src/vulkan/buffer.rs:53` | 128 MiB | unchanged |
| `froxel_xy_divisor` / `froxel_z_slices` | `crates/renderer/src/vulkan/upscaling.rs:135-136` | 8 / 64 | unchanged since 08-24 |
| `ENABLE_LEGACY_WRS` | `crates/renderer/src/shader_constants_data.rs:981` | **0** (shipped default) | unchanged — `#1799` guard intact |
| `MIN_BLAS_BUDGET_BYTES` / `BATCH_EVICTION_CHECK_INTERVAL` | `crates/renderer/src/vulkan/acceleration/constants.rs:61,74` | 256 MiB / 64 | unchanged |
| `PRE_PARSE_RAYON_MIN` | `byroredux/src/streaming.rs:1189` | 8 | unchanged — `#1262` guard intact |
| GPU timer brackets | `crates/renderer/src/vulkan/gpu_timers.rs` | 14 `cmd_*_start` pairs | **none covers the geometry rebuild** |

---

## Prior findings — triage (not re-filed)

### From `AUDIT_PERFORMANCE_2026-08-27.md` (Dimension 7 only)

| Finding | Issue | State at `969d81c8` |
|---|---|---|
| `PERF-D7-2026-08-27-01` — distant-LOD reconcile re-runs the per-quad archive-presence probe every frame | #3385 | **CLOSED — fix verified.** `c7a70d45` added `state.lod_terrain_available` / `state.lod_object_available` (`byroredux/src/streaming.rs:653-655`), both `FxHashMap` per the hot-path-hashing rule, memoised through `or_insert_with` at `byroredux/src/cell_loader/object_lod.rs:155-165`, and cleared in `drain_streaming_state` (`byroredux/src/streaming_helpers.rs:410-411`). I additionally checked the cache-key soundness the fix's own doc asserts: the key omits `worldspace_key`, which is only safe because `wctx` cannot change within one `WorldStreamingState` lifetime and a worldspace switch goes through `drain_streaming_state`. That invariant holds at HEAD. |
| `PERF-D7-2026-08-27-02` — `drain_streaming_state` / shutdown sweep unload one cell at a time | #3386 | **OPEN, code unchanged.** Not re-filed. |
| `PERF-D7-2026-08-27-03` — `NifImportRegistry::touch_keys` allocates a `String` per already-present key | #3387 | **OPEN, code unchanged.** Not re-filed. |
| `PERF-D7-2026-08-27-04` — `stamp_cell_root_range` inserts `CellRoot` one entity at a time | #3388 | **OPEN, code unchanged.** Not re-filed. |
| `PERF-D7-2026-08-27-05` — `block_hole_mask`'s `resident_full_cells` linear scan is unreachable | #3389 | **OPEN, code unchanged.** Not re-filed. |

### From `AUDIT_PERFORMANCE_2026-08-24.md`

| Finding | Issue | State at `969d81c8` |
|---|---|---|
| `PERF-D1-2026-08-24-01` — six NAVM-pathed AI procedures clone a `VecDeque<Vec3>` **twice** per entity per tick | #3269 | **CLOSED — fix verified** in `bbfd742f`. Travel/Guard/Escort now `pending.drain(..)`; Follow uses `mem::take`. Clone #1 inside `resolve_cached_waypoints`'s cache-hit arm (`byroredux/src/systems/navmesh_path.rs`) remains by design, as the issue directed. I re-checked whether it deserves re-filing on its own and concluded **no**: all seven procedures are still env-gated off the default scheduler (`byroredux/src/boot.rs:1131-1220`), so the site is unreachable in a shipped run. |
| `PERF-D4-2026-08-24-01` — `shader-pipeline.md` + `memory-budget.md` both stale at 348 B for `GpuMaterial` | *(never filed as an issue)* | **CLOSED by code.** `GpuMaterial` has since grown again to **432 B** and **both docs were updated in lockstep**: `docs/engine/shader-pipeline.md:284` and `docs/engine/memory-budget.md:34` both read 432 B, and the Rust pin is `gpu_material_size_is_432_bytes` (`crates/renderer/src/vulkan/material.rs:1494-1495`) with a matching GLSL mirror tail in `crates/renderer/shaders/include/bindings.glsl`. |
| `PERF-D6-2026-08-24-01` — GPU morph-target blending landed a new `std::collections::HashMap` on the per-frame per-entity skin path | *(folded into #3061)* | **STILL OPEN.** `morph_slots` is still `std::collections::HashMap` (`crates/renderer/src/vulkan/context/mod.rs:1478`) and is probed per skinned draw per frame at `crates/renderer/src/vulkan/context/draw.rs:3037-3038`. This is the seventh member of the #3061 cluster, which remains OPEN with `skin_dispatch_seen_scratch` (`:1325`), `skin_slots` (`:1467`), `failed_skin_slots` (`:1507`), `failed_skin_blas` (`:1527`), `blend_pipeline_cache` (`:1751`) and `blend_seen_scratch` (`:1758`). **Not re-filed** — it belongs to #3061's one-pass conversion. |
| Carry: `PERF-D7-01` — `resident_vwd_refr_cells` takes a fresh `GlobalTransform` lock per VWD entity | #3142 | **OPEN, code unchanged.** Not re-filed. |
| Carry: `PERF-D0-01` — skill text cites superseded constants | #3143 | **CLOSED.** |
| Carry: `PERF-D0-01` — bench-of-record past its own staleness gate | #3063 | **CLOSED** (tracked in ROADMAP as `R6a-stale-20`). |

---

## Regression guards verified INTACT (not re-proposed)

Each was read at HEAD, not inferred.

**Dimension 1 — CPU hot paths**
- `#1371` `drain_dirty_into` — `crates/core/src/ecs/packed.rs`; `make_world_bound_propagation_system` still owns a persistent `g_dirty: Vec<EntityId>` across frames and drains into it (`byroredux/src/systems/bounds.rs:108-121`). All five of that closure's `Vec`s are **closure-captured**, not per-frame — a naive grep reports them as per-frame allocations and is wrong.
- `#1374`/`#3192` billboard camera-parked early-out — `byroredux/src/systems/billboard.rs:91-141`. The parked-camera pass now drives from the `SpeedTreeWind` set (`for (entity, tree_wind) in swq.iter()`), not a full `Billboard` walk. `#3192` verified CLOSED.
- `#1376` debug-UI snapshot gate — `byroredux/src/app_frame.rs:67-96`. Still gated on `ui.visible || ui.game_menu_visible()`; the new `studio: studio_host::snapshot(world)` and `settings` fields landed **inside** the gated branch, and the hidden-overlay fallback is a 3-field `PanelSnapshot { .., ..Default::default() }`.
- `#3135`/`#3137`/`#3139` water scratch reuse — `crates/physics/src/water.rs`; `WaterContactScratch` is now taken and handed back **whole** (`std::mem::take(&mut *scratch)`), which also carries the new `in_current_prev`/`in_current_now` latch. `#3257`'s `WaterDisturbanceScratch` round-trip is live at `byroredux/src/systems/water.rs:252-320`.
- `#1794` `bone_world` steady-state reuse and `#1803` dead-probe removal — unchanged.

**Dimension 2 — draw & instancing**
- `#1377` GT-presence hoist and the `draw_sort_key` ordering are unchanged.
- `DRAW_SORT_PARALLEL_THRESHOLD` = 3000 still sits above four of five runtime baselines and below `fo4-InstituteBioScience` (3949 cmds) — the crossover the skill describes.
- `#2766` corrected the direct-draw call accounting (`crates/renderer/src/vulkan/context/geometry_pass.rs`) so `indirect_call_count` counts only *recorded* draws. This makes `bench_draws_gpu_calls` more trustworthy, not less — a future baseline diff on that column may move for this reason.

**Dimension 3 — GPU memory pressure**
- `#1792` `blas_over_budget` still gates both eviction sites (`crates/renderer/src/vulkan/acceleration/blas_static.rs:1026,1071`); `BATCH_EVICTION_CHECK_INTERVAL` = 64 still drives the mid-batch check at `:365`.
- BGSM/BGEM half-eviction, `NifImportRegistry` 2048-entry LRU, `MAX_FRAMES_IN_FLIGHT`-depth deferred destroy: unchanged.
- `MeshRegistry::destroy` still tears down `geometry_staging_pool` (`crates/renderer/src/mesh.rs:1704`) — the new pool is not leaked at shutdown.

**Dimension 4 — SSBO sizing & upload**
- `GpuInstance` = 160 B, layout tests present.
- **PBR resolved once at import** — `Material::resolve_pbr` has exactly three non-test call sites: `byroredux/src/material_translate.rs:565,702` (the NIFAL boundary) and `byroredux/src/commands/scene.rs:914,921` (the `mat.*` console command). `classify_pbr_keyword` and the new `path_indicates_ice` (`crates/core/src/ecs/components/material.rs:754`) are reachable **only** through `resolve_pbr`. No per-draw classifier re-entry.
- `MaterialTable::intern_by_hash` still takes the `#781` hash-first fast path. The hash walk has grown to ~92 `write_u32` steps with `GpuMaterial` (`crates/renderer/src/vulkan/context/mod.rs:578+`); at the FxHash rate `#1368` measured this is still well under 0.1 ms/frame, so it is **not** filed.

**Dimension 5 — GPU pipeline**
- `ENABLE_LEGACY_WRS` = 0 in `shader_constants_data.rs:981`; `triangle.frag`'s legacy reservoir arrays remain behind `#if ENABLE_LEGACY_WRS` (`:2733,2771`).
- The new Bethesda lighting lobes (`bethesdaDiffuseLightFactor` / `bethesdaRimFactor` / `bethesdaBackFactor`, `crates/renderer/shaders/include/lighting.glsl`) each early-out on a `materialFlags` bit before doing any `pow`, and `fresnelSchlickPower` preserves the exact fixed-x⁵ `schlickWeight` path for the neutral 5.0 default. The extra glass texture fetches are inside `if (isGlass)`. `triangle.frag.spv` grew 338 KB → 361 KB (+7%); a register-pressure claim would need RenderDoc and is **not** made here.

**Dimension 6 — skinning & BLAS**
- `#1195` dispatch-dirty gate, `#1196` refit gate (`slot.has_populated_output && !is_dirty` at `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:412-413,613-615`), `#1197` descriptor-rewrite skip, `SKINNED_BLAS_FLAGS` = `FAST_BUILD`, and the `#1791`/`#1796` `skin_dispatch_ran` rollback scope test: all present.
- `pose_dirty` is still `FxHashSet` on both sides of the crate boundary (`crates/core/src/ecs/resources/skin_slot_pool.rs`, `crates/renderer/src/vulkan/context/draw.rs:1502`), with the source-text guard in `context/mod.rs:2807-2811`.

**Dimension 7 — streaming**
- `PRE_PARSE_RAYON_MIN` = 8 (`byroredux/src/streaming.rs:1189,1201`), two-phase `pre_parse_cell`, `unload_cells` batched teardown, `World::despawn_batch` → `remove_entities_erased`: unchanged.
- **New this cycle and worth recording as a win**: `#3319` registered the `NavPath` storage in `build_world`. Before it, `World::query_mut::<NavPath>()` returned `None`, the write site was an `if let Some(..)`, and every pathed AI procedure re-ran a full A\* plus an O(~10.6k-triangle) `find_containing_triangle` localize scan **every tick**. The cache is now live. `#2369` additionally skips the persistent-CELL drain+rebuild when crossing identity is unchanged.

**Dimension 8 — NIF parse**
- `read_pod_vec` / `#[must_use] allocate_vec` / the `entry().get_mut()`-not-`or_insert(to_string())` counter pattern: unchanged. `#2272` (`bf5cc041`) moved a header POD-read allocation *inside* the helper's gate — a strengthening, not a regression. No new unbounded allocation site found in the `crates/nif` delta.

**Dimension 9 — telemetry & origin cost**
- `read_and_reset` is deliberately **without** `VK_QUERY_RESULT_WAIT_BIT` (`crates/renderer/src/vulkan/gpu_timers.rs:410-411`) — no per-frame readback stall.
- `RENDER_ORIGIN_SNAP` rebase still happens inside the existing O(visible-instances) loop (`crates/renderer/src/vulkan/context/draw.rs`, the `rebase_model_matrix` call at `:2841`), not as a second pass. `origin_corrected_prev_view_proj` history preservation unchanged.

---

## Findings

### PERF-2026-08-27b-01: `memory-budget.md`'s vertex/index-pool row does not carry `#3298`'s deliberate two-generation VRAM peak

- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `docs/engine/memory-budget.md:455-466` and `:548` vs
  `crates/renderer/src/mesh.rs:56-70` and `:1246-1270`
- **Status**: NEW
- **Description**: `#3298` (`ae7179a3`) made the global geometry SSBO rebuild
  resumable by allocating the **full-size replacement generation while the old
  one is still bound and serving draws**. The code says so explicitly. The
  authoritative VRAM ledger does not, and this skill is told in its own
  preamble not to re-derive that ledger but to cite it — so a budget decision
  made from `memory-budget.md` today is made against a peak that no longer
  exists.
- **Evidence**: The code states the trade-off (`crates/renderer/src/mesh.rs:66-70`):
  ```
  /// That means two full geometry SSBO generations are resident in
  /// device-local memory at once for the rebuild's duration. This is an
  /// accepted trade-off (#3298): it smooths a multi-hundred-ms atomic
  /// stall into several bounded per-frame chunks, at the cost of a
  /// temporarily higher VRAM high-water mark.
  ```
  and allocates both up front (`:1256-1259`):
  ```rust
  match Self::try_allocate_empty_geometry_buffers(
      device, allocator, vertex_size, index_size, rt_usage,
  ) {
      Ok((new_vertex_buffer, new_index_buffer)) => {
  ```
  The old generation is only released at swap-in (`:1447-1452`). Meanwhile the
  ledger carries a single-generation figure:
  ```
  | VERTEX_POOL_SOFT_CAP | 4 M vertices | ~416 MB (104 B/vertex) |
  | INDEX_POOL_SOFT_CAP  | 16 M indices | ~64 MB (4 B/index)     |
  ...
  | Vertex / index pools | ~208 MB | ~1.66 GB cap |
  ```
  Two consumers are missing from the "Peak" column:
  1. **The duplicate generation.** At the documented ~208 MB typical the peak
     is ~416 MB, not ~208 MB; at `VERTEX_POOL_HARD_CAP` + `INDEX_POOL_HARD_CAP`
     (~1.92 GB) the peak is ~3.84 GB — on its own past the page's stated
     `< 4 GB` engine budget target, before textures, BLAS or the froxel grid.
  2. **A session-retained 64 MiB `CpuToGpu` staging buffer.** The rebuild
     lazily constructs its own `StagingPool` (`crates/renderer/src/mesh.rs:1355-1357`)
     at `DEFAULT_STAGING_BUDGET_BYTES` = 128 MiB
     (`crates/renderer/src/vulkan/buffer.rs:53`). Each chunk acquires a buffer
     of exactly `GEOMETRY_REBUILD_CHUNK_BYTES` = 64 MiB, and because 64 MiB is
     inside the 128 MiB retention budget the pool **keeps** it after the
     rebuild finishes, for the process lifetime. Pre-`#3298` the atomic path
     acquired one staging buffer the size of the whole pool, which normally
     *exceeded* the budget and was therefore evicted on release — so this is a
     new steady-state resident allocation, not a pre-existing one.
- **Impact**: Not a runtime fault on the 12 GB dev card. The blast radius is
  decision-quality: `memory-budget.md` is cited as authoritative by
  `/audit-performance`, `/audit-renderer` and `/audit-safety`, and the same
  class of drift on the same page was filed and fixed three days ago as #3117
  (volumetrics row understating the froxel grid ~24×, breaking the documented
  4 GB ceiling). The 2× geometry peak is the largest single unrecorded VRAM
  consumer on the page.
- **Related**: `#3298`, `#3372`, `#2374`, #3117 (same doc, same drift class);
  the concurrent `/audit-renderer` HIGH on `mesh.rs:1244-1288`. **Note that
  fixing the renderer HIGH does not close this**: restoring the
  `GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES` gate only forces the single-generation
  path *above* 256 MiB; every rebuild under that threshold still holds two
  generations, which is exactly the ~208 MB typical case the ledger describes.
- **Suggested Fix**: Add a "Global geometry SSBO rebuild" row to the VRAM
  Rough Budget table with `2 × pool + 64 MiB` in the Peak column, and a note
  under the Mesh Registry section pointing at
  `GeometryRebuildInProgress`'s own doc comment as the source of truth.

---

### PERF-2026-08-27b-02: the resumable rebuild's 64 MiB chunk is one blocking staged copy per frame, and no timer in the engine can measure the slice its own doc says needs tuning

- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline & Pass Efficiency / Telemetry
- **Location**: `crates/renderer/src/mesh.rs:44-55` (the constant),
  `:1338-1428` (`advance_geometry_rebuild`),
  `crates/renderer/src/vulkan/buffer.rs:1529-1580` (`copy_bytes_range`)
- **Status**: NEW
- **Description**: `#3298`'s stated goal is to convert one multi-hundred-ms
  atomic stall into "several bounded per-frame slices". The slice is bounded
  **in bytes, not in time**, and the byte budget chosen — 64 MiB — is close to
  a whole frame's worth of host→device transfer, paid **synchronously** on the
  render thread. So the per-frame slice is still a dropped frame; there are
  just several of them instead of one long one. The constant's own doc admits
  it is untuned, and the measurement it defers to does not exist.
- **Evidence**: The constant is explicitly provisional
  (`crates/renderer/src/mesh.rs:44-48`):
  ```
  /// Per-`advance_geometry_rebuild`-call byte budget for a resumable global
  /// geometry SSBO copy (#3298). Chosen conservatively pending live
  /// `grid-cross` tuning against real FO4/Skyrim/FNV data
  ```
  Each call is one full synchronous round trip
  (`crates/renderer/src/vulkan/buffer.rs:1543-1571`): a `StagingPool::acquire`,
  a `copy_from_slice` of the whole chunk into mapped `CpuToGpu` memory, then
  `with_one_time_commands(...)` — which allocates a command buffer, submits,
  and **waits on a fence** before returning. `advance_geometry_rebuild`
  performs exactly one such call per invocation
  (`crates/renderer/src/mesh.rs:1362-1430`), and the frame driver calls it
  every frame while a rebuild is live (`byroredux/src/app_frame.rs:205-215`).

  Derived cost, stated as derivation and not as measurement: 64 MiB of
  `copy_from_slice` into write-combined host-visible memory plus 64 MiB across
  PCIe with a full fence wait is order **~10 ms** on this machine's PCIe 4.0
  x16 link — i.e. roughly the entire 16.7 ms budget at 60 Hz, and more than
  half the 23.5 ms the ROADMAP bench-of-record records for the heaviest
  scene (MedTek, native TAA). Two independent things make that estimate
  impossible to replace with a number:
  1. **No GPU timer bracket exists for it.** `gpu_timers.rs` exposes 14
     `cmd_*_start`/`_end` pairs (skin dispatch, BLAS refit, TLAS build, main
     render, TAA, SVGF, SSAO, bloom, composite, cluster cull, caustic splat,
     volumetrics, upscale, presentation). None covers the geometry-rebuild
     copy — and it could not, since the copy is submitted on its own one-time
     command buffer outside `draw_frame`'s recording.
  2. **No CPU phase isolates it either.** The rebuild call sits inside
     `render_one_frame`'s `rof_pre_t0` bracket (`byroredux/src/app_frame.rs:55`,
     closed at `:393`), which also contains `build_render_data`, material
     interning, the UI tick and the debug-UI snapshot. `cpu_ms:`
     (`byroredux/src/systems/debug.rs:98-112`) therefore reports it only as
     part of `rof_pre_draw`.
- **Impact**: On an exterior traversal that grows the pool past a few hundred
  MB, a rebuild spans several frames and each of them takes a full-frame CPU
  stall — visible as a short stutter burst rather than one long hitch, which
  is the improvement `#3298` intended, but not the "bounded slice" the doc
  claims. The compounding problem is that the constant cannot be tuned:
  ROADMAP's bench matrix has no exterior scene (see the delta section) *and*
  no timer resolves the phase, so there is no path from "chosen conservatively
  pending tuning" to a tuned value.
- **Related**: `#3298`, `#3372`; the concurrent `/audit-renderer` HIGH on
  `mesh.rs:1244-1288`; `AUDIT_PERFORMANCE_2026-08-27.md`'s Dim-7 observation
  that no exterior scene is in the bench matrix; `#2041`/`PERF-D9-02` (the
  batched timer read this would extend).
- **Suggested Fix**: Two independent, small steps, in this order.
  (a) Bracket `advance_geometry_rebuild` with a plain
  `Instant::now()`/`elapsed()` pair accumulated into the existing
  `FrameTimings` alongside `ssbo_build_ns` — the same shape
  `UnloadPhaseTimings` already uses, and enough to answer the question
  without touching the query pool. (b) Once a number exists, re-pick
  `GEOMETRY_REBUILD_CHUNK_BYTES` against it (a value near 8–16 MiB would put
  the slice inside a frame's slack rather than consuming the frame), and add
  an exterior `GridCross` scene to `scripts/fsr-bench-matrix.sh` so the choice
  is regression-gated.

---

### PERF-2026-08-27b-03: `vkGetBufferDeviceAddress` is called per skinned draw per frame for an address that cannot change

- **Severity**: MEDIUM
- **Dimension**: Skinning & BLAS Cost
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:3007-3028`
  (inside the `for draw_cmd in draw_commands` loop opened at `:2833`);
  sibling sites `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:536-570`
- **Status**: NEW
- **Description**: The per-instance `GpuInstance` build loop resolves each
  skinned draw's deformed-vertex buffer address by calling into the driver —
  `vkGetBufferDeviceAddress` — every frame, for every skinned draw. A
  `VkBuffer`'s device address is fixed for the buffer's lifetime once bound
  (Vulkan spec), and `SkinSlot`'s own documentation relies on exactly that
  invariant elsewhere. The address is therefore recomputable once, at slot
  creation, and stored — which is precisely what the sibling `MorphSlot`,
  added in the same subsystem days later, already does.
- **Evidence**: In the hot loop (`crates/renderer/src/vulkan/context/draw.rs:3007-3028`):
  ```rust
  let slot_address = (draw_cmd.bone_offset != 0)
      .then(|| self.skin_slots.get(&draw_cmd.entity_id))
      .flatten()
      .filter(|slot| skin_slot_backs_mesh(slot.vertex_count(), mesh.vertex_count))
      .map(|slot| {
          unsafe {
              self.device.get_buffer_device_address(
                  &vk::BufferDeviceAddressInfo::default()
                      .buffer(slot.output_buffer.buffer),
              )
          }
      });
  ```
  `SkinSlot` (`crates/renderer/src/vulkan/skin_compute.rs:83-148`) stores
  `output_buffer`, `output_size`, `descriptor_sets`, `vertex_count`,
  `last_used_frame`, `has_populated_output` and `descriptor_bindings` — **no
  address field** — while its own `descriptor_bindings` doc states the
  justifying invariant in as many words: *"`output_buffer` isn't tracked — it's
  a function of the slot itself, so once any FIF has been written it stays
  correct (the buffer doesn't move)."*

  The immediately following block, `#3231`'s morph lookup
  (`draw.rs:3033-3054`), reads `slot.delta_address()` / `slot.weight_address()`
  — plain field reads of `vk::DeviceAddress` members cached at construction
  (`crates/renderer/src/vulkan/morph_compute.rs:41,50,82-88`, populated once at
  `:149,155`). The two adjacent code paths do the same job two different ways.

  The same pattern repeats in the refit path: `refit_skinned_blas`
  re-queries the vertex, index and scratch addresses on every refit
  (`crates/renderer/src/vulkan/acceleration/blas_skinned.rs:536-570`), gated
  by `pose_dirty` so it costs one call per *moving* actor per frame rather
  than per draw.
- **Impact**: Derived, not measured. `vkGetBufferDeviceAddress` is a driver
  entry point reached through ash's loaded function table — tens of
  nanoseconds each, not free, and it sits in the innermost O(visible-instance)
  loop of `draw_frame`. `skyrim_se-WhiterunDragonsreach` reports
  `skin_pool_live = 83` (`.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv`);
  a Skyrim NPC contributes several draws (head, body, hands, feet, hair, worn
  ARMA meshes — and `#3357` has since increased that per-NPC mesh count), so
  the order is several hundred driver calls per frame in that cell, scaling
  linearly with crowd size. On a machine where a CPU bottleneck is a bug by
  definition, this is pure avoidable per-frame driver traffic on the exact
  path `#1195`/`#1196`/`#1197` were written to keep lean.
- **Related**: `#2219` (introduced the draw-loop lookup), `#3231` (the
  `MorphSlot` sibling that caches), `#2402` (the `skin_slot_backs_mesh` filter
  that must stay in front of any cached read), `#1797` (the shared-scratch
  serialization ceiling on the same refit path — quantify with `skin.coverage`
  before touching that one).
- **Suggested Fix**: Add `output_address: vk::DeviceAddress` to `SkinSlot`,
  populated once in `SkinComputePipeline::create_slot` next to the existing
  buffer creation, with a `pub fn output_address(&self)` accessor mirroring
  `MorphSlot::delta_address`. Keep the `skin_slot_backs_mesh` filter exactly
  where it is — the cached address must still be suppressed for a slot that no
  longer backs the mesh. The refit path's three queries can move to the same
  cached field plus one address on the shared scratch buffer.

---

### PERF-2026-08-27b-04: `select_interaction_target` re-acquires a storage read lock per candidate per frame

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/interaction.rs:744-770`, with
  `activation_is_blocked` at `:927-940` and `interaction_bound` at `:942-956`
- **Status**: NEW
- **Description**: `interaction_system` is the first `Stage::Update` exclusive
  (`byroredux/src/boot.rs:851`) and calls `select_interaction_target`
  **unconditionally every frame** — correctly, since it drives the HUD prompt,
  not just the activate edge. `#3059` already removed the per-frame allocation
  from that path by pooling the candidate map in
  `InteractionCandidateScratch`. What it did not remove is the per-candidate
  lock churn: each candidate is then passed through two helpers that reach the
  world with `World::get::<T>`, which opens a fresh `RwLock` read guard per
  call rather than reusing a hoisted query.
- **Evidence**:
  ```rust
  let mut targets: Vec<_> = candidates
      .iter()
      .filter(|(entity, _)| !activation_is_blocked(world, **entity))
      .filter_map(|(entity, kind)| {
          let bound = interaction_bound(world, *entity)?;
  ```
  `activation_is_blocked` performs up to two `world.get` calls (`Locked`, then
  `MG07LabyrinthianDoor`); `interaction_bound` performs up to three
  (`WorldBound`, `GlobalTransform`, `Transform`). `World::get`
  (`crates/core/src/ecs/world.rs:358-376`) is not a bare probe — per call it
  does a `HashMap<TypeId, _>` lookup, constructs a `lock_tracker::TrackedRead`
  (a thread-local `HashMap<TypeId, LockState>` insert, un-done on drop —
  always on, in release too, per `crates/core/src/ecs/lock_tracker.rs:8-12`),
  takes the `RwLock` read, and builds a `ComponentRef`. Every other per-frame
  consumer in this codebase hoists the query once and iterates
  (`byroredux/src/systems/bounds.rs:126-137` is the canonical shape).
  Also note `targets` is a fresh `Vec` per frame — the one allocation `#3059`
  left behind on this path.
- **Impact**: Small and bounded — candidates are `DoorTeleport` plus four
  scripted-activator component types (`populate_candidates`,
  `byroredux/src/interaction.rs:876-925`), so tens in an interior and low
  hundreds across a loaded exterior grid. Derived: a few hundred guard
  acquire/release pairs per frame, order tens of microseconds. Filed at LOW on
  magnitude, but flagged because this is the **un-owned gameplay slice** that
  `_audit-common.md` names as the highest-value coverage gap, the pattern is
  the one the project has now corrected three times elsewhere
  (#2149, #3265, #3059), and it will scale with the candidate set as more
  activator kinds are added.
- **Related**: #3059 (the same function's allocation half, CLOSED), #3265,
  #2149; `docs/engine/ecs.md`'s hoisted-query guidance.
- **Suggested Fix**: Convert `interaction_system` to a factory closure with a
  persistent `targets` buffer (the shape `make_animation_system` /
  `make_billboard_system` use), and hoist the five component queries once
  around the candidate loop, passing `&QueryRead<'_, T>` into
  `activation_is_blocked` / `interaction_bound` instead of `&World`. Acquire
  them in the canonical cluster order so the hoist does not introduce a new
  lock edge.

---

### PERF-2026-08-27b-05: `collect_newcomers` rescans every collider row every tick to answer "nothing new"

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/physics/src/sync.rs:807-866`
- **Status**: NEW
- **Description**: Phase 1 of the physics tick re-derives the newcomer set
  from scratch on every frame by walking the **entire** `CollisionShape`
  storage and probing `RapierHandles` for each row. In steady state — every
  collider already registered, which is the overwhelming majority of frames —
  the loop's entire output is an empty `Vec`, and the work is proportional to
  the resident collider count rather than to the (zero) number of newcomers.
- **Evidence**:
  ```rust
  for (entity, shape) in shape_q.iter() {
      if handles_q.contains(entity) {
          continue;
      }
      let Some(body_data) = body_q.get(entity) else {
  ```
  There is no dirty set, insertion queue, or generation counter feeding this;
  `physics_sync_system` (`crates/physics/src/sync.rs:190`) calls it
  unconditionally each tick, and the same scan is reached a second time
  through the new `register_newcomers_and_refresh_queries`
  (`:207-217`, added this cycle for cold-start spawn probing and cell-arrival
  grounding).
- **Impact**: Derived: a linear walk plus one sparse-set probe per collider
  row per frame — order 10 µs at the few-thousand-collider scale a Skyrim
  exterior reaches (the *tes_grounding_zero_mass_dynamic_fix* note records one interior
  going 19 → 416 colliders after the mass=0 reclassification alone). Below the
  noise floor of the current bench, hence LOW. Recorded because it is the
  structural reason the `#2867` bug was as expensive as it was — that defect
  re-collected and re-registered *the full newcomer set including `TriMesh`
  vertex/index clones* every frame, and the fix removed the re-registration
  without removing the rescan that made it possible.
- **Related**: #2867 (the re-registration leak this scan enabled), #1520.
- **Suggested Fix**: Give `PhysicsWorld` a `pending_registration:
  Vec<EntityId>` fed at the two sites that actually create colliders (cell
  spawn and ragdoll activation), and fall back to the full scan only when that
  queue is absent — the same "explicit dirty set beats a rescan" move
  `#1195`'s `pose_dirty` and `#3319`'s `NavPath` cache both made. A cheaper
  interim: skip the walk entirely when `shape_q.len() == handles_q.len()`.

---

## Candidates investigated and dropped (so a later sweep does not re-derive them)

- **`material_hash`'s ~92-field FxHash walk per draw per frame**
  (`crates/renderer/src/vulkan/context/mod.rs:578+`). Grew with `GpuMaterial`
  364 → 432 B. Dropped: `AUDIT_PERFORMANCE_2026-06-04.md` measured the
  post-`#1368` FxHash cost at 0.04–0.08 ms for ~60 fields; the field growth
  keeps it comfortably under 0.1 ms, and the `#781` hash-first design still
  avoids the far more expensive `to_gpu_material` build on the ~97% hit path.
- **`update_lod_coverage`'s four per-call `Vec`s and two O(n²) overlap scans**
  (`byroredux/src/streaming_helpers.rs:150-170`,
  `byroredux/src/cell_loader/lod_coverage.rs:53-84`). Dropped: it runs only
  while `lod_reconcile_pending` is set (`byroredux/src/app_step.rs:225-246`),
  i.e. in bursts after a boundary crossing, and the quad set is bounded by
  `MAX_LOD_RING_REACH_CELLS` = 64 to roughly a hundred entries per scheme.
- **`std::collections::HashSet` for `desired_set` in `stream_object_lod_blocks`**
  (`byroredux/src/cell_loader/object_lod.rs:170`). Dropped: same burst
  cadence as above, not a per-frame per-entity keyspace, so outside the
  hot-path hashing rule's scope.
- **`resolve_cached_waypoints`'s remaining cache-hit `VecDeque` clone**
  (`byroredux/src/systems/navmesh_path.rs`). Dropped: `#3269` explicitly left
  it, and all seven consuming procedures are env-gated out of the default
  scheduler (`byroredux/src/boot.rs:1131-1220`).
- **`resolve_armor_meshes` allocating a `Vec` that `resolve_armor_mesh` then
  reduces to one element** (`crates/plugin/src/equip.rs`). Dropped: load-time
  NPC-spawn path, already measured by `npc_spawn_wall`.
- **`copy_bytes_range` using `with_one_time_commands` rather than the
  `_reuse_fence` variant** (`crates/renderer/src/vulkan/buffer.rs:1558`).
  Dropped: ~5 µs of fence create/destroy against a ~10 ms copy.
- **`PackedStorage::remove_entities_erased` now marking removed ids dirty**
  (`crates/core/src/ecs/packed.rs`). Investigated as a possible
  boundary-teardown cost (tens of thousands of ids pushed onto the dirty set
  at exterior unload, then drained into a wasted probe pass). Dropped: it is
  a one-shot cost at a frame the batched-teardown work already dominates, and
  the marking is a correctness requirement for change-tracked storages.

---

## Prioritized Fix Order

1. **`PERF-2026-08-27b-03`** — cache `SkinSlot`'s device address. Mechanical,
   zero-risk (the invariant is already documented on the struct), removes
   driver calls from the innermost per-frame loop, and the sibling
   `MorphSlot` is a ready-made template.
2. **`PERF-2026-08-27b-02` step (a)** — add the CPU bracket around
   `advance_geometry_rebuild`. Cheap, and it is the prerequisite for every
   subsequent decision about `GEOMETRY_REBUILD_CHUNK_BYTES`. Do this before
   changing the constant.
3. **`PERF-2026-08-27b-01`** — update the VRAM ledger row. Pure
   documentation; do it in the same change as the `/audit-renderer` HIGH so
   the gate and the ledger land together.
4. **`PERF-2026-08-27b-04`** — hoist the interaction queries. Small, and it
   converts the last un-pooled allocation on that path at the same time.
5. **`PERF-2026-08-27b-02` step (b)** — re-pick the chunk size against the
   number step (a) produces, and add an exterior scene to
   `scripts/fsr-bench-matrix.sh`.
6. **`PERF-2026-08-27b-05`** — physics newcomer queue. Lowest yield; fold
   into the next physics-side change rather than doing it standalone.

Independently of this report's own findings, the **#3061 hot-path-hashing
cluster** (seven `std::collections` members, now including `morph_slots`)
remains the largest single OPEN perf item on the render path and is still
waiting on the one-pass conversion #3061, #3045 and #2985 all ask for.
