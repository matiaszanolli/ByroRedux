# Performance Audit — ByroRedux — 2026-06-14

**Command**: `/audit-performance` (all 9 dimensions, `--depth deep`) — part of an
`/audit-suite --preset comprehensive` sweep.
**Method**: 9 dimension agents (renderer / ECS / general specialists) + orchestrator
verification. Read-only, hot-path-traced with exact line citations. Every finding's
premise was re-read against current code and an attempt made to disprove it before
inclusion. Cross-dimension duplicates merged.
**Dedup baseline**: 23 open GitHub issues (`/tmp` snapshot 2026-06-14); prior
`AUDIT_PERFORMANCE_*` reports through 2026-06-11.
**Bench-of-record**: ROADMAP carries the stale `R6a-*` block (flagged non-gating in
ROADMAP itself) — no absolute FPS numbers are copied here. No live bench was run this
pass (no on-disk-data + Vulkan-device harness in scope); findings are static + GPU-timer
reasoning, severities are deltas vs the verified-intact baseline.

---

## Executive Summary

| Severity | Count (deduplicated, new/actionable) |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 5 |
| LOW | 7 |
| **TOTAL** | **13** |

**The engine remains in strong shape — every Session-46/47 must-not-regress guard
verified intact** (full ledger at the bottom). The single new HIGH is a *rendering
correctness* defect that rides on the draw-grouping perf optimization, not a CPU/GPU
cost regression: the indirect draw-group merge collapses batches that differ in opaque
`two_sided` and in depth state, applying the group leader's cull mode / depth state to
the whole merged span. It is invisible to `cargo test` (needs Vulkan + real cell
content) and explains back-face-culled two-sided opaque cutouts (fences/grates/foliage)
and depth-state bleed within a render layer.

**One prior finding closed since 2026-06-11**: PERF2-02 (BLAS build-scratch only shrank
on window resize) is **FIXED** — `shrink_blas_scratch_to_fit` is now also called at
cell-unload (`byroredux/src/cell_loader/unload.rs:134`).

**Two carry-overs re-affirmed (still no GitHub issue filed)**: the caustic 5×5 Gaussian
per-light loop-invariant `exp()` recomputation (prior F1) and the write-only ReSTIR-DI
reservoir attachment VRAM (prior F2/N1). Both should be filed so they stop being
re-discovered each sweep.

**Two new FO4-streaming MEDIUMs**: the `<Plugin> - Geometry.csg` archive is re-opened +
re-parsed per cell (the Starfield CDB analog is correctly `Arc`-cached, the CSG is not),
and the steady-state payload drain has no per-frame cell budget (the #877 two-phase
*pre-parse* moved parse off-thread, but the *spawn* phase — BLAS build + uploads — runs
unbounded on the main thread).

The remaining LOWs are an O(capacity) host flush that is a no-op on the dev NVIDIA card
(real only on non-coherent memory), four missed `read_pod_vec` bulk-read adoptions on
import paths, and a defensive snap-formula duplication.

---

## Hot-Path Analysis (per-frame)

