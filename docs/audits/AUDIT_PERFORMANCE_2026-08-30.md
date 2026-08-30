# Performance Audit — 2026-08-30

- **Scope**: `/audit-performance`, all 9 dimensions, `--depth deep`, no
  `--focus` filter. Part of a `--preset comprehensive` audit-suite run.
- **Orchestration**: followed the skill's own architecture — nine dimension
  Task agents dispatched in three concurrent batches of three, each writing
  `/tmp/audit/performance/dim_N.md`, consolidated here.
- **HEAD**: `64f64480`, branch `main`. **45 commits** since the previous full
  sweep's baseline `969d81c8` (`AUDIT_PERFORMANCE_2026-08-27b.md`).
- **Dedup baseline**: 200-issue fetch at `/tmp/audit/performance/issues.json`
  plus every prior `docs/audits/AUDIT_PERFORMANCE_*` report, with the three
  most recent (`2026-08-24`, `2026-08-27` Dim-7-only, `2026-08-27b`) triaged
  finding-by-finding in each dimension file.
- **Method**: static analysis + read-only `git`, plus — in Dimension 8 only —
  `cargo test` runs of the checked-in `dhat` heap-bound tests and short
  out-of-tree parse/import probes over three real mesh archives, and a
  reproducible out-of-tree `glslangValidator` recompile in Dimension 5 to
  byte-compare the shipped `triangle.frag.spv`. **No engine process was
  launched** (the *feedback_no_parallel_engine_launch* project note) and **no
  bench was run** — see the delta section immediately below. Every other
  magnitude is *derived from checked-in constants, struct layouts and the
  five `.claude/audit-baselines/runtime/*.tsv` baselines*, and is labelled as
  such. No FPS figure is manufactured.
- **Cross-audit deconfliction**: concurrent `/audit-concurrency` and
  `/audit-renderer` runs own CPU lock-ordering/scheduler-access and Vulkan
  correctness respectively. Findings here are cost findings only. One
  overlap is called out explicitly: `PERF-D1-2026-08-30-04` touches the same
  `lock_tracker.rs` lines as the concurrent ECS audit's `ECS-D3-01` and
  should be fixed with it.
- **Hardware context for every budget claim**: RTX 4070 Ti (12 GB VRAM),
  Ryzen 7950X (16c/32t). RT minimum is 6 GB VRAM; total engine budget should
  stay under ~4 GB. A CPU bottleneck on this machine is by definition a bug,
  not an expected cost — which is why the two Dimension 8 findings (73–81 %
  of per-NIF import cost pinned to the main thread on a 16-core part) are
  filed as MEDIUM defects rather than tuning notes.

---

## Observed-vs-ROADMAP bench delta

**Not measured this session, deliberately.** Stating why, rather than
producing a number:

1. ROADMAP's live Bench-of-record is the stepped-camera refresh at HEAD
   `34074b93` (2026-08-14), which ROADMAP itself flags as **660 commits
   stale and not gating** (`R6a-stale-20`). A single-run delta against it
   would not be a regression signal.
2. Reproducing it is a 75-run matrix (`scripts/fsr-bench-matrix.sh 3 300`,
   5 scenes × 5 configs × 3 runs) whose own caveat is that a resident GPU
   consumer shifts the whole table. Two sibling audit agents were running
   concurrently on this machine for the duration of this sweep — any timing
   taken here would be confounded by construction.
3. A Vulkan RTX 4070 Ti **is** present in this environment (`vulkaninfo`
   reports `NVIDIA GeForce RTX 4070 Ti`, driver 580.173.02, API 1.4.312), so
   the older ROADMAP note that this environment has no GPU is no longer
   accurate — a future session *can* refresh the bench-of-record on an
   otherwise idle machine. That is the correct place to spend the runs, not
   here.

The open regression tracker `#2367` (FO4 MedTek/Dugout ~33–34 % slower at
flat entity count) and ROADMAP's `PERF-REGRESSION-6c56e311` (`#2161`,
root-caused to the main-pass fragment shader) both remain open and are
untouched by this sweep.

---

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 17 |
| LOW | 22 |
| **Total** | **39** |

**Stale candidates dropped: 91** across the nine dimensions (12 / 11 / 9 / 15
/ 8 / 8 / 13 / 8 / 7). That is a 70 % drop rate against candidates raised —
consistent with the project's standing "verify the premise first" rule, and
above the historical ~1-in-6 figure because five of this sweep's dimensions
open directly onto guards landed in sessions 46, 47, 75 and 76. The
per-dimension reasons are recorded in each `dim_N.md`; representative drops:
the `AnimationStack` per-actor channel sort (no production site inserts
`AnimationStack` — path dormant), the "missing staging flush" family (moot:
gpu-allocator 0.28 mandates `HOST_COHERENT` for `MemoryLocation::CpuToGpu`),
the "per-cell CSG reopen" and "per-cell CDB parse" theories (both false at
HEAD), the "`morph_slots` is std `HashMap`" theory (converted under `#3061`),
and the Oblivion `parsed_size_cache` clone+sort (**measured** at 57 ms for
3000 NIFs / 437 MiB — not a hot spot).

**No guard eroded.** Every landed regression guard named across the nine
dimension briefs was verified INTACT, each with the exact symbol confirmed in
the corresponding `dim_N.md` "Guards verified" section —
including every Session 46/47 guard, the Session 75/76 batch, the
`ENABLE_LEGACY_WRS = 0` shader gate (proven by a byte-identical out-of-tree
recompile of `triangle.frag.spv`, not inferred), the `#1791`/`#1796`
`skin_dispatch_ran` rollback, the `#2923` `FxHash` hot-path rule on both
sides of the crate boundary, and the camera-relative origin contract
(`#1489`/`#1492`–`#1496`). The eroded-guard section of this report is
therefore empty, which is the useful result.

### Findings index

| ID | Sev | Dimension | Title |
|---|---|---|---|
| PERF-D2-2026-08-30-01 | MEDIUM | Draw & Instancing | the parallel-sort gate reads `raster_len`, but no checked-in metric measures it — the "fo4 crosses the gate" claim is unfalsifiable |
| PERF-D3-2026-08-30-01 | MEDIUM | GPU Memory Pressure | `MorphSlot::delta_buffer` holds mesh-static data but is allocated per-entity, with no residency cap, no telemetry, and no `memory-budget.md` row |
| PERF-D3-2026-08-30-02 | MEDIUM | GPU Memory Pressure | the 80 % DEVICE_LOCAL "approaching OOM" warning has exactly one caller — at engine init, before any cell loads — so it can never fire under the pressure it exists to detect |
| PERF-D4-2026-08-30-01 | MEDIUM | SSBO Sizing & Upload | the instance / previous-model / indirect dirty gates are defeated by the per-frame depth re-sort, so the documented steady-state saving only materialises with a parked camera |
| PERF-D4-2026-08-30-02 | MEDIUM | SSBO Sizing & Upload | `upload_terrain_tiles` uploads the full 1024-slot slab and blocks on a queue fence inside `draw_frame`, on every frame a terrain slot changes |
| PERF-D4-2026-08-30-03 | MEDIUM | SSBO Sizing & Upload | the bone-world staging memcpy and its GPU copy are O(high-water skin slots), not O(dirty slots), and the two-thirds of #1794 that was left undone has no live tracker |
| PERF-D5-2026-08-30-01 | MEDIUM | GPU Pipeline | the TLAS UPDATE gate compares an *ordered* BLAS-address sequence produced by the per-frame draw sort, so ordinary frustum churn forces a full BUILD |
| PERF-D5-2026-08-30-02 | MEDIUM | GPU Pipeline | `copy_depth_to_history` runs a full-render-resolution depth copy plus four layout barriers every frame for a feature most frames don't use, and no GPU timer covers it |
| PERF-D5-2026-08-30-03 | MEDIUM | GPU Pipeline | `memory-budget.md`'s G-buffer VRAM row understates the live attachment set by 4–10× and contradicts its own two columns |
| PERF-D6-2026-08-30-01 | MEDIUM | Skinning & BLAS | `SKINNED_BLAS_REFIT_THRESHOLD` has no per-entity stagger, so a cell's NPC cohort drops and rebuilds its skinned BLASes in lockstep every 601 dirty frames |
| PERF-D7-2026-08-30-01 | MEDIUM | Streaming & Cells | one dispatch batch parses the same NIF once per cell that references it, and the main thread then throws every duplicate away |
| PERF-D7-2026-08-30-02 | MEDIUM | Streaming & Cells | the interior cell load still runs its whole REFR + NPC spawn on an unlimited budget, though the resumable cursor #1798 deferred on now exists |
| PERF-D8-2026-08-30-01 | MEDIUM | NIF Parse | 73–81 % of the per-NIF CPU budget runs on the main thread — the streaming worker parallelises only the cheaper 15–30 % |
| PERF-D8-2026-08-30-02 | MEDIUM | NIF Parse | the dhat allocation gate stops at `parse_nif` — the import tier it should also cover is ~2× the peak live heap and 3–5× the CPU |
| PERF-D9-2026-08-30-01 | MEDIUM | Telemetry & Origin Cost | `between_frames_ms` is sampled after `draw_frame` returns, so it silently absorbs the entire in-engine render path it exists to exclude |
| PERF-D9-2026-08-30-02 | MEDIUM | Telemetry & Origin Cost (chronic scratch over-reserve) | `batches_scratch`'s per-frame `reserve()` and its end-of-frame shrink fight each other — two reallocations and a memcpy every frame on four of five baseline cells |
| PERF-D9-2026-08-30-03 | MEDIUM | Telemetry & Origin Cost | three per-frame GPU work items sit outside every `gpu_timers` bracket — including `skin_palette.comp`, which a sibling dimension's matrix records as covered |
| PERF-D1-2026-08-30-01 | LOW | CPU Hot Paths | the live animation path is the last unconverted per-frame per-entity SipHash keyspace — `AnimationClip.channels`, `NameIndex.map` and `SubtreeCache.map` are all `std::collections::HashMap` |
| PERF-D1-2026-08-30-02 | LOW | CPU Hot Paths | `reemit_water_planes` builds an entity→draw-slot index over **every** draw command each frame with no water-population early-out |
| PERF-D1-2026-08-30-03 | LOW | CPU Hot Paths | `apply_cell_region_ambient` re-resolves the REGN ambient directive every exterior frame — a `Vec` allocation plus a sort — and both the resource's own doc and the call site's cost comment say it does not |
| PERF-D1-2026-08-30-04 | LOW | CPU Hot Paths | the lock tracker materialises its `held_others` snapshot *before* the detector's own enabled check, so every ECS lock acquisition in a debug build heap-allocates while the code documents that path as "one relaxed load" |
| PERF-D2-2026-08-30-02 | LOW | Draw & Instancing | the #2691 note in `render/mod.rs` transcribed two derived counts, and both are now wrong — the exact rot its own last sentence warns against |
| PERF-D2-2026-08-30-03 | LOW | Draw & Instancing | the per-instance `GpuInstance` loop probes two `std::collections::HashMap`s per draw per frame, and the #3061 guard structurally cannot see them |
| PERF-D3-2026-08-30-03 | LOW | GPU Memory Pressure | `memory-budget.md` scene-buffer + texture-registry rows drift from the code on `MAX_LIGHTS`, `GpuTerrainTile` stride and descriptor-pool multiplicity |
| PERF-D3-2026-08-30-04 | LOW | GPU Memory Pressure | post-#2929 doc rot on the TLAS shrink path — two comments still assert `shrink_tlas_to_fit` destroys the slot |
| PERF-D4-2026-08-30-04 | LOW | SSBO Sizing & Upload | `CameraUBO` is the only hand-duplicated GPU struct with no field name/order/type lockstep test — it is pinned by size alone |
| PERF-D5-2026-08-30-04 | LOW | GPU Pipeline | the volumetrics per-froxel ray-budget annotation still quotes the divisor-4 grid retired four days after it was written |
| PERF-D5-2026-08-30-05 | LOW | GPU Pipeline | the volumetrics gate-off arm re-clears the whole integrated froxel volume every frame, with no already-cleared latch |
| PERF-D5-2026-08-30-06 | LOW | GPU Pipeline | the `svgf` GPU timer bracket is named for one dispatch but encloses four screen-sized ones |
| PERF-D5-2026-08-30-07 | LOW | GPU Pipeline | the render-pass construction comment still describes 7 attachments including a reservoir target retired by #1583/#1590 |
| PERF-D6-2026-08-30-02 | LOW | Skinning & BLAS | `update_morph_weights` heap-allocates a fresh `Vec<f32>` per morph slot per frame and unconditionally marks the slot dirty, discarding the right-sized `pending_weights` buffer `MorphSlot` already owns |
| PERF-D6-2026-08-30-03 | LOW | Skinning & BLAS | the `unsafe` push-constant slice in `SkinComputePipeline::dispatch` carries a SAFETY comment describing a 12-byte three-`u32` struct that has been 32 bytes with six fields since #3231, and cites a test name that does not exist |
| PERF-D7-2026-08-30-03 | LOW | Streaming & Cells | the LOD-coverage and terrain-seam diagnostics recompute from scratch on every reconcile frame, including two O(n²) scans, for two console commands |
| PERF-D7-2026-08-30-04 | LOW | Streaming & Cells | `PackedStorage::remove_entities_erased` reallocates and moves every *surviving* row per unload batch, so eviction cost is O(all resident rows), not O(victims) |
| PERF-D7-2026-08-30-05 | LOW | Streaming & Cells | `unload_cells` recomputes the whole-world cinematic-retention set once per victim cell, then hash-probes every victim against it even when it is empty |
| PERF-D8-2026-08-30-03 | LOW | NIF Parse | the skinning parse path — the parser's largest per-block allocation family — has zero gate coverage, and carries two unreserved growth sites |
| PERF-D9-2026-08-30-04 | LOW | Telemetry & Origin Cost | `between_frames` is the only `CpuFrameTimings` field the console `cpu_ms:` line omits — the remainder bucket is invisible to the headless triage surface |
| PERF-D9-2026-08-30-05 | LOW | Telemetry & Origin Cost | four declared per-frame renderer scratches are absent from `fill_scratch_telemetry`, against that function's own stated maintenance rule |
| PERF-D9-2026-08-30-06 | LOW | Telemetry & Origin Cost | `ScratchTelemetry` covers zero of the seven engine-binary per-frame scratches, including `draw_commands` — the largest per-frame Vec in the process |
---

## Hot Path Analysis

All figures below are **derived from checked-in constants, struct layouts and
the five runtime baselines** — no frame was sampled. Each row names the
telemetry hook that *would* measure it, so a future bench session can replace
derivation with measurement without re-deriving the map.

### Per-pass GPU coverage

Ordered as recorded in `crates/renderer/src/vulkan/context/draw.rs`
(`record_geometry_pass` → `copy_depth_to_history` → `record_post_passes`).
`WORKGROUP_X/Y/Z = 8`, `THREADS_PER_CLUSTER = 32`, `SKIN_WORKGROUP_SIZE = 64`
(`crates/renderer/src/shader_constants_data.rs`).

| Pass | Dispatch dimensionality | `gpu_timers` field | Timer |
|---|---|---|---|
| bone_world + bind_inverse staging copies | O(high-water skin slots) — **not** O(dirty) (`PERF-D4-2026-08-30-03`) | — | **NO** (`PERF-D9-…-03`) |
| `skin_palette.comp` | O(bones) | — | **NO** (`PERF-D9-…-03`) |
| `skin_vertices.comp` (per skinned entity) | O(vertices), `vertex_count.div_ceil(64)` | `skin_dispatch_ms` | yes |
| skinned BLAS first-sight build + refit | O(skinned entities) | `skin_blas_refit_ms` | yes |
| TLAS build / refit | O(TLAS instances) | `tlas_build_ms` | yes |
| `cluster_cull.comp` | O(clusters × lights), **fixed** 16×9×24 = 3 456 clusters | `cluster_cull_ms` | yes |
| main render pass | O(draws) × O(covered pixels) — the only legitimately mesh-scaled pass | `main_render_ms` | yes |
| `copy_depth_to_history` | O(pixels), full D32 copy + 4 barriers, **unconditional** | — | **NO** (`PERF-D5-…-02`) |
| `svgf_temporal.comp` + `svgf_atrous.comp` ×3 | O(pixels) × 4 | `svgf_ms` (name under-describes, `PERF-D5-…-06`) | yes |
| `caustic_splat.comp` | O(pixels) × 1–2 | `caustic_splat_ms` | yes |
| `volumetrics_inject.comp` | **O(froxels)** — off `froxel_extent`, never off draw count | `volumetrics_ms` | yes |
| `volumetrics_integrate.comp` | **O(froxel columns)** | `volumetrics_ms` (shared) | yes |
| `record_neutral_frame` (gate-off arm) | O(froxels) clear, **every frame, no latch** (`PERF-D5-…-05`) | — | no |
| `taa.comp` (TAA upscaler only) | O(pixels) | `taa_ms` | yes |
| `ssao.comp` | O(pixels) | `ssao_ms` | yes |
| composite | O(pixels), fullscreen triangle | `composite_ms` | yes |
| bloom (5 down + 4 up + apply) | **pure O(pixels)** per mip, `BLOOM_MIP_COUNT = 5` | `bloom_ms` | yes |
| FSR 3.1 upscale / native blit | O(output pixels) | `upscale_ms` | yes |
| presentation (exposure + ACES + UI quad) | O(output pixels) | `presentation_ms` | yes |
| `egui_pass` overlay | O(UI vertices) | — | no (`PERF-D9-…-03`; debug-only) |

**Verdict on the dimension's central invariant: no pass is O(meshes) except
the main render pass, where it is correct.** Every compute dispatch derives
its group count from a resolution, the fixed cluster grid, or `froxel_extent`
— never from `draw_commands.len()`. The volumetrics grid is confirmed
resolution-derived, not a fixed grid.

> **Intra-audit correction.** Dimension 5's own table recorded
> `skin_palette.comp` as covered by `skin_dispatch_ms`; Dimension 9 traced the
> bracket and found it opens inside `record_skinned_blas_refit`
> (`crates/renderer/src/vulkan/context/skinned_blas_refit.rs:379`), which is
> called *after* the palette dispatch and the two staging copies. The table
> above carries Dimension 9's corrected reading. This matters beyond
> bookkeeping: it means `PERF-D4-2026-08-30-03` (the O(high-water) bone-world
> copy) currently has **no instrument that could size or verify it**.

### G-buffer bandwidth (live, verified against `crates/renderer/src/vulkan/gbuffer.rs`)

7 attachments at **22 B/px** total: `normal` R16G16_SNORM (4),
`motion` R16G16_SFLOAT (4), `mesh_id` R32_UINT (4), `raw_indirect`
B10G11R11_UFLOAT (4), `albedo` B10G11R11_UFLOAT (4), `reactive` R8_UNORM (1),
`transparency` R8_UNORM (1). The expected 7→6→7 churn (`#1583`/`#1590` then
`f6d10838`) matches the live code — **no code-side bandwidth drift**. The
render pass adds HDR RGBA16F (8 B/px, per-FIF) plus a single D32 depth image
and one `depth_history` image.

| Set | 1920×1080 | 3840×2160 |
|---|---|---|
| GBuffer 7 attachments × 2 FIF | 91.2 MB | 365.0 MB |
| + HDR colour × 2 FIF | 124.4 MB | 497.7 MB |
| + depth + depth_history | **141.0 MB** | **564.0 MB** |

Against the ~4 GB total budget this is comfortable at 1080p and material at
4K. The drift is in the ledger, not the code — `docs/engine/memory-budget.md`
records `~23 MB / ~47 MB (4K)` for this row (`PERF-D5-2026-08-30-03`).

### Per-frame CPU phase coverage

`log_stats_system` (`byroredux/src/systems/debug.rs`) emits `cpu_ms:` at ~1 Hz
splitting the frame into `fence_wait` / `acquire` / `ssbo_build` /
`tlas_build` / `cmd_record` / `submit_present` / `geom_rebuild` / `rof_*` /
`atw_pre` / `atw_scheduler` / `atw_post`. Two structural properties matter for
anyone reading that line:

- **The split is hierarchical, not a partition, and the line does not say so.**
  `atw_post ⊇ rof_pre_draw + rof_draw_call + rof_post_draw`;
  `rof_draw_call ⊇ fence_wait + acquire + ssbo_build + tlas_build + cmd_record
  + submit_present`; `rof_pre_draw ⊇ geom_rebuild`. Summing the printed
  buckets double-counts. The nesting is documented only at
  `crates/core/src/ecs/resources/mod.rs`, not at the emission site.
- **The one bucket that would close the frame is both mis-measured and
  unprinted** — `between_frames` (`PERF-D9-2026-08-30-01` and `-04`).
- Collection cost is throttled and gated (`want_breakdown = slow_frame ||
  boundary`, `#2115` intact; `metrics_sample_system` behind
  `SAMPLE_PERIOD_SECS = 0.5`). Steady-state telemetry cost is a handful of
  `Instant::now()` calls — **no finding**.

`#3467` landed the full `take_geometry_rebuild_ns` → `geom_rebuild=` chain, so
`PERF-2026-08-27b-02`'s "no timer can measure the rebuild slice" premise is
now **stale** and was not re-filed.

### `ScratchTelemetry` coverage

`fill_scratch_telemetry` emits **13 rows**. Chronically over-reserved: exactly
one, and it is a tracked row — `batches_scratch`, whose `reserve()` argument
runs **13–19× its actual working set** on four of five baseline cells
(`PERF-D9-2026-08-30-02`):

| Baseline cell | `bench_draws_cmds` (the reserve) | `bench_draws_batches` (the fill) | over-reserve |
|---|---:|---:|---:|
| `fo4-InstituteBioScience.tsv` | 3949 | 296 | 13.3× |
| `fnv-FreesideAtomicWrangler.tsv` | 2110 | 109 | 19.4× |
| `fo3-MegatonPlayerHouse.tsv` | 1581 | 100 | 15.8× |
| `oblivion-ICMarketDistrictTheGildedCarafe.tsv` | 325 | 20 | 16.3× |

