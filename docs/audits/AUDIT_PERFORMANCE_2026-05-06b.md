---
date: 2026-05-06
audit: performance
focus: dimensions 7 (TAA & GPU Skinning), 9 (World Streaming & Cell Transitions)
depth: deep
---

# Performance Audit — 2026-05-06b

Companion audit to `AUDIT_PERFORMANCE_2026-05-06.md` (which covered dims 5 + 8). This run covers:

- **Dim 7** — TAA compute, skin compute, BLAS refit, M29.3 raster path
- **Dim 9** — cell load / stream / unload; pre_parse_cell; REFR + NPC spawn; BLAS LRU; texture upload budget

## Executive Summary

| Severity | Count | Files Touched |
|---|---|---|
| CRITICAL | 1 | cell_loader.rs (REFR placement GPU upload) |
| HIGH | 2 | npc_spawn.rs, texture.rs |
| MEDIUM | 4 | streaming.rs, cell_loader.rs (×2), streaming.rs (BSA mutex) |
| LOW | 3 | cell_loader_refr.rs, cell_loader.rs, draw.rs (BLAS barrier flag) |
| INFO | 5 | TAA history (×2), skin descriptor rewrite, jitter, BLAS LRU verified |
| **Total** | **15** | |

**Headline (Dim 9)**: The cell-load critical path has a real CRITICAL gap — every REFR placement re-uploads a fresh `Vec<Vertex>` + `Vec<u32>` index buffer to GPU even when the placement shares a cached `ImportedMesh`. Megaton's 929 REFRs accumulate ~2× num_meshes synchronous fence-waits per cell. The Arc<CachedNifImport> cache (#381) saves only the *parse* work; GPU buffers are not deduplicated.

**Headline (Dim 7)**: TAA + GPU skinning are clean. All 6 architecture-snapshot items pass: TAA dispatch is O(pixels) once per frame, skin compute is gated on bone presence + per-entity dedup'd, BLAS refit dominates rebuild (with a 600-frame threshold for chain-degradation), bone palette is single-buffer, jitter is CPU-side. Only M29.3 raster path (read pre-skin SSBO from triangle.vert) hasn't shipped — deliberate deferral.

**Estimated FPS impact**: Dim 7 findings are zero/marginal at runtime. Dim 9's CELL-PERF-01/02/03 trio drives the per-cell-transition stall (Megaton ~hundreds-of-ms today; goal is sub-100ms once dedup + budget caps land). Steady-state FPS unaffected by either dim.

**Profiling-infrastructure gap (new, recurring)**: dhat is the wrong tool for cell-streaming wall-clock findings. `tracing` spans around `consume_streaming_payload → finish_partial_import → load_one_exterior_cell → spawn_placed_instances` is the right infra and would give all 6 wall-clock findings a regression guard. Sits alongside the NIF-PERF-* "wire dhat" gap from earlier today's audit; orthogonal infrastructure.

## Architecture Snapshot

### Dim 7 (TAA / Skinning)

| Item | Status | Detail |
|---|---|---|
| TAA dispatch frequency | ✅ PRESENT | once per frame, single call site at draw.rs:1668 |
| TAA history alloc | ✅ PRESENT | 2× RGBA16F at swapchain res; persistent, recreated only on resize |
| TAA history footprint | ⚠ NOTE | 31.6 MB @ 1080p / 56.3 MB @ 1440p / **126.6 MB @ 4K** |
| Skin compute dispatch | ✅ PRESENT | gated on `bone_offset != 0` + per-entity dedup |
| BLAS path (skinned) | ✅ PRESENT | refit dominates; rebuild only on first sight or threshold (600 frames) |
| Bone palette upload | ✅ PRESENT | single 2 MB buffer per frame-in-flight, MAX_TOTAL_BONES = 32 768 |
| M29.3 raster path | ⏸ DEFERRED | triangle.vert still does inline skinning; output SSBO consumed only by BLAS refit |

### Dim 9 (Cell streaming)

