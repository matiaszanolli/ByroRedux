# Performance Audit — 2026-04-12

**Scope**: All 6 dimensions (GPU pipeline, GPU memory, draw calls, ECS queries, NIF parse, CPU allocations)
**Depth**: Deep — real code paths traced, operations counted
**Prior reports**: `AUDIT_PERFORMANCE_2026-04-04.md`, `AUDIT_PERFORMANCE_2026-04-11.md`

## Executive Summary

| Severity | Count | Key Impact |
|----------|-------|------------|
| HIGH     | 2     | RT ray budget, ECS lock thrashing |
| MEDIUM   | 8     | Draw sort defeating instancing, G-buffer bandwidth, HashMap rebuilds |
| LOW      | 11    | Minor allocs, cache waste, code hygiene |
| Positive | 4     | Confirmed correct patterns |
| **Total** | **25** | |

**Estimated FPS impact of top fixes**: +15-25% on RT-enabled scenes (ray budget), +10-20% draw call reduction (sort fix), ~1ms/frame CPU savings (HashMap + allocation amortization).

## Hot Path Analysis

| Operation | Per-frame cost | Bottleneck |
|-----------|---------------|------------|
| RT shadow rays (N per fragment) | 0.6-2.4ms | GPU ALU/RT cores |
| RT GI bounce (1 per fragment) | 0.3-1.0ms | GPU RT cores |
| SVGF temporal dispatch | 0.1-0.3ms | GPU compute |
| Composite fullscreen pass | 0.05ms | GPU fill |
| G-buffer write (6 targets, 34 B/px) | bandwidth | GPU ROP |
| animation_system lock cycles | 0.05-0.1ms | CPU contention |
| subtree_cache HashMap rebuild | 0.1-0.2ms | CPU alloc |
| build_render_data queries (11) | ~1us | CPU (negligible) |
| NIF import (per cell, not per frame) | 50-200ms | CPU alloc+parse |

## Findings

### HIGH

#### P1-03: Fragment shader fires up to N+2 ray queries per fragment — no shadow ray budget
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/triangle.frag:558-632,662-711,415-447`
- **Description**: RT enabled: one shadow ray per clustered light (no cap), one GI hemisphere bounce, one window portal ray. A fragment seeing 8 lights casts 10 ray queries.
- **Impact**: At 1080p with avg 4 lights/cluster: ~8M shadow rays + 2M GI rays. On RTX 4070 Ti: 0.6-2.4ms/frame. Single largest GPU cost.
- **Fix**: Budget shadow rays to top-K lights (K=2) by NdotL×atten. Or stochastic: pick 1 shadow light per pixel, rely on SVGF temporal accumulation.
- **Status**: NEW

#### D4-01: AnimationStack processing acquires/drops 6-9 RwLock guards per entity per frame
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/systems.rs:331-511`
- **Description**: Per animated entity: 3 separate AnimationStack read acquisitions, plus write queries for Transform, RootMotionDelta, AnimatedAlpha/Color/Visibility. With 50 entities: 300-450 lock operations/frame.
- **Impact**: ~50-100us/frame, scales linearly with animated entity count.
- **Fix**: Restructure into batched phases: advance all stacks under one write lock, sample all poses under one read lock, apply all transforms under one write lock.
- **Status**: NEW

### MEDIUM

#### D3-01: Sort key places depth before mesh_handle — defeats instanced batching
- **Dimension**: Draw Call Overhead
- **Location**: `byroredux/src/render.rs:331-345`
- **Description**: Opaque sort key is `(pipeline_key, depth_key, texture, mesh)`. Depth before mesh prevents consecutive-draw merging. The instancing infrastructure is fully built but the sort defeats it.
- **Impact**: 2-3x more draw calls than optimal. 400 identical rocks = 400 draws instead of 1 instanced draw of 400.
- **Fix**: For opaque, sort by `(pipeline_key, mesh, texture)` primarily. Early-Z benefit is modest vs. instancing savings.
- **Status**: NEW

#### P1-02: Per-vertex inverse-transpose matrix in vertex shader
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/triangle.vert:95-97`
- **Description**: `transpose(inverse(m3))` computed for every vertex (~40 ALU ops). Orthogonal matrices (no non-uniform scale) don't need it.
- **Impact**: ~20M wasted ALU ops/frame at 500K vertices.
- **Fix**: Flags field in GpuInstance; branch to skip inverse when no non-uniform scale.
- **Status**: NEW

#### P1-07: G-buffer normal uses RGBA16_SNORM when RG16_SNORM (octahedral) would suffice
- **Dimension**: GPU Pipeline / GPU Memory
- **Location**: `crates/renderer/src/vulkan/gbuffer.rs:32`
- **Description**: RGBA16_SNORM = 8B/pixel. W always 0. Octahedral fits in RG16_SNORM = 4B/pixel.
- **Impact**: At 4K: 63MB/frame saved, 12% G-buffer bandwidth.
- **Fix**: Octahedral normal encoding. Well-proven (UE4/5, Frostbite).
- **Status**: NEW

#### P2-06: G-buffer + SVGF intermediates: ~898 MB at 4K
- **Dimension**: GPU Memory
- **Location**: `gbuffer.rs`, `svgf.rs`, `composite.rs`
- **Description**: G-buffer (506MB) + SVGF history (265MB) + HDR (127MB) = 898MB at 4K.
- **Impact**: 11% of 8GB VRAM. Exhausts 4GB iGPU.
- **Fix**: Octahedral normal (-127MB), SVGF moments→RG16F (-63MB), indirect→R11G11B10F (-132MB). Combined: ~320MB saved.
- **Status**: NEW

#### D5-02: inherited_props.to_vec() at every NiNode during import
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/import/walk.rs:122,204`
- **Description**: Property inheritance creates a new Vec at every NiNode. 500-node NIF × 200 NIFs/cell = 100K allocations.
- **Fix**: Push/pop on a single mutable Vec or SmallVec<[BlockRef; 8]>.
- **Status**: NEW

