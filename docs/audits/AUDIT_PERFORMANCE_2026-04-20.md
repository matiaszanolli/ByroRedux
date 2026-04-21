# Performance Audit — 2026-04-20

**Scope**: GPU pipeline, GPU memory, draw-call batching, ECS query patterns,
NIF parse, CPU allocation hot paths. Depth: deep (hot-path traced).

**Dedup baseline**:
- `docs/audits/AUDIT_PERFORMANCE_2026-04-13b.md` (prior run, most items landed
  through Sessions 8–12).
- 49 open GitHub issues (cached at audit time).

**Per-dimension reports**: scratch files in `/tmp/audit/performance/dim_*.md`
during the run; findings merged below.

---

## Executive Summary

**Total findings: 39** across 6 dimensions. Zero CRITICAL. The codebase is in
good steady-state shape — the dominant remaining issues are *transient*
(cell-load allocator churn) and *high-water-mark retention* (BLAS scratch that
never shrinks after a large mesh).

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 3 |
| MEDIUM | 13 |
| LOW | 15 |
| ENHANCEMENT | 8 |

**Single highest-leverage fix**: `#239` / D2-H1 — texture uploads still bypass
the existing `StagingPool`. On a FO3 Megaton / FNV Freeside cell load, 200–700
fresh staging allocations hit the allocator lock per cell. Landing this cuts
cell-load transient VRAM by **150–200 MB** and removes the dominant lock
contention at load time.

**Single highest-leverage steady-state fix**: D2-M1 — BLAS scratch buffer
never shrinks. After one large mesh (Skyrim draugr, Starfield LODs) the engine
permanently holds 80–200 MB of scratch VRAM. On the 6 GB RT minimum target,
that's 3–5% of the budget lost forever after one peek.

**Single highest-leverage frame-time fix**: D1-1 — TLAS build barrier uses
`ALL_COMMANDS_BIT` on both sides. Estimated ~1.2–1.8 ms/frame GPU bubble on
RTX 4070 Ti when the TLAS rebuilds (i.e. every frame in a scene with dynamic
actors). Tightening to `ACCELERATION_STRUCTURE_BUILD_BIT → FRAGMENT_SHADER_BIT`
should recover most of it.

**Landed since 2026-04-13b (confirmed)**: P2-07 (range flush), P2-08 (reusable
fence), P2-09 (sized TLAS barrier), P2-10 (tuned allocator blocks), P1-09
(TLAS FAST_TRACE + UPDATE), #272 (instanced batching), #309 (multi-draw
indirect), #294 (global VB/IB), #306 (IEEE sortable key), #392 (blend pipeline
cache), #398 (extended dynamic depth state), #464 (BFS propagation via
VecDeque), #470 (terrain splat).

**Carried over from 2026-04-13b (still unfixed)**: #239 (texture staging pool),
P5-03 (Arc→String clones at import boundary), P5-06 (to_triangles capacity
hint), D1-6 (G-buffer mesh-ID oversized).

---

## Hot Path Baseline

### Exterior cell, ~5000 REFR, post-frustum-cull

| Stage | Count | Cost |
|---|---|---|
| DrawCommand emitted | 1200–1800 | CPU, `build_render_data` ~2 ms |
| Sort (rayon par_sort_unstable) | same | ~0.1 ms |
| DrawBatch after `#272` merge | 120–250 | collapse ratio ~8–10× |
| `cmd_draw_indexed_indirect` calls | 30–80 | further 3–4× collapse from `#309` |
| `cmd_bind_pipeline` per frame | 4–12 | opaque, opaque-2s, decal-bias, 1–8 blend variants, UI |
| `cmd_bind_descriptor_sets` (set 0, bindless) | 1 | once/frame |
| `cmd_bind_descriptor_sets` (set 1, scene) | 1 | once/frame |
| `cmd_push_constants` | 0 | fully SSBO-driven |
| TLAS rebuild | 1 | 0.4–1.0 ms GPU |
| BLAS builds (cell-load only) | 50–500 per load | batched per `#382` |

### Interior cell, ~800 entities, ~20 active clips

| System | Lock guards | Time |
|---|---|---|
| `fly_camera` | 1 R + 1 W | <10 µs |
| `animation_system::advance_all` | 3 R + 2 W | ~300 µs |
| `transform_propagation` | 2 R + 2 W | ~180 µs |
| `build_render_data` | 15 R | ~2 ms |
| `stats_system` | 3 R | <20 µs |
| **Scheduler overhead** | — | <1 µs per frame (10 systems × ~50 ns Fn indirection) |

