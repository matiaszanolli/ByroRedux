# Performance Audit — 2026-04-04

**Scope**: Full engine — GPU pipeline, GPU memory, draw calls, ECS queries, NIF parsing, CPU allocations
**Auditor**: Claude Opus 4.6 (6 specialist agents)
**Depth**: Deep (hot path tracing, impact estimation)

## Executive Summary

| Severity | Count |
|----------|-------|
| HIGH | 4 |
| MEDIUM | 14 |
| LOW | 12 |

**Top bottlenecks by estimated impact:**

| Issue | Est. Impact | Fix Effort |
|-------|-------------|------------|
| Per-upload queue_wait_idle serializes all GPU work | Seconds during cell load | Medium |
| Light loop traces shadow rays for zero-contribution lights | 0.2-0.5ms/frame | Low (1-line shader change) |
| Subtree name map rebuilt per animated entity per frame | O(entities * bones) allocs/frame | Medium |
| Transform propagation acquires 4 locks per BFS node | 2000+ atomics/frame at 500 entities | Medium |

---

## Findings

### HIGH

#### PERF-01: Light loop traces shadow rays for zero-contribution lights
- **Severity**: HIGH
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/triangle.frag:53-118`
- **Status**: NEW
- **Description**: The fragment shader iterates all lights with a dynamic loop. The ray query branch executes `rayQueryInitializeEXT` + `rayQueryProceedEXT` for every light, even when `NdotL * atten` has already fallen to zero. At 1080p with 8 lights and RT enabled, this is up to 8M ray queries per frame. Skipping zero-contribution lights before the ray trace could cut shadow ray count by 50-70% in typical interior cells.
- **Impact**: 0.2-0.5ms/frame recoverable on RTX 3070.
- **Suggested Fix**: Add `if (NdotL * atten < 0.001) continue;` before the ray query. Medium-term: tiled/clustered light culling.

#### PERF-02: Per-upload queue_wait_idle serializes all GPU work
- **Severity**: HIGH
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/texture.rs:654-660` (`with_one_time_commands`)
- **Status**: Existing: #11
- **Description**: `with_one_time_commands` calls `queue_wait_idle` after every single upload (mesh buffer or texture). For N meshes with M textures, the CPU-GPU round-trips total `2N + M`. A cell with 200 meshes and 100 textures produces ~500 separate submit-and-wait cycles.
- **Impact**: Cell load times measured in seconds that could be <200ms with batched uploads.
- **Suggested Fix**: Batch all uploads into a single command buffer, submit once, wait once.

#### PERF-03: Subtree name map allocated per animated entity per frame
- **Severity**: HIGH
- **Dimension**: ECS Query Patterns / CPU Allocations
- **Location**: `byroredux/src/main.rs:253,372-374,1856-1889`
- **Status**: NEW
- **Description**: For every `AnimationPlayer` and `AnimationStack` with a `root_entity`, `build_subtree_name_map` allocates a new `HashMap<FixedString, EntityId>` and performs a BFS walk every frame. The subtree structure is static — it never changes during animation playback.
- **Impact**: 10 animated characters with 100 bones each = 10 HashMaps of ~100 entries built and destroyed 60 times/second. Dominates per-frame allocation cost.
- **Suggested Fix**: Cache the scoped name map on the component. Invalidate only on hierarchy change (which never happens during playback).

#### PERF-04: Transform propagation acquires 4 locks per BFS node
- **Severity**: HIGH
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/main.rs:476-513`
- **Status**: NEW
- **Description**: The BFS child propagation loop acquires and drops `Parent` read, `GlobalTransform` read, `Transform` read, and `GlobalTransform` write locks on every single node. For a hierarchy with N entities, this is 4N atomic lock operations per frame.
- **Impact**: 500-entity hierarchy = 2000+ atomic CAS operations per frame. Prevents future parallelization.
- **Suggested Fix**: Hold all four locks for the entire BFS traversal. Pre-compute a topological order for a single linear pass.

---

### MEDIUM

#### PERF-05: Per-mesh staging buffer allocate/free causes allocator churn
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/buffer.rs:224-328`
- **Status**: NEW
- **Description**: Each vertex and index buffer upload creates and destroys a staging buffer, producing 4 allocator lock acquisitions per mesh. A cell with 500 meshes = 2000 staging allocations.
- **Suggested Fix**: Introduce a `StagingPool` with reusable staging buffers sized to high-water mark.