| Item | Status | Detail |
|---|---|---|
| Cell load architecture | ✅ Phase 1b | single-cell async; rayon-parallel pre-parse, sync finish_partial_import |
| Pre-parse worker | ✅ PRESENT | `byro-cell-stream` thread + rayon `into_par_iter` |
| Result consumption | ✅ PRESENT | non-blocking `try_recv` at main.rs:472-485 |
| NIF import cache | ✅ PRESENT | process-lifetime `Resource`; `unload_cell` does NOT clear (#381) |
| BLAS LRU eviction | ✅ PRESENT | invoked pre-batch + mid-batch in `build_blas_batched` |
| Texture upload budget | ⚠ MISSING | every fresh DDS = synchronous `wait_for_fences(.., u64::MAX)` on main |
| Shutdown drain | ⚠ STUBBED | `worker: Option<JoinHandle>` held but never `.take().join()` (#856) |
| GPU mesh dedup across REFR | ❌ MISSING | every placement re-uploads Vec<Vertex>+Vec<u32> via fresh `with_one_time_commands` |

## Hot Path Analysis

| Operation | When | Cost (estimate) | Status |
|---|---|---|---|
| Per-REFR vertex+index buffer upload | every cell load, per placement | 2× fence-wait per placement | ❌ CELL-PERF-01 |
| NPC skeleton/body NIF parse | every NPC spawn | 7× redundant parses per NPC | ❌ CELL-PERF-02 |
| DDS texture upload sync fence-wait | every fresh texture | ~1 ms × N unique textures per cell | ❌ CELL-PERF-03 |
| StringPool write lock per mesh name | every mesh in spawn loop | per-mesh atomic CAS pair | ⚠ CELL-PERF-05 |
| `unload_cell` 6× SparseSet scans over victims | every cell unload | O(victims × 6) lookups | ⚠ CELL-PERF-06 |
| BSA mutex inside rayon worker closure | every cell pre-parse | ~10–20% extra speedup left | ⚠ CELL-PERF-07 (= NIF-PERF-13/#877) |
| TAA dispatch | per frame | O(pixels), single dispatch | ✅ |
| Skin compute dispatch | per frame, per skinned-mesh | gated, per-entity dedup'd | ✅ |
| BLAS refit (skinned) | per frame, per refit slot | dominates rebuild; 600-frame threshold | ✅ |
| BLAS LRU eviction | streaming-driven | pre-batch + mid-batch | ✅ |
| Material SSBO upload | per frame, full re-upload | ~3 MB/s steady-state PCIe waste | ⚠ DIM8-01 (filed yesterday as #878) |

## Findings

### CRITICAL

#### CELL-PERF-01: per-REFR-placement re-uploads the same cached mesh as a fresh GPU vertex+index buffer
- **Dimension**: World Streaming & Cell Transitions
- **Location**: [byroredux/src/cell_loader.rs:1968-2029](byroredux/src/cell_loader.rs#L1968-L2029) (placement loop), [crates/renderer/src/mesh.rs:130-173](crates/renderer/src/mesh.rs#L130-L173) (`upload_scene_mesh`), [crates/renderer/src/vulkan/buffer.rs:854-859](crates/renderer/src/vulkan/buffer.rs#L854-L859) (`with_one_time_commands` × 2 per upload)
- **Cost**: Megaton (929 REFRs, hundreds of mesh-bearing) → ~2× num_unique_placements synchronous `wait_for_fences(.., u64::MAX)` on main. Per-cell-transition stall is dominant.
- **Evidence**:
  ```rust
  // cell_loader.rs:1967-2029 — inside spawn_placed_instances, called PER REFR
  for mesh in imported {                       // imported = &cached.meshes (Arc<CachedNifImport>)
      let vertices: Vec<Vertex> = (0..num_verts).map(|i| { ... }).collect();
      let mesh_handle = ctx.mesh_registry.upload_scene_mesh(   // fresh GPU upload per placement
          &ctx.device, alloc, &ctx.graphics_queue, ctx.transfer_pool,
          &vertices, &mesh.indices, ctx.device_caps.ray_query_supported, None,
      ).unwrap();
      blas_specs.push((mesh_handle, ...));
  }
  // mesh.rs:140-159
  let vertex_buffer = GpuBuffer::create_vertex_buffer(...)?;  // fence-wait #1
  let index_buffer  = GpuBuffer::create_index_buffer(...)?;   // fence-wait #2
  ```
  40 chairs sharing one `chair.nif` → 80 fence-waits, not 2.
- **Why it matters**: This is the dominant cell-load wall-clock cost. The `Arc<CachedNifImport>` cache (#381) saves only CPU bytes; GPU buffers are re-uploaded for every placement. The R1 instance SSBO (112 B/instance) already supports N instances per shared mesh — the bottleneck is purely upload-stage.
- **Proposed fix**: Add a second cache layer keyed on `(cache_key, sub_mesh_index)` → `GpuMeshHandle`. First placement uploads + populates; subsequent placements look up the existing handle, increment refcount, skip upload. Drop on `unload_cell` when refcount → 0. Mirrors `TextureRegistry::acquire_by_path` (#524).
- **Profiling gap**: Wall-clock issue, not allocation. dhat won't measure it. Needs `tracing` span on `spawn_placed_instances` per cell. File "wire `tracing` for cell-load critical path" follow-up.

### HIGH

#### CELL-PERF-02: NPC spawn bypasses NIF import cache — every NPC re-parses skeleton/body/head from BSA bytes
- **Dimension**: World Streaming × NIF Parse
- **Location**: [byroredux/src/npc_spawn.rs:355-377](byroredux/src/npc_spawn.rs#L355-L377), [:399-474](byroredux/src/npc_spawn.rs#L399-L474), [:481-622](byroredux/src/npc_spawn.rs#L481-L622); [byroredux/src/scene.rs:1296-1321](byroredux/src/scene.rs#L1296-L1321) (`load_nif_bytes_with_skeleton` always re-parses)
- **Cost**: per-NPC. Megaton's ~40 NPCs × ~7 NIFs (skeleton + upperbody + lefthand + righthand + head + …) = ~280 redundant parses per cell. Skeleton path is THE SAME for every male NPC.
- **Evidence**:
  ```rust
  // npc_spawn.rs:356-377 — skeleton extract + parse runs on EVERY NPC
  let skel_data = match tex_provider.extract_mesh(skel_path) { ... };
  let (_skel_count, skel_root, skel_map) = load_nif_bytes_with_skeleton(
      world, ctx, &skel_data, skel_path, tex_provider, /*…*/
  );
  // scene.rs:1296-1321 — no cache consultation
  pub(crate) fn load_nif_bytes_with_skeleton(...) {
      let scene = match byroredux_nif::parse_nif(data) { Ok(s) => s, ... };
      let imported = byroredux_nif::import::import_nif_scene_with_resolver(...);
      ...
  }
  ```
  Compare with `cell_loader.rs::load_references` which does check `NifImportRegistry::get` first.
- **Why it matters**: `NifImportRegistry` (#381) already exists and is sized for this exact use case. NPC spawn predates the registry's adoption. For NPC-dense interiors (TestQAHairM with 31 NPCs / 61 refs is the audit case) this is the per-cell stall.
- **Proposed fix**: Route `load_nif_bytes_with_skeleton` through `NifImportRegistry`. Keyed on lowercased model path. Skeleton-bearing meshes (with `external_skeleton`) need special handling for the `node_by_name` map, but parsed scene + meshes are content-addressable. The shared idle-clip pattern at `npc_spawn.rs:148-153` (`get_by_path` fast-path) is the local precedent.
- **dhat gap**: Allocation impact real (~50 KB skeleton × 40 NPCs = 2 MB redundant parse memory); needs dhat for quantitative regression guard. Wall-clock is the dominant axis; profile via `tracing`.

#### CELL-PERF-03: synchronous texture uploads with no per-frame budget cap stall the main thread
- **Dimension**: World Streaming × GPU Pipeline
- **Location**: [crates/renderer/src/vulkan/texture.rs:155-243](crates/renderer/src/vulkan/texture.rs#L155-L243) (RGBA), [:455-560](crates/renderer/src/vulkan/texture.rs#L455-L560) (DDS mip chain), [:719-812](crates/renderer/src/vulkan/texture.rs#L719-L812) (`with_one_time_commands_inner` blocks `wait_for_fences(.., u64::MAX)`)
- **Cost**: per-cell-transition. One sync fence-wait per fresh texture. A worldspace edge with 100 fresh DDS files × ~1 ms/fence = ~100 ms stall on top of parse + import work. Dedup'd at the unique-texture level by `TextureRegistry::acquire_by_path` (#524) — but no per-frame budget cap.
- **Evidence**:
  ```rust
  // texture.rs:769-808 — every texture upload waits to completion on main
  device.queue_submit(q, &[submit_info], fence)
      .context("submit one-time commands")?;
  device.wait_for_fences(&[fence], true, u64::MAX)
      .context("wait for one-time commands")?;
  ```
  No "upload at most N MB this frame" cap anywhere in `TextureRegistry::load_dds`, `acquire_by_path`, or `Texture::from_dds_with_mip_chain`.
- **Why it matters**: A cell load that touches 100 fresh DDS files accumulates 100 sync `wait_for_fences` on the main thread. Combined with CELL-PERF-01 the cell-load critical path is "sleep while the GPU drains the queue."
- **Proposed fix**: Introduce `TextureUploadBudget` resource with per-frame byte cap (e.g. 16 MB) and a deferred-upload queue. `acquire_by_path` enqueues DDS bytes; `tick_texture_uploads` (called once per `draw_frame`) issues uploads up to budget into the per-frame transfer command buffer already submitted with rest of per-frame work. Bindless descriptor write (`pending_writes` at `texture_registry.rs:107`) is already deferred per-frame — extending the same pattern to image upload completes the picture. Single `with_one_time_commands` per frame, not per texture.
- **Profiling gap**: Wall-clock — needs `tracing` span on `acquire_by_path`, NOT dhat.

### MEDIUM

#### CELL-PERF-04: `WorldStreamingState` has no `Drop` impl — worker thread detached, not joined, on shutdown
- **Status**: Re-finding of #856 (open).
- **Location**: [byroredux/src/streaming.rs:120-212](byroredux/src/streaming.rs#L120-L212), [byroredux/src/main.rs:837](byroredux/src/main.rs#L837)
- **Evidence**: `worker: Option<JoinHandle<()>>` held but never `.take().join()`'d. Window-close path at main.rs:837 calls `self.streaming.take()` which drops `request_tx` (signaling close via channel), but doesn't await worker exit. Comment at streaming.rs:166-168 explicitly admits this.
- **Why it matters**: Today the worker holds `Arc<TextureProvider>` + `Arc<ExteriorWorldContext>` (read-only, survive Drop). Any future refactor where the worker holds a write-borrow on a soon-to-be-destroyed resource will UAF on shutdown.
- **Proposed fix**: Add `Drop` impl calling `self.worker.take().map(|h| h.join().ok())` after dropping `request_tx`. Five lines. Already filed as #856.

#### CELL-PERF-05: per-mesh `world.resource_mut::<StringPool>()` inside spawn loop serializes on lock churn
- **Dimension**: World Streaming × ECS Locking
- **Location**: [byroredux/src/cell_loader.rs:2150-2154](byroredux/src/cell_loader.rs#L2150-L2154), [:2044-2064](byroredux/src/cell_loader.rs#L2044-L2064)
- **Cost**: per-mesh. Megaton hundreds of write-lock acquisitions inside one cell load. Uncontested today (single-threaded loop); fast but each acquisition is atomic CAS + drop.
- **Evidence**:
  ```rust
  // cell_loader.rs:2150-2154 — write lock per mesh
  if let Some(ref name) = mesh.name {
      let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
      let sym = pool.intern(name);
      drop(pool);
      world.insert(entity, Name(sym));
  }
  ```
  Pattern from #523 (batched commit at `load_references` end) wasn't extended into `spawn_placed_instances`.
- **Why it matters**: When CELL-PERF-01 lands and placements share GPU mesh handles, StringPool lock churn becomes the next-most-visible cost.
- **Proposed fix**: Accumulator pattern mirroring #523's `pending_new` + `pending_hits`. Gather `Vec<(EntityId, FixedString)>`, single write lock at end of cell load.
- **dhat gap**: Zero alloc impact; pure CPU lock cost. Profile via `tracing`.

#### CELL-PERF-06: `unload_cell` does six sequential SparseSet scans over the victim list
- **Dimension**: World Streaming × ECS Query Patterns
- **Location**: [byroredux/src/cell_loader.rs:160-206](byroredux/src/cell_loader.rs#L160-L206)
- **Cost**: per-cell-unload, O(victims × 6) hash lookups instead of O(victims) walks.
- **Evidence**:
  ```rust
  if let Some(mq) = world.query::<MeshHandle>() { for &eid in &victims { ... } }
  if let Some(tq) = world.query::<TextureHandle>() { for &eid in &victims { ... } }
  if let Some(nq) = world.query::<NormalMapHandle>() { for &eid in &victims { ... } }
  if let Some(dq) = world.query::<DarkMapHandle>() { for &eid in &victims { ... } }
  if let Some(eq) = world.query::<ExtraTextureMaps>() { for &eid in &victims { ... } }
  if let Some(ttq) = world.query::<TerrainTileSlot>() { for &eid in &victims { ... } }
  ```
- **Why it matters**: Today amortized (rare path); becomes hot when Phase 1b doorwalking is active.
- **Proposed fix**: Collect every component query once outside per-component loop; single `for &eid in &victims` fans out to all six lookups inline.

#### CELL-PERF-07: `pre_parse_cell` rayon closure holds BSA `Mutex<File>` across `extract_mesh` — workers serialize on file I/O
- **Status**: Re-finding of #877 (NIF-PERF-13) — open.
- **Location**: [byroredux/src/streaming.rs:355-398](byroredux/src/streaming.rs#L355-L398)
- **Cost**: ~10–20% additional speedup left over current 6–7× rayon scaling.
- **Proposed fix**: Two-phase pre-parse — serial extract (one worker) → parallel parse on `(path, Vec<u8>)` pairs. Already filed as #877; this finding deduped to that issue.

### LOW

#### CELL-PERF-08: `expand_pkin_placements` allocates fresh `Vec` even when PKIN has 1 content
- **Location**: [byroredux/src/cell_loader_refr.rs:269-303](byroredux/src/cell_loader_refr.rs#L269-L303)
- **Evidence**: `Vec::with_capacity(pkin.contents.len())` — common case is 1 element. Same pattern in `expand_scol_placements`.
- **Proposed fix**: `smallvec::SmallVec<[(u32, Vec3, Quat, f32); 4]>` — stack-allocates the common case.
- **dhat gap**: Small; per-PKIN, not per-REFR.

#### CELL-PERF-09: `stamp_cell_root` uses `entry.push` per entity — would benefit from `extend(first..last)`
- **Location**: [byroredux/src/cell_loader.rs:104-112](byroredux/src/cell_loader.rs#L104-L112)
- **Cost**: Marginal; capacity already reserved, branch overhead per push.
- **Proposed fix**: `entry.extend(first..last); entry.push(cell_root);`. Trivial.

#### D7-OBS-04: skin compute → BLAS refit barrier uses legacy `ACCELERATION_STRUCTURE_READ_KHR`
- **Status**: Re-finding of #661 (SY-4) — open.
- **Location**: [crates/renderer/src/vulkan/context/draw.rs:599-612](crates/renderer/src/vulkan/context/draw.rs#L599-L612)
- **Cost**: Zero observable today (driver-aliased flags); semantic-clarity only.
- **Why it matters**: `ACCELERATION_STRUCTURE_READ_KHR` is for traversal (TLAS dereference inside ray queries), not for AS build-input vertex reads. Semantically correct flag is `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR` (sync2 only).
- **Proposed fix**: Defer to sync2 migration. Already tracked at #661; re-flagged for visibility (also called out in `AUDIT_RENDERER_2026-05-06.md:161`).

### INFO

#### D7-OBS-01: M29.3 raster path not shipped — pre-skin SSBO unused by triangle.vert
- **Location**: [crates/renderer/shaders/triangle.vert:127-145](crates/renderer/shaders/triangle.vert#L127-L145), [crates/renderer/src/vulkan/skin_compute.rs:299-315](crates/renderer/src/vulkan/skin_compute.rs#L299-L315) (deferred-by-design comment)
- **Cost**: ~50 ALU ops/vertex on the table for the raster pass. 32 NPCs × 5K verts × 2 frames = ~320K matrix sums/frame avoided once shipped.
- **Status**: Deliberate deferral. Filed as INFO so a future audit doesn't re-discover it.

#### D7-OBS-02: TAA history at 4K = 126.6 MB, not the audit-spec's "~64 MB"
- **Location**: [crates/renderer/src/vulkan/taa.rs:42-44](crates/renderer/src/vulkan/taa.rs#L42-L44)
- **Detail**: Implementation uses RGBA16F (matches HDR target); audit spec quoted RGBA8. Decision is correct — RGBA8 reprojection on HDR content would clip neighborhood-clamp inputs and produce visible banding. Note for future audit-spec revision.

#### D7-OBS-03: #677 (TAA / SVGF resize layout barrier) is now stale
- **Location**: [crates/renderer/src/vulkan/context/resize.rs:401-417](crates/renderer/src/vulkan/context/resize.rs#L401-L417) (TAA), [:298-314](crates/renderer/src/vulkan/context/resize.rs#L298-L314) (SVGF)
- **Detail**: `recreate_on_resize` now unconditionally followed by `initialize_layouts` — the `UNDEFINED → GENERAL` one-time barrier the issue body claimed was missing. Recommend close.

#### D7-OBS-05: per-frame, per-skinned-mesh descriptor-set rewrite (3 writes × N entities) is intentional
- **Location**: [crates/renderer/src/vulkan/skin_compute.rs:399-444](crates/renderer/src/vulkan/skin_compute.rs#L399-L444), [:130-136](crates/renderer/src/vulkan/skin_compute.rs#L130-L136) (rationale comment)
- **Cost**: ~25 µs/frame at 32 NPCs. Cell transitions invalidate the input vertex SSBO; bone-buffer slot rotates per frame-in-flight. Capturing bindings at slot-creation would require explicit invalidation on cell load + bone rotation — strictly more code for the same cost. Filed to preempt re-discovery.

#### D7-OBS-06: TAA jitter — Halton(2,3) computed CPU-side per frame, uploaded as 2-float UBO field
- **Location**: [crates/renderer/src/vulkan/context/draw.rs:18-29](crates/renderer/src/vulkan/context/draw.rs#L18-L29) (helper), [:286-301](crates/renderer/src/vulkan/context/draw.rs#L286-L301) (per-frame compute)
- **Detail**: Confirms shader doesn't recompute. ~5 ns/frame cost. Could be precomputed `[(f32, f32); 8]` table but maintenance overhead not worth it.

#### CELL-PERF-10: BLAS LRU eviction correctly invoked from streaming path (verified)
- **Location**: [crates/renderer/src/vulkan/acceleration.rs:1218-1300](crates/renderer/src/vulkan/acceleration.rs#L1218-L1300), [:2329-2393](crates/renderer/src/vulkan/acceleration.rs#L2329-L2393)
- **Detail**: Pre-batch + mid-batch eviction in `build_blas_batched`. Eviction is O(N) over `blas_entries` (~50 µs at 5K entries on 7×7 grid); would warrant a min-heap if entry count grew past 50K.

#### CELL-PERF-11: process-lifetime NIF cache (#381) verified — three-tier lookup in cell-load path
- **Location**: [byroredux/src/cell_loader_nif_import_registry.rs:74-323](byroredux/src/cell_loader_nif_import_registry.rs#L74-L323), [byroredux/src/cell_loader.rs:1155-1246](byroredux/src/cell_loader.rs#L1155-L1246)
- **Detail**: `unload_cell` does NOT call `clear()` or evict registry entries. Three-tier lookup (`pending_new` shadow → registry read-lock → parse + insert) preserves #523 batching invariant.

## Prioritized Fix Order

### P0 — cell-load wall-clock reduction (sequence)

1. **CELL-PERF-01** (CRITICAL) — GPU mesh dedup cache layered on `Arc<CachedNifImport>`. Refcounted by `(cache_key, sub_mesh_index) → GpuMeshHandle`; release on `unload_cell`. Mirrors `TextureRegistry::acquire_by_path`. Estimated win: 2× num_unique_placements fewer `wait_for_fences`.
2. **CELL-PERF-02** (HIGH) — route `load_nif_bytes_with_skeleton` through `NifImportRegistry`. ~280 redundant parses/cell on Megaton's 40 NPCs. Trivial code path; the registry is already there.
3. **CELL-PERF-03** (HIGH) — `TextureUploadBudget` resource + per-frame transfer batching. Mirrors existing `texture_registry::pending_writes` deferred-flush pattern. Single `with_one_time_commands` per frame instead of per texture.

### P1 — quick wins

4. **CELL-PERF-09** (LOW) — `entry.extend(first..last)`. Trivial.
5. **CELL-PERF-08** (LOW) — `SmallVec<[_; 4]>` for PKIN/SCOL fanout.
6. **CELL-PERF-06** (MEDIUM) — collapse 6 SparseSet scans into 1 victim walk.
7. **CELL-PERF-05** (MEDIUM) — batched StringPool commit pattern (mirror #523).

### P2 — already-filed, defer to existing issues

8. **CELL-PERF-04** = #856 — Drop impl on `WorldStreamingState` joining worker.
9. **CELL-PERF-07** = #877 (NIF-PERF-13) — split pre_parse_cell into serial-extract + parallel-parse phases.
10. **D7-OBS-04** = #661 — sync2-migration cleanup of AS build barrier flag.
11. **D7-OBS-03** — close #677 (stale, fix already in place).

### P3 — deliberate deferrals (file as observation, no action today)

12. **D7-OBS-01** — M29.3 raster path. Track via ROADMAP M29 milestone.
13. **D7-OBS-02** — TAA RGBA16F at 4K. Update audit-spec wording from "~64 MB" to "~127 MB."
14. **D7-OBS-05/-06** — descriptor rewrite + jitter. Documented intentional.

### Infrastructure — orthogonal but blocks regression coverage

15. **Wire `tracing` for cell-load critical path** — span ladder around `consume_streaming_payload → finish_partial_import → load_one_exterior_cell → load_references → spawn_placed_instances`. Single piece of infra that gives CELL-PERF-01/02/03/05/06/07 wall-clock regression guards. Pairs alongside the NIF-PERF-* "wire dhat" follow-up from `AUDIT_PERFORMANCE_2026-05-06.md`.

## Closed since last audit (verified this run)

- **#641** (SH-3) — closed. `triangle.vert:140-145` correctly composes `xformPrev` from `bones_prev[]`.
- **#642 / #644** (scratch barrier) — closed. `record_scratch_serialize_barrier` invoked once per refit iteration.
- **#679** (refit-chain BVH degradation) — closed via `SKINNED_BLAS_REFIT_THRESHOLD = 600` + `should_rebuild_skinned_blas` gate.
- **#871** (L2 — slot leak on descriptor pool exhaust) — closed via explicit `output_buffer.destroy` rollback.
- **#681** (MEM-2-6 — VERTEX_BUFFER usage flag) — closed; deliberately omitted, comment block at `skin_compute.rs:299-315` explains M29.3 deferral.
- **#381** (NIF cache process-lifetime) — confirmed in place; `unload_cell` does NOT clear.
- **#510** (mid-batch BLAS eviction) — verified live at `acceleration.rs:1312-1326`.

## Notes

- **Profiling-infrastructure gap (recurring)**: dhat is wrong tool for cell-streaming wall-clock. `tracing` spans + Tracy/flame-graph dump is the right infra. None of the 6 wall-clock findings have a quantitative regression guard today. Filing one "wire `tracing` for cell-load critical path" follow-up gives all 6 a guard with single piece of infra.
- **dhat gaps (per audit-performance spec)**: CELL-PERF-08, CELL-PERF-09, and the alloc-coupled axis of CELL-PERF-02 each carry an alloc component whose savings are estimates only. Each warrants a follow-up "wire dhat for this site" issue.
- **Out-of-scope follow-ups touched by this audit**:
  - #779 (`MAX_MATERIALS = 4096` over-allocates BAR) — dim 8, not dim 9; SSBO it backs IS uploaded once per cell on first reference.
  - #841 (M41-PHASE-1BX body-skinning artifact) — correctness, not perf; surfaces through same path as CELL-PERF-02.
- **Not testable today (Dim 7)**: No GPU timestamp regression test for TAA dispatch cost or skin compute cost. `cargo test -p byroredux-renderer` exercises layout/stride invariants only. Future RenderDoc capture-diff workflow could pin per-pass µs counts; out of scope.