Untracked per-frame scratch: 4 in the renderer crate
(`PERF-D9-2026-08-30-05`) and **all 7** in the engine binary, including
`draw_commands` itself (`PERF-D9-2026-08-30-06`).

### Draw volume vs the parallel-sort gate

`DRAW_SORT_PARALLEL_THRESHOLD = 3000`, read from the checked-in baselines
rather than any remembered band (`#2691` retired the old "typical 400–1500"
figure):

| Baseline cell | entities | `bench_draws_cmds` | vs 3000 | batches | gpu_calls |
|---|---:|---:|:--:|---:|---:|
| oblivion GildedCarafe | 705 | 325 | below | 20 | 2 |
| fo3 MegatonPlayerHouse | 3 493 | 1 581 | below | 100 | 11 |
| fnv FreesideAtomicWrangler | 7 174 | 2 110 | below | 109 | 26 |
| skyrim_se WhiterunDragonsreach | 8 126 | 2 342 | below | 9 | 2 |
| fo4 InstituteBioScience | 18 256 | 3 949 | **above** | 296 | 16 |

But the gate is applied to `raster_len` (the in-frustum prefix), while
`bench_draws_cmds` is `draw_commands.len()` — the whole array including the
RT-only tail. **The gated quantity is not measured anywhere in the repo**
(`PERF-D2-2026-08-30-01`), so "fo4 takes the parallel path" is an inference
from a different quantity, not a verified fact.

---

## Cross-dimension relationships

Duplicates were removed at consolidation; these are distinct findings that
share a root cause or a fix, and should be scheduled together.

1. **One unstable sort, two victims.** `PERF-D4-2026-08-30-01` (instance /
   previous-model / indirect upload dirty gates miss) and
   `PERF-D5-2026-08-30-01` (TLAS UPDATE gate falls back to full BUILD) are
   both downstream of the same property: the per-frame draw order is not
   frame-stable, because the within-`mesh_handle` tiebreaker in
   `draw_sort_key` is an unquantised `f32_sortable_u32(clip.w)` and the
   raster/RT-only partition is an unstable swap. **Quantising the depth
   tiebreaker into buckets is one change that pays both**, and is the single
   highest-leverage item in this report.
2. **Skinning-path cost has no instrument.** `PERF-D4-2026-08-30-03`
   (bone-world copy is O(high-water slots)) cannot be sized until
   `PERF-D9-2026-08-30-03` (no GPU bracket around the palette dispatch and
   the staging copies) is fixed. Fix the telemetry first.
3. **Morph-target path, two independent costs.** `PERF-D3-2026-08-30-01`
   (per-entity VRAM duplication of mesh-static delta buffers, uncapped) and
   `PERF-D6-2026-08-30-02` (per-frame `Vec<f32>` weight allocation). The
   second is the half of `PERF-D6-2026-08-24-01` that fell through `#3061`'s
   close — `#3061` was scoped to the hashing conversion only and the
   allocation half was never filed.
4. **Residual hot-path SipHash after `#3051`/`#3061`.**
   `PERF-D1-2026-08-30-01` (animation channels / name index / subtree cache)
   and `PERF-D2-2026-08-30-03` (`handle_avg_rgb` / `handle_has_alpha`, probed
   per draw per frame on a *dense index*). Neither is visible to the existing
   guard: the `context/mod.rs` assertion covers only `SkinSlotPool`'s
   collections, and `#3061`'s source scan covers only
   `context/{mod,init,draw,skinned_blas_refit}.rs`. Both are LOW, but the
   *guard gap* is the durable part.
5. **`memory-budget.md` ledger drift, three separate rows.**
   `PERF-D5-2026-08-30-03` (G-buffer, 4–10× low), `PERF-D3-2026-08-30-03`
   (`MAX_LIGHTS` 512→1023, `GpuTerrainTile` 32→96 B, descriptor-pool
   multiplicity). Siblings of the already-open `#3463`, `#3447`, `#3450`.
   The skill instructs auditors not to re-derive these ceilings — which makes
   every drifted row a false premise handed to the next auditor.
6. **Streaming teardown, two O(resident) passes on the unbudgeted crossing
   frame.** `PERF-D7-2026-08-30-04` (`remove_entities_erased` moves every
   *surviving* row) and `PERF-D7-2026-08-30-05` (cinematic-retention set
   recomputed per victim).

### Existing issues re-confirmed live (deduplicated, not re-filed)

- `#2764` — `is_refractive_glass` does not gate on `alpha_blend`, so opaque
  `MULTI_LAYER_PARALLAX` draws carry `order_dependent_glass` into
  `group_state` and split indirect groups with identical pipeline state.
- `#3142`, `#3463`, `#3510`, `#2689`, `#3246`, `#3447`, `#3450` — all checked;
  premises still hold; excluded per the dedup rule.
- `#1797` (shared `blas_scratch_buffer` serializes N dirty skinned entities)
  and `#1793` (missing-rigid-BLAS recovery; multi-cell false-evict) verified
  present as documented-not-fixed, and **not** re-reported.

---

## Findings

Grouped by severity, CRITICAL first. **No CRITICAL and no HIGH findings were
produced by this sweep**, and no landed guard was found eroded — the
eroded-guard section is therefore empty.

### PERF-D2-2026-08-30-01: the parallel-sort gate reads `raster_len`, but no checked-in metric measures it — the "fo4 crosses the gate" claim is unfalsifiable

- **Severity**: MEDIUM
- **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/mod.rs:548-573` (`sort_draw_commands`), `byroredux/src/render/mod.rs:818-825` (the #2691 note), `byroredux/src/bench.rs:345` + `byroredux/src/bench.rs:530-532`, `.claude/audit-baselines/runtime/*.tsv`
- **Status**: NEW
- **Description**: `sort_draw_commands` partitions RT-only occluders to the tail and then applies the
  3000-element gate to **`raster_draws = &mut draw_commands[..raster_len]`** — the in-frustum prefix.
  The only draw-volume figure any baseline records is `bench_draws_cmds`, which is
  `draw_commands.len()` — the whole array, RT-only tail included. `raster_len` is computed, returned,
  and then used for nothing but a `BYRO_PROFILE`-gated log string (`render/mod.rs:840,868`); it is
  never written into `DebugStats` (`app_frame.rs:238` sets `draw_command_count` from
  `self.draw_commands.len()`) and is not in `REQUIRED_METRICS` (`bench.rs:519-533`). Consequently the
  in-source claim that "the FO4 baseline is *above* this gate and **takes the parallel path**"
  (`render/mod.rs:823-825`), and the prior sweep's "verified INTACT" restatement of it
  (`docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md`, Dimension 2 bullet 2), are inferences from a metric
  that measures a different quantity. On an interior cell with meaningful frustum culling the raster
  prefix can sit well below 3949 — nothing in the repo says whether the parallel branch has ever
  actually executed.
- **Evidence**:
  ```rust
  // byroredux/src/render/mod.rs:564-570
  const DRAW_SORT_PARALLEL_THRESHOLD: usize = 3000;
  let raster_draws = &mut draw_commands[..raster_len];
  if raster_draws.len() >= DRAW_SORT_PARALLEL_THRESHOLD {
      raster_draws.par_sort_unstable_by_key(draw_sort_key);
  ```
  vs. the producer of the recorded metric:
  ```rust
  // byroredux/src/bench.rs:345
  draws: draw_commands.len() as u32,
  ```
  The value **is** already computed — `render/mod.rs:868` prints
  `"... ({n_draws} draws, {raster_draws} raster) ..."` — but only under `BYRO_PROFILE=1`, which
  `/audit-runtime` does not set, so it never reaches a TSV.
  The in-tree calibration harness `manual_bench_draw_sort_serial_vs_parallel`
  (`byroredux/src/render/draw_sort_key_tests.rs:494-552`) also sorts a full `Vec<DrawCommand>` of
  size N with no in-raster partition, so its N axis is the same "all commands" quantity, not the
  quantity the gate reads.
- **Impact**: No runtime defect. The consequence is that a live tuning constant on the per-frame
  render path cannot be validated or invalidated from the repo's own telemetry, and two written
  records (production source and the previous audit's guard section) assert a branch selection that
  no measurement supports. The next person to touch `DRAW_SORT_PARALLEL_THRESHOLD` reasons from a
  column that is an upper bound of unknown tightness — on a heavily-culled interior it could be 2×
  the gated quantity.
- **Related**: #2691 / PERF-D2-03 (the prose this note replaced), #934 / PERF-DC-01, #2173, `883f57cd`; #516 (the in-raster/TLAS split that introduced the divergence); `docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md` Dimension 2.
- **Suggested Fix**: Add one row — `bench_draws_raster_cmds` — to the bench summary line and to
  `REQUIRED_METRICS` in `bench.rs`, sourced from `sort_draw_commands`'s existing return value (thread
  it into `DebugStats` alongside `draw_command_count`), then regenerate the five baselines and
  restate the note in `render/mod.rs` against that column. Until that row exists, the note should say
  the gated quantity is unmeasured rather than assert which branch fo4 takes.

---

### PERF-D3-2026-08-30-01: `MorphSlot::delta_buffer` holds mesh-static data but is allocated per-entity, with no residency cap, no telemetry, and no `memory-budget.md` row
- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:727-750` (creation),
  `byroredux/src/cell_loader/spawn/mesh_instance.rs:1107-1127` (`flatten_morph_targets`),
  `crates/renderer/src/vulkan/morph_compute.rs:114-166` (`MorphSlot::create`),
  `crates/renderer/src/vulkan/context/mod.rs:1523-1524` (`morph_slots` map)
- **Status**: NEW
- **Description**: `MorphSlot` (#3231, GPU morph-target blending) owns two buffers:
  a DEVICE_LOCAL `delta_buffer` and a host-visible `weight_buffer`. Only
  `weight_buffer` is genuinely per-entity — `MorphSlot::create` writes
  `delta_buffer` exactly once and nothing mutates it afterwards
  (`flush_pending_weights` / `upload_weights` touch only `weight_buffer`,
  `morph_compute.rs:201`). Its contents derive entirely from
  `ImportedMesh::morph_targets`, i.e. from the mesh, not the placement.

  The slot is nevertheless created **per spawned entity**, unconditionally, in
  the per-REFR mesh-spawn path. This is the exact asymmetry the mesh side already
  solves: the same `ImportedMesh` that N REFRs share resolves to **one** refcounted
  GPU mesh via `MeshRegistry::acquire_cached` (`crates/renderer/src/mesh.rs:814`),
  while its morph deltas are copied into VRAM N times.

  Three amplifiers, all verifiable at HEAD:
  1. `flatten_morph_targets` builds a **dense** `target_count × vertex_count`
     array of `[f32; 4]` — 16 B per (target, vertex) — zero-filling every target
     a sparse `NiGeomMorpherController` does not populate, and sizing
     `target_count` as `max(original_index) + 1` rather than the number of
     targets actually present.
  2. There is **no cap** on `morph_slots`. `SkinSlot` has `SKIN_MAX_SLOTS`
     (`crates/renderer/src/vulkan/context/mod.rs:81`, derived from
     `MAX_TOTAL_BONES`); the morph sibling has nothing — creation's only gate is
     `mesh.skin.is_some() && !morph_targets.is_empty()`, as
     `skinned_blas_refit.rs:781` itself notes.
  3. Creation is at **spawn**, not first draw, and `should_evict_skin_slot`
     returns `false` for the `last_used_frame == 0` sentinel
     (`crates/renderer/src/vulkan/skin_compute.rs:238-244`). A placed-but-never-drawn
     morph mesh therefore holds its delta buffer for the whole cell residency;
     only `pending_morph_unload_victims` at cell unload
     (`byroredux/src/cell_loader/unload.rs:291-295`) reclaims it.
- **Evidence**:
  ```rust
  // byroredux/src/cell_loader/spawn/mesh_instance.rs:727-741 — per world.spawn()
  if mesh.skin.is_some() {
      if let Some(morph_targets) = mesh.morph_targets.as_ref().filter(|t| !t.is_empty()) {
          let (deltas, target_count) = flatten_morph_targets(morph_targets, mesh.positions.len());
          match MorphSlot::create(upload_ctx, &deltas, target_count, vertex_count) {
              Ok(slot) => { ctx.morph_slots.insert(entity, slot); }
  ```
  ```rust
  // byroredux/src/cell_loader/spawn/mesh_instance.rs:1116 — dense, not sparse
  let mut deltas = vec![[0.0; 4]; target_count as usize * vertex_count];
  ```
  Per-entity DEVICE_LOCAL cost is exactly
  `target_count × vertex_count × 16 B`. Worked arithmetic (formula, not a
  measurement): a 3 000-vertex head with 20 targets = `20 × 3000 × 16` =
  **960 KB per entity**; ten such placements sharing one model = **9.6 MB**, of
  which 8.64 MB is a byte-identical duplicate of the first.
- **Impact**: VRAM scaling with *placement* count instead of *unique mesh* count
  on exactly the content class (skinned actors, morph-driven props) that is
  densest in the scenes this dimension cares about. Invisible when it happens:
  `grep -i morph` over `docs/engine/memory-budget.md` returns nothing — the
  ledger has no row for it at all — and no console command or `SkinCoverageStats`
  field reports slot count or bytes, so this allocation is unattributable in
  `ctx.scratch`, `mem.frag`, or `skin.coverage`. Bounded (cell unload drains it),
  so a peak problem, not a leak. Same class as the ReSTIR (#1814) and
  SVGF/bloom/caustic (#1872) ledger gaps this doc already documents as findings.
- **Related**: #3231 (feature), #3374 (the eviction sweep that bounds it),
  `SKIN_MAX_SLOTS` precedent. Adjacent-but-out-of-scope observation: the
  `morph_slot.last_used_frame` bump at `skinned_blas_refit.rs:401-403` is nested
  inside the `skin_slots.get_mut(&entity_id)` arm, so an entity with a `MorphSlot`
  but no `SkinSlot` ages out at `min_idle = 3` and, creation being spawn-only,
  never comes back — a correctness question for `/audit-renderer`, not memory
  pressure (it reclaims rather than pressures).
- **Suggested Fix**: Key the delta half by mesh instead of entity — a refcounted
  `FxHashMap<u32 /*mesh_handle*/, Arc<DeltaBuffer>>` mirroring
  `MeshRegistry::acquire_cached`, leaving `weight_buffer` per-entity. Independently,
  add a `morph_slots` row (count + bytes) to `memory-budget.md` and to
  `SkinCoverageStats` so the figure is observable before it is optimised.

---

### PERF-D3-2026-08-30-02: the 80 % DEVICE_LOCAL "approaching OOM" warning has exactly one caller — at engine init, before any cell loads — so it can never fire under the pressure it exists to detect
- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/allocator.rs:289-334`
  (`warn_threshold_bytes`, `log_memory_usage`),
  `crates/renderer/src/vulkan/context/resources.rs:430-433` (wrapper),
  `byroredux/src/app_events.rs:204` (sole call site),
  `docs/engine/memory-budget.md:555-560`
- **Status**: NEW
- **Description**: `memory-budget.md` closes the VRAM ledger with "A warning fires
  when total allocated bytes exceed 80% of the smallest DEVICE_LOCAL heap
  (`(heap / 5) * 4`, with a 2 GB fallback when no DEVICE_LOCAL heap is reported)."
  The formula is exactly right and matches
  `warn_threshold_bytes` verbatim. The *firing* is not: a workspace-wide grep
  finds `log_memory_usage` reachable from precisely one place —
  `App::resumed`, immediately before `log::info!("Engine ready — entering game loop")`.
  `step_streaming` and `step_debug_loads` run from `about_to_wait`
  (`app_events.rs:706, 715`), i.e. strictly after that sample, and nothing
  re-takes it: not per frame, not per cell load, not per cell unload, and not
  from any console command (`ctx.memory` / `mem.frag` read
  `generate_report` directly and never consult the threshold;
  `commands/world_info.rs:761-780`). The debug-UI metrics sampler
  (`byroredux/src/systems/metrics.rs:110-130`) does sample VRAM every tick, but
  it compares nothing, logs nothing, and uses `GpuMemoryBudget::total_vram_bytes`
  (the **sum** of DEVICE_LOCAL heaps) rather than `smallest_heap_bytes`, which is
  the tighter cap the guard was written against.
- **Evidence**:
  ```
  $ grep -rn "log_memory_usage" --include="*.rs" crates byroredux
  crates/renderer/src/vulkan/allocator.rs:304:pub fn log_memory_usage(          # definition
  crates/renderer/src/vulkan/context/resources.rs:432:    …allocator::log_memory_usage(  # wrapper
  byroredux/src/app_events.rs:204:  self.renderer.as_ref().unwrap().log_memory_usage();  # only caller
  ```
  ```rust
  // byroredux/src/app_events.rs:203-205
  self.scheduler.run(&self.world, 0.0);
  self.renderer.as_ref().unwrap().log_memory_usage();
  log::info!("Engine ready — entering game loop");
  ```
- **Impact**: The engine has no live VRAM-pressure signal. Every mechanism this
  dimension audits — BLAS LRU eviction, TLAS/scratch shrink, staging-pool trim,
  the texture bindless ceiling — degrades *quietly* by design (evict, fall back to
  the checkerboard handle, keep the oversized buffer), so the 80 % warn was the
  one place a session was told it was approaching the heap. On the 12 GB dev card
  the boot sample is ~0.3 GB against a ~9.6 GB threshold; on the 6 GB RT-minimum
  target the same sample is taken before the content that would breach 4.8 GB
  exists. Defense-in-depth gap, not a crash — hence MEDIUM.
- **Related**: #505 (which introduced the heap-scaled threshold precisely because
  the old 2 GB constant "warned on every large cell load" — that observation
  implies a call frequency the code no longer has). #2030's
  `check_slot_available` 90 % one-shot latch is the pattern that *does* work,
  because it sits on the allocation path.
- **Suggested Fix**: Call `log_memory_usage` from the cell-load / cell-unload
  boundary (`cell_loader::unload_cells`' finalization, or the end of
  `step_streaming`) with a one-shot `Once` latch on the WARN arm so a sustained
  breach logs once rather than per transition — the same shape
  `check_slot_available` already uses. Optionally reword
  `memory-budget.md:555-560` to say *when* it samples.

---

### PERF-D4-2026-08-30-01: the instance / previous-model / indirect dirty gates are defeated by the per-frame depth re-sort, so the documented steady-state saving only materialises with a parked camera
- **Severity**: MEDIUM
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:557-566` (`upload_instances`
  gate), `:612-616` (`upload_previous_models`), `:730-736` (`upload_indirect_draws`);
  `byroredux/src/render/mod.rs:519-530` (the opaque `draw_sort_key` arm) and `:545-571`
  (`sort_draw_commands`); `byroredux/src/render/static_meshes.rs:409` (`sort_depth`
  computation); `crates/renderer/src/vulkan/context/draw.rs:2801-2806`, `:3056`, `:3311-3319`
  (instances built in sorted order, then uploaded)
- **Status**: NEW
- **Description**: The three sibling dirty gates skip the copy + flush when the current
  frame's slice hashes byte-identical to the last one written into this frame-in-flight slot.
  Their docstrings justify the win as *"static interiors produce byte-identical slices each
  frame"*. That premise does not hold whenever the camera moves. `gpu_instances` is built by
  walking `draw_commands` **after** `sort_draw_commands`, and the opaque sort key's
  penultimate component is `cmd.sort_depth` — a full-precision `f32`-to-sortable-`u32`
  reinterpretation of clip-space `w`, recomputed per draw per frame. Any camera translation or
  rotation that inverts the depth order of two draws sharing a `mesh_handle` permutes their
  `GpuInstance` entries. The bytes are the same multiset; the *slice* is not, so the hash
  differs and all three gates miss.

  Instanced batching guarantees the collision case is the common one: the whole point of
  grouping on `mesh_handle` is that real cells place the same mesh many times, and those are
  exactly the draws whose relative order `sort_depth` arbitrates. The per-instance payload is
  otherwise stable under camera motion — `GpuInstance.model` is render-origin-relative and the
  origin only re-snaps on a cell-grid crossing (`RENDER_ORIGIN_SNAP`), and
  `texture_index` / `material_id` / `flags` do not depend on the view.

  The codebase already documents the reordering, in the field that exists to work around it:
  `GpuInstance.surface_id` is *"Stable draw identity used by temporal direct-shadow reservoirs.
  Unlike the per-frame instance-buffer index, this follows the ECS entity when **depth sorting**
  or animated actors **reorder draw commands**"* (`gpu_types.rs:157-160`), pinned by
  `restir_history_uses_stable_surface_id_not_instance_order`. So one part of the renderer treats
  per-frame instance reordering as a given while another part's optimisation assumes it away.
- **Evidence**:
  ```rust
  // byroredux/src/render/mod.rs:519-530 — opaque arm of draw_sort_key
  (rt_only, 0u8, 0u8, cmd.render_layer as u32, cmd.two_sided as u32, 0, 0,
   pack_depth_state(cmd) as u32,
   cmd.mesh_handle, // group identical meshes
   cmd.sort_depth,  // front-to-back within group   <-- view-dependent, recomputed per frame
   cmd.entity_id)
  ```
  ```rust
  // byroredux/src/render/static_meshes.rs:409
  let sort_depth = f32_sortable_u32(clip.w);   // f32 bit pattern, no quantisation
  ```
  ```rust
  // crates/renderer/src/vulkan/scene_buffer/upload.rs:566
  let hash = hash_instance_slice(&instances[..count]);
  if self.last_uploaded_instance_hash[frame_index] == Some(hash) { return Ok(()); }
  ```
- **Impact**: In gameplay the gates are not merely ineffective, they are net negative: a miss
  pays the full `FxHasher` pass over the slice **and** the memcpy + flush it was meant to
  avoid. At the docstring's own MedTek reference workload (7 359 draws) the per-frame read
  overhead is `7359 × (160 + 64 + 20) B` ≈ 1.80 MB hashed, on top of the same 1.80 MB copied —
  ~108 MB/s each at 60 fps. The documented ~54 MB/s saving is realised only while the camera is
  completely still (menus, `--bench-hold`, a parked bench camera), which is also the only state
  the gates were ever observed in. Nothing renders incorrectly.
- **Related**: #1134 (`upload_instances` gate), #878 (`upload_materials` gate), #1809
  (`upload_indirect_draws` gate), #2036 (`upload_lights` gate), #2692 (the 112→128 B figure
  correction in the same docstring, now itself 160 B). Distinct from #3246 (animated material
  float bits in the *material* dedup key — a different gate and a different mechanism).
  `PERF-D2-2026-08-30-01/-02` (this session) cover the sort's *cost*, not its effect on the
  upload gates.
- **Suggested Fix**: Quantise the opaque tiebreaker — replace `cmd.sort_depth` at slot 9 with a
  coarse bucket (e.g. the top 8–10 bits of the sortable `u32`) so sub-bucket camera motion no
  longer permutes the slice while front-to-back early-Z ordering is preserved at the granularity
  that actually matters. Failing that, correct the three docstrings so the next reader does not
  budget for a saving that only exists at rest.

---

### PERF-D4-2026-08-30-02: `upload_terrain_tiles` uploads the full 1024-slot slab and blocks on a queue fence inside `draw_frame`, on every frame a terrain slot changes
- **Severity**: MEDIUM
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:771-863`
  (`upload_terrain_tiles`), `crates/renderer/src/vulkan/context/draw.rs:3361-3374` (the call,
  inside `draw_frame` after `begin_command_buffer` at `:1872` and before `record_geometry_pass`
  at `:3642`), `crates/renderer/src/vulkan/context/resources.rs:15-27` (`fill_terrain_tiles`),
  `crates/renderer/src/vulkan/context/init.rs:1471`
  (`terrain_tiles: vec![None; scene_buffer::MAX_TERRAIN_TILES]`),
  `crates/renderer/src/vulkan/texture.rs:826` (`wait_for_fences(&[fence], true, u64::MAX)`)