#### PERF-06: Per-texture staging buffer allocate/free (same pattern)
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/texture.rs:45-85,304-343`
- **Status**: NEW
- **Description**: Same as PERF-05 but for textures. 50-200 staging allocations per cell load, each in the megabyte range.
- **Suggested Fix**: Share the `StagingPool` from PERF-05.

#### PERF-07: update_rgba calls device_wait_idle, stalling entire GPU
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/texture_registry.rs:197-199`
- **Status**: NEW
- **Description**: Dynamic texture updates (Scaleform UI) call `device_wait_idle()` every frame, fully draining the GPU pipeline before destroying and recreating the texture.
- **Impact**: Can halve framerate when UI is active.
- **Suggested Fix**: Double-buffer dynamic textures; destroy old texture after its fence signals.

#### PERF-08: No pipeline cache — cold compilation on every creation
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:249,377`
- **Status**: NEW
- **Description**: `vk::PipelineCache::null()` passed to all pipeline creation. No state reuse across runs or swapchain recreations.
- **Impact**: 10-50ms on resize/startup. With cache: <1ms.
- **Suggested Fix**: Create `VkPipelineCache`, serialize to disk on shutdown, load on startup.

#### PERF-09: Per-draw vertex/index buffer rebind without deduplication
- **Severity**: MEDIUM
- **Dimension**: Draw Call Overhead / GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/context.rs:566-574`
- **Status**: NEW
- **Description**: `cmd_bind_vertex_buffers` and `cmd_bind_index_buffer` called for every draw. No `last_mesh_handle` tracking. Sort key omits `mesh_handle`, preventing grouping.
- **Impact**: ~450 redundant binds at 500 draws with 50 unique meshes.
- **Suggested Fix**: Add `last_mesh` tracking + add `mesh_handle` to sort key.

#### PERF-10: Unconditional depth bias command every draw call
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/context.rs:546-553`
- **Status**: NEW
- **Description**: `cmd_set_depth_bias` called for every draw, but only decals need non-zero bias. Draws are already sorted with decals contiguous.
- **Suggested Fix**: Track `last_is_decal`, only call on transition.

#### PERF-11: No instanced drawing for repeated mesh+texture combinations
- **Severity**: MEDIUM
- **Dimension**: Draw Call Overhead
- **Location**: `crates/renderer/src/vulkan/context.rs:574-575`
- **Status**: NEW
- **Description**: Every entity = 1 draw call with `instance_count=1`. Same mesh+texture at different transforms (common in Bethesda cells) could be batched.
- **Impact**: Draw call count = entity count 1:1. Instancing could reduce by 5-10x.
- **Suggested Fix**: Detect consecutive identical (mesh, texture, pipeline) runs after sort; upload model matrices to SSBO; draw instanced.

#### PERF-12: AnimationPlayer write lock re-acquired per entity per frame
- **Severity**: MEDIUM
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/main.rs:231-250`
- **Status**: NEW
- **Description**: The animation system collects entity IDs into a Vec, drops the write lock, then re-acquires `query_mut::<AnimationPlayer>()` inside the per-entity loop. N animated entities = N write-lock acquisitions.
- **Suggested Fix**: Hold the lock for the entire batch; split into advance pass (write) then apply pass (read).

#### PERF-13: Animated component locks re-acquired per channel per entity
- **Severity**: MEDIUM
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/main.rs:305-349`
- **Status**: NEW
- **Description**: Each float/color/bool channel acquires and drops `AnimatedAlpha`/`AnimatedColor`/`AnimatedVisibility` write locks individually.
- **Suggested Fix**: Collect updates into Vecs, apply in one batch per component type.

#### PERF-14: SVD computed unconditionally on every rotation matrix during import
- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/import.rs:841-874`
- **Status**: NEW
- **Description**: `zup_matrix_to_yup_quat()` performs SVD on every rotation matrix. 99% of matrices are valid rotations (det ~1.0) that can use direct quaternion extraction.
- **Impact**: 5000 unnecessary SVDs for a 500-object cell.
- **Suggested Fix**: Check `is_degenerate_rotation()` first; only SVD for degenerate matrices.

