---
description: "Deep audit of the Vulkan renderer — pipeline, sync, memory, shaders, ray tracing, denoiser"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Renderer Audit

Audit the Vulkan renderer for correctness across the full pipeline: rasterization, ray tracing (BLAS/TLAS, ray queries, shadows, reflections, GI), deferred indirect lighting (G-buffer, SVGF), compositing, synchronization, GPU memory, and resource lifecycle.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3,7`). Default: all 10.
- `--depth shallow|deep`: `shallow` = check patterns only; `deep` = trace data flow and validate invariants. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: Vulkan Sync | GPU Memory | Pipeline State | Render Pass | Command Recording | Shader Correctness | Resource Lifecycle | Acceleration Structures | RT Ray Queries | Denoiser & Composite

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--depth`
2. `mkdir -p /tmp/audit/renderer`
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/renderer/issues.json`
4. Scan `docs/audits/` for prior renderer reports

## Phase 2: Launch Dimension Agents

### Dimension 1: Vulkan Synchronization
**Entry points**: `crates/renderer/src/vulkan/context/draw.rs` (draw_frame), `crates/renderer/src/vulkan/sync.rs`, `crates/renderer/src/vulkan/context/resize.rs`
**Checklist**:
- Semaphore/fence lifecycle (signal before wait, no double-signal)
- `images_in_flight` tracking correctness
- Swapchain recreation: are all resources properly waited on and destroyed?
- Queue submit ordering (graphics vs present)
- Per-frame fence wait before command buffer reuse
- TLAS build barrier before fragment shader read (`AS_WRITE → AS_READ`, build stage → fragment stage)
- SVGF compute dispatch barrier (compute write → fragment read)
- G-buffer attachment transitions between render pass and compute pass
- Composite pass dependency on SVGF output
**Output**: `/tmp/audit/renderer/dim_1.md`

### Dimension 2: GPU Memory
**Entry points**: `crates/renderer/src/vulkan/buffer.rs`, `crates/renderer/src/vulkan/allocator.rs`, `crates/renderer/src/vulkan/scene_buffer.rs`, `crates/renderer/src/vulkan/acceleration.rs`
**Checklist**:
- gpu-allocator usage: correct memory types (CpuToGpu vs GpuOnly)
- Buffer/image destruction before allocator drop
- Allocator dropped before device destruction
- No leaked VkDeviceMemory on shutdown
- BLAS scratch buffer reuse (high-water mark pattern — never shrinks; verify no use-after-free)
- TLAS instance buffer: HOST_VISIBLE, per-frame, properly sized with padding
- Global vertex/index SSBO growth policy (do they ever need resize? what happens?)
- G-buffer image allocations: properly freed on swapchain recreate
- SVGF history buffers: allocated once, recreated on resize
**Output**: `/tmp/audit/renderer/dim_2.md`

### Dimension 3: Pipeline State
**Entry points**: `crates/renderer/src/vulkan/pipeline.rs`, `crates/renderer/src/vulkan/descriptors.rs`, `crates/renderer/src/vulkan/context/helpers.rs`
**Checklist**:
- Vertex input matches shader layout (binding, location, format, offset)
- Push constant ranges match shader declarations
- Dynamic state correctly set each frame (viewport, scissor)
- Pipeline compatible with render pass (attachment formats, subpass)
- G-buffer pipeline outputs to all 6 render targets (direct, raw indirect, albedo, normal, motion, mesh ID)
- Composite pipeline inputs match G-buffer + SVGF outputs
- SSAO compute pipeline descriptor layout
- Cluster cull compute pipeline descriptor layout
**Output**: `/tmp/audit/renderer/dim_3.md`

### Dimension 4: Render Pass & G-Buffer
**Entry points**: `crates/renderer/src/vulkan/context/helpers.rs` (create_render_pass), `crates/renderer/src/vulkan/gbuffer.rs`
**Checklist**:
- Attachment load/store ops (CLEAR + STORE for all G-buffer outputs)
- Layout transitions (UNDEFINED → COLOR_ATTACHMENT → SHADER_READ for G-buffer targets)
- Subpass dependencies cover all stage/access masks
- G-buffer format choices match shader output types (R16G16B16A16_SFLOAT for normals, R16G16_SFLOAT for motion vectors, R32_UINT for mesh ID)
- Depth attachment format and load/store ops
- G-buffer images created with SAMPLED usage (needed by SVGF and composite reads)
**Output**: `/tmp/audit/renderer/dim_4.md`

### Dimension 5: Command Buffer Recording
**Entry points**: `crates/renderer/src/vulkan/context/draw.rs` (draw_frame command recording)
**Checklist**:
- Reset before re-record (RESET_COMMAND_BUFFER flag on pool)
- Begin/end balanced
- Render pass begin/end balanced
- TLAS build recorded before render pass begin (outside render pass)
- Per-draw state: depth bias for decals, pipeline bind, descriptor set bind, push constants, draw indexed
- Batch coalescing: draws sharing same texture use same descriptor set
- SVGF compute dispatch recorded after render pass end, before composite
- Composite render pass recorded last
- No commands recorded outside render pass that require it (or vice versa)
**Output**: `/tmp/audit/renderer/dim_5.md`

### Dimension 6: Shader Correctness
**Entry points**: `crates/renderer/shaders/triangle.vert`, `crates/renderer/shaders/triangle.frag`, `crates/renderer/shaders/svgf_temporal.comp`, `crates/renderer/shaders/composite.frag`, `crates/renderer/shaders/ssao.comp`, `crates/renderer/shaders/cluster_cull.comp`
**Checklist**:
- SPIR-V matches GLSL source (recompile and diff)
- Push constant struct layout matches Rust-side byte offsets
- Vertex attribute locations match Vertex struct field order
- Descriptor set/binding indices match Rust-side descriptor layout
- TLAS binding (set 1, binding 2) type is `accelerationStructureEXT`
- Global vertex/index SSBO bindings (set 1, bindings 8, 9) match MeshRegistry offsets
- `instance_custom_index` used correctly to index into SSBOs (not `InstanceId`)
- RT flag gating: `sceneFlags.x > 0.5` consistently checked before all ray queries
- Light SSBO struct layout matches Rust GpuLight fields (position, color, direction, params)
- Camera UBO layout: `position.w` = frame counter, `flags.x` = RT enabled
- SVGF temporal shader: motion vector reprojection math, mesh ID rejection, accumulation blend factor
- Composite shader: ACES tone mapping, direct + denoised indirect reassembly
**Output**: `/tmp/audit/renderer/dim_6.md`

### Dimension 7: Resource Lifecycle
**Entry points**: `crates/renderer/src/vulkan/context/mod.rs` (Drop impl, struct fields), `crates/renderer/src/vulkan/context/resize.rs`
**Checklist**:
- All VkShaderModule, VkPipeline, VkPipelineLayout destroyed in Drop
- Framebuffers destroyed before render pass
- Swapchain image views destroyed before swapchain
- Device destroyed after all resources
- AccelerationManager cleanup: all BLAS entries, TLAS states, scratch buffers
- SvgfPipeline cleanup: history images, descriptor sets, pipeline, layout
- GBuffer cleanup: all attachment images and views
- CompositePipeline cleanup: pipeline, layout, descriptor sets
- SSAO cleanup: noise texture, kernel buffer, output image
- TextureRegistry cleanup: all per-texture descriptor sets and images
- Reverse-order teardown of all 54+ VulkanContext fields
**Output**: `/tmp/audit/renderer/dim_7.md`

### Dimension 8: Acceleration Structures (RT)
**Entry points**: `crates/renderer/src/vulkan/acceleration.rs`, `crates/renderer/src/vulkan/context/resources.rs` (build_blas_for_mesh)
**Checklist**:
- BLAS build: vertex format matches Vertex struct (R32G32B32_SFLOAT at offset 0)
- BLAS build: index type matches mesh index buffer (UINT32)
- BLAS build: OPAQUE geometry flag correct (no transparency in AS traversal)
- BLAS build: PREFER_FAST_TRACE flag (not PREFER_FAST_BUILD)
- BLAS scratch buffer: SHADER_DEVICE_ADDRESS usage, proper alignment
- BLAS result buffer: ACCELERATION_STRUCTURE_STORAGE + SHADER_DEVICE_ADDRESS
- TLAS instance buffer: host write → AS read barrier correct
- TLAS build/update decision: `last_blas_addresses` comparison is correct (only device address sequence matters)
- TLAS UPDATE mode: spec requires same geometry count, same instance count — verify
- TLAS padding strategy (max(2x, 4096)) — does UPDATE work with unused instance slots?
- `instance_custom_index` encoding: matches draw command index for SSBO lookup
- Transform matrix conversion: column-major to 3×4 row-major (VkTransformMatrixKHR)
- `TRIANGLE_FACING_CULL_DISABLE` on all instances — correct for two-sided meshes
- Empty TLAS at init: valid descriptors from frame 0 (no validation errors)
- Device address queries: buffer must have SHADER_DEVICE_ADDRESS usage flag
**Output**: `/tmp/audit/renderer/dim_8.md`

### Dimension 9: RT Ray Queries (Shader)
**Entry points**: `crates/renderer/shaders/triangle.frag` (all `rayQueryEXT` usage)
**Checklist**:
- Shadow rays: origin = fragment world position, direction = toward light, tMin/tMax correct
- Shadow rays: point/spot jitter (concentric disk sampling on light physical disk) — correct geometry
- Shadow rays: directional jitter (~2.8° angular cone) — cos/sin correct, unit vector normalization
- Shadow ray result: `gl_RayQueryCommittedIntersectionNoneEXT` check → binary 0/1 shadow
- Contact-hardening penumbra: distance-dependent width scaling formula
- Reflection rays: origin bias along normal to avoid self-intersection
- Reflection rays: direction = reflect(viewDir, normal) — sign conventions correct
- Reflection rays: metalness/roughness gating thresholds (>0.3, <0.6) — consistent with PBR intent
- Reflection hit: barycentric interpolation of UVs from global vertex SSBO — index math correct
- Reflection hit: texture lookup from interpolated UV — descriptor indexing valid
- GI bounce rays: cosine-weighted hemisphere sampling — correct tangent-space construction
- GI bounce rays: distance cutoff (1500 units from camera) — check applied correctly
- GI miss: sky fill contribution — no NaN or inf from miss
- Window portal rays: through-ray direction, 2000-unit distance limit
- Interleaved gradient noise: frame counter seeded correctly, no visible patterns
- All ray queries: `rayQueryInitializeEXT` flags (gl_RayFlagsTerminateOnFirstHitEXT for shadows)
- All ray queries: TLAS binding is the correct descriptor (set 1, binding 2)
**Output**: `/tmp/audit/renderer/dim_9.md`

### Dimension 10: Denoiser & Composite Pipeline
**Entry points**: `crates/renderer/src/vulkan/svgf.rs`, `crates/renderer/src/vulkan/composite.rs`, `crates/renderer/shaders/svgf_temporal.comp`, `crates/renderer/shaders/composite.frag`
**Checklist**:
- SVGF temporal accumulation: history buffer ping-pong correct (read previous, write current)
- SVGF reprojection: motion vector application matches vertex shader output
- SVGF mesh ID rejection: disocclusion detection prevents ghosting
- SVGF blend factor: alpha clamped to valid range, first frame handled (no history → use current)
- SVGF dispatch: workgroup size matches image dimensions (ceiling division)
- SVGF descriptor updates: per-frame history buffer swap
- Composite pass: direct light + denoised indirect + albedo reassembly formula
- Composite pass: ACES tone mapping applied after reassembly (not before)
- Composite pass: fog handled correctly (applied to direct, not indirect)
- Composite pass: output to swapchain image (correct format, correct layout transition)
- SSAO integration: AO factor applied to indirect lighting (not direct)
**Output**: `/tmp/audit/renderer/dim_10.md`

## Phase 3: Merge

1. Read all `/tmp/audit/renderer/dim_*.md` files
2. Combine into `docs/audits/AUDIT_RENDERER_<TODAY>.md` with structure:
   - **Executive Summary** — Total findings by severity, pipeline areas affected
   - **RT Pipeline Assessment** — BLAS/TLAS correctness, ray query safety, denoiser stability
   - **Rasterization Assessment** — Pipeline state, render pass, command recording
   - **Findings** — Grouped by severity (CRITICAL first), deduplicated
   - **Prioritized Fix Order** — Correctness fixes first, then safety, then optimization
3. Remove cross-dimension duplicates

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/renderer`
2. Inform user the report is ready
3. Suggest: `/audit-publish docs/audits/AUDIT_RENDERER_<TODAY>.md`