- **Status**: NEW (sibling of **CLOSED #2719**, the same defect class fixed for the UI overlay)
- **Description**: This is the one upload in the whole scene-buffer path whose byte count is
  `O(MAX_*)` rather than `O(live)`. `terrain_tiles` is a fixed 1024-entry `Vec<Option<_>>`;
  `fill_terrain_tiles` does `dest.extend(tiles.iter().map(|t| t.unwrap_or_default()))`, so the
  scratch is always exactly 1024 × 96 B = 98 304 B no matter how many slots are occupied. One
  tile is allocated per terrain quad (`byroredux/src/cell_loader/terrain.rs:639`), so a
  radius-3 exterior grid holds on the order of 49 live tiles — roughly 95 % of every upload is
  `GpuTerrainTile::default()` padding.

  The bigger cost is the delivery mechanism. Each dirty upload creates a fresh 98 KB staging
  buffer (`create_staging_buffer` → create + allocate + bind + map), memcpys into it, then calls
  `with_one_time_commands`, which submits to `graphics_queue` and does a blocking
  `vkWaitForFences(…, u64::MAX)` — draining everything previously submitted on that queue —
  before destroying the staging buffer and its fence. All of this runs on the render thread in
  the middle of `draw_frame`'s command recording. `terrain_tiles_dirty` is set by **every**
  `allocate_terrain_tile` and `free_terrain_tile` call (`resources.rs:65`, `:49`), so the upload
  fires on every frame in which any terrain slot was touched — i.e. a run of consecutive frames
  during exterior grid streaming and cell unload, not the "few times a minute" the function's own
  comment assumes.
- **Evidence**:
  ```rust
  // context/resources.rs:15-27 — always 1024 entries, live count irrelevant
  dest.clear();
  dest.extend(tiles.iter().map(|t| t.unwrap_or_default()));
  ```
  ```rust
  // upload.rs:805-847 — transient staging per upload, then a blocking submit
  let (staging_buffer, staging_alloc) =
      super::super::buffer::create_staging_buffer(device, allocator, byte_size, "terrain_tile_staging")?;
  …
  let result = super::super::texture::with_one_time_commands(device, queue, command_pool, |cmd| { … });
  ```
  ```rust
  // upload.rs:792-796 — the comment the frequency claim rests on
  // "Terrain tile uploads run at cell-transition frequency (a few times
  //  a minute at most), so skip the StagingPool reuse overhead"
  ```
- **Impact**: A full graphics-queue drain plus a buffer create/allocate/bind/map/destroy cycle,
  on the render thread, on each frame of an exterior cell crossing — precisely the frames whose
  frame time already spikes from streaming. This is the same failure mode as CLOSED #2719 ("UI
  overlay allocates a fresh VkImage and does a blocking one-time submit every frame"), which the
  project treated as worth fixing at a comparable payload size.
- **Related**: #2719 (closed, same class), #497 / #470 (the design this function implements),
  #2463 (the `GpuTerrainTile` 96 B pin), `PERF-D3-2026-08-30-03` (the 32 B doc row).
- **Suggested Fix**: Track a live high-water slot index and upload only `[0 ..= high_water]`;
  take the staging buffer from the existing `StagingPool` rather than creating one per call;
  and record the copy into the frame's own command buffer with a TRANSFER→SHADER_READ barrier
  (as `record_bone_world_copy` already does) instead of a synchronous one-time submit.

---

### PERF-D4-2026-08-30-03: the bone-world staging memcpy and its GPU copy are O(high-water skin slots), not O(dirty slots), and the two-thirds of #1794 that was left undone has no live tracker
- **Severity**: MEDIUM
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:231-262`
  (`upload_bone_worlds`), `:269-315` (`record_bone_world_copy`), `:216-229` (the deferral
  comment), `byroredux/src/render/skinned.rs:138-149` (`required_slots =
  (pool.max_used_slot() + 1) * MAX_BONES_PER_MESH`),
  `crates/core/src/ecs/resources/skin_slot_pool.rs:355-357` (`max_used_slot`),
  `crates/renderer/src/vulkan/context/draw.rs:2473-2497` (the #1811 streak gate)
- **Status**: NEW (**Regression-adjacent to CLOSED #1794**: that issue's own "three-fold" cost
  was closed with only the CPU-fill third fixed; the code comment says so and names no successor)
- **Description**: `bone_world` is a sparse array indexed by `slot_id × MAX_BONES_PER_MESH`
  (144), sized to `(max_used_slot + 1) × 144` matrices. `upload_bone_worlds` memcpys **the whole
  array** into the per-FIF staging buffer and `record_bone_world_copy` issues a single
  `cmd_copy_buffer` for the same byte count, which `draw.rs` then also uses to size the
  `skin_palette.comp` dispatch (`bone_input_upload_bytes`). The only gate is #1811's
  `clean_skin_frames` streak: it skips the trio entirely while **no** pose changed, and runs the
  full-width trio as soon as **any** pose changed. There is no per-slot granularity.

  So one walking NPC in a scene whose pool high-water mark is 260 slots costs
  `261 × 144 × 64 B` ≈ 2.4 MB of host memcpy **plus** 2.4 MB of device copy **plus** a
  proportionally-sized compute dispatch, every frame, to deliver 9 KB of changed data. The
  information needed to narrow it already exists and is already threaded to the renderer:
  `SkinSlotPool::pose_dirty()` is passed into `FrameInputs.pose_dirty` (`app_frame.rs:540`) and
  is consumed per entity by `record_skinned_blas_refit` (`skinned_blas_refit.rs:412`, `:613`) —
  the bone-world copy is the one consumer that ignores it.

  Two independent multipliers, not one. `upload.rs:216-229` names only the *within-slot* one
  ("the full `MAX_BONES_PER_MESH`-wide stride … most of which is padding tail beyond that
  entity's own bone count") and defers it as "the remaining two-thirds of the issue's
  three-fold cost". The *across-slot* multiplier — copying clean slots at all — is not named in
  that comment, in #1794's body, or anywhere else. `max_used_slot` is a high-water figure that
  `sweep` only compacts when the freed set includes the contiguous top of the range, so a single
  surviving high-numbered entity keeps the full span alive.
- **Evidence**:
  ```rust
  // byroredux/src/render/skinned.rs:148-149
  let required_slots = (pool.max_used_slot() as usize + 1) * MAX_BONES_PER_MESH;
  bone_world_out.resize(required_slots, IDENTITY_4X4);
  ```
  ```rust
  // upload.rs:238-259 — no dirty-slot narrowing; whole array, one range
  let count = bone_world.len().min(MAX_TOTAL_BONES);
  let byte_size = (std::mem::size_of::<[[f32; 4]; 4]>() * count) as vk::DeviceSize;
  std::ptr::copy_nonoverlapping(bone_world.as_ptr() as *const u8, world_mapped.as_mut_ptr(), byte_size as usize);
  world_buf.flush_range(device, 0, byte_size)?;
  self.bone_input_upload_bytes[frame_index] = byte_size;
  ```
  ```rust
  // crates/core/src/ecs/resources/skin_slot_pool.rs:355-357
  pub fn max_used_slot(&self) -> u32 { self.next_slot.saturating_sub(1) }
  ```
  Closed #1794's own impact line, for the same workload: *"≥261 slots × 9216 B ≈ 2.4 MB/frame
  ≈ 144 MB/s sustained host-write + flush + GPU copy at 60 fps, most of it identity padding.
  Full-pool worst case (1365 slots) ≈ 12.6 MB/frame ≈ 755 MB/s."* Those figures still stand for
  the memcpy + copy halves; only the CPU identity-refill was removed (verified INTACT this
  session by sibling Dimension 1).
- **Impact**: The dominant per-frame host→device transfer in any scene with actors — larger
  than the instance SSBO — and it scales with skinned-entity *density and slot fragmentation*
  rather than with how much actually animated. It is also the sizing input for the
  `skin_palette.comp` dispatch, so the wasted bytes buy wasted GPU threads too. Nothing renders
  incorrectly. The process problem is the same one #1794 was filed about: a code comment
  deferring to work that no open issue owns, now one level deeper (it defers to a closed issue's
  unfinished remainder).
- **Related**: #1794 (closed, 1/3 delivered), #1811 (the frame-streak gate), #1284 (the
  `MAX_TOTAL_BONES` bumps), #2923 (`pose_dirty` kept `FxHashSet` across the crate boundary —
  the very set this path should be reading), M29.5/M29.6.
- **Suggested Fix**: Two separable steps. (1) Cheap and self-contained: build per-slot
  `vk::BufferCopy` regions from `pose_dirty` + `SkinSlotPool::entity_to_slot`, with a per-FIF
  "slot last written in this slot's buffer" stamp so a slot dirtied once is refreshed into both
  FIF copies (the same `MAX_FRAMES_IN_FLIGHT` safety margin `clean_skin_frames` already uses).
  (2) The #1794 remainder: plumb per-slot `skin.bones.len()` into `FrameInputs` so each region
  covers only the real prefix. Either way, re-file the untracked remainder so the comment stops
  pointing at a closed issue.

---

### PERF-D5-2026-08-30-01: the TLAS UPDATE gate compares an *ordered* BLAS-address sequence produced by the per-frame draw sort, so ordinary frustum churn forces a full BUILD
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs:218-228`
  (`decide_use_update`); `crates/renderer/src/vulkan/acceleration/tlas.rs:99-113`
  and `:445-463` (`build_tlas_instances`); producer
  `byroredux/src/render/mod.rs:546-572` (`sort_draw_commands`) and `:407-533`
  (`draw_sort_key`)
- **Status**: NEW
- **Description**: `decide_use_update` picks UPDATE (cheap refit) only when the
  current frame's BLAS device-address list is **element-for-element equal, in
  order**, to the list captured at the last BUILD. That list is materialised by
  `build_tlas_instances`, which walks `draw_commands` in exactly the order
  `sort_draw_commands` left them. That order is not frame-stable:
  * `sort_draw_commands` first runs an **unstable** in-place partition that hoists
    `in_raster` draws to the front. A single entity crossing the frustum boundary
    both moves the raster/RT-only boundary *and* re-permutes the RT-only tail
    (the tail order is a side effect of the swap sequence, and the function
    deliberately does not sort it).
  * The raster prefix is then fully re-sorted. In the opaque arm `mesh_handle`
    outranks `sort_depth`, so cross-mesh opaque order is stable — but the
    alpha-over arm is **depth-primary** (`!cmd.sort_depth` sits at slot 4, above
    `mesh_handle`), by deliberate design for the `#1804`/`#2237` compositing fix.
    `sort_depth` is `f32_sortable_u32(clip.w)` — full precision, unquantised — so
    any two TLAS-eligible transparents of different meshes swap places the moment
    the camera crosses their bisector.

  Neither churn source bumps `blas_map_generation`, so `decide_use_update` reaches
  its `layout_matches` zip-compare, fails it, and returns BUILD. The comparison
  itself is O(N) and, in this regime, is O(N) work spent proving that a BUILD is
  needed.
- **Evidence**: `predicates.rs:223-228`
  ```rust
  let layout_matches = cached_addresses.len() == current_addresses.len()
      && cached_addresses
          .iter()
          .zip(current_addresses.iter())
          .all(|(a, b)| a == b);
  (layout_matches, true)
  ```
  `tlas.rs:445-463` builds `instances` by iterating `draw_commands` in the sorted
  order and pushes `acceleration_structure_reference` per surviving entry;
  `mod.rs:546-562` is the unstable partition (`draw_commands.swap(raster_len, index)`)
  whose doc comment states outright that the tail is left unsorted.
- **Impact**: The engine's own instrumentation states the intent this defeats —
  `GpuTimerSnapshot::tlas_build_ms`'s doc comment
  (`crates/renderer/src/vulkan/gpu_timers.rs:141-144`) says "First-cell-load frames
  spike (full BUILD); steady-state should report an UPDATE-mode refit in the
  sub-millisecond range." Under camera motion — i.e. all of normal play — the
  address permutation changes and the refit path is not taken. Blast radius is
  every RT frame on every game; magnitude scales with TLAS instance count, so
  dense FO4/Skyrim city cells pay most. There is also **no build-vs-update counter
  anywhere in the telemetry surface** (`memory.rs` exposes only sizes), so the
  only current way to see this is `tlas_build_ms` — which is exactly why it has
  gone unnoticed.
- **Related**: `AUDIT_PERFORMANCE_2026-05-10.md:175` recorded "Static cells = REFIT
  every frame after the first. **Confirmation, not a finding.**" — that observation
  was made against a *static* camera and predates the depth-primary alpha arm
  (`RT-1`/`#2215` note in `draw_sort_key`) and `#2682`'s partition rework. Open
  `#2367` (FO4 ~33–34% slower) is a plausible but unproven consumer of this; it is
  **not** claimed here.
- **Suggested Fix**: TLAS instance *order* is semantically irrelevant — ray hits
  resolve through `instance_custom_index`, which `build_instance_map` sets per
  instance, so nothing downstream reads position. Either (a) emit TLAS instances in
  a frame-stable order (e.g. sorted by BLAS device address, or by `entity_id`)
  independent of the raster sort, or (b) make `decide_use_update` order-independent
  by comparing a commutative digest of the address multiset. (a) is preferable
  because it also stabilises the cached list. Both are pure-CPU changes testable
  against the existing `decide_use_update` unit tests; confirm the win with
  `tlas_build_ms` before and after. Flag: driver BVH quality can depend weakly on
  instance ordering, so the A/B should also watch `main_render_ms`.

---

### PERF-D5-2026-08-30-02: `copy_depth_to_history` runs a full-render-resolution depth copy plus four layout barriers every frame for a feature most frames don't use, and no GPU timer covers it
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs:54-145`
  (`copy_depth_to_history`); unconditional call site
  `crates/renderer/src/vulkan/context/draw.rs:3676`
- **Status**: NEW
- **Description**: Immediately after the main render pass, every frame, the engine
  transitions the depth image `DEPTH_STENCIL_READ_ONLY_OPTIMAL → TRANSFER_SRC`,
  the history image `SHADER_READ_ONLY → TRANSFER_DST`, issues a full-extent
  `vkCmdCopyImage` of the D32 depth buffer, then transitions both back. The sole
  consumer is the soft-particle depth fade in `crates/renderer/shaders/triangle.frag:970-983`,
  which is itself gated on `(mat.materialFlags & MAT_FLAG_EFFECT_SOFT) != 0u &&
  mat.softFalloffDepth > 0.0`. A scene with no soft-effect material pays the entire
  copy for a texture nothing samples.
- **Evidence**: `draw.rs:3676` calls it with no predicate of any kind:
  ```rust
  self.copy_depth_to_history(cmd);
  ```
  The shader-side gate is the *only* read of `depthHistoryTex`
  (`crates/renderer/shaders/include/bindings.glsl:333` declares it; `triangle.frag:973`
  is the single `texture(depthHistoryTex, …)` call site).
- **Impact**: Derived from the checked-in `D32_SFLOAT` depth format and
  `frame_extents.render`: 8.3 MB read + 8.3 MB write per frame at 1920×1080,
  33.2 MB each way at native 3840×2160 — plus two full `vkCmdPipelineBarrier`
  pairs that sit directly between the render pass and the whole post chain. This
  is also the **only per-frame GPU pass in the frame with no `gpu_timers`
  bracket**: it falls in the gap between `cmd_main_render_end` and
  `cmd_svgf_start`, so it is invisible to the console/bench per-pass summary and
  cannot be measured today without adding an instrument. That is the same
  measurement-gap shape as `PERF-2026-08-27b-02`.
- **Related**: `PERF-2026-08-27b-02` (a pass whose own doc defers tuning to a
  measurement no timer can produce). The `caustic_splat` pass next door already
  demonstrates the CPU-side skip pattern this wants.
- **Suggested Fix**: Two independent, cheap steps. (1) Add a
  `copy_depth_to_history` bracket to `GpuPerFrameTimers` (the query pool grows
  28→30, `active_bits` has spare bits) so the cost is measurable before anything
  is changed. (2) Gate the copy on a scene-level "any loaded material carries
  `MAT_FLAG_EFFECT_SOFT`" bit rather than a per-frame draw-list scan — a
  per-frame predicate would leave a newly-appearing FX sampling arbitrarily stale
  depth on its first frame, whereas a scene-level bit cannot. The skip is
  layout-neutral: the helper leaves the depth image in the same
  `DEPTH_STENCIL_READ_ONLY_OPTIMAL` it found it in, which is also the precondition
  `depth_capture_record_copy` documents on the very next line — so omitting the
  call cannot break the layout contract of anything downstream. Confidence on the
  layout-neutrality argument: high (in/out layouts are literally identical); on
  the magnitude: unmeasured, which is what step (1) is for.

---

### PERF-D5-2026-08-30-03: `memory-budget.md`'s G-buffer VRAM row understates the live attachment set by 4–10× and contradicts its own two columns
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `docs/engine/memory-budget.md:538`; ground truth
  `crates/renderer/src/vulkan/gbuffer.rs` (format constants + the seven
  `Attachment` fields) and `crates/renderer/src/vulkan/context/helpers.rs:190-223`
- **Status**: NEW
- **Description**: The row reads
  `| G-buffer (8 attachments × 2 FIF, incl. FSR reactive/transparency masks) | ~23 MB | ~47 MB (4K) |`.
  Every neighbouring row labels its columns 1080p / 4K. Computed from the shipped
  formats, the seven `GBuffer` attachments are 22 B/px → **91.2 MB** at 1080p ×2 FIF
  and **365.0 MB** at native 4K ×2 FIF. Read as the row's own "8 attachments"
  (i.e. including the `R16G16B16A16_SFLOAT` HDR colour) plus depth and
  depth_history, it is **141.0 MB** and **564.0 MB**. Either way the figures are
  4× to 10× low. The row is also self-inconsistent: a 4K "peak" only 2× the 1080p
  "typical" is impossible for a resolution-scaled allocation with 4× the pixels.
- **Evidence**: `git log -L538,538` shows the numbers were **incremented, never
  recomputed**, across the attachment-count churn: `78540d8e` seeded
  `7 attachments → ~35 MB / ~70 MB`; `ca874e41` (the `#1583`/`#1590` reservoir
  removal) wrote `6 attachments → ~22 MB / ~45 MB`; `2cb86be5` (the FSR mask
  addition) wrote `8 attachments → ~23 MB / ~47 MB` — i.e. two whole attachments,
  one of them 8 B/px, were added for +1 MB.
- **Impact**: `memory-budget.md` is named in `_audit-common.md` as an authoritative
  doc auditors are told to prefer over re-deriving, and this row feeds the
  `~1.74 GB` / `~3.4 GB at native 4K` totals two rows below. Understating one
  resolution-scaled subsystem by ~200 MB at 1080p and ~500 MB at 4K makes the
  "inside the < 4 GB target" conclusion unsound at native 4K — the same class of
  ledger error as `PERF-2026-08-27b-01` (vertex/index pool) and `#3447` (Instance
  SSBO), on a different row of the same table. `docs/engine/shader-pipeline.md`'s
  G-Buffer Layout table is, by contrast, correct and current.
- **Related**: `PERF-2026-08-27b-01`, open `#3447`. Both are the same defect class;
  neither covers this row.
- **Suggested Fix**: Replace the row with the computed figures and split HDR
  colour / depth / depth_history out explicitly, then re-total the table. Better:
  derive the number in code the way the volumetrics section already does
  (`FROXEL_BYTES_PER_SLOT` is read by a regression test *and* the boot log) — a
  *GBUFFER_BYTES_PER_PIXEL* constant next to the format constants, asserted by a
  test, would make this row impossible to drift again.