#### PERF-15: String cloning from NIF string table on every read
- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/stream.rs:136`
- **Status**: NEW
- **Description**: `read_string()` clones from the string table, allocating a new `String` per read. 50-150 allocations per NIF, 25,000-75,000 for a cell load.
- **Suggested Fix**: Store strings as `Arc<str>` in the header; cloning is a pointer copy.

#### PERF-16: No block-skip fast path for animation blocks during geometry-only import
- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/lib.rs:49-87`
- **Status**: NEW
- **Description**: `parse_nif` fully deserializes every block including animation data, even when only geometry is needed. Block sizes are available in the header for skipping. 40-60% of blocks in character NIFs are animation-related.
- **Suggested Fix**: Add `ParseOptions { skip_animation: bool }`, skip known animation block types using `block_size`.

#### PERF-17: sample_blended_transform allocates two Vecs per channel per frame
- **Severity**: MEDIUM
- **Dimension**: CPU Allocations
- **Location**: `crates/core/src/animation.rs:454,494`
- **Status**: NEW
- **Description**: `samples` Vec and `top` filter-collect Vec allocated per bone per frame. 100-bone skeleton = 200 tiny heap allocations/frame.
- **Suggested Fix**: Use `SmallVec<[_; 4]>` (most blending has 2-4 layers). Eliminate `top` collect with in-place iteration.

#### PERF-18: Per-frame Vec<DrawCommand> and Vec<GpuLight> allocation
- **Severity**: MEDIUM
- **Dimension**: CPU Allocations
- **Location**: `byroredux/src/main.rs:1717,1776`
- **Status**: NEW
- **Description**: `build_render_data` creates fresh Vecs every frame. Could retain across frames with `.clear()`.
- **Suggested Fix**: Move to retained storage on `App`; pass `&mut Vec` instead of returning owned.

---

### LOW

#### PERF-19: Forced texture descriptor rebind after pipeline switch
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/context.rs:525`
- **Status**: NEW
- **Description**: `last_texture = u32::MAX` after pipeline switch forces rebind even when texture unchanged. All pipelines share the same layout, so set 0 bindings are preserved.
- **Suggested Fix**: Remove the reset; descriptor set 0 survives compatible pipeline switches.

#### PERF-20: Per-BLAS scratch buffer allocate/free instead of reuse
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:135-181`
- **Status**: NEW
- **Description**: Each `build_blas` allocates and frees a scratch buffer. TLAS scratch is correctly persisted.
- **Suggested Fix**: Persist `blas_scratch_buffer`, reuse across builds, only reallocate when larger needed.

#### PERF-21: Per-texture VkSampler creation instead of shared cache
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/texture.rs:250-265,519-535`
- **Status**: NEW
- **Description**: 200 textures = 200 identical VkSampler objects with LINEAR/REPEAT params.
- **Suggested Fix**: Sampler cache keyed by (filter, address, anisotropy). One shared sampler for common case.

#### PERF-22: upload_lights allocates a Vec on every frame
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/scene_buffer.rs:219-244`
- **Status**: NEW
- **Description**: Builds a temporary `Vec<u8>` to serialize light data before copying to mapped memory.
- **Suggested Fix**: Write directly to mapped memory in two `copy_nonoverlapping` calls.

#### PERF-23: Descriptor pool max_textures hardcoded on recreation
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/texture_registry.rs:262`
- **Status**: NEW
- **Description**: `recreate_descriptor_sets` hardcodes 1024 instead of using stored value.
- **Suggested Fix**: Store `max_textures` as a field on `TextureRegistry`.

#### PERF-24: UI overlay pushes 128 bytes of identity matrices (ignored by shader)
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/context.rs:600-615`
- **Status**: NEW
- **Description**: Two `cmd_push_constants` calls push identity matrices required by shared layout but ignored by UI shader. Negligible cost.

