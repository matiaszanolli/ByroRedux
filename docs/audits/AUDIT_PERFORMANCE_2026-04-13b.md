# Performance Audit — 2026-04-13b

**Scope**: All 6 dimensions, deep analysis. Second audit this session — only NEW findings vs 04-13a.
Prior audits: 04-04, 04-11, 04-12, 04-13a.

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 8 |
| LOW | 11 |
| ENHANCEMENT | 3 |
| **Total** | **22** |

No critical or high-severity issues. The codebase is well-optimized after M31 and prior fixes.
Highest-impact new findings: NIF bulk reads (P5-04, 50K–100K fn calls/NIF), texture staging pool
bypass (P2-05, ~200 alloc/free cycles per cell load), AnimationStack channel cloning (P4-10/P6-06,
~100 heap allocs/frame), and per-block Box<dyn> allocation (P5-07, 1000 heap allocs per NIF).
Combined quick wins estimated at ~0.3–0.5ms/frame CPU + significant cell load speedup.

## Hot Path Analysis (per-frame, 1080p, ~800 draws)

| Operation | Est. Cost | Notes |
|-----------|-----------|-------|
| RT shadow rays (top-2 budget) | 0.3–0.8ms | Clean (M31 #270) |
| RT GI bounce | 0.2–0.5ms | Known P1-01: add TerminateOnFirstHit |
| SVGF temporal dispatch | 0.1–0.2ms | Clean |
| SSAO compute | 0.05–0.1ms | **P1-07: bilinear depth sampling** |
| Composite pass | 0.03–0.05ms | **P1-06: unused albedoTex binding** |
| TLAS build | 0.05–0.15ms | **P1-08: O(N) address compare** |
| Draw loop (batching) | 0.1–0.3ms | **D3-02: per-mesh VB/IB rebinding** |
| build_render_data | 0.05–0.15ms | **P4-06–08: redundant queries** |
| animation_system | 0.02–0.1ms | **P4-10: channel Vec cloning** |

## Hot Path Analysis (per cell load, one-time)

| Operation | Est. Cost | Notes |
|-----------|-----------|-------|
| NIF parse (1000-block) | 20–50ms | **P5-04: 50K–100K read_exact calls** |
| NIF import (walk + mesh) | 5–15ms | **P5-02/03/05: clones, nalgebra** |
| Texture upload (200 tex) | 30–80ms | **P2-05: staging pool bypass** |
| One-time fences | ~3.5ms | **P2-08: 700 fence create/destroy** |

---

## Findings

### MEDIUM

#### P5-04: NifStream primitive reads go through per-value read_exact calls
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/stream.rs:86-120`
- **Description**: Every `read_u8`, `read_u16_le`, `read_f32_le` goes through `Cursor<&[u8]>::read_exact()` individually. A single NiTriShapeData with 1000 vertices makes ~11,000 separate calls (positions + normals + UVs + indices). Across a full NIF: 50K–100K calls.
- **Impact**: Function call overhead + bounds checking dominates parse time for geometry-heavy NIFs.
- **Fix**: Add bulk-read methods (`read_ni_point3_array`, `read_u16_array`) that read `count * size` bytes in one call and reinterpret. Since endianness is validated as LE-only in the header, this is safe. Reduces call count from 3000 to 1 per vertex array.

#### P5-07: Box<dyn NiObject> per-block dynamic dispatch allocation
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/blocks/mod.rs:116`, `crates/nif/src/lib.rs:119`
- **Description**: Every parsed block is heap-allocated as `Box<dyn NiObject>`. For a 1000-block NIF: 1000 individual heap allocations of varying sizes (NiNode ~150B, NiTriShapeData hundreds of bytes). Adds vtable pointer overhead per block.
- **Impact**: Single largest source of per-block allocation. Varying sizes cause heap fragmentation.
- **Fix**: Long-term: `bumpalo` arena allocator. Medium-term: enum dispatch for top ~20 block types (covers 95%+). Short-term: no easy fix, but Vec::with_capacity already used for large internal arrays.

#### P2-05: Texture uploads bypass the StagingPool entirely
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/texture.rs:47-95` (from_rgba), `291-337` (from_bc)
- **Description**: Both `Texture::from_rgba()` and `from_bc()` create and destroy a fresh staging buffer per upload via `StagingGuard`. Never use the `StagingPool` that `buffer.rs` implements. During cell load with 200+ textures: ~200 alloc/free cycles through gpu-allocator.
- **Impact**: Fragmentation pressure from rapid create/destroy of varying-size staging buffers. Each cycle takes the allocator mutex lock twice.
- **Fix**: Accept `staging_pool: Option<&mut StagingPool>`, mirroring `create_device_local_buffer`. Release staging buffer back to pool after copy fence signals.

#### P4-10 / P6-06: AnimationStack dominant_channels clones channel Vecs per entity per frame
- **Dimension**: ECS Query Patterns / CPU Allocations
- **Location**: `byroredux/src/systems.rs:489-491`
- **Description**: For each AnimationStack entity, clones entire `float_channels`, `color_channels`, `bool_channels` Vecs to drop the read lock before acquiring write locks on AnimatedAlpha/Color/Visibility. For a character with 10 channels: 10 String + Channel heap clones per entity per frame.
- **Impact**: ~100 heap allocations per frame with 10 animated stack entities. Scales linearly.
- **Fix**: Extract only `clip_handle: u32` and `local_time: f32` (both Copy) from the dominant layer. After dropping the AnimationStack lock, use `registry.get(dominant_clip_handle)` (registry guard already held) to access channels without cloning.

#### P5-02: Per-node inherited_props.to_vec() clones in walk functions
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/import/walk.rs:145,190,270,302`
- **Description**: Both `walk_node_hierarchical` and `walk_node_flat` clone `inherited_props: &[BlockRef]` into a new Vec at every NiNode visited, then extend. For 200 NiNodes at depth 8: ~1600 Vec allocations.
- **Impact**: O(N × D) allocations where N = node count, D = average depth.
- **Fix**: Pass mutable `Vec<BlockRef>` by reference, push/pop around recursive calls (stack discipline). Eliminates all cloning.

#### P5-05: zup_matrix_to_yup_quat uses nalgebra on every node
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/import/coord.rs:17-57`
- **Description**: Creates nalgebra `Matrix3`, computes determinant, creates `Rotation3::from_matrix_unchecked`, converts to `UnitQuaternion` for every node. Even on the 99% fast path (non-degenerate). In contrast, `compose_transforms` has a proper fast path that skips nalgebra.
- **Impact**: 500 nalgebra Matrix3 constructions + quaternion extractions per 1000-block NIF.
- **Fix**: Hand-roll a `matrix3_to_quat` using the Shepperd method (~20 FLOPs) on `[[f32; 3]; 3]`. Only fall back to nalgebra SVD when determinant check fails.

#### P6-04: compute_palette returns a fresh Vec<Mat4> per skinned mesh per frame
- **Dimension**: CPU Allocations
- **Location**: `crates/core/src/ecs/components/skinned_mesh.rs:93`
- **Description**: `SkinnedMesh::compute_palette()` uses `.collect()` to return a new `Vec<Mat4>` each call. Called once per skinned entity per frame. For 20 skinned meshes at ~60 bones: 20 heap allocations of ~3.8 KB every frame.
- **Fix**: Add `compute_palette_into(&self, scratch: &mut Vec<Mat4>, ...)`. Clear and reuse persistent scratch buffer.

#### D3-02: Per-mesh vertex/index buffer rebinding despite global geometry SSBO
- **Dimension**: Draw Call Overhead
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:565-579`, `crates/renderer/src/mesh.rs:10-19`
- **Description**: Each mesh has its own `vertex_buffer` and `index_buffer`. The draw loop rebinds whenever mesh_handle changes. Meanwhile, a global geometry SSBO already accumulates all vertex/index data with per-mesh offsets (used for RT UV lookups). Using the global buffer for draws with per-draw `vertexOffset`/`firstIndex` would reduce rebinding to once per frame.
- **Impact**: ~200 rebind pairs per frame in a typical cell. Each is a command buffer state change.
- **Fix**: Create global buffer with `VERTEX_BUFFER | INDEX_BUFFER` usage flags. Use `mesh.global_vertex_offset` / `mesh.global_index_offset` in `cmd_draw_indexed`. Prerequisite for multi-draw indirect.

---

### LOW

#### P1-04: Redundant shadowDist recalculates worldDist
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/triangle.frag:549`
- **Description**: `shadowDist = length(fragWorldPos - cameraPos.xyz)` duplicates `worldDist` computed 31 lines earlier.
- **Fix**: `float shadowDist = worldDist;` — 1 line.

#### P1-05: Cluster logRatio recomputed per fragment
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/triangle.frag:257`
- **Description**: `log(CLUSTER_FAR / CLUSTER_NEAR)` computed every invocation. Both are compile-time constants but GLSL `log()` is not guaranteed constexpr-folded.
- **Fix**: Precompute as `const float LOG_RATIO = 11.512925;` at file scope.

#### P1-06: Composite pass binds albedoTex but never samples it
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/composite.frag:21`
- **Description**: Declared `sampler2D albedoTex` is never referenced in shader body. Wasted descriptor slot.
- **Fix**: Remove binding, or implement intended Phase 3+ albedo re-multiplication.

#### P1-07: SSAO reads depth via bilinear texture() instead of texelFetch()
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/ssao.comp:44,78`
- **Description**: Bilinear-filtered depth is physically meaningless at depth discontinuities — creates false occlusion halos at silhouettes.
- **Fix**: Use `texelFetch(depthTex, px, 0)` or change sampler to NEAREST filtering.

#### P1-08: TLAS BLAS-address comparison is O(N) per frame
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:781-792`
- **Description**: Full O(N) zip-compare of `last_blas_addresses` against current instances every frame. For 5000 instances: 5000 u64 comparisons.
- **Impact**: ~5–10us/frame. Not catastrophic but grows linearly.
- **Fix**: Track a dirty flag set on BLAS entry add/remove. Skip comparison on static frames.

#### P2-07: write_mapped flushes entire allocation, not written range
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/buffer.rs:536-588`
- **Description**: Flushes full 1.28MB instance SSBO even when only 32KB written. No-op on HOST_COHERENT (most desktop GPUs).
- **Fix**: Track actual byte count written, pass to `aligned_flush_range`.

#### P2-08: Per-texture one-time fence creation/destruction
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/texture.rs:655-669`
- **Description**: Every `with_one_time_commands` creates/waits/destroys a VkFence. During cell load: ~700 cycles × ~5us = ~3.5ms.
- **Fix**: Keep a reusable fence in VulkanContext. Reset between uses.

#### P2-09: TLAS instance barrier uses VK_WHOLE_SIZE
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:824-829`
- **Description**: Barrier covers full 8192+ instance buffer when only a fraction is written.
- **Fix**: Calculate `instance_count * size_of::<AccelerationStructureInstanceKHR>()`.

#### P5-03: Arc<str> converted to String at every ImportedNode/ImportedMesh
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/import/walk.rs:136,177`, `crates/nif/src/import/mesh.rs:130,349,603`
- **Description**: NIF parser stores names as `Arc<str>`. Import boundary converts to `Option<String>` via `.to_string()`, allocating per node/mesh (~500 allocs per 1000-block NIF).
- **Fix**: Change `ImportedNode.name` / `ImportedMesh.name` to `Option<Arc<str>>`.

#### P5-06: to_triangles() on NiTriStripsData has no capacity hint
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/blocks/tri_shape.rs:853`
- **Fix**: `Vec::with_capacity(self.num_triangles as usize)` — 1 line.

#### D3-03: Transparent sort key float-to-bits conversion not fully IEEE 754 correct
- **Dimension**: Draw Call Overhead
- **Location**: `byroredux/src/render.rs:354-363`
- **Description**: `!f32::to_bits()` for back-to-front ordering works for positive depths but breaks for NaN/denormalized values. Frustum culling prevents pathological cases in practice.
- **Fix**: Use proper float-to-sortable-u32: `let bits = f.to_bits(); if bits & 0x80000000 != 0 { !bits } else { bits ^ 0x80000000 }`.

---

### ENHANCEMENT

#### P1-09: TLAS PREFER_FAST_BUILD may be suboptimal now that REFIT handles most frames
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:695`
- **Description**: TLAS uses `PREFER_FAST_BUILD` but REFIT (#247) now handles most frames, making full rebuilds rare. Top-level BVH quality affects every ray query.
- **Fix**: Switch to `PREFER_FAST_TRACE | ALLOW_UPDATE`. Revisit if full rebuilds become frequent.

#### P2-10: gpu-allocator uses default block sizes (256MB device / 64MB host)
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/allocator.rs:22-34`
- **Description**: On a 4GB GPU, a single 256MB block reservation consumes 6.25% of VRAM before any content loads.
- **Fix**: Profile actual allocation patterns; consider 64MB device / 32MB host as starting point.

#### D3-05: No multi-draw indirect path (requires D3-02 first)
- **Dimension**: Draw Call Overhead
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:533-592`
- **Description**: One `cmd_draw_indexed` per batch. `vkCmdDrawIndexedIndirect` could submit all draws in a single API call. Requires D3-02 (global VB/IB) as prerequisite.
- **Fix**: After D3-02, build `VkDrawIndexedIndirectCommand` buffer and issue per pipeline group. Not urgent at current scene complexity.

---

## Confirmed-Correct Patterns

| Pattern | Status | Source |
|---------|--------|--------|
| Descriptor sets bound once per frame (set 0 + set 1) | Correct | Dim 1 |
| Zero push constants — fully bindless SSBO architecture | Correct | Dim 3 |
| Instanced draw batching (consecutive merge on pipeline+mesh) | Correct | Dim 1, 3 |
| Pipeline switch tracking (last_pipeline_key) | Correct | Dim 1, 3 |
| Vertex/index buffer rebind tracking (last_mesh_handle) | Correct | Dim 1, 3 |
| Depth bias state tracking (last_is_decal) | Correct | Dim 1 |
| TLAS BUILD vs REFIT logic (BLAS address sequence) | Correct | Dim 1 |
| All HOST→GPU barriers (draw.rs, accel, SVGF) | Correct | Dim 1 |
| SVGF temporal accumulation barriers | Correct | Dim 1 |
| G-buffer formats (18 B/pixel: RG16 normals, B10G11R11 indirect/albedo) | Correct | Dim 1, 2 |
| Compute workgroup sizes (8×8 = 64, matches wave/warp) | Correct | Dim 1 |
| Scene buffers double-buffered per frame-in-flight | Correct | Dim 2 |
| BLAS result buffers DEVICE_LOCAL | Correct | Dim 2 |
| TLAS double-buffered per frame-in-flight | Correct | Dim 2 |
| SVGF double-buffered (not triple) | Correct | Dim 2 |
| StagingPool budget-based eviction for buffer uploads | Correct | Dim 2 |
| BLAS LRU eviction with budget tracking (256MB) | Correct | Dim 2 |
| Aligned flush range avoids VK_WHOLE_SIZE | Correct | Dim 2 |
| Reverse-order cleanup in all destroy methods | Correct | Dim 2 |
| Scratch buffer amortization (mem::take + restore) | Correct | Dim 1, 3 |
| Transform propagation holds all 4 queries across BFS | Correct | Dim 4 |
| TypeId-sorted lock acquisition in query_2_mut | Correct | Dim 4 |
| NameIndex rebuild generation-gated by component count | Correct | Dim 4 |
| SubtreeCache persists across frames | Correct | Dim 4 |
| AnimationPlayer phase separation (advance vs apply) | Correct | Dim 4 |
| Stage-based scheduler (Early→Update→PostUpdate→Physics→Late) | Correct | Dim 4 |
| build_render_data scratch buffers (.clear() reuse) | Correct | Dim 6 |
| Transform/world-bound propagation closure-captured scratch Vecs | Correct | Dim 6 |
| AnimationStack scratch buffers (channel_names, updates) | Correct | Dim 6 |
| Effect mesh filter uses zero-allocation eq_ignore_ascii_case | Correct | Dim 6 |
| Animation interpolation returns stack values (no heap) | Correct | Dim 6 |
| NIF string table as Vec<Arc<str>> (modern NIFs) | Correct | Dim 5 |
| Geometry parser Vec::with_capacity for vertices/indices | Correct | Dim 5 |
| Block size skip for unknown types (zero allocation) | Correct | Dim 5 |
| NifStream uses Cursor<&[u8]> — no BufReader needed | Correct | Dim 5 |
| SVD only on degenerate rotations in compose_transforms | Correct | Dim 5 |
| skip_animation ParseOption for character NIFs | Correct | Dim 5 |
| read_sized_string zero-copy try-first (#254) | Correct | Dim 5 |

---

## Prioritized Fix Order

### Quick wins (< 30 min each)
1. **P5-06**: `Vec::with_capacity` for to_triangles — 1 line
2. **P1-04**: `float shadowDist = worldDist;` — 1 line
3. **P1-05**: `const float LOG_RATIO = 11.512925;` — 1 line
4. **P5-03**: Change ImportedNode/Mesh name to `Option<Arc<str>>` — type change + 5 call sites
5. **P1-06**: Remove unused albedoTex binding — 2 files

### Medium effort (1–2 hours each)
6. **P4-10/P6-06**: Extract clip_handle+local_time instead of cloning channels — eliminates ~100 allocs/frame
7. **P6-04**: Add `compute_palette_into` with scratch buffer — eliminates ~20 allocs/frame
8. **P5-02**: Push/pop inherited_props instead of cloning — eliminates ~1600 allocs/NIF
9. **P5-05**: Hand-roll matrix3_to_quat (Shepperd method) — ~20 FLOPs vs nalgebra overhead
10. **P2-05**: Route texture uploads through StagingPool — eliminates ~200 alloc/free cycles per cell
11. **P2-08**: Reusable fence for one-time commands — saves ~3.5ms per cell load
12. **D3-02**: Global VB/IB for draw calls — eliminates ~200 rebind pairs/frame

### Larger effort (2–4 hours)
13. **P5-04**: Bulk read methods for NifStream geometry — major parse speedup
14. **P1-08**: Dirty flag for TLAS address comparison
15. **P1-09**: TLAS PREFER_FAST_TRACE | ALLOW_UPDATE
16. **P2-10**: Tune gpu-allocator block sizes

### Architectural (deferred)
17. **P5-07**: Arena/enum dispatch for NIF blocks — largest single allocation source
18. **D3-05**: Multi-draw indirect (requires D3-02)
