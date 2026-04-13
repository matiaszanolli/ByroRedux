# Performance Audit — 2026-04-13

Scope: All 6 dimensions, deep analysis. Post-session 8 (35 commits, M30–M34, RT perf overhaul).
Prior audits: 04-04, 04-11, 04-12. Only NEW findings reported.

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 5 |
| LOW | 12 |
| ENHANCEMENT | 2 |
| **Total** | **19** |

No critical or high-severity performance issues. The M31 RT performance overhaul (shadow budget,
BLAS LRU, instanced batching) addressed the major bottlenecks. Remaining findings are incremental
optimizations with estimated total impact of ~0.5-1.0ms/frame on current workloads. The highest-
impact quick wins: GI TerminateOnFirstHit (~0.2ms), effect mesh bool flag (~300 allocs/frame),
3-pass blend sampling fusion.

## Hot Path Analysis (per-frame, 1080p, ~800 draws)

| Operation | Est. Cost | Notes |
|-----------|-----------|-------|
| RT shadow rays (2 per fragment) | 0.3-0.8ms | Budgeted by M31 (#270) |
| RT GI bounce (1 per fragment) | 0.2-0.5ms | **P1-01: could save 0.1-0.3ms with TerminateOnFirstHit** |
| RT reflection (1 per fragment) | 0.1-0.3ms | Correctly uses closest-hit |
| SVGF temporal dispatch | 0.1-0.2ms | Clean |
| SSAO compute | 0.05-0.1ms | Clean |
| Composite pass | 0.03-0.05ms | Clean |
| TLAS build | 0.05-0.15ms | **P2-01: host-visible read adds ~0.05ms** |
| Draw loop (batching) | 0.1-0.3ms | Improved by #272 |
| build_render_data | 0.05-0.15ms | Clean after #251/#252/#278 |
| animation_system | 0.02-0.1ms | Clean after scratch buffers |

---

## Findings

### MEDIUM

#### P1-01: GI ray query does not use TerminateOnFirstHit
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/triangle.frag:735-740`
- **Description**: The GI bounce ray uses `gl_RayFlagsOpaqueEXT` only. Shadow rays and window portal rays correctly use `TerminateOnFirstHit`, but GI does not. For GI with pre-computed avg_albedo in the SSBO, any-hit suffices — we don't need geometric closest-hit.
- **Impact**: GI traversal 30-60% slower than necessary. ~0.1-0.3ms/frame at 1080p.
- **Fix**: Add `gl_RayFlagsTerminateOnFirstHitEXT` to GI `rayQueryInitializeEXT`.

#### P4-01: AnimationStack re-locked 4x per entity in stack processing loop
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/systems.rs:337-471`
- **Description**: The AnimationStack processing loop acquires and drops the AnimationStack query lock 4 separate times per entity: (1) `query_mut` for advance_stack, (2) `query` for sampling channel names, (3) `query` for accum_root lookup, (4) `query` for dominant channel extraction. For N animated entities, 4N lock acquire/release cycles per frame.
- **Impact**: Measurable on scenes with 20+ AnimationStack entities.
- **Fix**: Merge the three read passes into a single `query::<AnimationStack>()` per entity. Reduces 4N to 2N.

#### P6-02: Per-frame to_ascii_lowercase() in draw command loop
- **Dimension**: CPU Allocations
- **Location**: `byroredux/src/render.rs:252`
- **Description**: `tp.to_ascii_lowercase()` allocates a new String for every textured entity each frame to check for effect mesh patterns (fxsoftglow, fxpartglow, etc.).
- **Impact**: ~300 String allocations per frame on a 500-entity cell.
- **Fix**: Add `is_effect_mesh: bool` to Material, populated once at import time. Eliminates per-frame string allocs entirely.

#### P6-05: sample_blended_transform iterates layers 3x per channel
- **Dimension**: CPU Allocations (CPU time)
- **Location**: `crates/core/src/animation/stack.rs:193-281`
- **Description**: Three full passes over `stack.layers` per channel name: (1) find max priority, (2) compute total weight, (3) blend transforms. Each pass repeats `registry.get()` + `clip.channels.get()` HashMap lookups. For 3 layers × 30 channels = 270 HashMap lookups/entity/frame.
- **Impact**: CPU-bound on blended animation scenes. String-key hashing dominates.
- **Fix**: Fuse passes 1 and 2 — find max_priority and accumulate total_weight simultaneously.

#### P2-01: TLAS instance buffer is HOST_VISIBLE — GPU reads traverse PCIe
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:663-669`
- **Description**: Instance buffer for AS build is host-visible. On discrete GPUs, GPU reads from BAR memory traverse PCIe (10-30x slower than VRAM). At 8192 instances × 64B = 512KB.
- **Impact**: ~0.05-0.1ms per TLAS build. Minimal at current interior sizes but grows with exterior cells.
- **Fix**: Double-buffer: write to host-visible staging, `cmd_copy_buffer` to device-local before AS build.

---

### LOW

#### P1-02: Unconditional depth bias command per batch
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:534-540`
- **Description**: Every batch calls `cmd_set_depth_bias` even when the decal state hasn't changed. ~190 redundant calls at 200 batches.
- **Impact**: ~2us/frame.
- **Fix**: Track `last_is_decal`, only emit on change.

#### P1-03: Redundant worldDist calculation in fragment shader
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/triangle.frag:508,539`
- **Description**: `length(fragWorldPos - cameraPos.xyz)` computed twice — once for cluster lookup, once for shadow distance fade.
- **Impact**: ~5 ALU ops × 2M fragments = 10M wasted ops.
- **Fix**: `float shadowDist = worldDist;`

#### P2-02: BLAS scratch buffer never shrinks
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:196-212`
- **Description**: High-water mark persists forever. A 16MB scratch from one complex mesh stays allocated after that mesh is LRU-evicted.
- **Impact**: 1-16MB wasted VRAM.
- **Fix**: After LRU eviction, destroy scratch if >4x remaining need or no entries remain.

#### P2-03: Scene buffer flush covers entire allocation
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/buffer.rs:503-530`
- **Description**: `flush_if_needed()` flushes full 1.28MB instance buffer even when only 8KB was written. No-op on HOST_COHERENT (most desktop GPUs).
- **Impact**: Wasted bandwidth on non-coherent memory (integrated/mobile GPUs only).
- **Fix**: Add `flush_range(device, offset, size)` with exact written byte count.

#### P2-04: GpuInstance doc comment says 144 bytes, actual is 160
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/scene_buffer.rs:45-46`
- **Description**: Stale comment from pre-avg_albedo layout.
- **Fix**: Update to "160 bytes per instance, 16-byte aligned (10x16)".

#### P4-01: build_render_data queries Material separately from Transform+MeshHandle
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/render.rs:183,244`
- **Description**: `mat_q` is a separate query from the main `(Transform, MeshHandle)` iteration. Each entity does a random-access `mat_q.get(entity)` lookup. A combined 3-component query would iterate in lock-step.
- **Impact**: ~1 extra HashMap lookup per entity (SparseSetStorage). Negligible at <1000 entities.
- **Fix**: Use `query_3` if available, or accept the random access pattern at current scale.

#### P6-03: entities_with_players and stack_entities Vecs allocated per frame
- **Dimension**: CPU Allocations
- **Location**: `byroredux/src/systems.rs:182,340`
- **Description**: Two `Vec<EntityId>` plus `Vec<PlaybackState>` collected every frame to work around borrow conflicts. Typically 10-50 entries each.
- **Fix**: Convert `animation_system` to closure-based system owning persistent scratch Vecs (same pattern as `make_world_bound_propagation_system`).

#### P4-02: SubtreeCache invalidated on any Name count change — not topology-aware
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/systems.rs:138-149`
- **Description**: Cache clears whenever `Name` component count changes. Adding a single named entity (e.g. UI element) invalidates the entire subtree cache, forcing BFS rebuild of all animated hierarchies.
- **Impact**: One-frame hitch (~200us) on entity spawn. Rare after initial load.
- **Fix**: Track per-root-entity generation or use a dirty flag on hierarchy changes only.

#### P4-03: GlobalTransform queried twice in build_render_data
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/render.rs:97,176`
- **Description**: `query::<GlobalTransform>()` acquired for skinning bone palette pass (line 97), dropped, then re-acquired for draw command pass (line 176). Both read-only.
- **Impact**: Two redundant read-lock round-trips per frame.
- **Fix**: Hoist query above both passes and reuse the guard.

#### P4-04: query_2_mut for lights uses unnecessary write lock
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/render.rs:402`
- **Description**: `query_2_mut::<GlobalTransform, LightSource>()` takes a write lock on LightSource but the loop only reads. Harmless now but will block concurrent readers under M27 parallel scheduler.
- **Fix**: Use two separate read queries.

#### P4-05: Name storage queried 3x at animation_system start
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/systems.rs:134,152,158`
- **Description**: Three `world.query::<Name>()` calls for SubtreeCache check, NameIndex check, and NameIndex rebuild. First two only need `q.len()`.
- **Fix**: Query once, extract count, reuse for both generation checks.

#### P6-01: draw_commands Vec not amortized across frames
- **Dimension**: CPU Allocations
- **Location**: `byroredux/src/render.rs:67`
- **Description**: `draw_commands` is `Vec::new()` each frame in `build_render_data`. The vec grows to ~800-3000 elements, is sorted, consumed by draw_frame, then dropped. The capacity is not reused across frames.
- **Impact**: ~24-96KB allocation per frame. Amortization would eliminate this.
- **Fix**: Store as a field on the App struct or a Resource, `.clear()` each frame.

#### P6-02: gpu_lights Vec not amortized across frames
- **Dimension**: CPU Allocations
- **Location**: `byroredux/src/render.rs:69`
- **Description**: Same pattern as draw_commands — fresh Vec each frame for GPU lights.
- **Impact**: ~2-4KB allocation per frame.
- **Fix**: Same — persistent Vec with `.clear()`.

---

### ENHANCEMENT

#### P3-01: Sort key uses u32 mesh_handle — could group by mesh_handle hash for cache locality
- **Dimension**: Draw Call Overhead
- **Location**: `byroredux/src/render.rs:343-356`
- **Description**: Opaque sort groups by mesh_handle (sequential u32 IDs). The allocation order of mesh handles doesn't correlate with similarity — meshes loaded from different NIFs get interleaved IDs. A hash-based grouping or explicit mesh-class bucketing would improve vertex buffer cache locality across instanced draws.
- **Impact**: Marginal. Only measurable at >2000 draws with heavy vertex cache pressure.
- **Fix**: Not cost-effective at current scale. Revisit for M40 (world streaming).

#### P5-01: Block parsers allocate Vec<BlockRef> per NiNode
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/blocks/node.rs`
- **Description**: Each NiNode allocates a `Vec<BlockRef>` for `children_refs` and `properties`. Typical counts: 2-8 children, 0-3 properties. SmallVec could eliminate heap allocation for >90% of nodes.
- **Impact**: ~500 heap allocs per 1000-node NIF. One-time parse cost, not per-frame.
- **Fix**: `SmallVec<[BlockRef; 8]>` for children, `SmallVec<[BlockRef; 4]>` for properties. Would require adding smallvec dependency.

---

## Confirmed-Correct Patterns

| Pattern | Status |
|---------|--------|
| TLAS BUILD vs REFIT logic | Correct — checks BLAS address sequence |
| BLAS LRU eviction (idle threshold, forced TLAS rebuild) | Correct |
| Instanced draw batching (consecutive merge on pipeline+mesh) | Correct |
| G-buffer formats (RG16_SNORM normals, B10G11R11 indirect) | Bandwidth-optimal |
| SVGF ping-pong isolation | Correct |
| Staging pool budget enforcement | Correct |
| All HOST→GPU barriers (draw.rs) | Present and correct |
| Reflection ray (closest-hit, no TerminateOnFirstHit) | Correct — needs geometric accuracy |
| Shadow ray budget (top-2 by contribution) | Correct |
| Animation scratch buffers (hoisted, cleared per entity) | Correct |
| SubtreeCache persistence across frames | Correct |
| accum_root as &str (no String alloc) | Correct |

## Prioritized Fix Order

### Quick wins (< 30 min each, no API changes)
1. **P1-01**: Add TerminateOnFirstHit to GI ray — 1 line, ~0.2ms/frame saved
2. **P6-02**: Add `is_effect_mesh: bool` to Material — eliminates 300 String allocs/frame
3. **P1-03**: Reuse worldDist for shadowDist — 1 line
4. **P2-04**: Fix stale comment — 1 line
5. **P6-01 + P6-02 (vecs)**: Amortize draw_commands/gpu_lights Vecs — move to App struct
6. **P4-05**: Query Name once instead of 3x at animation start

### Medium effort (1-2 hours)
7. **P6-05**: Fuse 3-pass sample_blended_transform into 2 passes — reduces 270 HashMap lookups/entity
8. **P4-01**: Merge AnimationStack 3 read passes into 1 — reduces 4N to 2N locks
9. **P2-01**: TLAS instance buffer double-buffer to device-local
10. **P1-02**: Track last_is_decal for depth bias command
11. **P4-02**: Per-root-entity subtree invalidation
12. **P6-03**: Convert animation_system to closure-based with persistent scratch Vecs

### Deferred (not cost-effective at current scale)
13. **P2-02**: BLAS scratch shrink — only wastes 1-16MB
14. **P2-03**: Precise flush range — no-op on desktop GPUs
15. **P4-03 + P4-04**: Redundant query locks in build_render_data — negligible at current scale
16. **P3-01**: Mesh-class sort bucketing — marginal
17. **P5-01**: SmallVec for block refs — one-time parse cost