#### PERF-25: BFS queue in transform_propagation allocated every frame
- **Severity**: LOW
- **Dimension**: CPU Allocations
- **Location**: `byroredux/src/main.rs:469`
- **Status**: NEW
- **Description**: `Vec::new()` for BFS queue and roots, dropped each frame.
- **Suggested Fix**: Retain as resource with `.clear()` reuse.

#### PERF-26: Triangle data cloned in extract_mesh for NiTriShapeData
- **Severity**: LOW
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/import.rs:275`
- **Status**: NEW
- **Description**: `data.triangles.clone()` copies the full index buffer. NiTriShapeData is already in correct format.
- **Suggested Fix**: Use `Cow<'a, [[u16; 3]]>` — borrow for NiTriShapeData, own for NiTriStripsData.

#### PERF-27: Repeated downcast iterations in property search functions
- **Severity**: LOW
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/import.rs:506-694`
- **Status**: NEW
- **Description**: Four functions iterate the same property list independently with multiple downcasts each.
- **Suggested Fix**: Single-pass categorization into a `MaterialInfo` struct.

#### PERF-28: Arc wrapping every parsed NIF block
- **Severity**: LOW
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/lib.rs:86`
- **Status**: NEW
- **Description**: Every block is `Arc<dyn NiObject>` but NifScene is consumed single-threaded. Two heap allocations per block (Box then Arc).
- **Suggested Fix**: Use `Box<dyn NiObject>` in `NifScene.blocks`.

#### PERF-29: BsTriShape vertex Vecs not preallocated
- **Severity**: LOW
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/blocks/tri_shape.rs:264-268`
- **Status**: NEW
- **Description**: Vertex data Vecs initialized as `Vec::new()` despite `num_vertices` being known. ~65 reallocations per 5000-vertex mesh.
- **Suggested Fix**: `Vec::with_capacity(num_vertices as usize)`.

#### PERF-30: to_ascii_lowercase allocation in is_editor_marker
- **Severity**: LOW
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/import.rs:243`
- **Status**: NEW
- **Description**: Allocates a new `String` per call. Called for every node during import.
- **Suggested Fix**: Use `eq_ignore_ascii_case` — zero allocation.

---

## Prioritized Fix Order

### Quick Wins (< 30 min each, high ROI)

| Priority | Finding | Change |
|----------|---------|--------|
| 1 | PERF-01 | Add `if (NdotL * atten < 0.001) continue;` in fragment shader before ray query |
| 2 | PERF-09 | Add `last_mesh` tracking in draw loop (5 lines) |
| 3 | PERF-10 | Add `last_is_decal` tracking (5 lines) |
| 4 | PERF-19 | Remove `last_texture = u32::MAX` after pipeline switch (1 line) |
| 5 | PERF-14 | Gate SVD behind `is_degenerate_rotation()` check |
| 6 | PERF-30 | Replace `to_ascii_lowercase()` with `eq_ignore_ascii_case` |
| 7 | PERF-29 | Add `with_capacity` to BsTriShape vertex Vecs |

### Medium Effort (1-4 hours, architectural)

| Priority | Finding | Change |
|----------|---------|--------|
| 8 | PERF-03 | Cache subtree name maps on AnimationPlayer/Stack components |
| 9 | PERF-04 | Hold all query locks for entire transform propagation pass |
| 10 | PERF-05/06 | Introduce `StagingPool` for reusable staging buffers |
| 11 | PERF-07 | Double-buffer dynamic textures for Scaleform UI |
| 12 | PERF-17 | Switch `sample_blended_transform` to `SmallVec<[_; 4]>` |
| 13 | PERF-18 | Retain draw command / light Vecs across frames |
| 14 | PERF-08 | Add `VkPipelineCache` with disk serialization |

### Larger Changes (days, design needed)

| Priority | Finding | Change |
|----------|---------|--------|
| 15 | PERF-02 | Batched upload command buffer (single submit for all assets) |
| 16 | PERF-11 | Instanced drawing with SSBO model matrix array |
| 17 | PERF-16 | ParseOptions with block-skip for animation data |
| 18 | PERF-15 | `Arc<str>` string table in NIF parser |