#### D5-04: Unconditional degenerate rotation check in compose_transforms
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/import/transform.rs:11-14`
- **Description**: 3x3 determinant computed on every transform composition. Degenerate matrices are <0.1%.
- **Fix**: Repair degenerate matrices once at parse time.
- **Status**: NEW

#### D6-01: subtree_cache HashMap recreated every frame
- **Dimension**: CPU Allocations
- **Location**: `byroredux/src/systems.rs:133-136`
- **Description**: Fresh HashMap<EntityId, HashMap<FixedString, EntityId>> per animation_system tick. 50 entities × 30 bones = 1500 insertions/frame.
- **Impact**: ~100-200us/frame.
- **Fix**: Persist in closure state, invalidate by generation counter.
- **Status**: NEW

### LOW

#### P1-08: Draw batches only merge consecutive — no sorted-order assertion
- **Location**: `draw.rs:350-370`
- **Status**: NEW

#### P1-09: Per-fragment 144B SSBO read includes vertex-only fields
- **Location**: `triangle.frag:294`
- **Status**: NEW

#### P2-08: Texture upload creates one-time fence per upload
- **Location**: `texture.rs:655-669`
- **Status**: NEW

#### P2-11: No VK_EXT_memory_budget tracking
- **Location**: `allocator.rs:44-70`
- **Status**: NEW

#### P2-12: Instance SSBO capped at 4096 — silent truncation
- **Location**: `scene_buffer.rs:32`
- **Status**: NEW

#### D3-06: No frustum culling of lights in build_render_data
- **Location**: `render.rs:348-376`
- **Status**: NEW

#### D5-06: Redundant .to_vec() in read_sized_string
- **Location**: `stream.rs:203`
- **Status**: NEW

#### D5-10: Double material extraction per NiTriShape
- **Location**: `mesh.rs:95,105`
- **Status**: NEW

#### D5-11: BSTriShape skin path clones bone_refs Vec
- **Location**: `mesh.rs:519,526`
- **Status**: NEW

#### D6-02: entities_with_players/stack_entities Vecs allocated per frame
- **Location**: `systems.rs:174,323`
- **Status**: NEW

#### D6-04: accum_root String allocated per stack entity per frame
- **Location**: `systems.rs:405`
- **Status**: NEW

### Positive (confirmed correct)

- **D3-05**: Alpha-blend back-to-front sort is correct (#241 fix verified)
- **D4-04**: build_render_data 11 read queries — optimal pattern, no deadlock risk
- **D4-05**: NameIndex generation comparison correctly skips static scenes
- **D4-06**: Transform propagation holds all 4 queries across BFS (fixed in #81)

## Existing Issues (deduplicated — not filed)

| Finding | Existing Issue | Status |
|---------|---------------|--------|
| Texture uploads bypass StagingPool | #239 | OPEN |
| build_geometry_ssbo bypasses StagingPool | #242 | OPEN |
| AnimationStack clones channel Vecs | #265 | OPEN |
| world_bound_propagation re-acquires queries | #250 | OPEN |
| find_key_pair Vec<f32> allocation | #240 | OPEN |
| Depth bias per batch | #51 (if exists) | — |

## Prioritized Fix Order

### Quick wins (1-2 hours each, immediate FPS gain)
1. **D3-01**: Fix sort key to enable instancing — biggest draw call reduction
2. **D6-01**: Persist subtree_cache across frames — 100-200us/frame saved
3. **D6-04**: Eliminate accum_root String allocation — trivial fix
4. **P1-08**: Add debug_assert on sorted order in draw.rs

### Medium effort (half-day each)
5. **P1-03**: Shadow ray budget (K=2) — 0.5-1.5ms/frame GPU savings
6. **D4-01**: Batch AnimationStack lock acquisitions — 50-100us/frame
7. **D5-02**: Push/pop property inheritance — eliminates 100K allocs/cell load
8. **P1-07**: Octahedral normal encoding — 12% G-buffer bandwidth saved

### Architectural (multi-day)
9. **P2-06**: Full G-buffer format optimization suite — 320MB saved at 4K
10. **P1-09**: Split GpuInstance into geometry + material SSBOs