---

### PERF-D6-2026-08-30-01: `SKINNED_BLAS_REFIT_THRESHOLD` has no per-entity stagger, so a cell's NPC cohort drops and rebuilds its skinned BLASes in lockstep every 601 dirty frames

- **Severity**: MEDIUM
- **Dimension**: Skinning & BLAS
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:270-281`; `crates/renderer/src/vulkan/acceleration/constants.rs:68`; `crates/renderer/src/vulkan/acceleration/predicates.rs:107-109`; `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:130-200,297-300,366`
- **Status**: NEW
- **Description**: `refit_count` starts at `0` for every skinned BLAS the moment its
  fresh BUILD registers (`blas_skinned.rs:366`) and is advanced by exactly one per
  *dirty* frame. `should_rebuild_skinned_blas_after` is a bare
  `refit_count >= SKINNED_BLAS_REFIT_THRESHOLD` with no per-entity offset, jitter, or
  per-frame rebuild budget. Every entity that first-sights in the same frame and then
  animates continuously therefore reaches 600 on the *same* frame, and
  `record_skinned_blas_refit` drops all of them and re-queues all of them into the
  same frame's `first_sight_builds` batch. An NPC playing any looping idle animation
  is pose-dirty every frame (the idle clip moves bones, so the FNV-1a pose hash
  changes), so "animating continuously" is the normal case for a populated interior,
  not a corner case.
- **Evidence**:
  ```rust
  // skinned_blas_refit.rs:270-281 — unconditional, per entity, per frame
  if accel.should_rebuild_skinned_blas(entity_id) { … accel.drop_skinned_blas(entity_id); }
  let needs_blas = accel.skinned_blas_entry(entity_id).is_none();   // now true → first_sight_builds
  ```
  ```rust
  // acceleration/predicates.rs:107-109 — no stagger term
  pub(super) fn should_rebuild_skinned_blas_after(refit_count: u32) -> bool {
      refit_count >= SKINNED_BLAS_REFIT_THRESHOLD          // = 600, constants.rs:68
  }
  ```
  The rebuild is *not* cheap per entity: `build_skinned_blas_batched_on_cmd` does a
  host `get_acceleration_structure_build_sizes` (`blas_skinned.rs:142`), a fresh
  `GpuBuffer::create_device_local_uninit` for the AS store (`:150`), and a
  `create_acceleration_structure` (`:170`) **per entity**, then records the builds
  with a full `record_scratch_serialize_barrier` between every pair
  (`:297-300`) — the same shared-scratch serialization #1797 documents for refits,
  but with BUILD-sized work instead of UPDATE-sized work.
- **Impact**: A periodic, self-synchronising frame spike roughly every 10 s of
  continuous NPC animation (600 dirty frames @ 60 FPS), scaling with the cohort size.
  Checked-in baselines put the resident skinned population at `skin_pool_live = 248`
  (`.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv`), `206`
  (`fnv-FreesideAtomicWrangler.tsv`) and `83`
  (`skyrim_se-WhiterunDragonsreach.tsv`). The cell loader's spawn budget
  (`byroredux/src/cell_loader/work_budget.rs`) spreads first-sight over the load
  window, so the cohort is several sub-groups rather than one — the burst is spread
  over a handful of frames, not concentrated in one — but it recurs on a fixed
  10 s period and nothing damps it. The spike is already observable without new
  instrumentation: `first_sight_attempted` / `first_sight_succeeded` and
  `cpu_skin_chain_ns` on `SkinCoverageFrame` (`skin.coverage` / `bench-stats
  --break-down skin`) jump together on the rebuild frame, and `gpu_skin_blas_refit_ms`
  covers the device side.
- **Related**: #679 / AS-8-9 (introduced the threshold), #1797 (the shared-scratch
  serialization ceiling this rides on — BUILDs share the same barrier chain as
  refits), #1196 (the refit gate that decides which entities advance `refit_count`),
  #1812 (`built_this_frame`, which correctly suppresses the *redundant refit* after a
  rebuild but does nothing about the rebuild clustering itself).
- **Suggested Fix**: Make the threshold per-entity rather than global — e.g. compare
  against `SKINNED_BLAS_REFIT_THRESHOLD + (entity_id % SKINNED_BLAS_REFIT_JITTER)`
  inside `should_rebuild_skinned_blas_after`, or cap the number of threshold-triggered
  rebuilds admitted per frame and let the rest slip to the next. Both are a change to
  one pure predicate plus a constant, and both are unit-testable exactly the way
  `should_rebuild_skinned_blas_after` already is.

---

### PERF-D7-2026-08-30-01: one dispatch batch parses the same NIF once per cell that references it, and the main thread then throws every duplicate away
- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/app_step.rs:178-182`, `byroredux/src/scene/world_setup.rs:833-836`, `byroredux/src/streaming.rs:877-908` (`queue_loads`), `byroredux/src/streaming.rs:1310-1322` (`pre_parse_cell`'s cache filter), `byroredux/src/cell_loader/partial.rs:44-50`
- **Status**: NEW (residual of #862, which is CLOSED and whose guard is intact — this is the gap the #862 design leaves open by construction, not a regression of it)
- **Description**: `cached_keys` is a **single snapshot of `NifImportRegistry`
  taken once, before the whole batch is queued**, and it is `Arc`-cloned into
  every `LoadCellRequest` (`streaming.rs:899`). The worker is one thread
  draining requests serially (`cell_pre_parse_worker`, `streaming.rs:1086`) and
  keeps **no memo of what it already parsed in this batch**: `pre_parse_cell`
  dedups `model_paths` within a cell (`HashSet`, `streaming.rs:1321`) and
  filters against the snapshot (`:1317`), but nothing filters against the
  cells earlier in the same batch. A model shared by K cells queued in one
  dispatch is therefore BSA-extracted, parsed and imported K times.

  The duplicate work is then **provably discarded**: `finish_partial_import`
  opens with #864's already-cached early-out (`partial.rs:44-50`), so every
  duplicate `PartialNifImport` the worker produced past the first is dropped
  unread. All that survives is the wall-clock and the peak RSS.
- **Evidence**: The bootstrap call site documents the worst case in its own
  comment (`world_setup.rs:830-832`): *"On initial-radius dispatch the cache is
  normally empty, so this typically returns an empty set and the worker parses
  everything"*. `stream_initial_radius` queues the whole initial radius in one
  `queue_loads` — 49 cells at `--radius 3`, 225 at the documented `--radius 7`
  ceiling — against that empty snapshot. Adjacent exterior tiles in a
  worldspace share the overwhelming majority of their statics (the #862 issue
  title itself measured ">95% cache hits on shared statics" on WastelandNV once
  the cache was warm), so K is the number of cells in the batch that reference
  a given rock/road/fence, not 1.

  The payload channel is `mpsc::channel()` — **unbounded**
  (`streaming.rs:772`) — and the main thread only blocks until the *centre*
  cell arrives (`bootstrap_waiting`, `world_setup.rs:843`). The remaining
  payloads accumulate, each holding a full `PartialNifImport` (parsed
  `NifScene` + imported meshes + collisions + embedded clip) for every model
  of its cell, K copies of each shared model resident at once.
- **Impact**: Off-main-thread CPU, but it lands on two things the engine
  measures: `StreamingTelemetry::worker_parse` (fed from
  `payload.timings.worker`) and `settle_full_detail`'s
  *"Exterior full detail settled around (x, y) in N ms"* line — the duplicate
  parses sit in front of the last cell's payload, so they directly extend
  time-to-settle on every fresh-content dispatch. Peak RSS during bootstrap
  scales with the duplication factor. Steady-state crossings are much milder
  (the snapshot covers everything the 42 resident cells already parsed, so only
  content genuinely new to the incoming column duplicates, K ≤ 7 at radius 3),
  which is why this has never surfaced as a frame spike — it is a latency and
  memory cost, not a hitch.
- **Related**: #862 (the snapshot that this is the residual of, CLOSED, guard
  intact); #864 (`finish_partial_import`'s early-out — the proof the work is
  discarded); #3038 (the shared `canonical_model_path_key` both sides use, so a
  batch-level memo can key off the same string).
- **Suggested Fix**: Give the worker a batch-local `HashSet<String>` of keys it
  has already produced this drain, consulted immediately after the
  `cached_keys.contains(&key)` check in `pre_parse_cell` and cleared when
  `request_rx.recv()` blocks (i.e. the queue is empty). Alternatively have the
  worker mutate a shared `Arc<Mutex<HashSet<String>>>` snapshot rather than
  taking a frozen clone. Before/after is measurable with the existing
  `worker_parse` summary plus a duplicate-skip counter alongside
  `skipped_cached` (`streaming.rs:1319`).

---

### PERF-D7-2026-08-30-02: the interior cell load still runs its whole REFR + NPC spawn on an unlimited budget, though the resumable cursor #1798 deferred on now exists
- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/references/mod.rs:227-267` (`load_references`, `FrameTimeBudget::unlimited()` at `:247`), called from `byroredux/src/cell_loader/load.rs:485` (`load_cell_with_masters`), reached from `byroredux/src/cell_loader/transition.rs:437-460` (`load_interior_cell`)
- **Status**: NEW — supersedes #1798 (CLOSED), which was closed by *measuring* the stall, not bounding it
- **Description**: Every exterior path now yields against a real deadline:
  `ExteriorCellApplyJob::advance` and `PersistentCellApplyJob::advance` both
  thread the `FrameTimeBudget` seeded from `STREAMING_APPLY_BUDGET`
  (`app_step.rs:33` = 16 ms, `:196-197`) into `load_references_budgeted`. The
  interior path calls the thin `load_references` wrapper, which constructs
  `FrameTimeBudget::unlimited()` and then asserts the job *cannot* yield
  (`unreachable!("an unlimited reference-load budget cannot yield")`,
  `references/mod.rs:266`). One door walk-in therefore spawns every `PlacedRef`,
  every SCOL/PKIN expansion and every NPC — each NPC being a multi-NIF
  `NpcSpawnJob` — inside a single frame, followed by the forced
  `flush_pending_cell_textures` fence wait.
- **Evidence**: `references/mod.rs:247`:
  ```rust
  let mut budget = FrameTimeBudget::unlimited();
  match load_references_budgeted(..., &mut budget) {
      ReferenceLoadProgress::Complete(result) => result,
      ReferenceLoadProgress::Pending(_) => {
          unreachable!("an unlimited reference-load budget cannot yield")
      }
  }
  ```
  #1798's closing comment is explicit that the fix shipped was measurement
  only: *"This is the minimal step the issue itself calls out — making the cost
  visible — rather than the larger chunked-spawn-budget rewrite … which needs
  real per-cell numbers to size correctly and is a substantially bigger change
  (a resumable cursor across frames)."* **That premise no longer holds.** The
  resumable cursor exists (`ReferenceLoadJob` with `next_ref` / `next_synth` /
  `current_ref_synth` / `active_npc`), it is the same function the interior
  already calls, and the per-frame allowance is already chosen and justified
  (`STREAMING_APPLY_BUDGET`). The remaining work is a caller-side loop plus a
  `stamp_cell_root_range` on each yield — the shape `ExteriorCellApplyJob`
  already implements.
- **Impact**: A multi-hundred-millisecond-to-multi-second freeze on every
  interior transition into a dense cell — door walk-in, `coc`-style debug load,
  and the M45.1 save-load cell reload — on a machine where nothing else in the
  streaming stack blocks a frame. It is also the one path where the shipped
  `npc_spawn_wall` number has no lever attached to it: the log tells the user
  how long the freeze was and nothing can act on it.
- **Related**: #1798 (CLOSED, measurement-only); #2275 (the identical gap for
  the worldspace persistent CELL, since fixed by `PersistentCellApplyJob` —
  the template for this fix); #881 (`flush_pending_uploads`, the fence wait
  that compounds it); #1698 (the *post*-load settle storm — adjacent, distinct).
- **Suggested Fix**: Drive `load_cell_with_masters`'s reference phase through
  `load_references_budgeted` behind an `InteriorCellApplyJob` (or reuse the
  `ExteriorCellApplyJob` shape), stepped from `App::step_cell_transition` under
  the same `STREAMING_APPLY_BUDGET` deadline, stamping `stamp_cell_root_range`
  on each yield so a cancelled transition stays reclaimable. Keep
  `load_references` as the synchronous wrapper for the remaining test /
  console callers.

---

### PERF-D8-2026-08-30-01: 73–81 % of the per-NIF CPU budget runs on the main thread — the streaming worker parallelises only the cheaper 15–30 %
- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Location**: `byroredux/src/streaming.rs:1163-1214` (`parse_one_nif`, worker) vs `byroredux/src/cell_loader/partial.rs:67-74` (`finish_partial_import`, main thread) · `byroredux/src/streaming_helpers.rs:515-530,653-663` (the `FinishImports` drain phase)
- **Status**: NEW
- **Description**: The two-phase pre-parse architecture (#830 → #877 → #1262 → #3089) moves `parse_nif` and the three *pool-free* imports onto a dedicated rayon pool of `available_parallelism()/2` threads (`build_stream_parse_pool`, `streaming.rs:1050-1060`). But the **mesh + collision import** — `import_nif_with_collision_and_resolver` — stays on the main thread inside `finish_partial_import`, because it needs `&mut StringPool` out of the `World` (`partial.rs:68-74`) and a `&dyn MeshResolver` backed by the archive provider. Measured on real archives, that main-thread stage is the overwhelming majority of the per-unique-NIF cost. The `FinishImports` phase drains **one import per budget unit** (`streaming_helpers.rs:654-657`), strictly serialised, so a fresh cell's whole import cost is a single-threaded queue on a 32-thread machine.
- **Evidence** (release build, per unique NIF, 3,000-file stratified sample per archive; buckets are the exact call sequence the two code paths make):

  | Archive | files / bytes | worker `parse_nif` | worker lights+emitters+anim | MAIN `summarize_collision_authoring` | **MAIN `import_nif_with_collision`** |
  |---|---|---|---|---|---|
  | `Skyrim - Meshes0.bsa` | 3000 / 302.4 MiB | 84.83 ms (23.8 %) | 10.28 ms (2.9 %) | 1.68 ms (0.5 %) | **260.18 ms (72.9 %)** |
  | `Fallout - Meshes.bsa` (FNV) | 3000 / 379.6 MiB | 40.55 ms (14.6 %) | 10.01 ms (3.6 %) | 2.04 ms (0.7 %) | **225.78 ms (81.1 %)** |
  | `Oblivion - Meshes.bsa` | 3000 / 436.7 MiB | 57.23 ms (29.7 %) | 7.20 ms (3.7 %) | — | **127.96 ms (66.5 %)** |

  The results are cached identically (`NifImportRegistry` is keyed per unique model path, `canonical_model_path_key`), so both buckets run exactly once per unique NIF — the ratio is apples-to-apples.
- **Impact**: Fresh-cell streaming latency (session start, first entry to a worldspace region, door teleports into un-warmed interiors) is dominated by a serial main-thread stage; the `N/2`-thread pool #3089 built sits idle for the majority of the work it was created to absorb. Not a frame hitch — `FrameTimeBudget` yields (`streaming_helpers.rs:607`) — but it converts a parallelisable cost into wall-clock cell-load latency that scales with core count not at all. The `partial.rs:92-99` comment ("Running the full `import_nif_scene` again here just to get the node names would double the per-NIF parse cost") shows the cost was believed to be ~1× parse; it is ~3×.
- **Related**: #830, #877, #1262, #3089 (the pre-parse parallelisation chain); PERF-D8-2026-08-30-02 (same tier, gate side); the un-fixed `flame_attach_offset` deferral at `partial.rs:92-99` is a symptom of the same boundary.
- **Suggested Fix**: The only main-thread dependency in the mesh walk is `pool.intern(texture_path)` (`walk/mod.rs:1700`, threaded through `extract_*_local(…, ctx.pool)`). Move the walk onto the worker with a **worker-local `StringPool`**, and re-intern into the `World` pool during the drain — `ImportedMaterial`'s texture slots already go through the generic `MaterialTextureSet<T>::map_ref`, so a `FixedString → String → FixedString` re-intern at the boundary is a single mapping pass over ≤22 slots per mesh rather than a re-walk. Measure first with a `--bench-hold` cell-load trace; the alternative (a lock-free interner shared across the pool) is a bigger change with an ECS-resource-access story.

---

### PERF-D8-2026-08-30-02: the dhat allocation gate stops at `parse_nif` — the import tier it should also cover is ~2× the peak live heap and 3–5× the CPU
- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Location**: `crates/nif/tests/heap_allocation_bounds.rs:150,352,454` · `crates/nif/tests/heap_allocation_bounds_geometry.rs:227` · guarded-but-unguarded code: `crates/nif/src/import/mod.rs:119` (`import_nif_scene`), `:536` (`import_nif_with_collision_and_resolver`)
- **Status**: NEW
- **Description**: All four bound tests call exactly one function — `byroredux_nif::parse_nif`. The `#831` / `#832` / `#833` / `#408` allocation discipline they exist to enforce at CI cadence applies just as much to the import tier (`import/mesh/`, `import/material/`, `import/walk/`, `import/collision/`), which is where the per-vertex / per-bone / per-target **output** vectors are actually built. A regression that reverted `import_nif_scene_impl`'s `#835` pre-sizing, or introduced a per-vertex `push` growth in `decode_sse_shape_buffer` / `extract_morph_targets`, would pass every current gate. This is a defense-in-depth gap in the gate itself, not merely absent test coverage: the gate's stated purpose (`heap_allocation_bounds.rs:1-31`) is to promote the allocation pins from audit cadence to CI cadence, and it currently promotes the smaller half.
- **Evidence**: dhat-instrumented run over a 400-file `Skyrim - Meshes0.bsa` sample, taking `HeapStats::max_bytes` immediately after `parse_nif` and again after `import_nif_scene` + `import_nif_lights` + `import_nif_particle_emitters` + `import_embedded_animations`:

  ```
  peak_after_parse_max = 5_325_133 B     peak_after_import_max = 12_360_375 B   (2.32x)
     peak_all=12_360_375  peak_parse= 5_325_133  meshes\effects\dragondeathtestexport.nif
     peak_all=10_098_464  peak_parse= 3_975_580  meshes\furniture\blacksmithingskyforgemarker.nif
     peak_all= 5_176_305  peak_parse= 2_599_361  meshes\architecture\whiterun\wrbuildings\wrtempleofk01.nif
     peak_all= 2_636_212  peak_parse= 1_367_976  meshes\dlc02\architecture\telvannitower\dlc2telvannigourdhouseext01.nif
  ```
  The top-8 worst NIFs all land in the 2.0–2.3× band. CPU split is in Finding 01's table.
- **Impact**: The one quantitative, CI-enforced allocation contract on the NIF load path bounds the cheaper half of it. Every allocation-hygiene finding in the import tier will keep being re-derived by hand at audit cadence — which is exactly the failure mode #1247 was filed to end.
- **Related**: #1247, #1381, #1763, #2114 (the four gate-landing issues); PERF-D8-2026-08-30-01; the 2026-08-24 report's Dim 8 note on `import/mesh/morph.rs` being "not dhat-bound yet".
- **Suggested Fix** (concrete, measured): add a third bound file `crates/nif/tests/heap_allocation_bounds_import.rs` (its own binary — `dhat::Profiler` is a process singleton), reusing `build_fo4_packed_vertex_nif(16)` from `heap_allocation_bounds.rs` but widened to ~256 vertices, and wrapping `parse_nif` **plus** `import_nif_scene(&scene, &mut StringPool::new())` in one profiler scope. Register it in the existing `nif-heap-allocation-bounds` CI job (`.github/workflows/ci.yml:182-185`). Pin the initial bound at the same ~5× headroom the siblings use, measured on first landing; the 2.0–2.3× parse→import ratio above is the sanity check the number must sit above. Bumping the packed-vertex fixture to 256 verts also gives the bound a slope, so a per-vertex `push`-growth revert shows up as a super-linear jump rather than a constant.

---

