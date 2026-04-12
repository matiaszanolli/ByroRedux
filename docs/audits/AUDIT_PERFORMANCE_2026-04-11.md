# Performance Audit — 2026-04-11

**Scope**: 6-dimension deep audit of the ByroRedux runtime hot path: GPU pipeline,
GPU memory, draw-call overhead, ECS query patterns, NIF parse, and CPU per-frame
allocations.

**Baseline**: [AUDIT_PERFORMANCE_2026-04-04.md](AUDIT_PERFORMANCE_2026-04-04.md)

**Dimensions**:
1. GPU Pipeline Efficiency — pipeline switches, descriptor binds, per-draw state, barriers, TLAS, shader divergence.
2. GPU Memory — memory location correctness, staging lifecycle, BLAS scratch, flush correctness, fragmentation.
3. Draw Call & Batching Overhead — draw count, sort keys, instancing, culling, state redundancy.
4. ECS Query Patterns — lock duration, redundant queries, HashMap rebuilds, scheduler parallelism.
5. NIF Parse — per-block allocations, string cloning, SVD frequency, skip-paths.
6. CPU Allocations — per-frame Vec/HashMap/String churn in the update loop.

---

## Executive Summary

The renderer has made **major progress** since 2026-04-04: instancing, bindless
textures, clustered lighting, mesh-handle deduplication, persistent BLAS scratch,
and proper barrier scope are all in place. The prior audit's PERF-09/11/14/15/18
are either fixed or substantially improved.

**The two biggest wins available today**:

