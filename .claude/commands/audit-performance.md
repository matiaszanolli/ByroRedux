---
description: "Audit GPU/CPU performance — draw calls, memory, queries, allocations, hot paths"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Performance Audit

Audit ByroRedux for GPU performance bottlenecks, CPU hot-path inefficiencies, memory allocation patterns, and rendering pipeline overhead.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3,5`). Default: all 9.
- `--depth shallow|deep`: `shallow` = check patterns only; `deep` = trace hot paths and measure impact. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: GPU Pipeline | GPU Memory | Draw Call Overhead | ECS Query Patterns | NIF Parse | CPU Allocations | TAA & GPU Skinning Cost | Material Table & SSBO Upload | World Streaming & Cell Transitions

## Reference Telemetry

`ScratchTelemetry` resource is refreshed per-frame; surface via `ctx.scratch` console command. Five tracked scratches: `gpu_instances`, `batches`, `indirect_draws`, `terrain_tile`, `tlas_instances`. Prospector baseline (1200 ent / 773 draws): 337 KB total, 320 B wasted. Use for diffing M40 cell transitions and R1 dedup wins.

## Known Infrastructure Gap (2026-05-04)

**dhat / alloc-counter regression coverage is NOT wired.** The 2026-05-04 batch shipped 5 perf fixes (#823–#828, #830–#833) without quantitative regression guards for allocation counts. Audits proposing alloc-reduction findings MUST flag this gap explicitly: a fix that improves allocation behavior today can silently regress tomorrow. Until dhat-infra lands, "estimated" / "expected" allocation savings in fix commits are the only baseline. Treat any new alloc-hot-path finding as warranting a follow-up "wire dhat for this site" issue.

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--depth`
2. `mkdir -p /tmp/audit/performance`
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/performance/issues.json`
4. Scan `docs/audits/` for prior performance reports

## Phase 2: Launch Dimension Agents

### Dimension 1: GPU Pipeline Efficiency
**Entry points**: `crates/renderer/src/vulkan/context/draw.rs` (draw_frame), `crates/renderer/shaders/triangle.frag`
**Checklist**: Unnecessary pipeline switches, redundant descriptor set binds, per-draw overhead (cmd_set_depth_bias on every draw?), shader branching cost (light loop divergence, RT ray query divergence), TLAS rebuild vs refit frequency, AS barrier placement, SVGF dispatch overhead per frame, TAA dispatch cost (fullscreen compute, RGBA16F sample bandwidth), caustic splat dispatch cost (fullscreen compute + atomic contention), composite pass fullscreen quad cost, G-buffer bandwidth (6 render targets per fragment), instanced draw batching (M31) effectiveness.
**Output**: `/tmp/audit/performance/dim_1.md`

### Dimension 2: GPU Memory & Allocation Patterns
**Entry points**: `crates/renderer/src/vulkan/buffer.rs`, `crates/renderer/src/vulkan/allocator.rs`, `crates/renderer/src/vulkan/scene_buffer/`, `crates/renderer/src/vulkan/acceleration/`
**Checklist**: Host-visible vs device-local usage, staging buffer lifecycle, BLAS scratch buffer reuse (high-water mark — does it grow unbounded?), per-frame SSBO/UBO mapped writes (flush needed?), texture upload staging reuse, gpu-allocator fragmentation, TLAS instance buffer sizing (2x padding policy), G-buffer memory footprint at high resolutions, SVGF history buffer double-allocation cost.
**Output**: `/tmp/audit/performance/dim_2.md`

### Dimension 3: Draw Call & Batching Overhead
**Entry points**: `byroredux/src/render.rs` (build_render_data), `crates/renderer/src/vulkan/context/draw.rs` (draw loop)
**Checklist**: Sort key efficiency, texture bind frequency, pipeline switch frequency, push constant overhead per draw, potential for instanced drawing (same mesh multiple transforms), draw call count vs entity count ratio.
**Output**: `/tmp/audit/performance/dim_3.md`

### Dimension 4: ECS Query Patterns
**Entry points**: `byroredux/src/systems/{animation, audio, billboard, bounds, camera, debug, particle, water, weather}.rs` (post-Session-34 split; `systems.rs` is a 27-line module index), `crates/core/src/ecs/world.rs`, `crates/core/src/ecs/query.rs`, `crates/core/src/ecs/lock_tracker.rs`
**Checklist**: Query lock duration (held across I/O or GPU ops?), redundant queries in same system, name index rebuild frequency, animation_system per-frame HashMap builds, transform_propagation_system BFS efficiency.
**2026-05-04 baseline (must not regress)**:
- `lock_tracker::held_others` Vec collection is `cfg(debug_assertions)`-gated (#823 ECS-PERF-01) — release builds were paying ~100 small allocs/frame for a no-op. Re-enabling for release is a regression
- `NameIndex.map` is refilled in place (HashMap::clear + insert), NOT replaced via `HashMap::new() + std::mem::swap` (#824 ECS-PERF-02) — the swap path costs ~3 ms cell-stream-in spike
- `transform_propagation_system` caches the root entity set keyed on `(Transform::len, Parent storage len OR 0 when Parent absent, world.next_entity_id())` (#825 ECS-PERF-03; see `crates/core/src/ecs/systems.rs` cache state + invalidation logic — third field is an `EntityId` value, not a count, and the Parent-len has `unwrap_or(0)` for scenes with no parent storage) — saves ~250 µs/frame at Megaton scale
- `animation_system` hoists `events` / `seen_labels` scratches out of the per-entity loop and uses `clone` instead of `mem::take` so capacity persists across iterations (#828 ECS-PERF-06)
- `World::despawn` poisoned-lock panic uses a `type_names` side-table to name the offending component (#466 E-03) — regression test must continue to pin the panic message format
**Output**: `/tmp/audit/performance/dim_4.md`

### Dimension 5: NIF Parse Performance
**Entry points**: `crates/nif/src/lib.rs` (parse_nif), `crates/nif/src/import/`, `crates/nif/src/blocks/`, `crates/nif/src/stream.rs` (allocate_vec, read_pod_vec), `byroredux/src/streaming.rs` (pre_parse_cell with rayon)
**Checklist**: Per-block allocation count, string cloning vs borrowing, Vec preallocation, SVD decomposition frequency (nalgebra overhead), block_size skip vs full parse for unused blocks.
**2026-05-04 baseline (must not regress)**:
- `pre_parse_cell` parallelises the model loop with rayon's `into_par_iter` (#830 NIF-PERF-06, `byroredux/src/streaming.rs::pre_parse_cell`) — drops cell-stream latency ~6-7× on FNV/SE exterior grids. Serial fallback is a regression
- `stream.allocate_vec::<T>(n)?;` is `#[must_use]` — bound-check-only call sites that discard the empty Vec are a leak/no-op pattern that #831 NIF-PERF-03 fixed at 9 sites; the `must_use` attribute prevents recurrence
- 6 NIF bulk-array readers go through `read_pod_vec<T>` to collapse double allocation (#833 NIF-PERF-02). Direct allocate-then-loop-and-fill is the regression pattern. The helper has a top-of-module compile-error gate for big-endian hosts; bytemuck path was rejected because bytemuck is NOT a workspace dep despite some audits claiming it
- Per-block parse-loop counters use `entry().get_mut() / insert` split, NOT `entry().or_insert(name.to_string())` (#832 NIF-PERF-01) — the to_string path leaked ~150 KB/cell of throwaway short-string allocations on Oblivion
**Output**: `/tmp/audit/performance/dim_5.md`

### Dimension 6: CPU Allocation Hot Paths
**Entry points**: `byroredux/src/systems/animation.rs` (animation_system, transform_propagation_system; post-Session-34 split), `byroredux/src/render.rs` (build_render_data)
**Checklist**: Per-frame Vec allocations (should use pre-allocated buffers?), String allocations in name lookups (already fixed with FixedString?), HashMap rebuilds, temporary Vec<DrawCommand> growth, scratch reuse vs realloc — diff against `ScratchTelemetry` baseline (337 KB / 320 B wasted on Prospector). Allocation findings should explicitly call out the dhat-infra gap (see Known Infrastructure Gap above) and note whether the proposed fix is testable today.
**Output**: `/tmp/audit/performance/dim_6.md`

### Dimension 7: TAA & GPU Skinning Cost (M37.5 + M29.5)
**Entry points**: `crates/renderer/src/vulkan/taa.rs`, `crates/renderer/src/vulkan/skin_compute.rs`, `crates/renderer/src/vulkan/acceleration/` (BLAS refit path), `crates/renderer/shaders/taa.comp`, `crates/renderer/shaders/skin_vertices.comp`
**Checklist**: TAA dispatch cost relative to scene cost (compute should be O(pixels) only, not O(pixels × meshes)). History image allocation: 2× RGBA16F at swapchain res — non-trivial at 4K (~64 MB). Skin compute dispatch frequency: per-skinned-mesh per-frame is the design; verify no dispatch for static meshes (pre-skin gating). BLAS refit cost vs full rebuild — refit must dominate; full rebuild only on bone count change. Per-skinned-mesh output buffer: lazily allocated, never re-uploaded with stale data. Bone palette upload: single buffer per frame, sized to MAX_TOTAL_BONES — no per-mesh upload churn. M29.3 raster path (when shipped): vertex shader reads pre-skinned vertex SSBO instead of inline matrix sum (~50 ALU ops saved per vertex).
**Output**: `/tmp/audit/performance/dim_7.md`

### Dimension 8: Material Table & SSBO Upload (R1)
**Entry points**: `crates/renderer/src/vulkan/material.rs`, `crates/renderer/src/vulkan/scene_buffer/` (MaterialBuffer SSBO), `byroredux/src/render.rs` (material intern call sites)
**Checklist**: Dedup ratio — N placements of the same material should produce 1 GpuMaterial entry; report dedup hit rate per cell. Per-frame upload size — should be O(unique materials), not O(draws). Hash-table churn — `MaterialTable::intern` should be O(1) amortized per lookup. SSBO resize policy — does the buffer over-allocate and reuse, or realloc-shrink each frame? GpuInstance struct size win — verify the post-R1 size (target 112 B vs ~400 B legacy) is realized in the `gpu_instance_is_112_bytes_std430_compatible` + `gpu_instance_field_offsets_match_shader_contract` + `gpu_instance_does_not_re_expand_with_per_material_fields` tests in `scene_buffer/gpu_instance_layout_tests.rs`. Memory bandwidth — confirm material table upload doesn't replace dedup wins with bandwidth losses on large scenes.
**Output**: `/tmp/audit/performance/dim_8.md`

### Dimension 9: World Streaming & Cell Transitions (M40)
**Entry points**: `byroredux/src/streaming.rs` (+ `streaming_tests.rs`), `byroredux/src/cell_loader/{load,unload,exterior,references,spawn,partial,refr,terrain,water,nif_import_registry,load_order}.rs` (post-Session-34 split — `cell_loader.rs` is a thin re-export, NOT the impl), `byroredux/src/npc_spawn.rs`
**Checklist**: Cell-transition stall budget (frame-time spike at boundary crossing). Async pre-parse worker thread doing real work off-main (verify with profiler). NIF import cache hit rate during streaming (cached across cells, not per-cell churn). BLAS LRU eviction at 1 GB budget triggers smoothly during streaming, no thrash. Texture upload budget per frame during streaming — staging buffer reuse, not realloc. Shutdown drain joins worker without leaks. Single-cell-at-a-time today (Phase 1a/1b) — multi-cell exterior grid is M40 follow-up; baseline must not regress.
**Output**: `/tmp/audit/performance/dim_9.md`

## Phase 3: Merge

1. Read all `/tmp/audit/performance/dim_*.md` files
2. Combine into `docs/audits/AUDIT_PERFORMANCE_<TODAY>.md` with structure:
   - **Executive Summary** — Total findings by severity, estimated FPS impact
   - **Hot Path Analysis** — Table of per-frame operations with estimated cost
   - **Findings** — Grouped by severity (CRITICAL first), deduplicated
   - **Prioritized Fix Order** — Quick wins first (cache reuse, preallocation), then architectural changes
3. Remove cross-dimension duplicates

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/performance`
2. Inform user the report is ready
3. Suggest: `/audit-publish docs/audits/AUDIT_PERFORMANCE_<TODAY>.md`