### PERF-D9-2026-08-30-01: `between_frames_ms` is sampled after `draw_frame` returns, so it silently absorbs the entire in-engine render path it exists to exclude
- **Severity**: MEDIUM
- **Dimension**: Telemetry & Origin Cost
- **Location**: `byroredux/src/app_frame.rs:593-596` (sample point), `:643` (stamp), `:55` (the correct anchor); doc at `crates/core/src/ecs/resources/mod.rs:731-739`
- **Status**: NEW
- **Description**: `CpuFrameTimings::between_frames_ms` is documented as *"Wall
  time between the END of one frame and the START of the next … If `acquire_ms`
  is small but this is large, the bottleneck is **outside** the engine's render
  path (compositor, OS, ECS systems running between frames)."* The code does not
  measure that. `last_redraw_end` is stamped at the end of `render_one_frame`
  (`:643`), but `elapsed()` is read at `:593` — inside the `Ok(needs_recreate)`
  arm, i.e. **after** `build_render_data` and after the whole `draw_frame` call
  have already run. The reading is therefore
  `true_gap + atw_pre + atw_scheduler + (part of atw_post) + rof_pre_draw + rof_draw_call`,
  not the gap. `render_one_frame`'s own start anchor, `rof_pre_t0 =
  Instant::now()` (`:55`), is the value the metric wants and is already in scope.
- **Evidence**:
  ```rust
  // app_frame.rs:55  — true start of the frame
  let rof_pre_t0 = Instant::now();
  ...
  // app_frame.rs:593 — sampled here, AFTER draw_frame returned
  cpu_t.between_frames_ms = self
      .last_redraw_end
      .map(|t| t.elapsed().as_nanos() as f32 * NS_TO_MS)
      .unwrap_or(0.0);
  ...
  // app_frame.rs:634 — rof_draw_call bracket closes only afterwards
  rof_draw_call_ns = rof_draw_call_t0.elapsed().as_nanos() as u64;
  // app_frame.rs:643 — stamp for the NEXT frame
  self.last_redraw_end = Some(Instant::now());
  ```
  `crates/core/src/ecs/resources/mod.rs:759-771` independently confirms the
  overlap: `rof_pre_draw_ms` and `rof_draw_call_ms` cover exactly the span the
  sample point sits after.
- **Impact**: The metric systematically over-attributes to "outside the engine"
  by the full magnitude of `rof_pre_draw + rof_draw_call` — i.e. by the two
  buckets that hold `build_render_data`, the SSBO build, command recording and
  present. It is the field an operator consults to decide *"is this a compositor
  problem or my problem?"*, and it answers "compositor" for engine-side cost.
  Blast radius is the egui Metrics panel (`metrics.rs:213`), which is the only
  surface that prints it, plus the Phase-9 "501 ms `between_frames` gap"
  conclusion the code comment at `app_events.rs:1106` still cites — that gap was
  measured with this same skew. This is diagnostic-only (no runtime behaviour
  changes), which is why it is MEDIUM rather than HIGH, but it is the same class
  of defect as #2171 (a trace that argued the opposite of the truth).
- **Related**: `PERF-D9-2026-08-30-04` (the same field is also unprinted on the
  console line); #2171 (origin-delta printed after the overwrite); the Phase-9
  / Phase-10 / Phase-15 bracket lineage.
- **Suggested Fix**: Capture the gap next to `rof_pre_t0` at `app_frame.rs:55`
  (`let between_frames_ns = self.last_redraw_end.map(|t| t.elapsed().as_nanos() as u64).unwrap_or(0);`)
  and assign that at `:593` instead of re-reading `elapsed()`. One line moved;
  the `last_redraw_end` stamp at `:643` is already correct.

---

### PERF-D9-2026-08-30-02: `batches_scratch`'s per-frame `reserve()` and its end-of-frame shrink fight each other — two reallocations and a memcpy every frame on four of five baseline cells
- **Severity**: MEDIUM
- **Dimension**: Telemetry & Origin Cost (chronic scratch over-reserve)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:2810-2812` (reserve), `:3978-4006` (shrink), predicate at `crates/renderer/src/vulkan/acceleration/predicates.rs:394-399`
- **Status**: NEW
- **Description**: `batches` is reserved to **`draw_commands.len()`** but filled
  to the post-merge **batch count**, which the repo's own baselines put 13–19×
  lower. At frame end the shrink policy targets `2 × max(batch_count, 512)`.
  For every baseline cell where `draw_commands.len() > 2 × max(batch_count, 512)`
  — four of the five — the shrink fires, and the next frame's `reserve()`
  immediately grows the Vec back. The result is a `shrink_to` realloc (copying
  the live batches) plus a growth realloc, **every frame**, on the render hot
  path — precisely the churn the field's own doc says it eliminates
  ("`mem::take` … amortizing their capacity across frames … See issue #243").
  The other members of the cluster do not thrash: `gpu_instances_scratch` and
  `previous_models_scratch` have working sets ≈ `draw_commands.len()`, so
  `2×working` comfortably exceeds the reserve.
- **Evidence**:
  ```rust
  // draw.rs:2810-2812
  let mut batches: Vec<DrawBatch> = std::mem::take(&mut self.batches_scratch);
  batches.clear();
  batches.reserve(draw_commands.len());     // ← keyed to the WRONG quantity
  ...
  // draw.rs:3978-4006
  let working_batches = batches.len();      // ← the RIGHT quantity
  self.batches_scratch = batches;
  super::super::acceleration::shrink_scratch_if_oversized(
      &mut self.batches_scratch, working_batches, 512);
  ```
  ```rust
  // predicates.rs:394-399
  let target = 2 * working_set.max(floor);
  if vec.capacity() > target { vec.shrink_to(target); }
  ```
  Worked through with `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv`
  (`bench_draws_cmds 3949`, `bench_draws_batches 296`): reserve forces
  `capacity ≥ 3949`; shrink target is `2 × max(296, 512) = 1024`; `3949 > 1024`
  so `shrink_to(1024)` reallocates and copies 296 elements; next frame
  `clear()` leaves `len = 0, cap = 1024` and `reserve(3949)` reallocates again.
  Same arithmetic holds for fnv (2110 vs 1024), fo3 (1581 vs 1024) and skyrim
  (2342 vs 1024). Only oblivion (325 < 1024) escapes.
- **Impact**: Two heap reallocations plus one `memcpy` of the live batch array
  per frame, at 60–150 fps, on every cell dense enough to matter — a permanent
  allocator-traffic floor on the exact path #243 was filed to remove. Byte
  magnitude is `size_of::<DrawBatch>()` × the counts above; I am not quoting a
  byte figure because `DrawBatch`'s layout is unpinned by any size test. Host
  RAM only — no GPU allocation, no leak — hence MEDIUM, not HIGH. It is also
  the single largest `wasted_bytes` contributor `ctx.scratch` would report, so
  the telemetry that exists already points at it.
- **Related**: #243 (the amortization the reserve defeats); #2486 / D5-01 (which
  extended the *shrink* half of the policy to the rest of the cluster but did
  not revisit the reserve arguments); `docs/audits/AUDIT_PERFORMANCE_2026-08-12.md:946`
  marked #243 PASS on the basis of "all `mem::take`+`clear`+`reserve`d", which is
  true and still misses this.