| Per-frame operation | Cost / scaling | Status |
|---|---|---|
| Indirect draw-group merge | groups by `(pipeline_key, render_layer)` only; spans `two_sided`/depth-state boundaries | **F1 (HIGH) — wrong state on merged batches** |
| `about_to_wait` mesh/tex in-use sets | 2 fresh `HashSet<u32>` + 2 O(entity) walks/frame, ungated | **F5 (MEDIUM)** |
| Caustic 5×5 Gaussian splat weights | N_LIGHTS × 50 `exp()` per caustic pixel, all loop-invariant | **F2 (MEDIUM, re-affirm F1)** |
| ReSTIR-DI reservoir write | 16 B/px × 2 FIF (~66 MB@1080p / ~265 MB@4K), write-only | **F3 (MEDIUM, re-affirm F2/N1)** |
| FO4 CSG archive open + chunk-table parse | per precombine cell-load (no `Arc` cache) | **F6 (MEDIUM)** |
| Cell-spawn payload drain | unbounded `loop{try_recv}` → sync BLAS build + uploads | **F7 (MEDIUM)** |
| Per-instance camera-relative rebase | O(instances), 3 subs each, inline in existing loop | clean (verified) |
| Skin dispatch / BLAS refit | `pose_dirty` + first-sight gated; skipped when pose unchanged | clean (#1195/#1196 intact) |
| Material / instance SSBO upload | O(unique)/O(live), content-hash dirty-gated | clean (#878/#1134/#1368) |
| Direct-copy SSBO *flush* | flushes full allocation, not written range | **F8 (LOW, non-coherent only)** |
| TAA / SVGF / bloom / volumetrics | O(pixels)/O(froxels), single dispatch | clean |
| GPU timer readback | prior-frame results post-fence, no stall | clean |
| WRS streaming reservoir loop | O(cluster.count × 16), NUM_RESERVOIRS=16 | Existing #1369 |

---

## Findings (deduplicated, grouped by severity)

### HIGH

#### F1 — Indirect draw-group merge spans batches differing in opaque `two_sided` and depth state; leader's cull/depth wrongly applied to the whole group
- **Severity**: HIGH
- **Dimension**: Draw & Instancing
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:2549-2561` (per-batch depth-state emit), `:2587-2598,2670` (per-batch cull emit), `:2679-2701` (indirect group-merge loop), `:1880-1884` (batch split keys)
- **Status**: NEW
- **Description**: Cull mode and extended-dynamic depth state are emitted once for the
  leader `batches[i]` at the top of each outer iteration; the `use_indirect` branch then
  merges all consecutive batches sharing `batch_state = (pipeline_key, render_layer)`
  into a single `cmd_draw_indexed_indirect` and advances `i = end`. The merge predicate
  (`:2681-2685`) excludes **only** `Blended && two_sided`. It does NOT exclude (a) opaque
  `two_sided` batches — `two_sided` is dynamic `cmd_set_cull_mode`, not a `pipeline_key`
  axis (#930), with `default_cull = NONE` only when `two_sided` (`:2587`), computed once
  from the leader; or (b) batches with differing `z_test`/`z_write`/`z_function` — these
  are real batch-split keys (`:1882-1884`) but not part of `batch_state`.
- **Evidence**: The opaque sort key is `(rt_only, 0, render_layer, two_sided, 0, 0,
  pack_depth_state, mesh, sort_depth, entity)` (`render/mod.rs:206-216`). `two_sided`
  (slot 3) and `pack_depth_state` (slot 6) sort *before* mesh, so within one
  `(pipeline_key, render_layer)` run the `two_sided=false`→`true` batches and the
  differing-depth-state batches are **adjacent** and get merged. `two_sided` is set on
  opaque static meshes from the `TwoSided` marker (`static_meshes.rs:189,557`); depth
  state from the material (`static_meshes.rs:336-337,660-662`). The blend path was
  guarded; the opaque-two_sided and depth-state axes were not (regression of the #1258
  grouping vs the #930 dynamic-cull collapse).
- **Impact**: Visible rendering defect: two-sided opaque cutout geometry (fences, grates,
  foliage cards, railings) loses its back faces when grouped behind a single-sided
  leader (`CULL_BACK` applied where `CULL_NONE` is required); opaque batches authored
  `z_write=0` (glow halos / sky-like) z-fight or write depth wrongly when grouped with a
  `z_write=1` leader, or vice-versa. Invisible to `cargo test` (Vulkan + cell content).
- **Related**: #1258 (post-merge batch telemetry), #930 (dynamic two-sided cull), #398 (dynamic depth state).
- **Suggested Fix**: Extend the merge stop-condition at `:2681` to also break on a change
  in `two_sided`, `z_test`, `z_write`, or `z_function`. Cheapest: compare a
  `group_state(b) = (b.pipeline_key, b.render_layer, b.two_sided, b.z_test, b.z_write,
  b.z_function)` instead of `batch_state`. The sort already clusters identical state, so
  this only fragments groups at genuine state boundaries — no instancing loss within a
  state-homogeneous run. Confidence: HIGH (data-flow + sort-order verified; render-output
  failure mode needs RenderDoc/smoke to *see*, but the state-application logic is exact).

### MEDIUM

#### F2 — Caustic 5×5 Gaussian splat weights recomputed per-light (loop-invariant `exp()`)
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/caustic_splat.comp:408-411` (wsum loop), `:416` (per-tap `exp`), inside the per-light loop `for li` opened at `:258`
- **Status**: RE-AFFIRMED (prior F1/PERF1-01, 2026-06-11) — still present, unfixed, **no GitHub issue filed**
- **Description**: For every light that produces a screen-projected splat, the shader
  recomputes the 5×5 Gaussian normalization `wsum` (25 `exp()`) and the per-tap weight
  `exp(...)/wsum` (25 more). All 50 `exp()` per light depend only on the fixed kernel
  offsets and constant σ=1 — fully loop-invariant.
- **Impact**: Up to N_LIGHTS × 50 transcendentals per glass/caustic pixel where 25
  computed once suffices. Bounded to glass-source pixels (O(pixels)) but pure waste in a
  transcendental-heavy compute pass on glass-heavy interiors with multiple caustic lights.
- **Suggested Fix**: Compute `wsum` once before the `for li` loop (it is a compile-time
  constant); precompute a `const float kGauss5[25]` of pre-normalized weights indexed
  `(ky+2)*5+(kx+2)`. Recompile with plain `glslangValidator -V`. **File a GitHub issue.**

#### F3 — Write-only ReSTIR-DI reservoir attachment (16 B/px × 2 FIF) — dead VRAM + write bandwidth
- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/gbuffer.rs:60-63` (format), `:231/:281/:444` (allocate); writer `crates/renderer/shaders/triangle.frag:58,3473-3479`
- **Status**: RE-AFFIRMED carry-over of F2/PERF2-03 (2026-06-11) and N1 (2026-06-04) — premise re-verified true, **no GitHub issue filed**
- **Description**: The 7th G-buffer attachment `gb_reservoir` (`R32G32B32A32_UINT`) is
  written every fragment but never read — the ReSTIR-DI resample compute pass that would
  consume it does not exist (`triangle.frag:3078-3080` states it is a separate milestone).
  `grep` across `draw.rs`/`svgf.rs`/`composite.rs` finds no reservoir read binding.
- **Impact**: ~66 MB @1080p, ~265 MB @4K of resident VRAM plus per-frame ROP write
  bandwidth for data with no consumer. Works against the < 4 GB VRAM target at 1440p+.
- **Suggested Fix**: Gate the attachment behind a feature flag (drop from framebuffer +
  render pass when off) until the resample pass lands, OR land the resample pass. If kept,
  consider `R32G32_UINT` (8 B/px) packing. **File a GitHub issue** so it stops being re-found.

#### F5 — `about_to_wait` allocates two fresh `HashSet<u32>` every frame for `meshes_in_use` / `textures_in_use`
- **Severity**: MEDIUM
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/main.rs:2093-2111`
- **Status**: NEW
- **Description**: The per-frame `about_to_wait` pre-scheduler phase builds two brand-new
  `HashSet<u32>` and does two full O(entity_count) component walks (`MeshHandle`,
  `TextureHandle`) to compute counts surfaced only via the `stats` command / window title
  / debug-UI panel. The block runs **unconditionally** — outside the `config_debug` gate
  (which starts at `:2186`) and with no throttle. The `CpuFrameTimings::atw_pre_ms`
  doc-comment (`crates/core/src/ecs/resources.rs:566-572`) already flags this walk as a
  growth risk.
- **Impact**: Two heap allocations + two rehash-growth sequences + two full component
  walks per frame, scaling with cell entity count; pure waste when nothing reads the
  result. Not frame-dominating on a 7950X but defeats the "zero steady-state allocations"
  posture of the rest of the hot path. No quantitative guard exists (#1381).
- **Suggested Fix**: (a) Gate the block on a live consumer (`config_debug` OR
  `debug_ui.visible` OR the existing 16-frame window-title throttle), and/or (b) hoist the
  two `HashSet`s to persistent `App` fields and `clear()`+reuse. Option (a) is strictly
  better — the walk itself is wasted when nothing reads it.

#### F6 — FO4 `<Plugin> - Geometry.csg` re-opened + re-parsed on every precombine cell-load (Starfield CDB analog is `Arc`-cached, CSG is not)
- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/precombined.rs:82` (`open_geometry_csg` inside `spawn_precombined_meshes`), reached per cell from `cell_loader/exterior.rs:342` (exterior) and `cell_loader/load.rs:236` (interior); `CsgArchive::open` body `crates/bsa/src/csg.rs:103-153`
- **Status**: NEW
- **Description**: The Starfield CDB is parsed once and held behind
  `Arc<ComponentDatabaseFile>` (`asset_provider.rs:680,733`), an O(1) `is_some()` check
  per material. Its FO4 analog is not cached: `spawn_precombined_meshes` calls
  `open_geometry_csg(plugin_path)` on every cell with precombines (the comment even reads
  "open the shared-geometry CSG once per cell load"). Each call re-opens the file,
  re-reads + parses the chunk table (a ~240 MB vanilla blob has ~3700 chunks ⇒ ~30 KB
  read + ~3700-entry `Vec<ChunkEntry>`), and constructs a fresh `ChunkCache` — whose
  whole purpose is to amortise zlib inflate *within* a load. Dropping the archive at
  function end discards the warm cache, so adjacent tiles sharing PSG regions re-inflate
  the same 64 KiB chunks.
- **Impact**: FO4 exterior streaming only — precisely the title where precombines are
  100% of vanilla architecture. Per-cell main-thread file open + chunk-table parse + loss
  of all inter-cell zlib-chunk reuse. Compounds F7 (runs inside the unbounded drain).
- **Related**: #1446 (CSG doc-rot only — unrelated).
- **Suggested Fix**: Hold `Option<Arc<CsgArchive>>` on `MaterialProvider` (or a small
  `CsgProvider` keyed by plugin stem, mirroring `sf_cdb`), resolve lazily on first
  precombine cell, pass the `Arc` into `spawn_precombined_meshes`. `CsgArchive` is already
  `Send`/`Sync`-friendly (inner `Mutex<File>` + `Mutex<ChunkCache>`).

#### F7 — Steady-state cell-spawn payload drain is unbounded per frame and runs all main-thread GPU work (BLAS build + uploads) synchronously
- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/main.rs:1071-1084` (`step_streaming` drain loop) → `streaming_helpers.rs:117-126` (`consume_streaming_payload`) → `cell_loader/exterior.rs:319-342` (`load_one_exterior_cell`)
- **Status**: NEW
- **Description**: The #877 two-phase pre-parse correctly moves NIF *parse* off-thread,
  but the drain that consumes those payloads is an uncapped `loop { try_recv() }` and each
  iteration runs `consume_streaming_payload` → `load_one_exterior_cell`, which
  synchronously spawns the terrain mesh, submits a batched BLAS build
  (`ctx.build_blas_batched`, `exterior.rs:320`), spawns water, decodes + spawns
  precombines (F6), and uploads vertex/index buffers. No per-frame cell budget: if N
  worker payloads are ready at frame start, all N cells spawn (with all GPU work) before
  the frame proceeds. The *pre-parse* split is intact; the *spawn* phase has no throttle.
- **Impact**: Frame-time spike when >1 payload completes in one frame — realistic on
  fast-travel/teleport (full new batch dispatched), post-stall catch-up (worker ran
  ahead), or larger `radius_load`. Steady-state at `radius_load=1` is mild (MEDIUM, not
  HIGH). On-disk-data + Vulkan-device → smoke-only, out of `cargo test`.
- **Related**: F6 (CSG open runs inside this loop); Dim 3 (BLAS batching).
- **Suggested Fix**: Cap the steady-state drain at 1–2 cells/frame and leave the rest
  queued (`break` after the cap — `try_recv` makes this trivial). Spreads spawn/upload/
  BLAS cost across frames at the price of slightly later pop-in, which the hysteresis
  radius already tolerates. Keep `stream_initial_radius`'s blocking boot path unchanged.

### LOW

#### F8 — `flush_if_needed` flushes the FULL buffer allocation, not the written byte range (O(capacity) flush on non-coherent memory)
- **Severity**: LOW (no-op on the dev RTX 4070 Ti; real only on non-coherent host-visible memory)
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/buffer.rs:629-652` (`flush_if_needed`), called by `scene_buffer/upload.rs` `upload_lights:74`, `upload_instances:514`, `upload_materials:572`, `upload_indirect_draws:619`, `upload_bone_worlds:201`
- **Status**: NEW
- **Description**: The direct-copy upload paths write only the live prefix
  (`byte_size = sizeof(T) * count`) but `flush_if_needed` flushes
  `aligned_flush_range(alloc.offset(), alloc.size())` — the entire allocation (29.4 MB for
  instances) — regardless of bytes written. The sibling `write_mapped` (`:683-711`) was
  explicitly fixed under #301 to flush only `len`, and its comment names this exact waste.
- **Impact**: ZERO on the dev GPU (NVIDIA `CpuToGpu` is HOST_COHERENT → early-return). On
  AMD/Intel/mobile non-coherent memory, `upload_lights`/`upload_indirect_draws`/
  `upload_bone_worlds` (no dirty gate) flush the full allocation every frame they run;
  instances/materials pay it on every gate miss.
- **Suggested Fix**: Have the `upload_*` callers flush via the already-existing
  `flush_range(device, 0, byte_size)` (`buffer.rs:722`) instead of `flush_if_needed`.
  No layout/shader change; pure flush-range narrowing.

#### F9 — Render-origin snap *formula* duplicated across crate boundary (const already unified)
- **Severity**: LOW (defensive only — zero runtime cost)
- **Dimension**: Telemetry & Origin Cost
- **Location**: `byroredux/src/render/camera.rs:161` + `crates/renderer/src/vulkan/context/draw.rs:632-635`
- **Status**: PARTIALLY FIXED since prior F6/PERF3-01. The `RENDER_ORIGIN_SNAP` *constant*
  is now a single source of truth (`scene_buffer/constants.rs:318`, imported by both
  sites). Only the snap *expression* `(pos / SNAP).floor() * SNAP` remains hand-written
  in two places, guarded by prose only.
- **Impact**: The two formulas MUST produce bit-identical origins or the per-instance
  rebase (draw.rs) and the uploaded relative view_proj (camera.rs) disagree, shifting
  geometry by up to 4096 units. Identical and safe today; a future one-sided edit silently
  desyncs with no compile-time guard.
- **Suggested Fix**: Extract `pub fn snap_render_origin(camera_pos: Vec3) -> Vec3` next to
  the const and call it from both sites.

#### F10–F13 — Four missed `read_pod_vec` bulk-read adoptions on import paths
- **Severity**: LOW (import-time CPU-call overhead; F13 is alloc-churn)
- **Dimension**: NIF Parse
- **Status**: NEW (all 4); same class — per-element `read_u16/f32/bytes` push loops that
  should be a single `read_pod_vec`-family bulk read (the wrapper is already used for the
  identical type elsewhere, often in the same file). None are double-allocations
  (`allocate_vec` pre-size is correct, so #831/#833 hold).
  - **F10** `crates/nif/src/blocks/bs_geometry.rs:382-390` — Starfield BSGeometry *primary*
    triangle loop; the *LOD* path at `:504` already uses `read_u16_triple_array`. Hot import path.
  - **F11** `crates/nif/src/blocks/extra_data.rs:1030-1036` — FO4 `BSPackedCombinedGeomData`
    triangle loop; the sibling `.csg` reader (`precombine.rs:139`) already bulk-reads.
  - **F12** `crates/nif/src/blocks/collision/compressed_mesh.rs:166-175` — Skyrim+ Havok
    `big_verts: Vec<[f32;4]>` loop; `read_ni_color4_array` applies (used at `shape_compound.rs:31`).
  - **F13** `crates/nif/src/blocks/legacy_particle.rs:392-398` — `read_bytes(32)` inside a
    per-particle loop allocates a throwaway 32-byte `Vec<u8>` every iteration (lifetime
    churn the peak-live dhat gates structurally cannot catch).
- **Suggested Fix**: Replace each with the matching bulk reader (F10/F11
  `read_u16_triple_array`, F12 `read_ni_color4_array`). For F13, add `[u8; 32]` to the
  `impl_any_bit_pattern!` set + `read_pod_vec::<[u8;32]>` and a **lifetime-total** dhat
  assertion. Natural follow-ons to #1381.

#### F14 — SVGF moments-history 4th RGBA16F channel unused (informational)
- **Severity**: LOW (informational carry-over of prior F5)
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/svgf.rs`
- **Status**: OPEN carry-over. ~8 B/px spare (μ₁, μ₂, history-length + unused). No universal
  3-channel 16F format exists; effectively unavoidable. Listed for ledger continuity only.

---

## Closed / Fixed Since Last Audit

- **PERF2-02 (BLAS build-scratch shrink) — FIXED.** Prior audit (2026-06-11) flagged
  `shrink_blas_scratch_to_fit` as only reachable from `resize.rs` (window resize), leaving
  up to ~200 MB pinned after a heavy cell unloaded. A second production call site now
  exists at `byroredux/src/cell_loader/unload.rs:134`, invoked right after `drop_blas` at
  cell unload (with the documented "no BLAS build in flight" safety justification). The
  pinned-scratch failure mode is closed at the correct boundary. No action required.

---

## Existing Open Issues Confirmed (not re-filed)

- **#1369** (OPEN) — WRS streaming reservoir loop `O(cluster.count × 16)`,
  `NUM_RESERVOIRS=16` hard-coded (`triangle.frag:3104`), spec-constant + loop-invariant
  hoist not yet landed. The per-thread storage cut (320 B → 128 B) HAS landed. Confirmed
  still open; not re-filed.
- **#1381** (OPEN) — dhat / alloc-counter coverage unwired for render/ECS hot paths.
  Referenced by F5 / F10–F13 as the missing quantitative guard. Not re-filed.
- **#1387** (OPEN) — skin output buffer missing `VERTEX_BUFFER` usage flag (M29.3
  deferred). Confirmed present verbatim (`skin_compute.rs:413-419`) with its tracking
  comment; deliberate, not a regression, not re-filed.
- **#1505** (OPEN) — `gpu_timers.rs` doc claims `cmd_reset_query_pool`; actual is
  host-side `reset_query_pool`. Pure doc rot, confirmed; not re-filed.
- **#1499** (OPEN) — `MaterialTable::intern` doc claims 4096 cap (actual 16384) + stale
  pre-split file refs; the dirty-gate comments also call `GpuInstance` "72 B" (actual
  112 B). Doc accuracy, folded under #1499; not a perf defect.
- **L5 (2026-06-04)** — `build_conversation_tree` O(N²) `.find()` scan
  (`crates/plugin/src/esm/records/misc/ai.rs:473,533`). STILL present but all callers
  remain test-only and it is NOT reachable from cell load. Verdict unchanged: LOW,
  test-only, off the streaming hot path — not re-raised.

---

## Baselines Verified Intact (must-not-regress)

**Dimension 1 — CPU Hot Paths**: #1371 `drain_dirty_into` (capacity-preserving, used by
transform-propagation + bounds; `take_dirty` on no hot path; guard test present); #1372
animation scratch (all scratch declared once per call, reused via `clear()` — the
2026-06-04 L2/L3 player/stack-Vec findings are resolved); #1374 billboard `last_cam` gate;
#1376 debug-UI snapshot gated on visible; #1379 `SkinSlotPool::sweep` `next_slot`
contraction. The 2026-06-04 #1380 `light_anim` Vec + dead-lock finding is FIXED (file
rewritten — single `query_2_mut`, in-place intensity write, no `Vec<LightUpdate>`, no dead
Transform lock).