### NIF parse (1000-block mesh)

| Phase | Time |
|---|---|
| Header parse | 0.5 ms |
| Block dispatch (giant match) | 0.2 ms |
| Geometry (200 shapes, vertex read) | 10–15 ms |
| Transform composition | 0.5 ms |
| SVD repair (5–10 degenerate) | 0.05 ms |
| Import walk + extract | 2–4 ms |
| **Total** | **13–20 ms** (parse no longer the bottleneck; texture I/O dominates) |

---

## Findings

### HIGH

#### D1-H1: TLAS build barrier uses `ALL_COMMANDS_BIT` on both sides
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/acceleration.rs` (TLAS rebuild path)
- **Status**: NEW (regression of a prior tightening that was widened in
  Session 11 to chase a sync validation error).
- **Description**: `src_stage = ALL_COMMANDS_BIT`, `dst_stage = ALL_COMMANDS_BIT`.
  Drains every prior command (including the G-buffer pass that already
  finished) before the next frame's geometry pass can begin.
- **Impact**: ~1.2–1.8 ms/frame GPU bubble on 4070 Ti when TLAS rebuilds
  (every frame with dynamic actors). Appears as idle SM time in GPUView
  between compute and graphics queues.
- **Suggested Fix**: Narrow to
  `src = ACCELERATION_STRUCTURE_BUILD_BIT_KHR`,
  `dst = RAY_TRACING_SHADER_BIT_KHR | FRAGMENT_SHADER_BIT`,
  `src_access = ACCELERATION_STRUCTURE_WRITE_BIT`,
  `dst_access = ACCELERATION_STRUCTURE_READ_BIT`. If the original validation
  error returns, the G-buffer transition is the one that needs fixing, not
  this barrier.

#### D1-H2: Per-draw descriptor-set rebind despite bindless architecture
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (main geometry pass)
- **Status**: NEW
- **Description**: The geometry pass rebinds set 1 (scene) for each draw even
  when adjacent batches share all state. Because the sort key groups by
  pipeline (not descriptor set), the bind count equals the batch count rather
  than the unique-descriptor count.
- **Impact**: Thousands of redundant `vkCmdBindDescriptorSets` per frame on
  FO3 Megaton (929 REFRs). Visible CPU overhead in command-buffer recording.
- **Suggested Fix**: Track `last_bound_scene_set` and skip rebind when
  unchanged. One `u64` compare per draw.
- **Note**: Conflicts with Dim 3's claim that set-1 is bound once/frame —
  Dim 3 looked at the top-of-pass bind; Dim 1 found additional rebinds inside
  the batch loop. **Verify by grepping `cmd_bind_descriptor_sets` in
  `draw.rs`** before acting.

#### D2-H1: Texture uploads bypass `StagingPool` (Existing #239, P2-05 carried over)
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/texture.rs:48-95, 291-337, 524-557`
- **Status**: **Existing: #239** (open)
- **Description**: `Texture::from_rgba` / `from_bc` / `from_dds` still hit
  `allocator.lock().allocate(...)` directly per texture; the `StagingPool`
  landed by `#99` is wired through mesh uploads only. Texture uploads pay
  200–700 allocator round-trips per cell load, contending with mesh upload
  on the same allocator mutex.
- **Impact**: Cell-load transient peaks of 50–200 MB host-visible staging
  churn. Dominant allocator contention during load. Steady state unaffected.
- **Suggested Fix**: Thread `staging_pool: Option<&mut StagingPool>` through
  `Texture::from_*`, mirroring `GpuBuffer::create_vertex_buffer`. `StagingGuard`
  RAII is replaced by `pool.release(buf, alloc, cap)` after the fence signals.

---

### MEDIUM

#### D1-M1: SVGF history barrier missing `SHADER_WRITE_BIT` on dst access
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/svgf.rs` (temporal accumulation dispatch)
- **Status**: NEW
- **Description**: The ping-pong history transitions with `dst_access = SHADER_READ_BIT`
  only, but the next frame's compute dispatch writes the same image via a
  storage descriptor. Missing `SHADER_WRITE_BIT` in dst access.
- **Impact**: Latent on NVIDIA (implicit flush). On AMD / Intel, risks
  stale-read corruption manifesting as temporal ghost streaks.
- **Suggested Fix**: Add `SHADER_WRITE_BIT` to the dst access mask on the
  read-view barrier preceding the temporal dispatch.

#### D1-M2: Composite pass dependency over-broad (`ALL_GRAPHICS_BIT`)
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/composite.rs`, `context/helpers.rs`
- **Status**: NEW
- **Description**: The geometry→composite external subpass dependency uses
  `ALL_GRAPHICS_BIT` on both ends. Should be
  `COLOR_ATTACHMENT_OUTPUT_BIT → FRAGMENT_SHADER_BIT` with
  `COLOR_ATTACHMENT_WRITE → SHADER_READ`.