- **Suggested Fix**: Either drop the `reserve` (let `push` amortize from the
  retained capacity — `Vec`'s own growth is already amortized O(1)), or key it
  to the batch count instead: reserve `self.batches_scratch.capacity()`-worth by
  reserving nothing, or track last frame's `working_batches` and reserve that
  with a slack factor. A one-line change either way; the shrink policy is fine
  as is.

---

### PERF-D9-2026-08-30-03: three per-frame GPU work items sit outside every `gpu_timers` bracket — including `skin_palette.comp`, which a sibling dimension's matrix records as covered
- **Severity**: MEDIUM
- **Dimension**: Telemetry & Origin Cost
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:2496` (bone_world copy), `:2520` (bind_inverse copies), `:2536-2579` (`skin_palette.comp` dispatch), `:3729-3752` (egui overlay pass). Bracket boundaries: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:379` / `:440`.
- **Status**: NEW
- **Description**: The `skin_dispatch_ms` bracket does **not** cover the bone
  palette pass. `cmd_skin_dispatch_start` / `_end` are written inside
  `record_skinned_blas_refit` (`skinned_blas_refit.rs:379`, `:440`) and wrap only
  the per-entity `skin_vertices.comp` dispatch loop. `record_skinned_blas_refit`
  is called from `draw.rs:2600` — *after* the palette dispatch at `:2536-2579`
  and after the two SSBO transfer copies at `:2496` / `:2520`. All three are
  therefore recorded into the command buffer with no bracket around them. A
  fourth item, the egui overlay render pass at `:3729-3752`, is likewise
  unbracketed and runs on **every** frame (`app_frame.rs:119-126` runs egui
  unconditionally whenever `debug_ui` exists, because it draws the crosshair and
  interaction prompt even with the panel hidden) and includes a queue-locking
  `set_textures` upload. `gpu_timers.rs:9-10` labels slots 0/1 "skin compute
  dispatch loop", which reads as covering the whole skin chain and does not.
- **Evidence**:
  ```rust
  // draw.rs:2536  — palette pass, NO bracket
  if let Some(ref mut skin_palette) = self.skin_palette {
      ...
      skin_palette.dispatch(&self.device, cmd, frame, /* … */);
  }
  // draw.rs:2600  — bracket only starts inside here
  self.record_skinned_blas_refit(cmd, frame, draw_commands, pose_dirty);
  ```
  ```rust
  // skinned_blas_refit.rs:378-380
  if let Some(ref mut timers) = self.gpu_timers {
      timers.cmd_skin_dispatch_start(&self.device, cmd, frame);
  }
  ```
  **Correction to a sibling**: `/tmp/audit/performance/dim_5.md:55` records
  "`skin_palette.comp` + `skin_vertices.comp` → `skin_dispatch_ms` → timer
  present: yes". The second half is right; the first is not.
- **Impact**: The palette pass is the one this matters most for right now:
  sibling Dim 4 (`dim_4.md:230, :275`) reports it dispatches over the **full**
  bone range every frame with only #1811's coarse `skip_skin_gpu_refresh` gate,
  so "the wasted bytes buy wasted GPU threads too". No checked-in instrument can
  size that waste or confirm a fix — the CPU-side `cpu_skin_chain_ms` (#2803)
  measures host work, not the dispatch. The egui overlay is a whole render pass
  on the frame's critical path with no cost visibility at all. The transfer
  copies are the smallest of the four. All four land in `cmd_record` on the CPU
  side, which lumps them with the entire rest of the frame.
- **Related**: Dim 5's `PERF-D5-2026-08-30-02` (`copy_depth_to_history`, the
  fifth unbracketed item — **not re-filed here**, and its claim to be "the only
  per-frame GPU pass with no bracket" is what this finding corrects); Dim 4's
  full-range palette dispatch finding, which this blocks from being measured;
  #1194 (the bracket set's origin).
- **Suggested Fix**: Add one bracket pair (`Q_SKIN_PALETTE_START/END`, raising
  `QUERIES_PER_FRAME` 28 → 30 and adding `BIT_SKIN_PALETTE`) around
  `draw.rs:2536-2579`, and extend it upward to `:2496` if the transfer copies
  should be attributed with it. Bracket egui separately, or accept it as a known
  hole and say so in `gpu_timers.rs`'s module doc. Update the slot table's
  "skin compute dispatch loop" wording either way.

---

### PERF-D1-2026-08-30-01: the live animation path is the last unconverted per-frame per-entity SipHash keyspace — `AnimationClip.channels`, `NameIndex.map` and `SubtreeCache.map` are all `std::collections::HashMap`
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/core/src/animation/types.rs:238` (`pub channels: HashMap<FixedString, TransformChannel>`, `use std::collections::HashMap` at `:5`); `byroredux/src/components.rs:1287` (`NameIndex.map`) and `:1298` (`SubtreeCache.map`) — both reached through `use std::collections::{HashMap, ...}` at `byroredux/src/components.rs:13`, while `rustc_hash::FxHashMap` is *already imported* one line above at `:12`. Probe sites: `byroredux/src/systems/animation.rs:681-690` (`scoped_map` / `resolve_entity`) and `:699` (`for (channel_name, channel) in &clip.channels`).
- **Status**: NEW — not a regression of #2923/#3051/#3061. Those closed the *renderer/skinning* cluster (`SkinSlotPool`, `skin_offsets`, `pose_dirty`, `skin_slots`, `morph_slots`, `blend_pipeline_cache`); the animation-system trio was never in scope and is not covered by the `context/mod.rs:2889` guard, which only pins renderer-owned fields. No open or closed issue names these three.
- **Description**: The project has decided three times (#1368, #2174, #2923/#3061) that std's SipHash-1-3 is the wrong hasher for a per-frame per-entity keyspace, and `_audit-common.md`'s "Hot-path hashing" rule records it as doctrine. The animation player path — the one that actually runs on live game data — probes std maps once per animated *channel* per animated *entity* per frame, at three layers:
  1. `for (channel_name, channel) in &clip.channels` iterates a std `HashMap` (random bucket order, poor locality) once per player entity per frame;
  2. `resolve_entity(channel_name)` → `scoped.get(sym)` on `SubtreeCache`'s inner `HashMap<FixedString, EntityId>`, or `name_index.map.get(sym)`, once per channel;
  3. `apply_float_channels` / `apply_color_channels` / `apply_bool_channels` / `apply_texture_flip_channels` (`animation.rs:766-793`) each call the same `resolve_entity` again per channel they own.

  The key is `FixedString` = `string_interner::DefaultSymbol` (`crates/core/src/string/mod.rs:18`), i.e. a 4-byte integer — the exact input shape where SipHash-1-3's fixed setup/finalisation dominates and `FxHash` wins by the largest factor.
- **Evidence**:
```rust
// crates/core/src/animation/types.rs:238
pub channels: HashMap<FixedString, TransformChannel>,     // std ⇒ SipHash-1-3

// byroredux/src/components.rs:1287,1298
pub(crate) struct NameIndex   { pub(crate) map: HashMap<FixedString, EntityId>, … }
pub(crate) struct SubtreeCache{ pub(crate) map: HashMap<EntityId, HashMap<FixedString, EntityId>>, … }

// byroredux/src/systems/animation.rs:684-690, then :699
let resolve_entity = |sym: &FixedString| -> Option<EntityId> {
    if let Some(scoped) = scoped_map { scoped.get(sym).copied() }   // SipHash
    else { name_index.map.get(sym).copied() }                        // SipHash
};
…
for (channel_name, channel) in &clip.channels {                      // std HashMap iteration
    let Some(target_entity) = resolve_entity(channel_name) else { continue; };
```
- **Impact**: One SipHash round per bone channel per animated actor per frame, plus the same again for every float/colour/bool/flipbook channel the clip carries. Magnitude is bounded by the actor population, for which the repo's own checked-in baselines are the honest citation: `skin_pool_live` = 206 (`.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv`), 248 (`fo4-InstituteBioScience.tsv`), 83 (`skyrim_se-WhiterunDragonsreach.tsv`). **The per-channel count is not recorded anywhere in the repo and I did not measure it — that multiplier is unknown.** No `dhat` or timing guard covers this site; `log_stats_system`'s `cpu_ms:` line (`byroredux/src/systems/debug.rs:206`) has no animation bracket, so a regression here is currently invisible to every checked-in instrument.
- **Related**: #2923, #3051, #3061 (the renderer half of the same doctrine, all CLOSED); #1368, #2174; `_audit-common.md` "Hot-path hashing (#2923)".
- **Suggested Fix**: Switch the three declarations to `rustc_hash::FxHashMap` (`rustc_hash` is already a dep of both `crates/core` and `byroredux`, and `FxHashMap` is already imported at `byroredux/src/components.rs:12`). `AnimationClip.channels` is a public field, so pair it with a type alias or fix the handful of construction sites. Then extend the `context/mod.rs:2889`-style source-scan assertion to cover these three so the conversion cannot silently revert.

---

### PERF-D1-2026-08-30-02: `reemit_water_planes` builds an entity→draw-slot index over **every** draw command each frame with no water-population early-out
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/render/water.rs:111-127`, called unconditionally from `byroredux/src/render/mod.rs:950`
- **Status**: NEW
- **Description**: #3141 correctly replaced an `O(draws × water)` rescan with a single `O(draws)` index build. But the function's only early-out is `world.query::<WaterPlane>()` returning `None`, which happens only when *no entity in the process has ever* carried `WaterPlane` (`World::query`'s contract, `crates/core/src/ecs/world.rs:468-470`). Once an exterior or a water interior has been visited, the storage exists forever; every later frame — including every frame of a dry interior after a door transition, and every frame after a streaming unload emptied the resident water set — still clears, `reserve()`s and re-`extend()`s the map with one entry per draw command, and then iterates zero water planes against it. `QueryRead::is_empty()` already exists (`crates/core/src/ecs/query.rs:79`) and is the exact predicate the function needs.
- **Evidence**:
```rust
// byroredux/src/render/water.rs:111-127
let Some(wq) = world.query::<WaterPlane>() else { return; };   // ONLY guard: storage never created
let mut scratch = world.try_resource_mut::<WaterDrawIndexScratch>();
…
draw_indices.clear();
draw_indices.reserve(draw_commands.len());
draw_indices.extend(
    draw_commands.iter().enumerate().map(|(i, c)| (c.entity_id, i)),
);                                                              // O(all draws), unconditional
let fq = world.query::<WaterFlow>();
let rq = world.query::<RippleEvent>();
for (entity, plane) in wq.iter() { … }                          // may be zero iterations
```
- **Impact**: One `FxHashMap` insert per draw command per frame, thrown away. The repo's own baselines give the draw counts: `bench_draws_cmds` = 3949 (`fo4-InstituteBioScience.tsv`), 2342 (`skyrim_se-WhiterunDragonsreach.tsv`), 2110 (`fnv-FreesideAtomicWrangler.tsv`), 1581 (`fo3-MegatonPlayerHouse.tsv`), 325 (`oblivion-ICMarketDistrictTheGildedCarafe.tsv`) — all five are interiors. **No allocation or timing guard exists for this site**: after warm-up the map keeps its capacity so this is CPU work, not heap churn, and neither `ScratchTelemetry` nor the `cpu_ms:` breakdown brackets `build_render_data`'s water tail. A secondary, smaller observation at the same site: with a *small* live water count the map build can be slower than the scan it replaced (one hash insert per draw vs one integer compare per draw), so #3141's "dozens of surfaces" premise is what makes it a win — that premise is not asserted anywhere.
- **Related**: #3141 (CLOSED, the index that introduced this); `PERF-D2-01` in `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md`.
- **Suggested Fix**: Add `if wq.is_empty() { return; }` immediately after the `wq` acquisition, before the `WaterDrawIndexScratch` resource acquisition and the index build. One line, no behaviour change — `wq.iter()` was already going to yield nothing.

---

### PERF-D1-2026-08-30-03: `apply_cell_region_ambient` re-resolves the REGN ambient directive every exterior frame — a `Vec` allocation plus a sort — and both the resource's own doc and the call site's cost comment say it does not
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/app_step.rs:87` (the unguarded call) → `byroredux/src/scene/world_setup.rs:509-523` → `byroredux/src/components.rs:552-575` (`RegionAmbientRes::resolve`) → `crates/plugin/src/esm/records/misc/world.rs:792-804` (`select_active_region_sound`)
- **Status**: NEW
- **Description**: `step_streaming` runs every frame (`byroredux/src/app_events.rs:706`). `apply_cell_region_ambient` is deliberately placed *outside* the `grid_changed` guard (`app_step.rs:82-87`) so a session starting inside a region-tagged cell gets its directive on frame 0. That placement is correct; the cost claim attached to it is not. The comment covering the pair says the unguarded placement "*Costs one map lookup and an `Option<u32>` compare on every other frame*" (`app_step.rs:72-73`) — true of `apply_cell_climate_override`, which early-returns after a couple of map lookups with no allocation, but not of the region call, which runs the full resolve first and only compares afterwards. `select_active_region_sound` **collects a fresh `Vec<&RegionDataEntry>` and sorts it** on every one of those frames. Independently, `RegionAmbientRes`'s own doc block asserts the opposite lifecycle: "*computed once at cell-apply time from data already parsed into `EsmIndex`, **not recomputed per-frame***" (`byroredux/src/components.rs:526-527`). The resource is a `Copy` struct of two `Option<u32>` whose value can only change when the resident grid cell changes.
- **Evidence**:
```rust
// crates/plugin/src/esm/records/misc/world.rs:796-803
let mut candidates: Vec<&RegionDataEntry> = region_form_ids
    .iter()
    .filter_map(|id| regions.get(id))       // std HashMap
    .flat_map(|r| r.entries.iter())
    .filter(|e| e.kind == RegionDataKind::Sound)
    .collect();                             // heap allocation, every frame
candidates.sort_by_key(|entry| std::cmp::Reverse(entry.priority));
candidates.into_iter().next()

// byroredux/src/components.rs:526-527  (the contradicted claim)
/// … computed once at cell-apply time from data already parsed into
/// `EsmIndex`, not recomputed per-frame.
```
- **Impact**: Bounded and small — vanilla exterior cells carry a handful of XCLR regions and the parser's own doc records 788 `RDAT` entries total across `Oblivion.esm` + `FalloutNV.esm` + `Skyrim.esm` (`crates/plugin/src/esm/records/misc/world.rs:827-829`), so `candidates` is short. The allocation is skipped entirely when the cell's regions contribute no `Sound` entry (`collect()` on an empty filter chain does not allocate). The real cost is a malloc/free pair plus a sort per exterior frame for a value that changes only at a cell boundary. **No allocation guard exists for this site**, and the misleading cost comment is what would let the next unguarded per-frame call be added beside it on the same false precedent.
- **Related**: EX-16 item 1 / #2372 (the change that added the call); #2451 / EXAL-03 (the climate sibling the comment actually describes).
- **Suggested Fix**: Cache the resolved `RegionAmbientRes` against the `(worldspace_key, player_grid)` it was computed for — the same shape `applied_climate` already uses for the sibling — and recompute only when that pair moves. Then correct the `app_step.rs:72-73` cost comment to describe both calls, and drop or reword the "not recomputed per-frame" sentence in `RegionAmbientRes`'s doc so it matches whichever behaviour ships.

---

### PERF-D1-2026-08-30-04: the lock tracker materialises its `held_others` snapshot *before* the detector's own enabled check, so every ECS lock acquisition in a debug build heap-allocates while the code documents that path as "one relaxed load"
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/core/src/ecs/lock_tracker.rs:115-122` (`track_read`) and `:160-167` (`track_write`); the early-outs they feed are at `:344-348`
- **Status**: NEW. **Not** a regression of #823 — that baseline is "the `held_others` collection is `#[cfg(debug_assertions)]`-gated", and it still is (re-verified; `AUDIT_ECS_2026-08-30.md:775` records the same). This finding is about the *debug*-build path #823 deliberately left on, and about the cost claim written beside the enabled flag.
- **Description**: `ENABLED`'s doc comment states the design intent plainly: "*Cached in an atomic so the per-acquire fast-path is one relaxed load*" (`lock_tracker.rs:270-272`). It is not. Both callers build the snapshot unconditionally inside the `cfg(debug_assertions)` block and only then call `record_and_check`, which checks `held_others.is_empty()` first and `ENABLED.load(...)` second. So in any debug build with `BYRO_LOCK_ORDER_CHECK` unset — the default, and the mode `CLAUDE.md`'s Quick Reference documents as the way to launch the engine (`cargo run`) and run the suite (`cargo test`) — every `world.query` / `query_mut` / `resource` / `try_resource` / `World::get` taken while at least one other lock is held iterates the thread-local `HashMap` and collects a `Vec<(TypeId, &'static str)>` that is then discarded at the `ENABLED` load.
- **Evidence**:
```rust
// crates/core/src/ecs/lock_tracker.rs:115-122  (identical block at :160-167)
#[cfg(debug_assertions)]
{
    let held_others = locks
        .borrow()
        .iter()
        .map(|(id, state)| (*id, state.type_name))
        .collect::<Vec<_>>();                       // allocates before any enabled check
    global_order::record_and_check(type_id, type_name, &held_others);
}

// :344-348 — the checks that make the work pointless, both AFTER the collect
if held_others.is_empty() { return; }
if !ENABLED.load(Ordering::Relaxed) { return; }
```
  The nesting depth is what makes it compound: `collect_static_mesh_draws` holds ~24 read queries concurrently (`byroredux/src/render/static_meshes.rs:100-166`), so acquisitions 2..24 each allocate a `Vec` of length 1..23; and `animation_system_inner` holds `AnimationClipRegistry` + `NameIndex` across the whole body (`byroredux/src/systems/animation.rs:515-600`) while re-acquiring `Transform` / `AnimationTextKeyEvents` / the animated-channel sinks **per animated entity** (`:696`, `:757`, `:766-793`).
- **Impact**: Debug-build only — release is genuinely zero-cost, and #823's stated contract is not violated. But debug is the everyday development and test configuration, so this inflates `cargo test` wall time and makes any debug-build profile of the ECS hot path unrepresentative. Allocation count scales as (locks already held) × (acquisitions per frame), which for the animation path is per-entity: the checked-in actor baselines are `skin_pool_live` 206 / 248 / 83 (`.claude/audit-baselines/runtime/*.tsv`). **No `dhat` or allocation guard covers this site.** I did not measure the wall-clock cost and it is unknown.
- **Related**: #823 (ECS-PERF-01, the `cfg(debug_assertions)` gate — still intact); #2675 (the reachability generalisation inside `record_and_check`); #2384; `AUDIT_ECS_2026-08-30.md` ECS-D3-01, which reports a *correctness* gap in the same block (the `recursive_read` early return skipping `record_and_check`) — a fix for that and a fix for this touch the same lines and should land together.
- **Suggested Fix**: Expose a `global_order::is_enabled()` (or reuse `set_enabled_for_tests`' flag) and hoist both the emptiness and the enabled test above the `collect`: `if locks.borrow().len() > 0 && global_order::is_enabled() { … }`. Behaviour is identical — `record_and_check` already returns on both conditions — and the fast path then really is one relaxed load, as documented.

---

### PERF-D2-2026-08-30-02: the #2691 note in `render/mod.rs` transcribed two derived counts, and both are now wrong — the exact rot its own last sentence warns against

- **Severity**: LOW
- **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/mod.rs:818-825`
- **Status**: NEW (the note itself is the #2691 fix; this is fresh drift in it, not a re-file of #2691)
- **Description**: The note reads:
  > "…see the `bench_draws_cmds` column of `.claude/audit-baselines/runtime/*.tsv`, where **exactly one
  > of five cells falls in that band**, **three sit in the 1800–2600 range**, and the FO4 baseline is
  > *above* this gate and takes the parallel path. Cited rather than transcribed, per the audit's
  > cite-don't-copy rule — **a number copied here is a number that rots.**"

  Checked against the five TSVs at HEAD: **zero** cells fall in the quoted 400–1500 band (oblivion 325
  is below it, fo3 1581 is above it), and **two**, not three, sit in 1800–2600 (fnv 2110, skyrim 2342).
  The note avoided copying the raw `bench_draws_cmds` values but copied the *counts derived from them*,
  which rot identically — and did rot, one day after the note landed. The third clause is separately
  unsupported (see PERF-D2-2026-08-30-01).
- **Evidence**: `bench_draws_cmds` at HEAD = 325 / 1581 / 2110 / 2342 / 3949 (table above).
  The counts were **already wrong when written**: `.claude/issues/2691-2692-2695-2696/ISSUE.md:127-135`
  records the then-current column as 324 / 1839 / 2342 / 2553 / 3440 — of which none is in 400–1500
  either, while three were in 1800–2600. `git log -p` on the fo3 TSV shows `bench_draws_cmds` 1839 →
  1581 at `fb21f9ee`, corrected again at `e0a9ee54` (#3407, 2026-08-28), which is what took the
  1800–2600 count from three to two.
- **Impact**: Documentation only, but it is the specific document written to stop the next tuner
  reasoning from a stale distribution — and a reader who checks the "exactly one of five" claim and
  finds it false has no way to tell which of the note's remaining assertions still hold.
- **Related**: #2691 / PERF-D2-03, #3407 (`e0a9ee54`), #3005 (CLOSED at `cc666a48`)
- **Suggested Fix**: Replace both counts with the qualitative statement the evidence actually
  supports — "no baseline cell sits in the quoted 400–1500 band; the median cell is ~2 100 commands"
  — or drop the counts entirely and point at the TSVs, which is what the note's own cite-don't-copy
  rule prescribes.

---

### PERF-D2-2026-08-30-03: the per-instance `GpuInstance` loop probes two `std::collections::HashMap`s per draw per frame, and the #3061 guard structurally cannot see them

- **Severity**: LOW
- **Dimension**: Draw & Instancing
- **Location**: `crates/renderer/src/texture_registry.rs:134` (`texture_has_alpha: HashMap<TextureHandle, bool>`) and `:141` (`texture_avg_rgb: HashMap<TextureHandle, [f32; 3]>`); read sites `crates/renderer/src/vulkan/context/draw.rs:2939-2942` and `:2981-2990`
- **Status**: NEW — a new site of the #3061 (CLOSED) hot-path-hashing cluster, in a file that cluster's fix and guard never covered
- **Description**: In the `for draw_cmd in draw_commands` loop that builds `GpuInstance`,
  `handle_avg_rgb(draw_cmd.texture_handle)` is called **unconditionally for every draw command**
  (outside the `skip_batch` gate, because RT hits read `avg_albedo` off off-frustum instances), and
  `handle_has_alpha(draw_cmd.texture_handle)` is called for every alpha-blend draw. Both resolve
  through `std::collections::HashMap` — SipHash-1-3 — over a `TextureHandle` key that is a **dense
  index** (`let handle = self.textures.len() as TextureHandle;`, `texture_registry.rs:678`) into the
  registry's own `textures: Vec<TextureEntry>` (`:127`). This is the per-frame per-entity keyspace the
  #2923 hot-path-hashing rule names, in the crate the rule names, and it is the highest-volume site in
  the cluster — once per `DrawCommand`, i.e. up to 3 949 probes/frame on `fo4-InstituteBioScience`,
  against #3061's morph/skin sites which are bounded by skinned-entity count.
  The guard that exists to stop this cluster drifting back is a source-text scan whose corpus is
  `include_str!("mod.rs")` + `init.rs` + `draw.rs` + `skinned_blas_refit.rs`
  (`crates/renderer/src/vulkan/context/mod.rs:2823-2828`, `:2877`, `:2882`, `:2977`) — `texture_registry.rs`
  is outside it, so these two fields are invisible to it by construction.
- **Evidence**:
  ```rust
  // crates/renderer/src/vulkan/context/draw.rs:2981-2990 — no gate above it
  let gi_albedo = match self
      .texture_registry
      .handle_avg_rgb(draw_cmd.texture_handle)
  { Some(mean) => [ /* … */ ], None => draw_cmd.avg_albedo };
  ```
  ```rust
  // crates/renderer/src/texture_registry.rs:718-720
  pub fn handle_avg_rgb(&self, handle: TextureHandle) -> Option<[f32; 3]> {
      self.texture_avg_rgb.get(&handle).copied()
  }
  ```
  Both maps are written only at the two DDS-load points (`:693`, `:1001`, `:1013`) — load-time, never
  per frame — so nothing about them is DoS-facing (unlike `path_map: HashMap<String, …>` at `:128`,
  which should stay std). The remaining three callers of `handle_has_alpha`
  (`byroredux/src/cell_loader/terrain.rs:692`, `byroredux/src/cell_loader/spawn/mesh_instance.rs:853`,
  `byroredux/src/scene/nif_loader.rs:1104`) are all load-time.
- **Impact**: Small but strictly-wasted CPU on the frame's largest loop, growing linearly with draw
  count — i.e. worst exactly on the cell (`fo4-InstituteBioScience`, 44.3 FPS p50) that is already the
  slowest of the five. No correctness effect. The structural half matters more than the cycles: the
  guard the project added after revisiting this cluster four times (#1368 → #2174 → #2923 → #3061)
  cannot observe the two busiest remaining sites.
- **Related**: #3061 (CLOSED — the conversion landed for `skin_slots` / `morph_slots` /
  `failed_skin_slots` / `failed_skin_blas` / `blend_pipeline_cache` / `blend_seen_scratch`), #2923,
  #2174, #1368; `PERF-D6-2026-08-24-01` (`AUDIT_PERFORMANCE_2026-08-24.md`, the morph sibling, same class)
- **Suggested Fix**: Because `TextureHandle` is a dense index into `textures`, the right fix removes
  the hashing rather than swapping the hasher: move `has_alpha: bool` and `avg_rgb: Option<[f32; 3]>`
  onto `TextureEntry` and make both accessors `self.textures.get(handle as usize)`. If that is too
  invasive, `FxHashMap` is the minimum. Either way, extend the #3061 source-scan corpus to include
  `texture_registry.rs` so the cluster cannot re-grow outside `context/`.

---

### PERF-D3-2026-08-30-03: `memory-budget.md` scene-buffer + texture-registry rows drift from the code on `MAX_LIGHTS`, `GpuTerrainTile` stride and descriptor-pool multiplicity
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `docs/engine/memory-budget.md:30` (Light SSBO row), `:35`
  (Terrain tile row), `:421` (texture descriptor pool row);
  code: `crates/renderer/src/shader_constants_data.rs:41-44`,
  `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:9-18`,
  `crates/renderer/src/texture_registry.rs:434-446`
- **Status**: NEW (the `GpuInstance` / `GpuCamera` rows in the same table are
  **Existing: #3447**; the total-row understatement is mostly downstream of it)
- **Description**: Three rows in the authoritative ledger no longer match the code
  they document, and none of the three is covered by #3447/#3450/#3463:
  1. **`MAX_LIGHTS`** — the doc says `512`. `MAX_LIGHTS` is now
     `RESERVOIR_LIGHT_MASK as usize`, i.e. `(1 << RESERVOIR_LIGHT_BITS) - 1` with
     `RESERVOIR_LIGHT_BITS = 10` → **1023**. That coupling is the fix for closed
     #2778 ("`RESERVOIR_LIGHT_MASK` has no lockstep guard against `MAX_LIGHTS`");
     the ledger did not follow. Light SSBO is `sizeof(LightHeader) + 1023 × 64 B`
     per FIF, ≈ 131 KB across both copies, not the documented 64 KB.
  2. **`GpuTerrainTile`** — the doc says `32 B` per slot → 32 KB total. The struct
     is three `[u32; 8]` arrays = **96 B**, test-pinned by
     `gpu_terrain_tile_is_96_bytes` (`scene_buffer/gpu_instance_layout_tests.rs:300-307`,
     the #2463 lockstep test) and used verbatim as the SSBO stride in
     `buffers.rs:479-480`. Real total ≈ 96 KB.
  3. **Texture descriptor pool** — the doc says `max_textures × MAX_FRAMES_IN_FLIGHT`
     combined image samplers. `texture_registry.rs:438` allocates
     `max_textures * 2 * MAX_FRAMES_IN_FLIGHT` (two bindings: `sampler2D[]` +
     `samplerCube[]`), and `:1735` repeats the `× 2` on the rebuild path — a 2×
     understatement of descriptor-pool sizing.

  Two per-FIF buffers allocated in `allocate_scene_render_buffers` have **no row
  at all**: the DALC cube UBO (`GpuDalcCube`) and the selected-ray probe SSBO
  (`GpuSelectedRayProbe`, 144 B test-pinned). Both are small; the gap is that the
  table reads as exhaustive.
- **Evidence**:
  ```rust
  // crates/renderer/src/shader_constants_data.rs:41-44
  pub const RESERVOIR_LIGHT_BITS: u32 = 10;
  pub const RESERVOIR_LIGHT_MASK: u32 = (1u32 << RESERVOIR_LIGHT_BITS) - 1;   // 1023
  pub const MAX_LIGHTS: usize = RESERVOIR_LIGHT_MASK as usize;
  ```
  ```rust
  // crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:9-18 — 3 × uint[8] = 96 B
  pub struct GpuTerrainTile {
      pub layer_diffuse_index:  [u32; 8],
      pub layer_normal_index:   [u32; 8],
      pub layer_specular_index: [u32; 8],
  }
  ```
  ```rust
  // crates/renderer/src/texture_registry.rs:438
  descriptor_count: max_textures * 2 * MAX_FRAMES_IN_FLIGHT as u32,
  ```
- **Impact**: Small in absolute bytes (~100 KB across all three), but this page is
  the file every other audit and every budgeting decision is told to cite instead
  of re-deriving. A row that is wrong by 2× is worse than a missing row, because
  it is trusted. The `MAX_LIGHTS` case additionally hides that the light cap is
  now *derived* from a reservoir bit-field, so raising it is a shader-contract
  change, not a constant bump — precisely the kind of thing a budget page exists
  to say.
- **Related**: #3447 (same table, `GpuInstance` 128→160 B and the resulting
  Instance-SSBO / total-row understatement — **not re-filed here**), #3450
  (`GpuCamera` 352→368 B in the audit SKILL files), #2778 (closed, the change that
  moved `MAX_LIGHTS`), #2463 (closed, the test that pins 96 B).
- **Suggested Fix**: Correct the three rows, add DALC + selected-ray-probe rows,
  and recompute the "Total resident scene buffers" line once #3447's
  `GpuInstance` correction lands (code total is ≈ 243 MB vs the documented
  ~225 MB, ~17 MB of which is #3447's Instance SSBO row).

---

### PERF-D3-2026-08-30-04: post-#2929 doc rot on the TLAS shrink path — two comments still assert `shrink_tlas_to_fit` destroys the slot
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/memory.rs:246-252`
  (`shrink_tlas_scratch_to_fit` case-1 doc),
  `crates/renderer/src/vulkan/context/draw.rs:4040-4047` (call-ordering comment)
- **Status**: NEW
- **Description**: #2929 / CON-D1-01 changed `shrink_tlas_to_fit` from
  "`take()` the slot and destroy the AS + its three buffers" to "set
  `tlas_shrink_pending[slot_index] = true` and let `ensure_tlas_state` fold the
  shrink into its allocate-then-swap path". The function body and its own
  `#2929` block comment are correct and the behaviour is verified below. Two
  *other* comments were not updated and now describe the removed behaviour:
  - `shrink_tlas_scratch_to_fit`'s case-1 doc says the arm handles
    "`tlas[slot_index]` is `None` (slot was destroyed by
    [`Self::shrink_tlas_to_fit`])". That producer no longer exists;
    `shrink_tlas_to_fit` never leaves the slot `None`.
  - `draw.rs`'s ordering comment justifies the call order as "run AFTER
    `shrink_tlas_to_fit` so a destroyed slot lets the scratch shrink hit its
    'tlas[slot] is None → drop scratch entirely' arm in one tick". That
    interaction cannot occur; the ordering is now arbitrary.
- **Evidence**:
  ```rust
  // acceleration/memory.rs — what the code actually does now
  let old_max = slot.max_instances;
  self.tlas_shrink_pending[slot_index] = true;   // request, do not destroy
  ```
  ```rust
  // acceleration/memory.rs:246-252 — what the sibling doc still claims
  /// 1. `tlas[slot_index]` is `None` (slot was destroyed by
  ///    [`Self::shrink_tlas_to_fit`]) — drop the scratch entirely.
  ```
- **Impact**: No runtime effect — both arms remain correct in isolation and the
  reserve floors hold (verified below). The cost is to the next reader of the
  shrink path: the stale comments make case 1 look reachable-by-design from the
  sibling call and make the `draw.rs` ordering look load-bearing when it is not,
  on a code path whose whole history (#1782, #2673, #2915, #2929) is
  destroy-ordering bugs. `AUDIT_RENDERER_2026-08-24` already flagged #2774's
  case-2 reachability claim as needing re-verification for the same reason.
- **Related**: #2929, #2915, #2673, #2774 (case-2 reachability, flagged
  2026-08-24 and still open).
- **Suggested Fix**: Reword case 1 to name its real producers (fresh slot at
  startup; a slot never rebuilt after a failed `ensure_tlas_state`) and drop the
  "order matters" claim in `draw.rs`, or state the real reason to keep the order
  if one is wanted.

---

### PERF-D4-2026-08-30-04: `CameraUBO` is the only hand-duplicated GPU struct with no field name/order/type lockstep test — it is pinned by size alone
- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:359` (`pub struct GpuCamera`), `crates/renderer/src/vulkan/reflect.rs:606-641`
  (`camera_ubo_size_matches_gpu_camera_in_every_shader`),
  `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:66-79`
  (`gpu_camera_is_368_bytes`); the five GLSL declarations at
  `crates/renderer/shaders/include/bindings.glsl:280`, `triangle.vert:106`, `water.vert:83`,
  `cluster_cull.comp:57`, `caustic_splat.comp:68`
- **Status**: NEW (test-gap; **no live drift** — all five declarations verified identical at
  368 B this session)
- **Description**: Every other multi-copy GPU struct in this crate has a parsed, field-by-field
  lockstep test: `GpuInstance` across five GLSL mirrors plus the Rust struct
  (`gpu_instance_glsl_copies_stay_in_lockstep`, #2748), `GpuLight` across four
  (`gpu_light_glsl_copies_stay_in_lockstep`, #1916), `GpuMaterial` against
  `include/bindings.glsl` including a per-field scalar-type check
  (`gpu_material_glsl_field_order_matches_rust_struct`, #1657 / #2688). `CameraUBO` — declared
  by hand in five GLSL sources and read by six shaders — has neither. Its only guards are
  `size_of::<GpuCamera>() == 368` on the Rust side and a SPIR-V *block size* reflection pin on
  the shipped `.spv`. Both are blind to a within-size reorder (`skyTint` ↔ `sunDirection`,
  say — two adjacent `vec4`s in a struct that is entirely `vec4`s) and to a type flip
  (`uvec4 renderDebug` → `vec4`, whose contents are bitcast flags). #2688 established that exact
  type-flip class as "byte-lethal" for `GpuMaterial` and added a check; the camera got none.

  The parser infrastructure to close it already exists in the same file
  (`parse_glsl_struct_fields_typed` / `parse_rust_struct_fields_typed`,
  `shader_contract_tests.rs:1260-1300`).

  Adjacent, same root: four shader sources still direct the reader to `scene_buffer.rs` for the
  camera contract — `caustic_splat.comp:62`, `cluster_cull.comp:51`, `cluster_cull.comp:59`,
  `triangle.vert:99` (the last spells out `crates/renderer/src/vulkan/scene_buffer.rs`). That
  file was split into `scene_buffer/` in Session 34; the struct now lives in
  `scene_buffer/gpu_types.rs`.
- **Evidence**:
  ```rust
  // reflect.rs:632-641 — size only, no field identity
  let size = uniform_block_size_by_name(spv, "CameraUBO")…;
  assert_eq!(size, expected, "{name}.spv CameraUBO is {size} B but GpuCamera is {expected} B …");
  ```
  ```rust
  // shader_contract_tests.rs:1746-1748, on the GpuInstance test — naming the precedent
  /// … the full lockstep guard `GpuMaterial` and `GpuLight` already have.
  ```
- **Impact**: None today. The exposure is that the camera UBO's five hand-maintained copies are
  the least-guarded GPU contract in the renderer, in a repo where this exact class has recurred
  eight times (#417, #1447, #1493, #1657, #1916, #2688, #2748, #3231) and where a same-size
  reorder produces wrong lighting/motion-vector math with no validation-layer signal.
- **Related**: #2748, #1916, #1657, #2688, #1447; #3447 (the stale "352 B … plus ten" prose in
  `gpu_camera_is_368_bytes`'s own doc comment is already listed in that issue's locations and is
  **not** re-reported here).
- **Suggested Fix**: Add `camera_ubo_glsl_copies_stay_in_lockstep` alongside the `GpuLight` and
  `GpuInstance` tests, parsing `uniform CameraUBO` out of the five GLSL sources with the
  existing typed parser and comparing name, order and scalar type against
  `pub struct GpuCamera`. Repoint the four stale `scene_buffer.rs` shader comments at
  `scene_buffer/gpu_types.rs` in the same change.

---

### PERF-D5-2026-08-30-04: the volumetrics per-froxel ray-budget annotation still quotes the divisor-4 grid retired four days after it was written
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:539-546` (the doc
  comment on `VOLUMETRIC_OUTPUT_CONSUMED`)
- **Status**: NEW
- **Description**: The comment states the shader casts up to 10 ray-query
  traversals per froxel, "~36.9M ray queries/frame at the default 320x180x64 grid
  for a 1280x720 render extent". 320×180 at 1280×720 is `froxel_xy_divisor = 4`.
  The shipped default is **8** (`VolumetricsConfig::default`,
  `crates/renderer/src/vulkan/upscaling.rs`), so the default grid at that extent is
  160×90×64 = 921 600 froxels and the worst-case budget is ~9.2M ray queries/frame,
  not ~36.9M.
- **Evidence**: `fc9e3e39` ("perf(volumetrics): froxel_xy_divisor default 4 -> 8,
  and record the divisor's measured perceptual cost", 2026-08-21) changed the
  default; the annotation was written by `0ff7b537` (2026-08-17) while the default
  was still 4, and was not revisited. `docs/engine/memory-budget.md:235` **was**
  updated ("default **8**, Frostbite's own density"), so the in-code comment is the
  lone survivor.
- **Impact**: This is the module's headline cost figure and the one an engineer
  tuning `--froxel-xy-divisor` or the `MAX_FROXEL_LIGHTS = 8` cap
  (`crates/renderer/shaders/volumetrics_inject.comp:2506`) reads first. Overstating
  it 4× argues for cutting a budget that is already a quarter of what the comment
  says. `#2509`, which produced this annotation, is CLOSED — so nothing will
  revisit it.
- **Related**: closed `#2509`; `docs/engine/memory-budget.md:328-329` already
  records the divisor history for the VRAM ledger.
- **Suggested Fix**: Restate as "~9.2M ray queries/frame at the default
  160×90×64 grid for a 1280×720 render extent (`froxel_xy_divisor = 8`)", and
  reference `VolumetricsConfig::default` rather than hardcoding the grid so the
  next divisor change invalidates one place, not two.

---

### PERF-D5-2026-08-30-05: the volumetrics gate-off arm re-clears the whole integrated froxel volume every frame, with no already-cleared latch
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:2561-2600`
  (`record_neutral_frame`); call sites
  `crates/renderer/src/vulkan/context/post_passes.rs:515` and `:727`
- **Status**: NEW
- **Description**: When `requires_dispatch` returns false (no global medium, no fog
  volumes, no lingering combustion), `record_volumetrics_pass` calls
  `record_neutral_frame`, which issues two image barriers and a full
  `cmd_clear_color_image` over the integrated froxel volume — **every frame, for as
  long as the gate stays off**. The image is already neutral after the first such
  frame; nothing writes it in between.
- **Evidence**: the gate-off arm is unconditional —
  ```rust
  if !vol.requires_dispatch(volumetric_time_seconds, scatter_coef > 0.0, fog_volumes) {
      vol.record_neutral_frame(&self.device, cmd, frame);
  }
  ```
  Contrast the caustic pass ~200 lines above in the same file, which solves exactly
  this with a per-FIF latch: `caustic_skip_clear_decision(ran, self.caustic_cleared_on_skip[frame])`
  returns `(should_clear, next_latch)` so the clear happens once per skip streak,
  and the predicate is a pure, unit-tested function
  (`post_passes.rs:1145-1190`).
- **Impact**: Derived from the shipped `FROXEL_FORMAT` (`R16G16B16A16_SFLOAT`,
  8 B/froxel) and the default grid: 7.4 MB of clear traffic per frame at a
  1280×720 render extent, 22.1 MB at 1080p, **66.4 MB at native 4K** — repeated at
  frame rate, in fog-free cells, to write zeros over zeros. It is inside the
  `volumetrics_ms` bracket, so it is measurable today. Frequency depends on how
  many cells author no fog medium at all, which I have not sampled — hence LOW
  rather than MEDIUM.
- **Related**: `caustic_skip_clear_decision` / `caustic_cleared_on_skip` (`#2507`)
  is the in-repo precedent and the template for the fix.
- **Suggested Fix**: Add a *volumetrics_cleared_on_skip: [bool; MAX_FRAMES_IN_FLIGHT]*
  latch and reuse the `caustic_skip_clear_decision` shape (or lift it into a shared
  pure helper — it is already generic over "ran / already_cleared"). Reset the latch
  wherever the caustic one resets, and on `recreate_on_resize`.

---

### PERF-D5-2026-08-30-06: the `svgf` GPU timer bracket is named for one dispatch but encloses four screen-sized ones
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:21-22` (slot table) and
  `:149-151` (`svgf_ms` doc comment); actual bracket contents
  `crates/renderer/src/vulkan/svgf.rs:1290-1385` inside one `dispatch` call,
  bracketed at `crates/renderer/src/vulkan/context/post_passes.rs:330-334`
- **Status**: NEW
- **Description**: The query-slot table calls slots 12/13 "SVGF temporal dispatch",
  and `svgf_ms`'s doc says "SVGF temporal accumulation compute dispatch —
  motion-vector reprojection of last frame's denoised indirect." The bracket
  actually wraps `SvgfPipeline::dispatch` in full: one temporal dispatch **plus**
  `ATROUS_ITERATIONS` (= 3, `crates/renderer/src/vulkan/svgf.rs:98`) à-trous
  dispatches, each `width.div_ceil(8) × height.div_ceil(8)`, each followed by a
  COMPUTE→COMPUTE barrier. Three of the four full-screen dispatches under the
  number are not what the number is named after.
- **Evidence**: `post_passes.rs:330-334` places `cmd_svgf_start`/`cmd_svgf_end`
  around the single `svgf.dispatch(...)` call; `svgf.rs:1339-1385` runs the
  `for k in 0..ATROUS_ITERATIONS` loop inside that same call.
  `docs/engine/shader-pipeline.md:104-108` documents the pipeline correctly
  ("`svgf_atrous.comp` ×3"), so only the instrument's own labels are wrong.
- **Impact**: Directly degrades the instrument this audit dimension is required to
  cite. An operator reading a high `svgf_ms` will chase temporal reprojection when
  75% of the bracketed dispatches are the spatial filter — and the à-trous loop is
  precisely where a past audit found redundant per-iteration variance work
  (`AUDIT_PERFORMANCE_2026-07-02.md:351`, then at `ATROUS_ITERATIONS = 5`). It also
  blocks the obvious next question — "temporal or spatial?" — which two brackets
  would answer for free.
- **Related**: `#2278`/`PERF-D9-01` (the `_active` flags) is the prior work on this
  instrument's honesty.
- **Suggested Fix**: Minimum: rename the slot-table rows and the `svgf_ms` doc to
  "SVGF temporal + à-trous (`ATROUS_ITERATIONS`) dispatches". Better: split into
  *svgf_temporal_ms* / *svgf_atrous_ms* (query pool 28→30) so the two costs are
  separable — the à-trous loop is the tunable one.

---

### PERF-D5-2026-08-30-07: the render-pass construction comment still describes 7 attachments including a reservoir target retired by #1583/#1590
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/context/init.rs:304-305`
- **Status**: NEW
- **Description**: The comment immediately above the `create_render_pass` call
  reads "Main render pass: 7 color attachments (HDR + G-buffer + raw_indirect +
  albedo + reservoir) + depth." The pass has **8** colour attachments plus depth
  (`crates/renderer/src/vulkan/context/helpers.rs:148`, `:223`, and the nine-entry
  `attachments` array at `:314-323`), and there has been **no reservoir
  attachment** since `#1583`/`#1590` retired it — the `GBufferFormats` struct the
  very next lines populate names `fsr_mask_format` where the comment says
  "reservoir".
- **Evidence**: the comment and the struct literal it introduces disagree
  field-for-field; `helpers.rs:109-111`'s own doc comment ("The nine attachment
  formats the main render pass writes — eight G-buffer color targets … plus
  depth") is correct.
- **Impact**: Pure doc-rot, but on the exact premise this audit dimension is
  warned about — "the ReSTIR reservoir attachment was retired 7→6, then 6→7" — so
  it is a live source of the stale-premise findings the skill's HARD RULE 1 exists
  to catch. `#3433` (open) is the same rot in `docs/engine/ui.md`, which still
  describes a 6-attachment main render pass.
- **Related**: open `#3433`; closed `#1583`, `#1590`.
- **Suggested Fix**: One-line correction to "8 color attachments (HDR + normal +
  motion + mesh_id + raw_indirect + albedo + FSR reactive + FSR transparency) +
  depth". Also fix the duplicated `// 10.` step number on the following block while
  in there.

---

### PERF-D6-2026-08-30-02: `update_morph_weights` heap-allocates a fresh `Vec<f32>` per morph slot per frame and unconditionally marks the slot dirty, discarding the right-sized `pending_weights` buffer `MorphSlot` already owns

- **Severity**: LOW
- **Dimension**: Skinning & BLAS
- **Location**: `byroredux/src/render/skinned.rs:285-292`; `crates/renderer/src/vulkan/morph_compute.rs:175-183,188-191`
- **Status**: NEW — first flagged as the unfixed half of `PERF-D6-2026-08-24-01`
  (`docs/audits/AUDIT_PERFORMANCE_2026-08-24.md:231-281`, re-confirmed still open at
  `docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md:119`), which was deferred into #3061
  "one-pass conversion" and never filed on its own. **#3061 is now CLOSED**
  (`c82f4f29`) and its commit body scopes it to the `FxHashMap`/`FxHashSet`
  conversion only — so this half fell through the close. Credit to the 2026-08-24
  audit for the original observation.
- **Description**: `MorphSlot::create` already allocates `pending_weights: vec![0.0;
  target_count]` (`morph_compute.rs:165`) — a permanently right-sized staging buffer.
  `update_morph_weights` nonetheless builds a brand-new `Vec<f32>` via `collect()`
  every frame for every live morph slot and hands it to `stage_weights`, which
  *replaces* `pending_weights` — dropping (freeing) the previous allocation. That is
  one malloc + one free per morphed entity per frame on the per-frame render path,
  for data that is almost always byte-identical to what the slot already holds.
  The same call also sets `pending_weights_dirty = true` unconditionally, so
  `flush_pending_weights` re-executes its mapped-memory `copy_from_slice` +
  `flush_if_needed` for every slot every frame even when no weight changed — the
  early-out at `morph_compute.rs:189` can never fire in steady state.
- **Evidence**:
  ```rust
  // byroredux/src/render/skinned.rs:285-292 — per frame, per morph slot
  for (&entity, slot) in ctx.morph_slots.iter_mut() {
      let Some(weights) = weights_q.get(entity) else { continue; };
      let target_count = slot.target_count() as usize;
      let flat: Vec<f32> = (0..target_count).map(|i| weights.get(i)).collect();   // malloc
      slot.stage_weights(flat);                                                   // free of the old one
  }
  ```
  ```rust
  // crates/renderer/src/vulkan/morph_compute.rs:175-183
  pub fn stage_weights(&mut self, weights: Vec<f32>) {
      …
      self.pending_weights = weights;        // discards the pre-sized buffer
      self.pending_weights_dirty = true;     // unconditional → flush can never early-out
  }
  ```
- **Impact**: Bounded and small per entity, but it is squarely on the per-frame
  per-entity render path the #2923/#3061 hot-path rule exists to keep clean, and it is
  the last remaining allocator traffic on that path. Size is unmeasurable today
  because `SkinCoverageFrame` carries no morph counter — the number of live
  `MorphSlot`s is a strict subset of `skin_pool_live` (248 / 206 / 83 on the FO4 /
  FNV / Skyrim baselines) but is not itself recorded anywhere. This finding is
  deliberately LOW: it is allocator churn and a redundant small mapped write, not a
  per-frame leak.
- **Related**: `PERF-D6-2026-08-24-01` (origin), #3061 (CLOSED — covered only the
  hashing half), #3231 (landed the morph path), #3244 (the dual-fence rule
  `flush_pending_weights` implements — any fix must keep the flush after the fence
  wait, only make it conditional).