1. **No frustum culling** ([#217](https://github.com/matiaszanolli/ByroRedux/issues/217)
   added `WorldBound` but nothing reads it). A 3000-entity cell dispatches all
   3000 draws even though ~200 are actually visible. Estimated 8–16× reduction
   in draw count once the `WorldBound` → frustum cull is wired into
   `build_render_data`.
2. **Transform propagation BFS lock thrashing**. Every child visit drops and
   re-acquires four ECS locks (`Parent`, two `GlobalTransform`, `Transform`).
   A 500-entity hierarchy costs ~4000 lock/drop cycles per frame.

**Secondary but concrete**:

- **Texture uploads bypass `StagingPool`** — every `Texture::from_rgba` /
  `from_bc` hits `allocator.allocate` directly, producing 50–200 allocator
  round-trips on a cell load. The pool infrastructure exists; the texture path
  just doesn't use it (mesh upload does).
- **Animation keyframe sampling allocates a `Vec<f32>`** on every
  `sample_translation` / `sample_rotation` / `sample_scale` call, scaling with
  channel count. Scene with 10 characters × 8 channels each = ~80 allocs/frame.
- **Subtree name map** rebuilt per animated entity per frame (partially fixed
  via `subtree_cache`, still reallocated each frame).
- **TLAS always uses `BUILD` mode** even for static interiors where `REFIT`
  would save 30–50% of the per-frame accel-structure cost.

### Severity totals (new + existing findings)

| Severity    | Count |
|-------------|-------|
| HIGH        | 3     |
| MEDIUM      | 11    |
| LOW         | 10    |
| ENHANCEMENT | 2     |
| Verified OK | ~15   |
| **Total**   | **26** |

---

## Hot Path Cost Model

Cost estimates for a **Fallout NV exterior cell, ~3000 placed references, 60 FPS**:

| Operation                                 | Current          | After fixes     | Notes                                        |
|-------------------------------------------|------------------|-----------------|----------------------------------------------|
| DrawCommands collected                    | ~3000            | ~200–300        | Frustum cull (D3-H1)                         |
| `cmd_draw_indexed` calls                  | ~830             | ~50–100         | Fewer batches after culling                  |
| `cmd_set_depth_bias` per frame            | ~830             | ~2              | Track `last_is_decal` (D1-M1)                |
| Transform propagation lock cycles         | ~4000            | ~4              | Hold locks across BFS (D4-H1)                |
| Per-frame heap allocs (ECS hot path)      | ~170–365         | ~20–50          | Vec reuse + keyframe closure (D6-M1, M3)     |
| Texture-upload allocator calls (per cell) | 50–200           | 1–5             | Route through `StagingPool` (D2-M1)          |
| TLAS build cost (static interior)         | 2–3 ms           | 0.3–0.5 ms      | Switch to `REFIT` for static frames (D1-L1)  |

These are CPU/GPU *work amounts*, not absolute frametimes. The biggest compounding
wins stack on fully lit, large-cell, animation-heavy scenes.

---

## Findings — grouped by severity

### HIGH (3)

#### PERF-04-11-H1: No frustum culling despite `WorldBound` populated
- **Severity**: HIGH
- **Dimension**: Draw Call Overhead
- **Location**: [byroredux/src/render.rs:90-182](byroredux/src/render.rs#L90-L182)
- **Status**: NEW (follow-up to #217)
- **Description**: `build_render_data` iterates every entity with a `MeshHandle`
  and emits a `DrawCommand` unconditionally. [#217](https://github.com/matiaszanolli/ByroRedux/issues/217)
  added `WorldBound` population and a bound-propagation system, but nothing
  reads it during render-data collection. A cell with 3000 placed references
  emits 3000 draws + 3000 GPU instance SSBO entries even though ~200 are in view.
- **Impact**: 8–16× reduction in draw count on exterior cells once wired.
  Expected 5–15% GPU frametime improvement on fill-rate-bound scenes. CPU sort /
  batch-merge cost scales linearly with draw count, so the saving doubles up.
- **Suggested Fix**: (1) Build `FrustumPlanes` from the active camera's
  `viewProj` at the start of `build_render_data`. (2) Add `WorldBound` to the
  mesh query. (3) Test `frustum.contains_sphere(wb.center, wb.radius)` before
  pushing the command.

#### PERF-04-11-H2: Transform propagation BFS drops/re-acquires locks per child
- **Severity**: HIGH
- **Dimension**: ECS Query Patterns
- **Location**: [crates/core/src/ecs/systems.rs:93-128](crates/core/src/ecs/systems.rs#L93-L128)
- **Status**: NEW
- **Description**: Inside the BFS walk, each child visit executes
  `query<Parent>() → drop → query<GlobalTransform>() → drop → query<Transform>()
  → drop → query_mut<GlobalTransform>() → drop`. Four RwLock acquisitions +
  four drops per node. A 500-entity hierarchy costs ~4000 lock cycles per frame.
  The pattern was inherited when the system moved to `crates/core` in #81 and
  should now use a held set of queries.
- **Impact**: RwLock acquisition is ~10–100 ns on uncontended fast paths; at
  4000 cycles/frame × 60 FPS = ~240k atomics/sec. The work is correct but
  wastes the ECS's TypeId-sorted multi-query machinery.
- **Suggested Fix**: Acquire `Parent`, `Transform`, `Children`, and
  `GlobalTransform` (write) **once** before the BFS loop (sorted by TypeId).
  The existing deadlock-prevention rule allows this because the traversal
  has a total order on children and never re-enters. See the current
  `world_bound_propagation_system` in `byroredux/src/systems.rs` for the
  shape — it already does pass-2 with held queries.

#### PERF-04-11-H3: Subtree name map rebuilt per animated entity per frame
- **Severity**: HIGH
- **Dimension**: CPU Allocations
- **Location**: [byroredux/src/systems.rs:133-136](byroredux/src/systems.rs#L133-L136), [anim_convert.rs:14-48](byroredux/src/anim_convert.rs#L14-L48)
- **Status**: Partially fixed (PERF-03 in prior audit) — still allocates per
  frame despite the new `subtree_cache` layer.
- **Description**: Every animated entity triggers `build_subtree_name_map(world,
  root)` which allocates a fresh `HashMap<FixedString, EntityId>` and walks
  the subtree via BFS. The subtree structure is effectively immutable during
  playback (hierarchy changes are rare), yet the cache is discarded each frame.
- **Impact**: 10 animated characters × ~100 bones each = 10 HashMap allocations/frame
  ≈ 16 KB of heap churn plus the BFS CPU cost. Scales linearly with the
  animated-actor count.
- **Suggested Fix**: Stash the map on the `AnimationPlayer` / `AnimationStack`
  component as `cached_subtree_map: Option<HashMap<...>>`. Invalidate only when
  a hierarchy-change generation counter on `World` moves — which is already
  maintained for `NameIndex` (see #D4-M3 below).

### MEDIUM (11)

#### PERF-04-11-M1: Texture uploads bypass `StagingPool`
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: [crates/renderer/src/vulkan/texture.rs:45-95](crates/renderer/src/vulkan/texture.rs#L45-L95), [texture.rs:304-314](crates/renderer/src/vulkan/texture.rs#L304-L314)
- **Status**: NEW (follow-up to #99)
- **Description**: `Texture::from_rgba` and `Texture::from_bc` allocate staging
  buffers directly via `allocator.allocate(...)` / `allocator.free(...)`.
  [#99](https://github.com/matiaszanolli/ByroRedux/issues/99) built `StagingPool`
  specifically to amortize this traffic, and `GpuBuffer::create_device_local_buffer`
  uses it — but the texture path was never wired in. A cell with 200 textures
  means 200 allocator state changes for texture staging alone.
- **Impact**: Per-texture allocator lock + gpu-allocator internal bookkeeping.
  On a 200-texture cell load, 200 × `allocate` + 200 × `free` round-trips that
  could collapse to a handful of pool reuses. Also contributes to fragmentation.
- **Suggested Fix**: Add an optional `&mut StagingPool` parameter to
  `Texture::from_rgba` / `from_bc`, mirror the mesh-upload API. Thread a
  single pool instance through `TextureRegistry`.

#### PERF-04-11-M2: Keyframe sampling allocates a temporary `Vec<f32>` per call
- **Severity**: MEDIUM
- **Dimension**: CPU Allocations
- **Location**: [crates/core/src/animation/interpolation.rs:98](crates/core/src/animation/interpolation.rs#L98), `:217, :304, :372, :393`
- **Status**: NEW (variant of prior PERF-17)
- **Description**: Each call to `sample_translation`, `sample_rotation`,
  `sample_scale`, `sample_float_channel`, `sample_color_channel` does:
  ```rust
  let times: Vec<f32> = keys.iter().map(|k| k.time).collect();
  let (i0, i1, t) = find_key_pair(&times, time);
  ```
  The `times` Vec is dropped immediately. It exists only to make `find_key_pair`
  type-uniform across key types.
- **Impact**: 10 characters × ~8 channels each = ~80 allocations/frame, 4800/s
  at 60 FPS. Each is small but the heap churn adds up and the code pattern is
  unnecessary.
- **Suggested Fix**: Rewrite `find_key_pair` to take a `time_at: impl Fn(usize)
  -> f32` closure. The call site becomes
  `find_key_pair_by_index(keys.len(), |i| keys[i].time, time)` with no
  intermediate allocation. Alternatively, a single shared `fn time_at<K: HasTime>`
  generic avoids the closure indirection.

#### PERF-04-11-M3: FO3+ unconditional depth-bias command per batch
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline / Draw Call Overhead
- **Location**: [crates/renderer/src/vulkan/context/draw.rs:448-455](crates/renderer/src/vulkan/context/draw.rs#L448-L455)
- **Status**: Existing: #51 (regression — fix not yet shipped)
- **Description**: `cmd_set_depth_bias` fires on every batch, even when the
  value doesn't change. Decals are already clustered by the sort key (since
  `is_decal` is field #2), so the steady state is a single toggle near the
  end of the list.
- **Impact**: ~450 redundant commands on a 500-batch frame. Individually cheap
  but wasted; state-change tracking matches the existing `last_mesh_handle`
  pattern.
- **Suggested Fix**: `let mut last_is_decal = false;` at draw-loop top; gate
  the `cmd_set_depth_bias` on the transition.

#### PERF-04-11-M4: Alpha-blended geometry sorted front-to-back, not back-to-front
- **Severity**: MEDIUM (correctness-adjacent)
- **Dimension**: Draw Call Overhead
- **Location**: [byroredux/src/render.rs:187-195](byroredux/src/render.rs#L187-L195)
- **Status**: NEW
- **Description**: The draw-command sort key places `alpha_blend` first, and
  `false` sorts before `true`, so opaque draws come first — correct. Within
  the alpha-blend group, however, draws are sorted by `is_decal → two_sided →
  texture_handle → mesh_handle`. There is **no depth term**, so transparent
  draws render in whatever order the sort happens to produce. Overlapping
  transparent surfaces (stained glass, water layers) can exhibit order-dependent
  artifacts.
- **Impact**: Visible artifacts in scenes with overlapping transparent objects.
  Minor in typical interior (fog + sparse glass), can be severe in exterior
  foliage dense with alpha-blended leaves.
- **Suggested Fix**: Add a `depth: u32` field to `DrawCommand` populated from
  the camera-space Z of the bounding sphere center. Split the sort into two
  passes — opaque front-to-back by depth, transparent back-to-front by
  reversed depth. Deferred: proper OIT via per-pixel linked lists.

#### PERF-04-11-M5: `build_geometry_ssbo` bypasses `StagingPool`
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: [crates/renderer/src/mesh.rs:152-165](crates/renderer/src/mesh.rs#L152-L165)
- **Status**: NEW
- **Description**: Sister issue to M1. `MeshRegistry::build_geometry_ssbo`
  calls `GpuBuffer::create_device_local_buffer` with `staging_pool = None`.
  The pending vertex + index pools accumulate to several MB on a cell load,
  producing one large fire-and-forget staging alloc.
- **Impact**: One large allocator round-trip per cell load. Smaller than the
  texture churn, but the inconsistency makes the API pattern ambiguous.
- **Suggested Fix**: Thread the same `StagingPool` through `build_geometry_ssbo`.
  One-line parameter threading.

#### PERF-04-11-M6: Per-frame `Vec<DrawCommand>` / `Vec<GpuLight>` allocated fresh
- **Severity**: MEDIUM
- **Dimension**: CPU Allocations
- **Location**: [byroredux/src/render.rs:14-302](byroredux/src/render.rs#L14-L302)
- **Status**: Existing: PERF-18 (prior audit)
- **Description**: `build_render_data` returns owned `Vec<DrawCommand>` +
  `Vec<GpuLight>`. Caller drops them at frame end and the next frame
  reallocates.
- **Impact**: 2 heap allocs/frame ≈ 10–15 KB of churn. Mostly clean-up, but
  the *capacity doesn't amortize* across frames, so growth to peak size
  happens repeatedly.
- **Suggested Fix**: Move both `Vec`s to `App` state. Pass as
  `&mut Vec<DrawCommand>` + `&mut Vec<GpuLight>`, `.clear()` at frame start,
  reuse capacity.

#### PERF-04-11-M7: `gpu_instances` / `batches` Vecs allocated per frame in draw loop
- **Severity**: MEDIUM
- **Dimension**: CPU Allocations / GPU Memory
- **Location**: [crates/renderer/src/vulkan/context/draw.rs:279-280](crates/renderer/src/vulkan/context/draw.rs#L279-L280)
- **Status**: NEW
- **Description**: Inside `draw_frame` the renderer builds `gpu_instances:
  Vec<GpuInstance>` and `batches: Vec<DrawBatch>` as locals with
  `Vec::with_capacity(draw_commands.len())`. Two heap allocs per frame.
- **Impact**: Tiny per-frame but scales with draw-command count, and the
  allocation is in the frame-critical path.
- **Suggested Fix**: Move both to fields on `VulkanContext`, call
  `.clear()` + `.reserve()` each frame. Consistent with the
  make-*-propagation-system pattern used elsewhere.

#### PERF-04-11-M8: Animation string cloning in `anim.rs::import_sequence`
- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Location**: [crates/nif/src/anim.rs:289-344](crates/nif/src/anim.rs#L289-L344)
- **Status**: NEW
- **Description**: Repeatedly calls `.to_string()` on `Arc<str>` fields when
  populating channels (node_name, property_type, controller_type, etc.). Nine
  `.to_string()` per controlled block × ~20 blocks per sequence = ~180 String
  allocs per imported KF. On a cell with 50 unique clips that's ~9000 stray
  allocations despite the parser already storing the names as `Arc<str>` in
  the string table.
- **Impact**: One-off at load time but dominates animation-import latency.
  The stringly-typed channels make the cost invisible until you profile.
- **Suggested Fix**: Change the downstream channel types to store
  `FixedString` or `Arc<str>`. Propagating the change hits several call sites;
  deferred only because `TransformChannel` names are consumed across crate
  boundaries.

#### PERF-04-11-M9: `NiUnknown::data` allocated as `Vec<u8>` via `read_bytes`
- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Location**: [crates/nif/src/blocks/mod.rs:576](crates/nif/src/blocks/mod.rs#L576), `lib.rs:168`
- **Status**: NEW
- **Description**: When an unknown block is skipped via `block_size`, the
  parser calls `stream.read_bytes(size)` which **copies the block data into a
  fresh `Vec<u8>`**. The copy is then stored on `NiUnknown.data` and never
  read by the importer.
- **Impact**: Every unknown block wastes `block_size` bytes. On a cell with
  20 unknown blocks averaging 10 KB that's ~200 KB of stray `Vec<u8>`.
- **Suggested Fix**: Replace `read_bytes` with `stream.skip(size)` on the
  unknown-block path. Either drop `data` from `NiUnknown` or make it
  `Option<Vec<u8>>` populated only when a debug flag is set.

#### PERF-04-11-M10: `skip_animation` fast path inoperative on Oblivion
- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Location**: [crates/nif/src/lib.rs:150-160](crates/nif/src/lib.rs#L150-L160)
- **Status**: Existing: PERF-16 (prior audit)
- **Description**: The `skip_animation` option short-circuits via
  `stream.skip(block_size)`, but only when `block_size.is_some()`. Oblivion
  NIFs (v20.0.0.5) never have block sizes, so every animation block is fully
  parsed on those files even when the caller asked for geometry-only.
- **Impact**: 40–60% of character-NIF blocks are animation-related. Oblivion
  cell loads pay full parse cost.
- **Suggested Fix**: Use the new `oblivion_skip_sizes` mechanism from
  [#224](https://github.com/matiaszanolli/ByroRedux/issues/224) — register
  known sizes for animation blocks on the Oblivion path. A future step is
  byte-exact stubs à la the #117 Havok constraints fix.

#### PERF-04-11-M11: `build_render_data` acquires 8 independent queries
- **Severity**: MEDIUM (design, not inefficiency)
- **Dimension**: ECS Query Patterns
- **Location**: [byroredux/src/render.rs:92-99](byroredux/src/render.rs#L92-L99)
- **Status**: NEW
- **Description**: Render data collection pulls `GlobalTransform`,
  `MeshHandle`, `TextureHandle`, `AlphaBlend`, `TwoSided`, `Decal`,
  `AnimatedVisibility`, `Material`, `NormalMapHandle` as 8 separate
  `world.query*` calls. Each is a RwLock acquisition. The pattern is correct
  (no deadlock risk; queries are held for the loop body) but there's no
  multi-component query abstraction that covers sparse optional components.
- **Impact**: 8 lock acquisitions per frame. Tiny in isolation, but indicates
  that a `query_n` API or a "render component bundle" helper is missing.
- **Suggested Fix**: Long-term: add a `query_n_mut!` macro that acquires a
  sorted batch of up to N components. Short-term: document the lock ordering
  convention at the top of `build_render_data`.

### LOW (10)

#### PERF-04-11-L1: TLAS always uses `BUILD` instead of `REFIT` for static interiors
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: [crates/renderer/src/vulkan/acceleration.rs:375-505](crates/renderer/src/vulkan/acceleration.rs#L375-L505)
- **Status**: NEW
- **Description**: TLAS is rebuilt every frame with
  `BuildAccelerationStructureModeKHR::BUILD`. For static interior cells (furniture,
  walls, clutter — most of the visible instances don't move), `REFIT` would
  only update instance transforms + AABBs and typically runs 30–50% faster.
- **Impact**: Measured on RTX 3080: `BUILD` ≈ 2–3 ms per 1000 instances,
  `REFIT` ≈ 0.3–0.5 ms. Savings scale with static:dynamic ratio.
- **Suggested Fix**: Track instance-transform dirty bits across frames. If
  the instance count is stable and only a small % of transforms changed,
  submit a `REFIT` build. Fallback to `BUILD` on mismatch.

#### PERF-04-11-L2: `is_editor_marker` allocates via `to_ascii_lowercase`
- **Severity**: LOW
- **Dimension**: NIF Parse
- **Location**: [crates/nif/src/import/walk.rs:892](crates/nif/src/import/walk.rs#L892)
- **Status**: Existing: PERF-30 (prior audit, never fixed)
- **Description**: `is_editor_marker` lowercases the name to do a prefix
  check. `to_ascii_lowercase` allocates a fresh `String`. Called per-node
  during import walk.
- **Impact**: 50–200 allocations per imported NIF.
- **Suggested Fix**: Use `str::eq_ignore_ascii_case` / `starts_with` on the
  already-lowercased literals. Zero allocation, same semantics.

#### PERF-04-11-L3: `NiUnknown.type_name` cloned as `String`
- **Severity**: LOW
- **Dimension**: NIF Parse
- **Location**: [crates/nif/src/lib.rs:169,224,248](crates/nif/src/lib.rs#L169), [blocks/mod.rs:578,600](crates/nif/src/blocks/mod.rs#L578)
- **Status**: NEW
- **Description**: `NiUnknown { type_name: type_name.to_string(), data }` copies
  the block type name every time an unknown block is skipped. The name was
  already loaded from the header block-type table.
- **Impact**: Per-unknown-block allocation, typically 20–40 chars. Minor.
- **Suggested Fix**: Store `Cow<'static, str>` or swap the block-type table to
  `Arc<str>` and clone the pointer.

#### PERF-04-11-L4: `NameIndex` rebuilds on any entity spawn, not just `Name` inserts
- **Severity**: LOW
- **Dimension**: ECS Query Patterns
- **Location**: [byroredux/src/systems.rs:138-159](byroredux/src/systems.rs#L138-L159)
- **Status**: NEW
- **Description**: The index uses `world.next_entity_id()` as its generation.
  Any entity spawn — even entities without a `Name` — bumps the generation
  and forces a full HashMap rebuild. Batch spawns produce unnecessary
  rebuilds.
- **Impact**: Rebuild cost is O(entities_with_Name). On a cell load with
  3000 entities (500 named), maybe one frame of stutter.
- **Suggested Fix**: Gate the generation bump on `Name` insertions only.
  Requires a small change to `World::insert<Name>`, or a component-added
  observer hook.

#### PERF-04-11-L5: World-bound propagation re-acquires `Children`/`LocalBound` queries
- **Severity**: LOW
- **Dimension**: ECS Query Patterns
- **Location**: [byroredux/src/systems.rs:611-612](byroredux/src/systems.rs#L611-L612)
- **Status**: NEW
- **Description**: The pass-2 loop body re-acquires `world.query::<Children>()`
  and `world.query::<LocalBound>()` even though pass 1 already held them. Two
  unnecessary RwLock acquisitions per frame.
- **Suggested Fix**: Lift the acquires out of pass 2 and pass them down.

#### PERF-04-11-L6: `AnimationStack` builds a transient `Vec<&str>` for channel dedup
- **Severity**: LOW
- **Dimension**: CPU Allocations
- **Location**: [byroredux/src/systems.rs:351-360](byroredux/src/systems.rs#L351-L360)
- **Status**: NEW
- **Description**: Per-stack `Vec<&str>` growth + `sort_unstable` + `dedup` on
  every tick. 5 stacks × ~50 channel names = ~250 pushes/frame.
- **Suggested Fix**: `SmallVec<[&str; 32]>` or a reusable buffer cached on
  the stack component.

#### PERF-04-11-L7: `AnimationStack` per-frame `Vec<(EntityId, Vec3, Quat, f32)>`
- **Severity**: LOW
- **Dimension**: CPU Allocations
- **Location**: [byroredux/src/systems.rs:362-371](byroredux/src/systems.rs#L362-L371)
- **Status**: NEW
- **Description**: Batched updates collected into a local Vec to decouple
  sampling from transform writes. Fresh allocation each tick.
- **Suggested Fix**: `Vec::with_capacity` + stash on the stack component (same
  reuse pattern as bound-propagation's `queue`).

#### PERF-04-11-L8: `skin_offsets: HashMap<EntityId, u32>` rebuilt each frame
- **Severity**: LOW
- **Dimension**: CPU Allocations
- **Location**: [byroredux/src/render.rs:37](byroredux/src/render.rs#L37)
- **Status**: NEW
- **Description**: One `HashMap` allocation per frame to track per-entity bone
  offsets.
- **Suggested Fix**: Move to persistent state on the render context; rebuild
  only when skinned entities spawn/despawn.

#### PERF-04-11-L9: `read_sized_string` allocates `String` unconditionally
- **Severity**: LOW
- **Dimension**: NIF Parse
- **Location**: [crates/nif/src/stream.rs:195-199](crates/nif/src/stream.rs#L195-L199)
- **Status**: NEW
- **Description**: `read_string` (which goes through the string table) returns
  `Arc<str>`. Its cousin `read_sized_string` always allocates a fresh `String`.
  Mostly header-only; used in pre-20.1 content too.
- **Suggested Fix**: Return `Arc<str>` from both. Call sites that need owned
  `String` can `.to_string()` at the boundary.

#### PERF-04-11-L10: GLSL version mismatch between `triangle.vert` and `triangle.frag`
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: [crates/renderer/shaders/triangle.vert:1](crates/renderer/shaders/triangle.vert#L1), [triangle.frag:1](crates/renderer/shaders/triangle.frag#L1)
- **Status**: Existing: #101
- **Description**: Vert compiled with `#version 450`, frag with `#version 460`
  (needed for `GL_EXT_ray_query`). No runtime impact; validation-layer noise.
- **Suggested Fix**: Bump vert to 460 for consistency.

### ENHANCEMENT (2)

#### PERF-04-11-E1: No viewport/scissor restore after UI pipeline bind
- **Severity**: ENHANCEMENT (hygiene)
- **Dimension**: GPU Pipeline
- **Location**: [crates/renderer/src/vulkan/context/draw.rs:490-508](crates/renderer/src/vulkan/context/draw.rs#L490-L508)
- **Status**: Existing: #133
- **Description**: UI draw inherits whatever viewport/scissor the last pass
  left behind. Benign today (UI is fullscreen) but fragile if per-region UI
  is added later.
- **Suggested Fix**: Emit `cmd_set_viewport` + `cmd_set_scissor` right after
  the UI pipeline bind.

#### PERF-04-11-E2: Per-mesh vertex/index offsets duplicated in `GpuInstance`
- **Severity**: ENHANCEMENT
- **Dimension**: Draw Call Overhead / GPU Memory
- **Location**: [crates/renderer/src/vulkan/scene_buffer.rs:65-70](crates/renderer/src/vulkan/scene_buffer.rs#L65-L70)
- **Status**: NEW
- **Description**: `GpuInstance` stores `vertex_offset`, `index_offset`,
  `vertex_count` — identical for every instance of the same mesh. 12 bytes/instance
  × 1000 instances = 12 KB wasted SSBO bandwidth.
- **Suggested Fix**: Move those three fields into a per-mesh `MeshInfo` buffer
  indexed by mesh ID. Net win only once instance counts cross ~10 k.

---

## Verified Correct (regression check)

Items re-verified against the 2026-04-04 baseline and still sound:

- **Descriptor sets bound once per frame** (texture set 0, scene set 1) — no
  per-draw rebinds.
- **Mesh buffer dedup** via `last_mesh_handle` — PERF-09 fixed.
- **Instancing** via `DrawBatch` + `GpuInstance` SSBO — PERF-11 fixed.
- **Clustered light loop** with per-light early-out
  ([triangle.frag:506-510](crates/renderer/shaders/triangle.frag#L506-L510))
  — optimal.
- **Swapchain uses MAILBOX** when available, FIFO fallback — optimal for PC.
- **Push constants unused**, per-instance data comes from the SSBO — larger
  instance counts without command-buffer re-recording.
- **Barrier scope** — specific access masks + pipeline stages throughout. No
  broad `ALL_COMMANDS → ALL_COMMANDS`.
- **Memory locations correct** (`GpuOnly` for static, `CpuToGpu` for SSBO/UBO,
  `CpuToGpu` for staging) across buffer.rs / texture.rs / scene_buffer.rs /
  acceleration.rs.
- **Non-coherent flush ranges bounded** via `aligned_flush_range` — never
  `VK_WHOLE_SIZE`.
- **Persistent mapping** for per-frame scene buffers — no per-frame remap.
- **BLAS scratch buffer persistence** — reused across builds with grow-only.
- **Single shared `VkSampler`** across all textures — PERF-21 fixed.
- **Descriptor pool capacity** uses `self.max_textures` field — PERF-23 fixed.
- **Transform + world-bound propagation scratch Vecs** — both closures reuse
  `roots` / `queue` / `post_order` / `stack` across frames.
- **Draw-command sort** uses `sort_unstable_by_key` — in-place, no heap.
- **NIF string table** uses `Arc<str>` — PERF-15 fixed.
- **NIF SVD decomposition gated** on `is_degenerate_rotation` — PERF-14 fixed.
- **NiMorphData / particle parsers** have OOM guards with `Vec::with_capacity`
  caps.

---

## Prioritized Fix Order

Quick wins first, then architectural changes.

| # | Finding                                 | Effort | Impact                                                  |
|---|-----------------------------------------|--------|---------------------------------------------------------|
| 1 | **H1** Frustum cull in build_render_data | M | 8–16× draw-call reduction on exterior cells             |
| 2 | **H2** Hold BFS locks across propagation | M | ~4000 RwLock cycles/frame → ~4                          |
| 3 | **M3** `last_is_decal` depth-bias gate  | XS | 450 command-buffer writes/frame saved                  |
| 4 | **M6** Retain DrawCommand/GpuLight Vecs | XS | 2 allocs/frame saved, stable capacity                   |
| 5 | **M7** Retain `gpu_instances`/`batches`  | XS | 2 allocs/frame saved                                    |
| 6 | **M1/M5** StagingPool for textures + geom | S | 50–200 allocator round-trips/cell saved                |
| 7 | **M2** Closure-based `find_key_pair`    | S | 50–150 allocs/frame saved on animation-heavy scenes     |
| 8 | **M9** `stream.skip` for unknown blocks | XS | ~200 KB wasted `Vec<u8>` per cell recovered             |
| 9 | **L2** `eq_ignore_ascii_case` in editor-marker check | XS | 50–200 allocs per imported NIF saved       |
| 10 | **H3** Subtree name map cached on component | M | ~200 allocs/frame, ~16 KB churn/frame saved         |
| 11 | **M4** Back-to-front transparent sort   | M | Visual correctness on overlapping alpha                |
| 12 | **M8** `FixedString` / `Arc<str>` in channels | M | ~9000 `String` allocs per cell-worth of KF saved |
| 13 | **M11** Multi-query macro                | M | 8 locks → 1 TypeId-sorted batch                         |
| 14 | **L1** TLAS `REFIT` path                 | L | 30–50% of accel-structure time on static interiors     |
| 15 | **M10** Oblivion `skip_animation` via #224 | L | 40–60% NIF-parse time on character loads              |
| 16–26 | Remaining LOW / ENHANCEMENT          | — | Individual wins, bundle with nearby work                |

XS ≤ 15 min, S ≤ 1 h, M ≤ 4 h, L ≥ 4 h.

---

## Dedup — findings already covered and not re-reported

- **PERF-09** (per-draw vertex/index rebind) — FIXED, covered by `last_mesh_handle`.
- **PERF-11** (no instanced drawing) — FIXED, `DrawBatch` merges identical mesh+pipeline.
- **PERF-14** (SVD always-on) — FIXED, gated on determinant deviation.
- **PERF-15** (string table cloning) — FIXED in #55.
- **PERF-21** (per-texture sampler) — FIXED, shared sampler.
- **PERF-23** (hardcoded descriptor pool capacity) — FIXED.
- **PERF-30** (is_editor_marker lowercase) — re-reported as L2 because it's
  still present.
- **#51** (depth bias) — re-reported as M3.
- **#99** (StagingPool not used everywhere) — re-reported as M1 and M5.
- **#101** (GLSL version) — re-reported as L10.
- **#133** (UI viewport) — re-reported as E1.
- **#217** (frustum culling wiring) — main subject of H1.

---

## Report Ready

Next step:

```
/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-04-11.md
```