- **Impact**: Minor on desktop (~0.2 ms); much larger on mobile/integrated.
  Blocks tile-based GPUs from overlapping geometry tail with composite head.
- **Suggested Fix**: Tighten the dep. Consider promoting attachments to input
  attachments + `VK_EXT_subpass_merging` in the M37 spatial-filter pass.

#### D1-M3: `triangle.frag` lacks `early_fragment_tests`, wastes RT queries on overdraw
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/triangle.frag`
- **Status**: NEW
- **Description**: Reflection + GI ray queries run before depth-test culls
  overdrawn fragments. Early-Z is defeated because the fragment writes to
  G-buffer storage attachments and doesn't declare
  `layout(early_fragment_tests) in;`.
- **Impact**: On FO3 exterior worldspaces with overlapping terrain/rock
  overdraw, fragment shader invocations are 2–3× higher than strictly needed.
  Each extra invocation costs ~2 ray queries.
- **Suggested Fix**: Add `layout(early_fragment_tests) in;` at the top of
  `triangle.frag`. Legal because the shader does not conditionally discard
  based on values derived from the ray queries themselves. Alternative: move
  motion-vector + mesh-ID writes into a dedicated depth prepass.

#### D2-M1: BLAS scratch buffer never shrinks (permanent VRAM pin after one big mesh)
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:106, 206-212, 416-432, 659-675`
- **Status**: NEW
- **Description**: `scratch_needs_growth` is strictly monotonic. Once a mesh
  requiring 80 MB of BLAS scratch is seen (Skyrim draugr, FO4 LOD terrain,
  Starfield Saturn.nif), the scratch buffer is pinned to 80 MB
  DEVICE_LOCAL for the entire process lifetime. Same pattern on the per-frame
  TLAS scratch buffers.
- **Impact**: 80–200 MB steady-state VRAM permanently reserved after any
  single "big" cell load or NIF peek, even back in tiny interiors. On the
  stated 6 GB RT minimum target, 3–5% of the budget permanently burned.
- **Suggested Fix**: After `evict_unused_blas`, recompute max `build_scratch_size`
  across surviving BLAS; shrink buffer if capacity exceeds new max by
  >2× or >16 MB. Simpler alternative: `shrink_scratch_to()` called from
  `unload_cell`.