**Dimension 2 — Draw & Instancing**: GT-presence hoist (#1377); `par_sort_unstable_by_key`
≥2000-command gate (#934); M31 instanced merge with SSBO-contiguity guard; zero per-draw
allocations; descriptor sets bound once per pass; R1/NIFAL no per-draw `classify_pbr_keyword`.

**Dimension 3 — GPU Memory**: BLAS budget DYNAMIC (`device_local/3`, 256 MB floor — NOT
static 1 GB); smooth oldest-`last_used_frame` LRU eviction with `MIN_IDLE_FRAMES` guard +
break-on-budget, no thrash; mid-batch eviction at 90% every 64 builds; all TLAS/BLAS
shrinks respect `WORKING_SET_FLOOR`=8192 and run post-fence-wait; MeshRegistry caps
warn/`bail!`, no silent overrun; BGSM/BGEM #1430 half-eviction in VecDeque lockstep;
NifImportRegistry 2048-entry true-LRU; deferred-destroy frees only after
`MAX_FRAMES_IN_FLIGHT`=2 (checks `==0` before decrement — no early free of in-flight memory).

**Dimension 4 — SSBO Upload**: copy side O(live) everywhere; both dirty gates present
(FxHash over clamped live prefix, stamped post-flush); material dedup O(unique), intern
O(1) amortized; GpuInstance 112 B (3 pin tests); NIFAL/per-draw PBR plain `f32`
resolve-once; GpuCamera 336 B pin.

**Dimension 5 — GPU Pipeline**: TLAS build→ray-query barrier correct
(`AS_WRITE → FRAGMENT|COMPUTE / AS_READ`); refit-vs-rebuild intact; skinned-BLAS
COMPUTE→AS_BUILD barriers ordered; volumetrics O(froxels) (and gated off); bloom
O(pixels); SVGF/TAA/SSAO single dispatch; cluster_cull O(lights); `inv(viewProj)`
computed once on CPU, read from UBO (only shader `inverse()` is a 3×3 normal matrix);
Disney BSDF lobes gated; G-buffer format-minimal.

**Dimension 6 — Skinning & BLAS**: ALL 8 guards traced to live code and intact — M29.5
palette compute pass, M29.6 persistent bind-inverses (O(first-sight) upload), #1195
dispatch-dirty gate (`dispatches_total` single write site, first-sight invariant honored),
#1196 three-conjunct refit gate (`FAST_BUILD` not `FAST_TRACE`, 600-frame threshold),
#1197 descriptor-rewrite skip, #1194 instrumentation, SkinSlotPool LRU + capacity-stale +
scratch padding, hot-path allocation discipline (pooled `mem::take`).

**Dimension 7 — Streaming**: #877 two-phase split distinct; `PRE_PARSE_RAYON_MIN=8`
fast-path + shared `parse_one_nif`; cross-cell `NifImportRegistry` + key snapshot (#862);
CDB-once-as-`Arc` O(1); shutdown join without leak (#1169); generation gating; hysteresis.

**Dimension 8 — NIF Parse**: #830 rayon pre-parse, #831 `allocate_vec` `#[must_use]`
(no discarded results), #832 `get_mut`/`insert` per-block counters, #833 `read_pod_vec`
(grown to 11 wrappers, BE compile gate, no bytemuck dep); both dhat alloc-bound gates GREEN;
import-side particle walk is one-time (no per-frame re-walk). The 4 findings are missed
*adoptions* of the intact helper, not erosions of it.

**Dimension 9 — Telemetry & Origin Cost**: GPU timer readback uses prior-frame results
post-fence (no stall, double-gated host reset); CPU scratch telemetry reuses its `rows`
buffer + reports real len-vs-capacity; camera-relative rebase is 3 inline subtractions in
the existing loop (NOT a second pass); TAA/SVGF history preserved across grid crossings
via `origin_corrected_prev_view_proj` (#1489, verbatim hot path when ΔO==0).

---

## Prioritized Fix Order

**Highest impact (correctness, rides on perf opt):**
1. **F1** — Extend the indirect group-merge stop-condition to break on `two_sided` /
   depth-state changes (`group_state` tuple). Fixes back-face-culled two-sided opaque
   cutouts + depth-state bleed. RenderDoc/smoke to confirm visually; logic fix is exact.

**Quick wins (file issues + small edits):**
2. **F2** — const-fold the caustic 5×5 Gaussian weights (shader-only, recompile). File issue.
3. **F5** — gate the `about_to_wait` mesh/tex-in-use walk on a live consumer (one conditional).
4. **F8** — swap `flush_if_needed` → `flush_range(0, byte_size)` on the direct-copy uploads.
5. **F9** — extract `snap_render_origin` helper (removes the prose-only cross-crate contract).
6. **F10–F13** — adopt the bulk readers; add the F13 lifetime-total dhat bound.

**Decision needed:**
7. **F3** — feature-gate the ReSTIR reservoir attachment until the resample pass lands, or
   shrink to `R32G32_UINT`. File issue so it stops being re-discovered.

**Streaming (medium effort):**
8. **F6** — cache the FO4 CSG as `Arc<CsgArchive>` on the provider (mirror `sf_cdb`).
9. **F7** — cap the steady-state payload drain at 1–2 cells/frame.

**Process:**
10. **#1381** — wire dhat for `about_to_wait` / animation hot paths to regression-lock F5
    and the F10–F13 alloc bounds. Recurring gap.