- **Suggested Fix**: Change `stage_weights` to take `&[f32]` (or a closure) and write
  in place into `pending_weights`, setting `pending_weights_dirty` only when the new
  values differ from the stored ones. `update_morph_weights` then writes directly into
  the slot's own buffer with no allocation and no `collect()`.

---

### PERF-D6-2026-08-30-03: the `unsafe` push-constant slice in `SkinComputePipeline::dispatch` carries a SAFETY comment describing a 12-byte three-`u32` struct that has been 32 bytes with six fields since #3231, and cites a test name that does not exist

- **Severity**: LOW
- **Dimension**: Skinning & BLAS
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs:680-688`
- **Status**: NEW
- **Description**: `SkinPushConstants` gained `morph_delta_address: u64` and
  `morph_weight_address: u64` at the *front* and `morph_target_count: u32` at the tail
  under #3231 — six fields, 32 bytes, pinned by `push_constants_size_is_32_bytes`
  (`skin_compute.rs:1177`) and mirrored in `skin_vertices.comp:92-110`. The SAFETY
  comment justifying the `from_raw_parts` byte view was not updated: it still asserts
  *"`repr(C)` with three u32 fields, 12 bytes, no interior padding"* and directs the
  reader to a `push_constants_size_is_12_bytes` test that exists nowhere in the tree.
  The code itself is correct — the length argument is `PUSH_CONSTANTS_SIZE`, derived
  from `size_of::<SkinPushConstants>()` (`:76`) — so this is a wrong justification,
  not a wrong slice.
- **Evidence**:
  ```rust
  // crates/renderer/src/vulkan/skin_compute.rs:680-684
  // SAFETY: `SkinPushConstants` is `repr(C)` with three u32 fields,
  // 12 bytes, no interior padding. The slice is contiguous +
  // aligned (…; mismatched
  // shape is caught by `push_constants_size_is_12_bytes` test).
  ```
  ```
  $ grep -rn "push_constants_size_is" crates/renderer/src/
  crates/renderer/src/vulkan/skin_compute.rs:684:  // …`push_constants_size_is_12_bytes` test).
  crates/renderer/src/vulkan/skin_compute.rs:1177: fn push_constants_size_is_32_bytes() {
  ```
  The struct at `:48-74` now has `morph_delta_address: u64`, `morph_weight_address:
  u64`, `vertex_offset: u32`, `vertex_count: u32`, `bone_offset: u32`,
  `morph_target_count: u32`. The "no interior padding" claim is now load-bearing in a
  different way than the comment describes: it holds only *because* #3231 deliberately
  put the two `u64`s first (its own doc comment at `:50-58` explains this), which the
  SAFETY comment does not mention.
- **Impact**: No runtime effect today. The cost is that the next person changing this
  struct is told to verify against a size and a test that are both wrong — exactly the
  failure mode `_audit-common.md`'s backticked-symbol rule was added for, and the same
  class as `GpuMaterial` sitting documented at 300 B after it grew to 348 B. Per
  `_audit-severity.md`, an `unsafe` block whose justification does not describe the
  code is at least a hardening gap; it is LOW rather than MEDIUM because a real
  (correct) `size_of`-derived length and a real 32-byte pin both exist.
- **Related**: #3231 (grew the struct), `feedback_shader_struct_sync` (the
  Rust↔GLSL lockstep rule this comment is the local instance of),
  `skin_vertices.comp:92-110` (the GLSL mirror, which *is* correct and even names the
  right test).
- **Suggested Fix**: Rewrite the comment to say six fields / 32 bytes, name
  `push_constants_size_is_32_bytes`, and reference the #3231 field-ordering rationale
  as the reason there is no interior padding.

---

### PERF-D7-2026-08-30-03: the LOD-coverage and terrain-seam diagnostics recompute from scratch on every reconcile frame, including two O(n²) scans, for two console commands
- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/streaming_helpers.rs:128-130` (both calls, unconditional at the tail of `reconcile_lod_rings`), `:152-203` (`update_lod_coverage`), `:228-303` (`update_terrain_seam_stats`), `byroredux/src/cell_loader/lod_coverage.rs:53-111` (`find_overlaps`, `find_full_detail_overlaps`, `find_terrain_full_detail_overlaps`)
- **Status**: NEW
- **Description**: `reconcile_lod_rings` runs every frame while
  `state.lod_reconcile_pending` is set — the entire post-crossing settle
  window. Its last two statements run **unconditionally**, including on frames
  where the budget produced `attempted == 0` and nothing in `state.lod_blocks`
  / `state.object_lod_blocks` / `state.loaded` changed:
  - `update_lod_coverage` allocates four fresh `Vec`s of the resident key sets
    (`terrain_keys`, `object_keys`, `full_cells`, `terrain_keys_with_holes`),
    then runs `find_overlaps` **twice** — an all-pairs `O(n²)` rect scan
    (`lod_coverage.rs:55-62`) — plus two `O(lod_keys × full_cells)` scans, plus
    `resident_vwd_refr_cells`, which is a full `VisibleWhenDistant` query with a
    per-hit `GlobalTransform` lock (that per-entity-lock half is **#3142, OPEN —
    not re-filed here**).
  - `update_terrain_seam_stats` re-runs `check_seam` (33 height comparisons +
    33 normal-byte comparisons, `terrain_seam.rs:124`) over every adjacent
    resident pair — ~2 × `state.loaded.len()` pairs — against `LandscapeData`
    that is immutable for the worldspace's lifetime, so a pair's verdict cannot
    change until the resident set does.
- **Evidence**: `streaming_helpers.rs:128-130`:
  ```rust
  let complete = terrain_complete && object_complete && placement_complete;
  update_lod_coverage(world, state, complete);
  update_terrain_seam_stats(world, state);
  ```
  The only readers of the two resources they write are two `byro-dbg` console
  commands — `byroredux/src/commands/world_info.rs:491` (`LodCoverageStats`) and
  `:512` (`TerrainSeamStats`). Nothing in the render or streaming path consumes
  either; there is no `cfg`, env-var, or `--bench` gate.
  Ring size is set by the band ladder: `fBlockMaximumDistance = 250_000` BU
  (`lod_bands.rs:110`) over `EXTERIOR_CELL_UNITS`, with four levels
  (`LOD_LEVELS = [4, 8, 16, 32]`, `lod_bands.rs:86`) — so `terrain_keys` and
  `object_keys` are each in the low hundreds and `find_overlaps` is tens of
  thousands of rect tests per scheme per frame.
- **Impact**: Pure diagnostic overhead on the exact frames the settle-latency
  benchmark measures, so it inflates `StreamingTelemetry::lod_slices` and the
  *"Exterior LOD settled around (x, y) in N ms"* line it is supposed to be an
  observer of. It is the same shape as #3385 (a per-frame recompute of a value
  that only changes on a residency event) and #3389, both of which were
  accepted as worth fixing.
- **Related**: #3142 (OPEN — the `resident_vwd_refr_cells` per-entity lock,
  one component of this block); #3385 (the LOD-availability memo, same fix
  shape, LANDED); #3389 (`block_hole_mask`'s dead scan, LANDED).
- **Suggested Fix**: Gate both on a residency-change epoch — bump a counter in
  `stream_lod_blocks` / `stream_object_lod_blocks` / `stream_placement_lod_blocks`
  and in the `state.loaded` insert/remove sites, and skip both updaters when the
  epoch is unchanged since the last sample (`LodCoverageStats::settled` still
  needs the per-frame `settled` flag, which is a one-field write). The seam
  stats can go further: their input is `state.loaded`'s key set alone, so they
  only need recomputing on a boundary crossing.

---

### PERF-D7-2026-08-30-04: `PackedStorage::remove_entities_erased` reallocates and moves every *surviving* row per unload batch, so eviction cost is O(all resident rows), not O(victims)
- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `crates/core/src/ecs/packed.rs:256-286`, driven from `byroredux/src/cell_loader/unload.rs:334` (`world.despawn_batch(victims)`) via `crates/core/src/ecs/world.rs:181-186`
- **Status**: NEW
- **Description**: The merge-compaction that #2397 introduced (correctly —
  the prior `Vec::remove` loop was quadratic) rebuilds both backing vectors
  from scratch on every call:
  ```rust
  let old_entities = std::mem::take(&mut self.entities);
  let old_data = std::mem::take(&mut self.data);
  let mut retained_entities = Vec::with_capacity(old_entities.len());
  let mut retained_data = Vec::with_capacity(old_data.len());
  ```
  `despawn_batch` calls it once per registered storage
  (`world.rs:181-186`), so a boundary eviction of three cells pays
  `2 × sizeof(row) × live_rows` of allocate-plus-move for **each**
  `PackedStorage` component type — `Transform`, `GlobalTransform`,
  `SceneFlags`, `WorldBound` in production — regardless of how few entities
  the three victim cells actually own. The retained 90-plus percent of the
  exterior population is copied out and back on every crossing, and eight
  allocations of the full live size are handed to the allocator and freed.
- **Evidence**: `packed.rs:265-268` above; the four production `PackedStorage`
  declarations are `crates/core/src/ecs/components/transform.rs:69`,
  `global_transform.rs:152`, `scene_flags.rs:118`, `world_bound.rs:109`. The
  cost is already isolated by telemetry: it is precisely
  `UnloadPhaseTimings::despawn` (`unload.rs:332-352`), aggregated into
  `StreamingTelemetry::unload_despawn`.
- **Impact**: Boundary-frame CPU that scales with the *resident* world rather
  than with what is being torn down, and allocator churn proportional to the
  same. This sits on the boundary frame **outside** any budget (the unload at
  `app_step.rs:141` runs before `streaming_deadline` is even computed at
  `:196`), so it cannot be yielded away.
- **Related**: #2397 (introduced the merge pass — this is a refinement of that
  fix, not a regression of it); #2396 (its sort-order / dirty-marking test);
  #2148 (`shrink_storages`, the sibling pass in `finish_unload_batch`).
  Storage internals are `/audit-ecs` territory; filed here because exterior
  eviction is the sole hot caller and it is the streaming boundary's cost.
- **Suggested Fix**: Do the compaction **in place** with a read/write cursor
  over `self.entities` / `self.data` (the `Vec::retain` shape, but driven by
  the sorted victim cursor so the `mark_dirty` call is preserved). Same single
  pass, same output order, zero allocations, and half the memory traffic. Both
  existing tests (`remove_entities_erased_preserves_ascending_order`,
  `remove_entities_erased_marks_exactly_the_removed_ids_dirty`) pin the
  observable contract and should pass unchanged.

---