#### D2-M2: `drain_terrain_tile_uploads` allocates fresh 32 KB Vec per dirty frame
- **Dimension**: GPU Memory / CPU Allocations
- **Location**: `crates/renderer/src/vulkan/context/resources.rs:50-61`
- **Status**: NEW (introduced by #470)
- **Description**: `.collect()` over `self.terrain_tiles` produces a fresh
  32 KB Vec every dirty frame. Fires `MAX_FRAMES_IN_FLIGHT` times per cell
  transition (128 KB of heap churn per cell load).
- **Suggested Fix**: Persistent `terrain_tile_scratch: Vec<GpuTerrainTile>`
  on `VulkanContext`; return `&[GpuTerrainTile]`. Same pattern as
  `tlas_instances_scratch`.

#### D2-M3: Terrain tile SSBO allocated per-frame-in-flight, but data is static until next cell
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/scene_buffer.rs:336, 371-372, 412-417`
- **Status**: NEW (introduced by #470)
- **Description**: Buffer is HOST_VISIBLE, sized 32 KB, duplicated per
  frame-in-flight. GPU reads it read-only; data rewrites only on cell load.
  Per-frame-in-flight double-buffering is the wrong pattern — a single
  DEVICE_LOCAL buffer with one-time staging upload at cell-load time would do.
- **Impact**: 64 KB permanently pinned on the scarce BAR heap (~256 MB total
  on NVIDIA) when 32 KB DEVICE_LOCAL would suffice.
- **Suggested Fix**: Promote to single DEVICE_LOCAL buffer, upload via
  `StagingPool` at cell-load. Delete `tiles_dirty_frames` counter. Same
  descriptor for every frame-in-flight (read-only data).

#### D2-M4: `write_mapped` re-queries memory properties per call
- **Dimension**: GPU Memory / CPU Overhead
- **Location**: `crates/renderer/src/vulkan/buffer.rs:549-608`
- **Status**: NEW
- **Description**: `alloc.memory_properties().contains(HOST_COHERENT)` is
  called per mapped-write. Result is known at bind time but re-checked
  ~10 times/frame (camera, lights, bones, instances, indirect, SSAO, TAA,
  caustic, SVGF, composite).
- **Impact**: 5–15 µs CPU/frame, shows up in Linux `perf record`.
- **Suggested Fix**: Cache `is_coherent: bool` on `GpuBuffer` at bind. One-time
  read, reused by `write_mapped` / `flush_if_needed` / `flush_range`.

#### D3-M1: Blended sort key omits `(src_blend, dst_blend)` — pipeline thrash in particle scenes
- **Dimension**: Draw Call Overhead
- **Location**: `byroredux/src/render.rs:597-619`
- **Status**: NEW
- **Description**: The blended branch of the sort key is
  `(1u8, is_decal, two_sided, !sort_depth, depth_state, texture_handle, mesh_handle)` —
  `(src_blend, dst_blend)` are absent entirely. Additive vs alpha vs modulate
  draws interleave by depth within the same two-sided cohort, producing a
  `cmd_bind_pipeline` through the `blend_pipeline_cache` on every alternation.
- **Impact**: ~20–40 extra blend-pipeline switches/frame in particle-heavy
  scenes (Megaton firelight, Vault 101 steam). ~0.02–0.08 ms GPU/frame.
  Also blocks same-blend-same-mesh particle instance-merging at different
  depths.
- **Suggested Fix**: Add `(src_blend, dst_blend)` to the blended sort key
  between slots 2 and 3 (after `two_sided`, before `!sort_depth`). Correctness
  preserved: back-to-front order is only required *within* one pipeline state.

#### D3-M2: `debug_assert!` tuple order mismatches actual sort key order
- **Dimension**: Draw Call Overhead / Testability
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:470-479` vs
  `byroredux/src/render.rs:597-619`
- **Status**: NEW
- **Description**: The debug assert reads
  `(w[0].alpha_blend, w[0].two_sided, w[0].is_decal)` — but the real sort key
  uses `(alpha_blend, is_decal, two_sided, …)`. The assert validates the
  wrong clustering order; the `sort_depth` escape clause papers over most
  mismatches. Near-useless as a safety net.
- **Suggested Fix**: Either delete and replace with a unit test on
  `build_render_data`'s output, or align tuple to
  `(alpha_blend, is_decal, two_sided)`. The unit test is better — the sort
  lives in `render.rs`, the assert lives in `renderer/`, and the two drift.

#### D4-M1: `build_render_data` holds ~15 concurrent read locks for 1.5–2 ms
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/render.rs::build_render_data`
- **Status**: NEW
- **Description**: `build_render_data` opens ~15 `QueryRead` guards (MeshHandle,
  GlobalTransform, AnimatedVisibility/Alpha/Color, AlphaBlend, TwoSided, Decal,
  Parent, Name, Skin, FormIdComponent, LightSource, Camera, markers) and holds
  them all live across the outer iteration. Fine single-threaded; blocks the
  parallel-scheduler story.
- **Impact**: Today: 0 ns. Future: up to 2 ms stall per frame if any parallel
  system writes Transform/MeshHandle/AnimatedAlpha mid-build.
- **Suggested Fix**: Introduce a `RenderExtract` stage that snapshots
  `(entity, GlobalTransform, MeshHandle, material_flags, animated_overrides)`
  into a `Vec<RenderInstance>` resource in one pass; then `build_render_data`
  iterates the Vec with zero locks. Equivalent to Bevy's extract stage. Defer
  until M40 parallel scheduler.

#### D4-M2: `query_2_mut` re-acquires storages on every call, no persistent view
- **Dimension**: ECS Query Patterns
- **Location**: `crates/core/src/ecs/world.rs::query_2_mut*`
- **Status**: NEW
- **Description**: Each call acquires two TypeId-sorted RwLock guards
  (~25 ns each) and returns them. ~500 ns/frame aggregate across the scheduler.
  Below signal floor today; matters if someone writes a hot-loop system that
  calls it inside an inner loop.
- **Suggested Fix**: Defer. Document that systems should acquire queries once
  at top of function. If a hot case appears, add `World::view_2<A, B>()` that
  returns a `View` holding both guards with a sort-merge iterator.

#### D5-M1: Import-boundary String clones (Arc<str> → Option<String>)
- **Dimension**: NIF Parse
- **Location**: `walk.rs:148, 215`, `mesh.rs:139, 429, 557, 565`
- **Status**: **Existing (P5-03 carried over from 2026-04-13b)**
- **Description**: NIF parser stores block names as `Option<Arc<str>>`
  (zero-copy). The import layer converts to `Option<String>` via `.clone()`
  at 10+ sites. ~500 String allocs per 1000-block NIF.
- **Impact**: ~2–3% of parse time (~0.4 ms per 20 ms parse). Negligible
  absolute; trivially avoidable.
- **Suggested Fix**: Change `ImportedNode.name` + `ImportedMesh.name` from
  `Option<String>` to `Option<Arc<str>>`. Eliminates clones at all sites.

#### D5-M2: Bulk read methods for geometry arrays not implemented
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/stream.rs` (NifStream)
- **Status**: **Existing (P5-04 carried over from 2026-04-13b)**
- **Description**: No `read_ni_point3_array(count)`, `read_u16_array(count)`
  bulk methods. Per-element `read_exact` calls fire ~50K times per
  1000-block NIF.
- **Impact**: Reduces ~50K read_exact calls to ~1K. Hard to quantify
  precisely — unsafe reinterpret requires endianness care.
- **Suggested Fix**: 4–6 h design/test for safe reinterpret path. Deferred.

---

### LOW

#### D1-L1: G-buffer mesh-ID and raw-indirect formats oversized
- **Dimension**: GPU Memory (adjacent to pipeline)
- **Location**: `crates/renderer/src/vulkan/gbuffer.rs`
- **Status**: NEW (prior audit flagged but current formats already updated
  per Dim 2 — mesh_id is now R16_UINT, raw_indirect is B10G11R11_UFLOAT).
  **Partial regression candidate — verify**: Dim 1's HIGH-severity claim of
  16_SFLOAT for mesh_id contradicts Dim 2's reported R16_UINT. One of the
  two reads the code wrong. Reader: confirm by opening `gbuffer.rs`.
- **Suggested Fix**: If mesh_id is already R16_UINT, close as fixed.
  Otherwise, downsize to R32_UINT (4 B/texel).

#### D1-L2: Fence wait stall with `MAX_FRAMES_IN_FLIGHT = 2` limits CPU pipelining
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (frame acquire)
- **Status**: NEW
- **Description**: Per-frame fence waited with `timeout = u64::MAX` before
  `vkResetCommandBuffer`. With 2 frames in flight, this is the dominant CPU
  stall on fast frames.
- **Impact**: At 240 Hz target, stalls CPU thread on GPU frame N-1 finish
  before frame N+1 can record.
- **Suggested Fix**: Raise `MAX_FRAMES_IN_FLIGHT` to 3 + per-frame sync
  primitives. ~1 MB extra UBO/fence cost.

#### D2-L1: gpu-allocator single global pool, no per-usage separation
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/allocator.rs:16-48`
- **Status**: NEW
- **Description**: Single `Arc<Mutex<Allocator>>` services linear DEVICE_LOCAL,
  non-linear DEVICE_LOCAL (images), and HOST_VISIBLE (staging). No per-lifetime
  segregation. Long-lived (G-buffer, BLAS result) shares blocks with
  short-lived (TLAS instance staging, transient texture staging).
- **Impact**: Over many cell transitions, fragmentation at block boundaries
  can strand long-lived 1 KB allocations inside 64 MB blocks. No observed OOM
  yet; `log_memory_usage` doesn't surface fragmentation at all.
- **Suggested Fix**: Short-term: add fragmentation metric
  (`largest_free_range / total_free` per block). Long-term: gpu-allocator
  v0.28 per-scope budgets.

#### D2-L2: Per-frame terrain descriptor write is redundant with D2-M3
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/scene_buffer.rs:623-627`
- **Status**: NEW (fold into D2-M3)
- **Suggested Fix**: Subsumed by D2-M3's promotion to a single DEVICE_LOCAL
  buffer.

#### D2-L3: `tlas_instances_scratch` reserve never shrinks
- **Dimension**: GPU Memory / CPU Allocations
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:109, 1017, 1451`
- **Status**: NEW
- **Description**: After a 10K-draw exterior peak, the Vec retains 640 KB
  capacity forever. Same pattern as D2-M1 at the CPU side.
- **Suggested Fix**: After `mem::replace`, shrink if
  `capacity > 2 * instance_count.max(512)`.

#### D2-L4: `log_memory_usage` warn threshold hard-coded at 2 GB
- **Dimension**: Observability
- **Location**: `crates/renderer/src/vulkan/allocator.rs:71-79`
- **Status**: NEW
- **Description**: 2 GB is 33% of the 6 GB minimum (too low for FO4 exteriors)
  and 16% of the 12 GB dev GPU (triggers unhelpful warn per big cell).
- **Suggested Fix**: Enable `VK_EXT_memory_budget` (core in 1.1+). Threshold
  = 0.8 × smallest DEVICE_LOCAL heap budget.

#### D3-L1: Particles sorted back-to-front, preventing cross-emitter instance-merging
- **Dimension**: Draw Call Overhead
- **Location**: `byroredux/src/render.rs:476-571, 597-619`
- **Status**: NEW
- **Description**: Per-emitter particles with same `(mesh, src, dst, two_sided)`
  interleave by depth, fragmenting instance merging. Cosmetic post-#309
  (indirect call still collapses), but ~80 extra batches/frame in particle
  scenes.
- **Suggested Fix**: Optional — document and leave alone, or emit in
  emitter-group order with emitter-local depth.

#### D3-L2: Each LAND cell has a unique `mesh_handle` — terrain never instance-merges
- **Dimension**: Draw Call Overhead
- **Location**: `byroredux/src/cell_loader.rs::spawn_terrain_mesh`
- **Status**: NEW (post-#470 observation)
- **Description**: Splat flag correctly avoids a pipeline switch per tile, but
  terrain meshes don't share handles, so no cross-tile instance-merging
  happens. Trivial at 3×3 (9 tiles); revisit at 9×9 (81 tiles).
- **Suggested Fix**: Defer. If 9×9 exteriors become a perf concern, refactor
  to a shared unit patch + per-tile displacement in `GpuTerrainTile`.

#### D3-L3: Unstable sort with no tiebreaker means capture/replay is non-deterministic
- **Dimension**: Draw Call Overhead
- **Location**: `byroredux/src/render.rs:597` (`par_sort_unstable_by_key`)
- **Status**: NEW
- **Description**: Rayon's work-stealing can reorder full-tuple ties across
  frames. Benign for rendering (bindless texture array ignores draw order),
  but breaks screenshot-diff / RenderDoc capture determinism.
- **Suggested Fix**: Append `entity.id()` as secondary tiebreaker, or use
  serial `sort_unstable_by_key` under a threshold (~500 commands).

#### D3-L4: Unconditional `cmd_bind_pipeline` at render-pass begin is always overridden
- **Dimension**: Draw Call Overhead
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:725-726`
- **Status**: NEW
- **Description**: Unconditional bind of `self.pipeline`, immediately replaced
  by first batch iteration. One wasted API call/frame.
- **Suggested Fix**: Initialize `last_pipeline_key` to the first opaque
  pipeline key; let the batch loop handle binding.

#### D4-L1: Console `find_by_name` bypasses name index for substring/prefix
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/commands.rs`
- **Status**: NEW
- **Description**: Exact-match lookups hit #243's StringPool-keyed HashMap.
  Substring/prefix fall back to linear Name-storage scan. Debug-only.
- **Suggested Fix**: Leave as-is. Build a trie if interactive autocomplete
  ever needs it.

#### D4-L2: Transform propagation re-scans root set every frame
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/systems.rs::transform_propagation`
- **Status**: NEW
- **Description**: BFS root enumeration iterates the full Transform storage
  checking for absence of `Parent`. ~12 µs per frame on 800 entities. Below
  signal floor.
- **Suggested Fix**: Defer. `RootEntities: Resource<HashSet<EntityId>>`
  incrementally maintained on `Parent` insert/remove if measured >50 µs.

#### D4-L3: Animation system may re-read `DeltaTime` per animated entity (unverified)
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/systems.rs::animation_system`
- **Status**: NEW (flagged for triage)
- **Description**: If `advance_stack` / `advance_time` take `&World`, they
  re-read DeltaTime per entity. Needs verification.
- **Suggested Fix**: Confirm `advance_stack(dt: f32)` signature; refactor if
  it takes `&World`.

#### D5-L1: `NiTriStripsData::to_triangles()` missing `Vec::with_capacity`
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/blocks/tri_shape.rs` (around `to_triangles`)
- **Status**: **Existing (P5-06 carried over from 2026-04-13b)**
- **Description**: One-line fix never landed. ~20–30 unnecessary reallocations
  per NIF with strip geometry.
- **Suggested Fix**: Add `.with_capacity(self.num_triangles as usize)`.

#### D6-L1: `palette_scratch` fresh-allocated each `build_render_data`
- **Dimension**: CPU Allocations
- **Location**: `byroredux/src/render.rs:133`
- **Status**: NEW
- **Description**: `Vec<Mat4>` created stack-local every frame; correctly
  reused across the skinned-mesh loop but doesn't survive across frames.
  ~60 KB churn/frame.
- **Suggested Fix**: Hoist onto the caller (engine state or `RenderDataResource`)
  and pass `&mut`, same pattern as `draw_commands` / `gpu_lights`.

---

### ENHANCEMENT

#### D2-E1: BLAS eviction doesn't run mid-batch during cell-load burst
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:1467-1528`
- **Description**: Eviction runs only between frames; during a synchronous
  cell load the engine can build several hundred BLAS without checking budget.
- **Suggested Fix**: Call `evict_unused_blas` every ~100 BLAS builds inside
  the batched-build path. On a 6 GB GPU, removes the OOM-during-load failure
  mode.

#### D2-E2: StagingPool 64 MB budget too small once D2-H1 lands
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/buffer.rs:16`
- **Description**: 2K BC7 full-mip chain ~5.3 MB, 4K BC7 ~22 MB. A 20-texture
  burst during cell transition can momentarily need ~100 MB of staging.
- **Suggested Fix**: Bump default to 128–256 MB, or expose
  `StagingPool::reserve(total)` for known bulk-load working sets. Pairs
  with D2-H1.

#### D3-E1: UI quad could use fullscreen-triangle + bindless texture, drop VB/IB rebind
- **Dimension**: Draw Call Overhead
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1005-1014`
- **Description**: UI quad still has its own VB/IB and `cmd_bind_*` calls.
  Common Vulkan idiom: generate NDC from `gl_VertexIndex` in the vertex
  shader and call `cmd_draw(3, 1, 0, 0)`.
- **Suggested Fix**: ~5 lines of GLSL change, ~2 µs/frame saving.

#### D3-E2: Non-uniform-scale detection cacheable per entity
- **Dimension**: Draw Call Overhead
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:494-500`
- **Description**: Three column-length squared-diffs computed per `DrawCommand`
  each frame. ~22.5K FLOPs/frame on 1500 statics that never change.
- **Suggested Fix**: Compute once on spawn, store as `NonUniformScale` marker
  component. ~0.01 ms/frame savings.

#### D4-E1: NIF import does per-entity single-component inserts
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/scene.rs::load_nif_*`
- **Description**: 5000-entity cell load = 25,000 write-lock acquire/release
  pairs. Estimated ~30 ms of the measured ~180 ms cell-load time is lock
  thrash.
- **Suggested Fix**: `World::insert_batch<T>(items)` — one write lock per
  batch. Apply to scene import first.

#### D4-E2: Scheduler lacks `should_run` predicate for empty-query early-out
- **Dimension**: ECS Query Patterns
- **Location**: `crates/core/src/ecs/scheduler.rs::run`
- **Description**: Every registered system runs every frame regardless of
  whether its query has matches. ~500 ns/frame total. Sub-noise today; more
  valuable with parallel scheduler.
- **Suggested Fix**: Add optional `fn should_run(&self, world: &World) -> bool`
  on the `System` trait. Defer to M40.

#### D5-E1: Per-block `Box<dyn>` heap allocation
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/blocks/mod.rs::parse_block`
- **Status**: **Existing (P5-07 carried over from 2026-04-13b)**
- **Description**: ~1000 `Box::new` allocations per NIF. Unavoidable without
  arena allocator (bumpalo) or enum dispatch.
- **Suggested Fix**: Major refactor. Defer unless NIF parse becomes the
  bottleneck (currently ~15% of cell load).

#### D6-E1: Scratch-buffer `mem::take` + `clear()` pattern deserves a helper
- **Dimension**: CPU Allocations
- **Location**: `draw.rs:463-468, 1229-1230`; `acceleration.rs:1017, 1451`
- **Description**: Pattern repeated at 3+ sites, easy to forget to restore.
- **Suggested Fix**: Small macro / RAII guard. Nice-to-have.

---

## Prioritized Fix Order

### Quick wins (< 1 hour each)

1. **D2-M4** — cache `is_coherent` on `GpuBuffer` at bind. ~5 lines.
2. **D2-L3** — `shrink_to` on `tlas_instances_scratch` after peak. ~2 lines.
3. **D2-L4** — `VK_EXT_memory_budget` threshold. ~20 lines.
4. **D3-M2** — delete / fix mismatched `debug_assert!` tuple. Or replace with
   unit test on `build_render_data`.
5. **D3-L4** — remove unconditional `cmd_bind_pipeline` at pass begin.
6. **D5-L1** — `to_triangles` capacity hint (P5-06). 1 line.
7. **D6-L1** — hoist `palette_scratch` out of `build_render_data`.
8. **D1-L1** — verify G-buffer mesh-ID actually is R16_UINT (close as fixed
   if so); otherwise downsize.

### Medium effort (1–4 hours)

9. **D1-H1** — tighten TLAS build barrier. Biggest GPU win.
10. **D1-M3** — add `layout(early_fragment_tests)` to `triangle.frag`.
11. **D3-M1** — add `(src_blend, dst_blend)` to blended sort key.
12. **D2-M3** — promote terrain tile SSBO to DEVICE_LOCAL single-buffer.
13. **D2-M2** — persistent `terrain_tile_scratch`.
14. **D2-E1** — mid-batch BLAS eviction during cell-load.
15. **D5-M1** (#P5-03) — `ImportedNode/Mesh.name` to `Arc<str>`.
16. **D1-H2** — investigate + fix per-draw descriptor-set rebind
    (confirm-then-fix; conflicts between Dim 1 and Dim 3 reports).
17. **D1-M1** — SVGF history barrier `SHADER_WRITE_BIT`.
18. **D1-M2** — tighten composite subpass dependency.

### Larger effort (4–8 hours)

19. **D2-H1** (#239) — thread `StagingPool` through `Texture::from_*`.
    Biggest cell-load win (150–200 MB transient, dominant lock contention).
20. **D2-M1** — BLAS scratch shrink policy. Biggest steady-state VRAM win
    (80–200 MB on a 6 GB GPU).
21. **D4-E1** — `World::insert_batch` to halve cell-load lock thrash.
22. **D5-M2** (P5-04) — bulk-read methods for geometry arrays.

### Architectural / deferred

23. **D4-M1** — `RenderExtract` stage (pairs with M40 parallel scheduler).
24. **D4-M2** — persistent `World::view_N` cache (only if a hot-loop system
    demands it).
25. **D5-E1** — NIF arena allocator (only if parse becomes the bottleneck).
26. **D1-L2** — `MAX_FRAMES_IN_FLIGHT = 3`.

### Nice-to-have / defer

27. **D3-E1, D3-E2, D4-L1, D4-L2, D4-L3, D4-E2, D6-E1**.

---

## Estimated Aggregate Impact

| Category | Quick wins | Medium | Large |
|---|---|---|---|
| GPU frame time | –1–2 ms | –0.5 ms | — |
| CPU frame time | –0.1 ms | –0.1 ms | — |
| VRAM steady state | –0.6 MB | –0.5 MB | –80–200 MB |
| VRAM cell-load peak | — | — | –150–200 MB |
| Cell-load time | — | — | –30 ms |

**Combined (everything lands)**: ~1.5–2.5 ms/frame GPU recovery,
~100–400 MB VRAM headroom on a 6 GB GPU, ~30 ms off cell-load.

---

## Cross-Dimension Dedup Notes

- **D2-M2 vs D6-L1** — different scratch buffers (`terrain_tile_scratch` vs
  `palette_scratch`); both legitimate.
- **D1-H2 (descriptor rebind)** conflicts with D3's "descriptor sets bound
  once" claim. **Triage before acting** — grep
  `cmd_bind_descriptor_sets` in `draw.rs`.
- **D1-L1 (G-buffer formats)** reports R16G16B16A16_SFLOAT; D2 reports
  R16_UINT / B10G11R11. Verify by opening `gbuffer.rs`.
- **D2-L2 subsumed by D2-M3** — same fix.
- **D5-M1, D5-M2, D5-L1, D5-E1** all carry over from 2026-04-13b under
  prior P5-* IDs — renumbered here for the 04-20 report.

---

## Confirmed Landed Since 2026-04-13b

- **Allocator + sync**: P2-07 (range flush), P2-08 (reusable fence),
  P2-09 (sized TLAS barrier), P2-10 (tuned block sizes), P1-09 (TLAS
  FAST_TRACE + UPDATE).
- **Rendering**: #272 (instance merge), #309 (multi-draw indirect),
  #294 (global VB/IB), #306 (IEEE sortable key), #392 (blend pipeline
  cache), #398 (extended dynamic depth state), #470 (terrain splat).
- **ECS / hot path**: #243 (name index), #249 (transform dirty bit),
  #278 (subtree cache), #287 (scheduler order), #464 (BFS propagation).
- **NIF**: #408 (allocate_vec sweep), #254 (string read optimization),
  #251–#252 (animation scratch buffers), #381 (process-lifetime NIF cache).

---

*Next step*: `/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-04-20.md`
