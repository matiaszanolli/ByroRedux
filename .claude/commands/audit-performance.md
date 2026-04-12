---
description: "Audit GPU/CPU performance — draw calls, memory, queries, allocations, hot paths"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Performance Audit

Audit ByroRedux for GPU performance bottlenecks, CPU hot-path inefficiencies, memory allocation patterns, and rendering pipeline overhead.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3,5`). Default: all 6.
- `--depth shallow|deep`: `shallow` = check patterns only; `deep` = trace hot paths and measure impact. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: GPU Pipeline | GPU Memory | Draw Call Overhead | ECS Query Patterns | NIF Parse | CPU Allocations

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--depth`
2. `mkdir -p /tmp/audit/performance`
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/performance/issues.json`
4. Scan `docs/audits/` for prior performance reports

## Phase 2: Launch Dimension Agents

### Dimension 1: GPU Pipeline Efficiency
**Entry points**: `crates/renderer/src/vulkan/context/draw.rs` (draw_frame), `crates/renderer/shaders/triangle.frag`
**Checklist**: Unnecessary pipeline switches, redundant descriptor set binds, per-draw overhead (cmd_set_depth_bias on every draw?), shader branching cost (light loop divergence, RT ray query divergence), TLAS rebuild vs refit frequency, AS barrier placement, SVGF dispatch overhead per frame, composite pass fullscreen quad cost, G-buffer bandwidth (6 render targets per fragment).
**Output**: `/tmp/audit/performance/dim_1.md`

### Dimension 2: GPU Memory & Allocation Patterns
**Entry points**: `crates/renderer/src/vulkan/buffer.rs`, `crates/renderer/src/vulkan/allocator.rs`, `crates/renderer/src/vulkan/scene_buffer.rs`, `crates/renderer/src/vulkan/acceleration.rs`
**Checklist**: Host-visible vs device-local usage, staging buffer lifecycle, BLAS scratch buffer reuse (high-water mark — does it grow unbounded?), per-frame SSBO/UBO mapped writes (flush needed?), texture upload staging reuse, gpu-allocator fragmentation, TLAS instance buffer sizing (2x padding policy), G-buffer memory footprint at high resolutions, SVGF history buffer double-allocation cost.
**Output**: `/tmp/audit/performance/dim_2.md`

### Dimension 3: Draw Call & Batching Overhead
**Entry points**: `byroredux/src/render.rs` (build_render_data), `crates/renderer/src/vulkan/context/draw.rs` (draw loop)
**Checklist**: Sort key efficiency, texture bind frequency, pipeline switch frequency, push constant overhead per draw, potential for instanced drawing (same mesh multiple transforms), draw call count vs entity count ratio.
**Output**: `/tmp/audit/performance/dim_3.md`

### Dimension 4: ECS Query Patterns
**Entry points**: `byroredux/src/systems.rs` (all system functions), `crates/core/src/ecs/world.rs`, `crates/core/src/ecs/query.rs`
**Checklist**: Query lock duration (held across I/O or GPU ops?), redundant queries in same system, name index rebuild frequency, animation_system per-frame HashMap builds, transform_propagation_system BFS efficiency.
**Output**: `/tmp/audit/performance/dim_4.md`

### Dimension 5: NIF Parse Performance
**Entry points**: `crates/nif/src/lib.rs` (parse_nif), `crates/nif/src/import.rs`, `crates/nif/src/blocks/`
**Checklist**: Per-block allocation count, string cloning vs borrowing, Vec preallocation, SVD decomposition frequency (nalgebra overhead), block_size skip vs full parse for unused blocks.
**Output**: `/tmp/audit/performance/dim_5.md`

### Dimension 6: CPU Allocation Hot Paths
**Entry points**: `byroredux/src/systems.rs` (animation_system, transform_propagation_system), `byroredux/src/render.rs` (build_render_data)
**Checklist**: Per-frame Vec allocations (should use pre-allocated buffers?), String allocations in name lookups (already fixed with FixedString?), HashMap rebuilds, temporary Vec<DrawCommand> growth.
**Output**: `/tmp/audit/performance/dim_6.md`

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