### PERF-D7-2026-08-30-05: `unload_cells` recomputes the whole-world cinematic-retention set once per victim cell, then hash-probes every victim against it even when it is empty
- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/unload.rs:18-48` (`cinematic_retained_entities`), `:176-177` (per-cell call + `retain`), driven per victim from `:112-122` (`unload_cells`)
- **Status**: NEW
- **Description**: `unload_cell_inner` opens with
  ```rust
  let retained = cinematic_retained_entities(world);
  victims.retain(|entity| !retained.contains(entity));
  ```
  `cinematic_retained_entities` is a **whole-world** property: it queries
  `HorseTetherState`, `ActorCinematicState` and `Children`, and walks the
  render hierarchy of whatever it finds. `unload_cells` calls
  `unload_cell_inner` once per root (`:118`), so the boundary's three-cell
  eviction ring builds that set three times. Its inputs cannot change between
  those calls — retained entities are explicitly removed from `victims`
  (`:177`) so they are never among the entities `despawn_batch` drops.

  Second, `victims.retain(...)` is an unconditional `std::collections::HashSet`
  (SipHash) probe per victim entity. `retained` is empty in every session that
  is not mid-cinematic, which is the universal case — the vanilla content that
  populates `HorseTetherState` / `ActorCinematicState` is a handful of scripted
  vehicle sequences.
- **Evidence**: `unload.rs:18-48` builds a fresh `HashSet<EntityId>` and takes
  three query read-guards on every call; `unload.rs:118`
  (`timings.absorb(unload_cell_inner(world, ctx, cell_root))`) is inside the
  per-root loop. The `retain` at `:177` has no `retained.is_empty()` guard, and
  the sibling early-out on the very next line (`if !retained.is_empty()`, `:178`)
  shows the author already had the predicate in hand for the other half.
  Charged to `UnloadPhaseTimings::ownership_index` (`:174`), so the fix is
  directly verifiable against `StreamingTelemetry::unload_ownership_index`.
- **Impact**: Small but on the unbudgeted boundary frame, and it scales with
  victim count × victim cells. `drain_streaming_state`'s whole-resident-set
  teardown makes it worse: 49 roots at `--radius 3`, 121 at
  `DEFAULT_TRANSITION_RADIUS = 5` (`app_step.rs:931`) — that is 121 whole-world
  cinematic scans for one door transition, where one would do.
- **Related**: #3380 (the victim-dedup discipline in the same function); #3386
  (the batching this finding extends — `unload_cells` hoisted
  `finish_unload_batch` out of the per-root loop but left this in it).
- **Suggested Fix**: Compute `cinematic_retained_entities` once in
  `unload_cells` and pass `&HashSet<EntityId>` down to `unload_cell_inner`
  (with `unload_cell` computing it for its single root), and skip the `retain`
  + the `CellRoot` removal entirely when the set is empty.

---

### PERF-D8-2026-08-30-03: the skinning parse path — the parser's largest per-block allocation family — has zero gate coverage, and carries two unreserved growth sites
- **Severity**: LOW
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/blocks/skin.rs:299` (`NiSkinPartition` strip branch) · `crates/nif/src/import/mesh/sse_recon.rs:134` (`reconstruct_sse_geometry`) · gate gap: `crates/nif/tests/heap_allocation_bounds*.rs` (no fixture declares `NiSkinData` / `NiSkinPartition` / `BSDismemberSkinInstance`)
- **Status**: NEW
- **Description**: Two sites grow a file-driven bulk vector from `Vec::new()` even though the element count is already in hand, which is the exact `#833` / `#831` pattern the `allocate_vec` / `read_pod_vec` family exists to remove:
  1. `blocks/skin.rs:299` — `let mut triangles = Vec::new();` then, in the strip branch, `triangles.extend(destrip(&strip))` once per strip (`:311-314`). `num_triangles` was read 47 lines earlier at `:252`. The sibling non-strip branch at `:318` correctly bulk-reads via `read_u16_triple_array`.
  2. `import/mesh/sse_recon.rs:134` — `let mut indices = Vec::new();` then three `push`es per triangle across every partition (`:141-153`). The total is `partition.partitions.iter().map(|p| p.triangles.len()).sum() * 3`, computable up front.

  Neither is reachable from any dhat fixture, so neither has a CI floor, and neither would be caught by a regression that made them worse.
- **Evidence**: `skin.rs:249-360` — `num_vertices` / `num_triangles` / `num_bones` / `num_strips` read at `:251-255`; `triangles` initialised `Vec::new()` at `:299`; strip `extend` loop `:311-314`. `sse_recon.rs:133-158` — `vertex_count` known at `:133`, `indices` `Vec::new()` at `:134`, `push` ×3 at `:150-152`. The module's own comment (`sse_recon.rs:113-127`) measures the corpus at 18,753,141 triangles over 26,940 skinned shapes, i.e. ~696 tri/shape ≈ 2,088 `push`es and ~11 reallocations per shape on a Skyrim actor/facegen load.
- **Impact**: Small in absolute terms (~11 doubling reallocs + ~8 KB of memcpy per skinned shape), but it is the guarded pattern re-appearing in the *one* block family that no gate watches, on the game with the largest skinned-content footprint. The real risk is the gate gap: `NiSkinPartition` is the deepest nested file-driven allocator in the parser (`allocate_vec(num_partitions)` → per partition `read_u16_array(num_vertices)` + `read_f32_array(num_vertices × num_weights_per_vertex)` + `read_bytes(num_vertices × num_weights_per_vertex)`), and nothing bounds it.
- **Related**: #833, #831, #1549 (the de-strip landing), #3355 (the SSE triangle bound retarget); PERF-D8-2026-08-30-02 (same gate).
- **Suggested Fix**: `skin.rs:299` → `let mut triangles = stream.allocate_vec_sized::<[u16; 3]>(num_triangles as u32)?;` (the sized variant, since `[u16;3]` has an honest 6-byte wire size). `sse_recon.rs:134` → `Vec::with_capacity(partition.partitions.iter().map(|p| p.triangles.len()).sum::<usize>() * 3)` (in-memory count, no file-driven bound needed). Separately, extend the import bound file proposed in Finding 02 with a fixture carrying one `NiSkinData` + one `NiSkinPartition` (2 bones, 8 verts, 4 tris) so the family gets a CI floor at all.

---

### PERF-D9-2026-08-30-04: `between_frames` is the only `CpuFrameTimings` field the console `cpu_ms:` line omits — the remainder bucket is invisible to the headless triage surface
- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost
- **Location**: `byroredux/src/systems/debug.rs:103-124` (`cpu_breakdown`), vs `byroredux/src/systems/metrics.rs:205-226` (the overlay, which does include it)
- **Status**: NEW
- **Description**: `cpu_breakdown` prints thirteen fields —
  `fence_wait acquire submit_present ssbo_build geom_rebuild tlas_build
  cmd_record rof_pre_draw rof_draw_call rof_post_draw atw_pre atw_scheduler
  atw_post` — and omits `between_frames_ms`. That is the one field that is not
  nested inside another printed bucket, i.e. the only one that can expose the
  time the process spends outside `about_to_wait` (compositor throttling,
  Wayland frame-callback wait, event-loop sleep). The egui Metrics panel does
  surface it (`metrics.rs:213`), so the omission is specific to the *console*
  line — which is the surface a `byro-dbg` / `--bench-hold` / headless-log
  operator has, and the one the SLOW-FRAME warning uses.
- **Evidence**:
  ```rust
  // debug.rs:104-124 — the format string, verbatim field list
  "fence_wait={:.0} acquire={:.0} submit_present={:.0} ssbo_build={:.0} \
   geom_rebuild={:.0} tlas_build={:.0} cmd_record={:.0} rof_pre_draw={:.0} \
   rof_draw_call={:.0} rof_post_draw={:.0} atw_pre={:.0} atw_scheduler={:.0} \
   atw_post={:.0}"
  ```
  `metrics.rs:213`: `cpu_pass_ms.insert("between_frames".to_string(), cpu.between_frames_ms);`
  — the field is live and produced, just not printed here.
- **Impact**: `cpu_breakdown`'s own doc (`debug.rs:96-102`) frames the line as
  "the decisive localizer for a multi-second frame whose GPU passes are cheap"
  and enumerates the conclusions it supports — but the "compositor / OS /
  outside-the-engine" conclusion has no bucket on the line to support it. On a
  hitch the operator can localize to `fence_wait` / `atw_post` / `acquire` but
  cannot rule the frame *out* of the engine. Diagnostic-only; LOW. Fixing this
  without finding 01 first would print a number that means the wrong thing.
- **Related**: `PERF-D9-2026-08-30-01` (fix that first); #2183 (the same class of
  omission on the GPU line, for `upscale`/`presentation` — closed).
- **Suggested Fix**: Add `between_frames={:.0}` to `cpu_breakdown`'s format
  string, after finding 01 lands. Consider a one-line note in the doc comment
  that the buckets nest (`atw_post ⊇ rof_* ⊇ …`) so the line is not summed.

---

### PERF-D9-2026-08-30-05: four declared per-frame renderer scratches are absent from `fill_scratch_telemetry`, against that function's own stated maintenance rule
- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:2090-2206` (the producer, 13 rows); omitted fields at `context/mod.rs:1809`, `acceleration/mod.rs:157`, `acceleration/mod.rs:166`, `water.rs:268`
- **Status**: NEW
- **Description**: `fill_scratch_telemetry`'s doc states the rule explicitly:
  *"every persistent `Vec` scratch declared in this crate must show up here.
  Adding a new scratch field on `VulkanContext` (or its sub-managers) without a
  row added below reintroduces the pre-R6 blind spot where scratches grow with
  zero observability."* Four declared scratches are missing.
  `blend_seen_scratch` is the clearest violation — it is `pub`-documented as
  *"Per-frame scratch … Cleared at the top of the walk; capacity persists across
  frames"* and is cleared at `draw.rs:3436` every frame. It is an `FxHashSet`
  rather than a `Vec`, but the function already emits rows for three hash
  containers (`skin_dispatch_seen_scratch`, `previous_rigid_models`,
  `current_rigid_models_scratch`), so the container type is not the reason.
  `tlas_addresses_scratch` and `tlas_missing_samples_scratch` sit on
  `AccelerationManager` next to `tlas_instances_scratch`, which **is** reported
  via `tlas_instances_scratch_telemetry()`. `WaterPipeline::param_scratch` is a
  per-frame packing buffer in a sub-manager the function never reaches.
- **Evidence**: `rows.push(` appears 13 times in `context/mod.rs`; none names
  any of the four. Declarations:
  ```rust
  // context/mod.rs:1809
  blend_seen_scratch: FxHashSet<(u8, u8, bool, bool)>,
  // acceleration/mod.rs:157
  pub(super) tlas_addresses_scratch: Vec<u64>,
  // acceleration/mod.rs:166
  pub(super) tlas_missing_samples_scratch: Vec<String>,
  // water.rs:268
  param_scratch: Vec<GpuWaterParams>,
  ```
- **Impact**: Small in bytes — `blend_seen_scratch`'s key domain is four engine-
  derived material bits; `tlas_addresses_scratch` is documented as ~64 KB at the
  8k-instance ceiling; `tlas_missing_samples_scratch` is capped at
  `MISSING_BLAS_SAMPLE_LIMIT = 5`. The real cost is the rule itself: an
  observability invariant that is 4/17 violated stops being a guard, and the
  next scratch added has no reason to be added here either. LOW.
- **Related**: #2042 (the same producer's row count drifting out of its doc —
  closed by making the doc defer to this function); #2486 (the shrink half of
  the same cluster policy); #3061 / dim_2 (which touch `blend_seen_scratch` for
  its *hasher*, not its telemetry — different defect, not a dup).
- **Suggested Fix**: Add four `rows.push(ScratchRow { … })` entries, routing the
  two `AccelerationManager` fields through an accessor beside the existing
  `tlas_instances_scratch_telemetry()` and adding one on `WaterPipeline`.

---

### PERF-D9-2026-08-30-06: `ScratchTelemetry` covers zero of the seven engine-binary per-frame scratches, including `draw_commands` — the largest per-frame Vec in the process
- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost
- **Location**: `byroredux/src/main.rs:248-285` (declarations), `byroredux/src/app_frame.rs:150-160` (per-frame use), consumer `byroredux/src/commands/world_info.rs:165-171`
- **Status**: NEW
- **Description**: `ScratchTelemetry` is populated exclusively by the renderer
  (`ctx.fill_scratch_telemetry(&mut tlm.rows)`); the engine binary contributes
  only the three material counters (`app_frame.rs:172-176`). Its seven own
  per-frame scratches — `draw_commands`, `water_commands`, `gpu_lights`,
  `gpu_fog_volumes`, `light_sort_scratch`, `bone_world`, `skin_offsets` — are
  all `App` fields handed to `build_render_data` as `&mut`, cleared on entry and
  refilled every frame with capacity retained, and none appears in any row.
  `draw_commands` is the input the renderer's own tracked scratches are all
  sized *from*: `gpu_instances_scratch`, `previous_models_scratch`,
  `batches_scratch` and both rigid maps are each `reserve(draw_commands.len())`.
  So the quantity that drives five reported rows is itself unreported.
- **Evidence**:
  ```rust
  // main.rs:248-285 (declarations), :572-591 (all Vec::new() / FxHashMap::default())
  draw_commands: Vec<DrawCommand>,
  water_commands: Vec<…::WaterDrawCommand>,
  gpu_lights: Vec<…::GpuLight>,
  gpu_fog_volumes: Vec<…::GpuFogVolume>,
  light_sort_scratch: Vec<(f32, …::GpuLight)>,
  bone_world: Vec<[[f32; 4]; 4]>,
  skin_offsets: rustc_hash::FxHashMap<EntityId, u32>,
  ```
  `byroredux/src/render/mod.rs:630-632` names the same set as caller-owned
  scratch: *"All scratch buffers — `draw_commands`, `gpu_lights`,
  `gpu_fog_volumes`, `light_sort_scratch`, `bone_world`, `skin_offsets` — are
  owned by the caller and cleared on entry."*
- **Impact**: The scratch cluster with the highest per-element count in the
  frame has no capacity-vs-used or wasted-bytes visibility, and neither
  `ctx.scratch` nor its "renderer not initialized yet" message tells the reader
  the report is renderer-only. Two sibling dimensions already hit this blind
  spot from the other side: `dim_1.md:100` notes that "neither `ScratchTelemetry`
  nor the `cpu_ms:` breakdown brackets `build_render_data`'s water tail", and
  `dim_1.md:75` notes no instrument covers the animation path. LOW — this is a
  visibility gap, not a defect in the scratches themselves (all seven are
  correctly reused; none reallocates per frame the way finding 02's does).
- **Related**: `PERF-D9-2026-08-30-05` (the renderer-side half of the same
  coverage question); `PERF-D9-2026-08-30-02` (the over-reserve `draw_commands`
  drives); #780 / #1066 / #2711 (the material-counter half of this resource).
- **Suggested Fix**: Push seven `ScratchRow`s from `app_frame.rs` alongside the
  existing material-counter block, and have `ctx.scratch` label the two groups
  (`renderer:` / `engine:`) so the report's scope is legible.
---

## Prioritized Fix Order

Quick wins (scratch reuse, preallocation, gate restoration, one-line guards)
before architectural changes. Nothing here requires a Vulkan render-pass,
barrier or pipeline-state restructure; the two findings that touch GPU
scheduling (`PERF-D5-…-01`, `-02`) are gate/skip changes with existing
telemetry to verify them, not restructures.

### Tier 1 — one change, disproportionate payoff

1. **Quantise the depth tiebreaker in `draw_sort_key`** into buckets so the
   per-frame draw order is stable under camera motion. Fixes
   `PERF-D4-2026-08-30-01` (instance/previous-model/indirect upload dirty
   gates start hitting) and `PERF-D5-2026-08-30-01` (TLAS returns to
   UPDATE-mode refit in steady state) with one edit. Verify with the existing
   `tlas_build_ms` timer and the instance-hash dirty counters; add a
   build-vs-update counter, which does not exist today.
2. **Close the skinning telemetry gap** (`PERF-D9-2026-08-30-03`): move or add
   a `gpu_timers` bracket so `skin_palette.comp` and the two staging copies
   are inside one. Prerequisite for sizing item 4.
3. **Fix `between_frames_ms`'s sample point** (`PERF-D9-2026-08-30-01`) — a
   one-line move out of the post-`draw_frame` `Ok` arm — and print it on the
   `cpu_ms:` line (`-04`). Without these the engine's primary triage surface
   attributes in-engine render cost to "outside the engine".

### Tier 2 — cheap, local, measurable

4. **Gate the bone-world staging copy on `pose_dirty`**
   (`PERF-D4-2026-08-30-03`). `pose_dirty` already crosses the crate boundary
   and is consumed per entity by the BLAS refit; the bone copy is the one
   consumer ignoring it.
5. **Align `batches_scratch`'s `reserve()` with its shrink hysteresis**
   (`PERF-D9-2026-08-30-02`) — reserve against the batch high-water, not
   `draw_commands.len()`.
6. **Skip `copy_depth_to_history` when no draw carries
   `MAT_FLAG_EFFECT_SOFT`** (`PERF-D5-2026-08-30-02`); the skip is
   layout-neutral. Add its bracket at the same time.
7. **Latch the volumetrics gate-off clear** (`PERF-D5-2026-08-30-05`) — the
   caustic pass 200 lines away already has exactly that latch to copy.
8. **Early-out `reemit_water_planes` on an empty water query**
   (`PERF-D1-2026-08-30-02`, one line, `QueryRead::is_empty()` exists) and
   cache the REGN ambient resolution (`PERF-D1-2026-08-30-03`).
9. **Reuse `MorphSlot`'s existing right-sized `pending_weights` buffer**
   (`PERF-D6-2026-08-30-02`) via `clear()+extend()`, and stop marking the slot
   dirty unconditionally so `flush_pending_weights`' early-out can fire.
10. **Stagger `SKINNED_BLAS_REFIT_THRESHOLD`** (`PERF-D6-2026-08-30-01`) with
    a per-entity jitter term or a per-frame rebuild cap, in one pure
    predicate — otherwise a cell's whole NPC cohort rebuilds in lockstep
    roughly every 10 s.
11. **Move the lock tracker's `held_others` materialisation after the
    `ENABLED` check** (`PERF-D1-2026-08-30-04`) — coordinate with the
    concurrent ECS audit's `ECS-D3-01`, same lines.
12. **Convert the four residual std-hash hot-path sites** (`PERF-D1-…-01`,
    `PERF-D2-…-03`), and widen `#3061`'s source-scan guard corpus so
    `texture_registry.rs` and the animation path stop being invisible to it.

### Tier 3 — ledger and guard hygiene (cheap, and these are the premises the *next* audit will trust)

13. Correct the three drifted `memory-budget.md` rows (`PERF-D5-…-03`,
    `PERF-D3-…-03`) and the four rotted comments (`PERF-D3-…-04`,
    `PERF-D5-…-04`, `-06`, `-07`, `PERF-D6-…-03`, `PERF-D2-…-02`).
14. Add the `bench_draws_raster_cmds` metric (`PERF-D2-2026-08-30-01`) so the
    parallel-sort gate becomes falsifiable; regenerate the five baselines.
15. Add `CameraUBO`'s field-order lockstep test (`PERF-D4-…-04`) — the only
    hand-duplicated GPU struct pinned by size alone.
16. Register the untracked scratch in `fill_scratch_telemetry`
    (`PERF-D9-…-05`, `-06`).
17. Re-sample `log_memory_usage` (`PERF-D3-2026-08-30-02`) from somewhere that
    executes after cells load — today the 80 % DEVICE_LOCAL warning has one
    caller, at engine init, and can never fire.

### Tier 4 — structural, schedule deliberately

18. **Interior cell load still spawns every REFR + NPC in one frame**
    (`PERF-D7-2026-08-30-02`). This supersedes `#1798`, which was closed
    measurement-only on the rationale that a resumable cursor was too large a
    change — that premise is now false: `ReferenceLoadJob` exists and two
    exterior job types already drive it under `STREAMING_APPLY_BUDGET`. The
    interior path calls the same function through a `FrameTimeBudget::
    unlimited()` wrapper.
19. **Batch-local NIF memo in the streaming worker**
    (`PERF-D7-2026-08-30-01`): a model shared by K cells in one crossing is
    extracted and parsed K times, and `finish_partial_import`'s `#864`
    early-out then discards every duplicate unread.
20. **Move `import_nif_with_collision_and_resolver` off the main thread**
    (`PERF-D8-2026-08-30-01`). Measured on three real archives, this is
    **73–81 % of per-NIF CPU** sitting on one core of a 16-core part, which
    by this project's own hardware rule is a bug. The sole main-thread
    dependency is `pool.intern(texture_path)`; a worker-local `StringPool`
    with ≤22 re-interned `MaterialTextureSet` slots at the drain removes it.
21. **Extend the dhat gate to the import tier**
    (`PERF-D8-2026-08-30-02`): a third bound file
    *crates/nif/tests/heap_allocation_bounds_import.rs*, registered in the
    existing `nif-heap-allocation-bounds` CI job. Measured peak live heap
    after parse+import is 2.0–2.3× parse-only. Add the skinning path's two
    unreserved growth sites at the same time (`PERF-D8-2026-08-30-03`).
22. **Cap and de-duplicate `MorphSlot::delta_buffer`**
    (`PERF-D3-2026-08-30-01`): mesh-static data currently allocated per
    placed entity with no residency cap, no telemetry and no ledger row,
    while the mesh itself is correctly deduped by `MeshRegistry::
    acquire_cached`.
23. **In-place retain in `PackedStorage::remove_entities_erased`**
    (`PERF-D7-2026-08-30-04`) and hoist the cinematic-retention set out of
    `unload_cells`' per-victim loop (`-05`).

---

## Appendix — dimensions that produced no findings

**None.** All nine dimensions produced at least one finding. The thinnest
were Dimension 2 (Draw & Instancing, 3) and Dimensions 6 and 8 (3 each);
Dimension 5 (GPU Pipeline) produced the most at 7. Dimension 2's own verdict
is worth recording as a positive result: sort-key ordering, per-draw dynamic
state (`cmd_set_depth_bias`, depth test/write/compare-op, `cmd_set_cull_mode`
are all `!=`-gated), descriptor-set and vertex/index binding (once per frame)
and push-constant churn (none in the batch loop) were all checked and are
clean — its three findings are about *measurability* and residual hashing,
not about batching waste.
