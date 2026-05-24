---
description: "Deep audit of the Vulkan renderer — pipeline, sync, memory, shaders, ray tracing, denoiser"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Renderer Audit

Audit the Vulkan renderer for correctness across the full pipeline: rasterization, ray tracing (BLAS/TLAS, ray queries, shadows, reflections, GI), deferred indirect lighting (G-buffer, SVGF), compositing, synchronization, GPU memory, and resource lifecycle.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3,7`). Default: all 20.
- `--depth shallow|deep`: `shallow` = check patterns only; `deep` = trace data flow and validate invariants. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: Vulkan Sync | GPU Memory | Pipeline State | Render Pass | Command Recording | Shader Correctness | Resource Lifecycle | Acceleration Structures | RT Ray Queries | Denoiser & Composite | TAA | GPU Skinning | Caustics | Material Table (R1) | Sky/Weather/Exterior Lighting | Tangent-Space & Normal Maps (M-NORMALS) | Water (M38) | Volumetrics (M55) | Bloom (M58) | M-LIGHT v1 Soft Shadows

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
**Entry points**: `crates/renderer/src/vulkan/buffer.rs`, `crates/renderer/src/vulkan/allocator.rs`, `crates/renderer/src/vulkan/scene_buffer/`, `crates/renderer/src/vulkan/acceleration/`
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
- G-buffer format choices match shader output types (RG16_SNORM octahedral-packed for normals per Schied 2017, R16G16_SFLOAT for motion vectors, **R32_UINT** for mesh ID per `gbuffer.rs::MESH_ID_FORMAT` — 31-bit id + bit 31 (0x80000000) = ALPHA_BLEND_NO_HISTORY flag for SVGF disocclusion (`triangle.frag` `computeMeshId`), encoding ceiling `0x7FFFFFFF`, runtime cap `MAX_INSTANCES = 0x40000` per `scene_buffer/constants.rs::MAX_INSTANCES`, guarded by `debug_assert!` in `context/draw.rs`; see `context/helpers.rs` `encode_mesh_id` and #992)
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
- **DrawCommand input count vs GPU-call count distinction** (#1258, 2026-05-23): `DrawCommand` enumeration is the pre-batch input set; the post-batch GPU draw count is a separate metric. `mem.stats` / `bench-stats` surface both. Regression pattern is reporting a single conflated number — confirm the two counters are independent
- **Blend pipeline pre-pop fast path** (#1259, 2026-05-23): in steady state, the blend pipeline cache hit avoids the full pipeline rebuild on every draw. Verify the cache hit path exists at the per-draw blend-state bind site and is not silently fallen through to the slow path
- **Off-frustum flag assembly skip** (#1260, 2026-05-23): draws outside the camera frustum skip the `GpuInstance.flags` assembly cost (post-batch they're already gated out by `in_raster=false`). Verify the skip path doesn't accidentally drop required state on visible draws bordering the frustum
- **SceneFlags parity at cell-loader spawn boundary** (#1235): every REFR spawned during cell load sees the same `SceneFlags` ((`flags.x` RT-enabled, `jitter.w` is_exterior) as the steady-state draws. Drift here is a 1-frame visual glitch at cell-load completion — verify the spawn path reads `SceneFlags` from the world resource, not a cached snapshot
- **`render_finished` semaphore — per-swapchain-image, not per-frame** (`548c1b69`, fix for VUID-vkQueueSubmit-pSignalSemaphores-00067): if signal is per-frame, queue-submit validation fires when MAX_FRAMES_IN_FLIGHT > swapchain image count. Verify the semaphore is allocated per swapchain image and indexed by `image_index`, not `frame_index`
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
**Entry points**: `crates/renderer/src/vulkan/acceleration/`, `crates/renderer/src/vulkan/context/resources.rs` (build_blas_for_mesh)
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
- **TLAS-scratch shrink alive (#1226, was dead code)** (`cf3d8ec6`, 2026-05-23): `shrink_tlas_scratch_to_fit` now uses TLAS-calibrated slack (was BLAS-calibrated, effectively unreachable). Verify the slack calculation matches `tlas_instance_should_shrink` predicate at `acceleration/predicates.rs` and the function is called from the end-of-frame / cell-unload site
- **`missing_blas` counter split by cause** (#1228, `289fb07a`): three independent counters (skinned, rigid, ssbo_evicted) replace the conflated single counter. Verify all three increment-paths are wired and surfaced through `bench-stats`. Regression pattern: a fix that only updates one bucket while another silently inflates
- **`built_flags` pinned as runtime VUID-03667 flag check** (#1145, `b2fd533f`): `BlasEntry.built_flags` records the BUILD-time AS flag set; the matching refit asserts the pair (UPDATABLE | PREFER_FAST_TRACE | ALLOW_COMPACTION must match the original BUILD). Regression here surfaces as Vulkan validation, not silent corruption
- **`SKINNED_BLAS_FLAGS` / `UPDATABLE_AS_FLAGS` bit composition pinned by tests** (#1144, `a2597487`): the flag-set constants live as module constants in `crates/renderer/src/vulkan/acceleration/`. Drift here is a Vulkan-version-rev breaking change risk; the pin-tests fail loud if either constant flips
- **GpuCamera.rt_flag post-TLAS patch — cell-load warmup gone** (#1227, `6dd40d3f`): first 1-2 frames after a cell load no longer render with RT-disabled. Verify the post-TLAS rt_flag patch is in `draw_frame` and not removed by a future refactor — regression pattern is visible as a 1-2-frame raster-only flash on cell entry
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
- Window portal rays: through-ray direction, 2000-unit distance limit; window-portal demote path on glass that would otherwise infinite-loop (#789 — texture-equality identity check breaks the IOR refraction self-passthrough on coincident glass surfaces)
- IOR refraction: roughness spread basis built via Frisvad orthonormal basis (#820 / REN-D9-NEW-01) — the legacy `cross(N, world-up)` construction degenerates near vertical surfaces; verify Frisvad is in use and tangent/bitangent are unit-length
- IOR refraction: ray budget is `GLASS_RAY_BUDGET = 8192` (raised from 512 in 9a4dc15) — verify cap is wired and not silently exceeded; sky-tint fallback on miss is replaced by cell-ambient for interiors (bb53fd5 — no more open-sky tint inside dungeons)
- IOR refraction diagnostics: `DBG_VIZ_GLASS_PASSTHRU = 0x80` flag exposes the passthrough decision per-fragment for bisecting glass loops; verify it's still wired in `triangle.frag` (symbol-anchored: `DBG_VIZ_GLASS_PASSTHRU` callsites at the diagnostic-state setup + the two viz-write branches in the refraction/glass loop) and not stripped by a refactor
- Interleaved gradient noise: frame counter seeded correctly, no visible patterns
- All ray queries: `rayQueryInitializeEXT` flags (gl_RayFlagsTerminateOnFirstHitEXT for shadows + reflection + glass)
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
- Composite pass: direct light + denoised indirect + albedo + TAA-resolved HDR reassembly formula
- Composite pass: ACES tone mapping applied after reassembly (not before)
- Composite pass: fog handled correctly (applied to direct, not indirect)
- Composite pass: output to swapchain image (correct format, correct layout transition)
- SSAO integration: AO factor applied to indirect lighting (not direct)
- Caustic accumulator (R32_UINT) sampled via `usampler2D`, divided by fixed-point scale, added to direct lighting (not indirect)
**Output**: `/tmp/audit/renderer/dim_10.md`

### Dimension 11: TAA — Temporal Antialiasing (M37.5)
**Entry points**: `crates/renderer/src/vulkan/taa.rs`, `crates/renderer/shaders/taa.comp`, camera-UBO jitter assembly in `byroredux/src/render/camera.rs` and `crates/renderer/src/vulkan/scene_buffer/`
**Checklist**:
- Halton (2,3) sequence: index advances per frame, wraps without seam, jitter applied to projection matrix in NDC pixel units (not clip units)
- Camera UBO carries the un-jittered projection alongside the jittered one (motion-vector reconstruction must use un-jittered)
- Per-frame-in-flight history slot: this frame writes its own slot, next frame reads it via reprojection — confirm `MAX_FRAMES_IN_FLIGHT` slots, no aliasing
- Reprojection: motion-vector sample uses linear filtering (or 5-tap dilation), not point — wrong filter causes edge wobble
- YCoCg neighborhood clamp: 3×3 min/max in YCoCg, prev-frame sample clamped before blend (prevents history bleed during disocclusion)
- Mesh ID disocclusion: prev-frame mesh_id sampled at reprojected UV, mismatch → discard history (use current pixel as pure)
- First-frame / `should_force_history_reset` path: no NaN, no garbage history read, weight α forced to 1.0
- Layout: history images held in `GENERAL` (storage write + sampled read), no UNDEFINED transitions per frame
- Descriptor sets: 7 bindings (curr HDR, motion, curr+prev mesh_id, prev history, out storage, params UBO) match the docstring layout in `taa.rs`
- SPIR-V reflection (`validate_set_layout` from `reflect.rs`) actually fires and matches Rust-side bindings
- Composite samples the TAA output (not the raw HDR) when TAA is on
- Disable path: when TAA off, composite must read raw HDR; the TAA dispatch should be skipped entirely (not run + ignored)
**Output**: `/tmp/audit/renderer/dim_11.md`

### Dimension 12: GPU Skinning Compute + BLAS Refit (M29.5 + M29.3)
**Entry points**: `crates/renderer/src/vulkan/skin_compute.rs`, `crates/renderer/shaders/skin_vertices.comp`, `crates/renderer/src/vulkan/acceleration/` (per-skinned-entity BLAS refit), `byroredux/src/render/skinned.rs` (skinned-mesh enumeration)
**Checklist**:
- `VERTEX_STRIDE_FLOATS = 25` matches `crates/renderer/src/vertex.rs::Vertex` exactly (100 B / vertex; widened from the pre-M-NORMALS 21 / 84 B per #783 once tangent + bitangent_sign landed). Drift here corrupts every skinned vertex
- `SkinPushConstants` (vertex_offset, vertex_count, bone_offset) matches the GLSL `PushConstants` struct in skin_vertices.comp; total ≤ 128 B
- Per-skinned-mesh output buffer usage flags include `STORAGE_BUFFER` AND `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR` (BLAS reads it). M29.3 Phase 3 also re-adds `VERTEX_BUFFER` (#681 / `MEM-2-6` regression note in roadmap)
- Bone palette SSBO is DEVICE_LOCAL with HOST_VISIBLE staging, uploaded once per frame, sized for `MAX_TOTAL_BONES`
- COMPUTE → AS-BUILD → FRAGMENT barrier chain: skin write → BLAS refit → ray-query read in fragment shader (audit barrier scopes match)
- BLAS refit (UPDATE mode) called per frame for skinned entities; geometry count + vertex count must match the original BUILD or Vulkan validation faults
- BLAS refit budget / LRU interaction: skinned BLAS must not be evicted by M36 LRU mid-frame (pin while in flight)
- `MAX_TOTAL_BONES` overflow guard (in `byroredux/src/render/skinned.rs` — `Once`-gated warn at the bone-palette emit site) actually fires when the bone-palette buffer is full — silent truncation past the cap was the regression in M29. Pinned by `byroredux/src/render/bone_palette_overflow_tests.rs`
- Workgroup size 64 matches `local_size_x` in skin_vertices.comp; dispatch uses `(vertex_count + 63) / 64` invocations
- Phase status: confirm whether raster reads inline-skinning (`triangle.vert:147-204`) or pre-skinned (M29.3) — both are valid but cannot coexist in a single mesh
**Output**: `/tmp/audit/renderer/dim_12.md`

### Dimension 13: Caustic Splat (#321 Option A)
**Entry points**: `crates/renderer/src/vulkan/caustic.rs`, `crates/renderer/shaders/caustic_splat.comp`, composite consumption in `crates/renderer/shaders/composite.frag`
**Checklist**:
- Per-frame-in-flight `caustic_accum` images created with `STORAGE | SAMPLED | TRANSFER_DST` and R32_UINT format
- Frame begins with `vkCmdClearColorImage` to zero the accumulator BEFORE the dispatch
- HOST→COMPUTE barrier on params UBO + CLEAR→COMPUTE barrier on the image, both before dispatch
- Atomic accumulation: shader uses `imageAtomicAdd` on a u32 fixed-point representation (no float storage, no race)
- Fixed-point scale documented in CausticParams matches the divide in composite.frag
- COMPUTE→FRAGMENT barrier before composite samples the accumulator (or layout-transition-equivalent dependency)
- Layout stays in `GENERAL` for the accumulator (composite reads via usampler2D, legal in GENERAL)
- 9-binding descriptor set matches the layout table in caustic.rs docstring (depth, normal, mesh_id, lights, camera, instances, TLAS, accum image, params UBO)
- Source-pixel selection (only refractive / water surfaces splat): material flag check pulls from the right field after R1 indirection (`materials[material_id]`, not the legacy GpuInstance copy)
- Output added to direct lighting only — never doubled into the indirect path that SVGF already denoised
**Output**: `/tmp/audit/renderer/dim_13.md`

### Dimension 14: Material Table (R1 Refactor — closed 2026-05-01, hardened 2026-05-04/05, capacity raised 2026-05-23 / `7823eb59`)
**Entry points**: `crates/renderer/src/vulkan/material.rs`, `crates/renderer/src/vulkan/scene_buffer/` (GpuInstance, **`MAX_MATERIALS = 16384`** post-`7823eb59` — was 4096; bump came after Riverwood / FO4 cells saturated the 4096 cap), `byroredux/src/render/mod.rs` (build_render_data), `byroredux/src/render/static_meshes.rs` (material intern call sites), all 5 shaders that declare `struct GpuInstance` (`triangle.vert`, `triangle.frag`, `ui.vert`, `water.vert`, `caustic_splat.comp`) — verify via `grep -l "struct GpuInstance" crates/renderer/shaders/`
**Checklist**:
- `GpuMaterial` is exactly **260 bytes** (`gpu_material_size_is_260_bytes` test pins it; was 272 B until #804 / R1-N4 dropped the unread `avg_albedo_r/g/b` field). Any field add/remove must update both Rust + GLSL `struct GpuMaterial` in lockstep
- Per-field offset pinning (`gpu_material_field_offsets_match_shader_contract`, #806): all 65 named-field offsets across 16 vec4 slots are asserted; size-only pin cannot catch within-vec4 reorders (e.g. swapping `texture_index ↔ normal_map_index`). Any new field must add a matching offset assertion
- ALL `GpuMaterial` fields are scalar (f32/u32) — NEVER `[f32; 3]`. std430 vec3 alignment would silently desync byte-Hash dedup
- Hash + Eq impls treat `GpuMaterial` as raw bytes; named pad fields explicitly zeroed at construction (no uninit bytes from `MaybeUninit`)
- `MaterialTable::intern` produces stable `material_id`s within a frame; identical materials collapse to one entry. **Cap**: over-cap interns return id `0` (sharing the first material's record) with a one-shot `warn!` — verify the warn fires and over-cap entries do NOT corrupt SSBO indexing (#797 SAFE-22)
- Per-frame MaterialBuffer SSBO uploaded once, sized to `min(intern_count, MAX_MATERIALS)`; truncation at the upload site is the safety net, capping at intern is the source of truth (no over-allocation, no reuse-without-resize bug)
- **Capacity overflow surfaced via mem stats** (`7823eb59`, 2026-05-23): when intern overflows `MAX_MATERIALS = 16384`, the over-cap counter is exposed in `mem.stats` for diagnostic visibility. Verify the counter is reset per-frame and the warn-once still fires
- Dedup-ratio telemetry exposed (#780 PERF-N1): unique material count vs placement count surfaced via console — Prospector baseline 1200 placements → 87 unique (~14× hit rate). Regression = audit finding even if correctness holds
- **Shader flag bits routed through generated header** (#1190 / TD4-NEW-01): `MAT_FLAG_*` bits in `crates/renderer/src/shader_constants_data.rs` emit into the auto-generated `crates/renderer/shaders/include/shader_constants.glsl` consumed by `triangle.frag`. Any new flag added Rust-side must NOT be hand-written into the shader — the generated header is the source of truth. Sibling of the `DBG_*` lockstep contract pinned at Dim 16
- **BSLightingShaderProperty PBR scalars at import** (#1241): smoothness / IOR / specular-strength surfaced into `MaterialInfo` so the Disney lobe (Dim 21) has authored data to consume. BGSM smoothness is normalized to glossiness scale at parse (`98383caf`); verify the conversion doesn't double-apply on raw FO4 BGSM authoring
- **WaterShaderProperty + BSShaderPropertyBaseOnly consumers wired at import** (#1243 / #1244, FO3/FNV legacy): water materials and bare BSShaderProperty stubs both route through MaterialInfo now. Verify both produce distinct GpuMaterial entries (no dedup collapse with glass / opaque siblings)
- **HasModelSpaceNormals flag routed for direct-TXST REFRs** (#972): FO4 TXST records with model-space normals (no DerivedNormalMap) flag through to the shader, gating the PBR / SSS path at `triangle.frag` (#1147 Phase 2b)
- `GpuInstance.material_id: u32` ships in the Phase 3+ instance struct; legacy per-instance fields confirmed dropped from Phases 4–6 already-migrated slices
- Shader-side `materials[instance.material_id].foo` reads use the same offsets as the Rust struct (Phase 4–5 mechanical migration check). The #785 R-N1 regression of `ui.vert` reading the wrong MaterialBuffer offset is a recurring trap — verify `ui.vert` is in lockstep with the offset-pin contract, not just `triangle.frag`
- All 5 shaders updated in lockstep — `triangle.vert`, `triangle.frag`, `ui.vert`, `water.vert`, `caustic_splat.comp` (per `feedback_shader_struct_sync.md`). Use `grep -l "struct GpuInstance" crates/renderer/shaders/` as the canonical drift check
- Identity invariant: render output for a scene with N copies of the same material must be byte-identical pre/post R1 dedup
- Phase status check: any per-instance fields that R1 did NOT migrate yet should be flagged (Phase 6 was the closeout — verify nothing remains in DrawCommand/GpuInstance that should now live in GpuMaterial)
**Output**: `/tmp/audit/renderer/dim_14.md`

### Dimension 15: Sky / Weather / Exterior Lighting (M33 / M33.1 / M34)
**Entry points**: `byroredux/src/systems/weather.rs` (weather_system; post-Session-34 split — was in monolithic systems.rs), `byroredux/src/render/sky.rs` (sun arc + TOD palette assembly; post-#1115 split — was in monolithic render.rs), `crates/plugin/src/esm/records/weather.rs`, `crates/renderer/shaders/triangle.frag` (sky gradient + cloud sample + fog application)
**Checklist**:
- `weather_system` advances game time monotonically; sun arc derived from CLMT TNAM hours, not hardcoded
- TOD color interpolation between WTHR NAM0 colors uses the right easing (linear vs cosine) — verify against legacy
- Weather fade (`WeatherTransitionRes`) blends over 8 s post-TOD-sample (color blend AFTER TOD lookup, not before)
- All 4 cloud layers active in exterior cells (M33.1 closed) — layers 2/3 sample ANAM/BNAM with parallax scroll
- Cloud parallax direction vector is in world XY, not screen-space; magnitude scales with TOD wind multiplier
- Sky gradient: zenith → horizon RGB pulled from active TOD palette, applied in non-RT miss-fill path; consistent with the GI miss "sky fill contribution" used in Dim 9
- Sun directional: direction vector from sun arc, color/intensity from TOD, shadow ray budget bounded
- Fog: applied to direct lighting only, NOT to indirect (composite Dim 10 invariant — re-check after sky changes)
- Interior fill at 0.6× ambient + `radius=-1` (unshadowed); `triangle.frag` `isInteriorFill = radius < 0.0` gates RT shadow on `!isInteriorFill` — verify the gate (symbol-anchored, post-#1200 convention) hasn't drifted
- Disabled-WTHR fallback: when no weather record loaded, defaults must produce neutral lighting (no NaN, no pitch-black)
- M40 streaming interaction: cell transition does not strobe TOD (palette is per-worldspace + global TOD clock, not per-cell)
**Output**: `/tmp/audit/renderer/dim_15.md`

### Dimension 16: Tangent-Space & Normal Maps (M-NORMALS, Sessions 26–29)
**Entry points**: `crates/nif/src/import/mesh/tangent.rs` (extract_tangents_from_extra_data, synthesize_tangents), `crates/nif/src/import/mesh/bs_tri_shape.rs` (BSTriShape inline-tangent decode), `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs` (VF_TANGENTS = 0x010, packed-vertex tangent stride; split out post-#1118, 2026-05-20), `crates/renderer/shaders/triangle.frag` (perturbNormal, DBG_BYPASS_NORMAL_MAP / DBG_VIZ_NORMALS / DBG_VIZ_TANGENT)
**Checklist**:
- Oblivion / FO3 / FNV path: per-vertex tangents pulled from `NiBinaryExtraData` named `"Tangent space (binormal & tangent vectors)"` — Bethesda's blob is `[tangents..., bitangents...]` Z-up, but their "tangent" field is actually `∂P/∂V` and "bitangent" is `∂P/∂U` (`CalcTangentSpace` swap). The decoder MUST read the **bitangent half** (offset `num_verts * 12`) into `Vertex.tangent.xyz` and use the tangent half to derive the bitangent sign — handedness regression here was #786 (fixed 5dde345). Audit for any new path that re-reads the blob without honoring the swap
- FO4+ BSTriShape inline tangents: when `VF_TANGENTS | VF_NORMALS` are both set on the packed-vertex flag (`tri_shape/bs_tri_shape.rs::BsTriShape` packed-vertex loop, symbol-anchored per post-#1040 convention), tangents ship inline in the packed-vertex blob, NOT in a separate `NiBinaryExtraData`. This is distinct from the Skyrim path; verify the FO4 inline decode (#795 / #796, b63ab0c) still fires and is not gated behind the wrong BSVER
- Synthesized fallback: when the authored blob is missing or malformed (size mismatch warns, see `mesh.rs:87`), the importer falls through to nifly's `CalcTangentSpace` synthesis (`synthesize_tangents`). Verify the fallback path produces unit-length tangents and consistent bitangent signs
- Bitangent sign convention: `B = bitangent_sign * cross(N, T)` — the sign is reconstructed shader-side from `Vertex.tangent.w`. Verify the convention is consistent across the three import paths (Bethesda authored, FO4 inline, synthesized)
- Coordinate conversion: Z-up (Gamebryo) → Y-up (renderer) applied to tangent xyz components in lockstep with normal conversion (no path that converts N but not T, or vice versa)
- `perturbNormal` is **default-on** (#787 / #788, b8ab477) with the Path-1 transform fixed; `DBG_BYPASS_NORMAL_MAP = 0x10` is the runtime opt-out for bisecting (`crates/renderer/shaders/triangle.frag::DBG_BYPASS_NORMAL_MAP`). Verify the bit is still recognized
- Permanent diagnostic bit catalog (Rust source-of-truth at `crates/renderer/src/shader_constants_data.rs::DBG_*`; emitted by `build.rs` into the auto-generated `crates/renderer/shaders/include/shader_constants.glsl` which `triangle.frag` `#include`s): `DBG_BYPASS_POM = 0x1`, `DBG_BYPASS_DETAIL = 0x2`, `DBG_VIZ_NORMALS = 0x4`, `DBG_VIZ_TANGENT = 0x8`, `DBG_BYPASS_NORMAL_MAP = 0x10`, `DBG_RESERVED_20 = 0x20` (formerly `DBG_FORCE_NORMAL_MAP`, no-op post-#1035), `DBG_VIZ_RENDER_LAYER = 0x40`, `DBG_VIZ_GLASS_PASSTHRU = 0x80`, `DBG_DISABLE_SPECULAR_AA = 0x100`, `DBG_DISABLE_HALF_LAMBERT_FILL = 0x200`. Pinned in lockstep by two tests in `crates/renderer/src/shader_constants.rs`: `tests::generated_header_contains_all_defines` (positive side — every `DBG_*` const is emitted into the generated header with the correct value) and `tests::triangle_frag_dbg_bits_not_redeclared` (negative side — `triangle.frag` does NOT redeclare the const block, polarity-flipped post-#1162 when the consts moved out of the shader). Audit for drift: any added bit must not collide; any dropped bit must not orphan shader code
- "Chrome posterized walls" red herring: per `feedback_chrome_means_missing_textures.md`, that artifact is the magenta-checker placeholder × a (correctly loaded) tangent-space normal map. Audit findings claiming a tangent-space bug from chrome fragments alone are stale — the audit MUST run `tex.missing` first before recommending a tangent-space fix
**Output**: `/tmp/audit/renderer/dim_16.md`

### Dimension 17: Water Rendering (M38)
**Entry points**: `crates/renderer/src/vulkan/water.rs` (WaterPipeline), `crates/renderer/shaders/water.vert/frag`, `byroredux/src/cell_loader/water.rs` (water-plane spawn from XCWT / cell water refs), `byroredux/src/systems/water.rs` (`submersion_system`), `byroredux/src/components.rs` (WaterPlane, WaterVolume, SubmersionState components)
**Checklist**:
- WaterPlane ECS component spawned from interior/exterior cell water records; height + extent match cell record
- WaterPipeline vertex displacement: amplitude bounded, no NaN at edge tessellation, no Z-fighting with shoreline geometry
- Fresnel term: schlick approximation against view dir, base reflectance ~0.02 for water (do not reuse glass IOR 1.5)
- RT reflection ray: TLAS query reflected about water normal; missed rays sample sky (not black, not magenta)
- RT refraction ray: TLAS query along refracted dir, IOR ~1.33; missed rays sample backdrop with proper fog
- Camera SubmersionState write: `submersion_system` flips component when camera Y crosses water plane; underwater fog / tint applied via composite (verify no per-frame strobe at the boundary)
- Cell unload: water entities despawn cleanly; no leaked BLAS entries against post-unload TLAS (`AccelerationManager::evict_unused_blas` covers them)
- Shadow casting: water surface does NOT contribute to shadow rays for opaque geometry (reflection-only)
- Two-sided rendering: water disables back-face cull (underwater view from below); confirm via dynamic CULL_MODE not pipeline duplicate
- Sort key: water plane rendered after opaques, before transparents (or in transparent pass with alpha-blend) — verify against `render::sort_key` ordering
- Material slot: water uses a distinct GpuMaterial entry (separate from glass) — verify dedup doesn't collapse them
- **Water-side caustic synthesis — FULLY LANDED across #1210 Phases A-E** (2026-05-22 → 2026-05-23). The 2026-05-19 audit's "deferred or wired?" question is now answered: WIRED. Verify each phase is still live:
  - **Phase A+B** (`8a1a06b4`): `sun_direction` plumbed through `CameraUBO` — required input for water-side caustic synthesis. Verify `camera_ubo.sun_direction.xyz` is uploaded each frame and not stale-from-init
  - **Phase C** (`5f1a9158` / #1255): `water_caustic_accum` image lifecycle correct (per-frame-in-flight, R32_UINT, STORAGE|SAMPLED|TRANSFER_DST). Layout: GENERAL for compute write, TRANSFER_DST for clear, GENERAL for composite sample. Verify reverse-order destroy in `WaterPipeline::destroy`
  - **Phase D** (`19dfc79c` / #1256): water-side caustic synthesis ACTIVE in `water.frag` — verify the synthesis code (sun-direction-based caustic pattern) actually runs and writes to `water_caustic_accum` via `imageAtomicAdd`. Sibling check: `caustic_splat.comp:213-215`'s "deferred to water.frag" comment should now match a live water.frag implementation, not a stub
  - **Phase E** (`c87ca9db` / #1257): composite samples the accumulator at the gather site (composite.frag) and adds to direct lighting (not indirect — same invariant as the glass/MultiLayerParallax caustics from Dim 13)
  - **INSTANCE_FLAG_CAUSTIC_SOURCE** (#1234): `caustic_splat.comp` uses the named macro from `shader_constants_data.rs`, not a hex literal. Sibling of the `MAT_FLAG_*` lockstep contract (Dim 14)
**Output**: `/tmp/audit/renderer/dim_17.md`

### Dimension 18: Volumetric Lighting (M55)
**Entry points**: `crates/renderer/src/vulkan/volumetrics.rs` (160×90×128 froxel grid, ~14 MiB / slot), `crates/renderer/shaders/volumetrics_inject.comp`, `crates/renderer/shaders/volumetrics_integrate.comp`, composite consumption in `crates/renderer/shaders/composite.frag`
**Checklist**:
- Froxel dimensions `160 × 90 × 128` match `volumetrics_inject.comp` `local_size_x/y/z`; dispatch group count covers exactly the grid (no over-dispatch)
- Per-frame-in-flight buffer sizing: one ~14 MiB image per frame-in-flight slot, not shared across frames (avoid WAR hazard on integrate read)
- Inject pass: per-froxel HG phase scattering, single shadow ray vs TLAS per froxel — verify `gl_RayFlagsTerminateOnFirstHitEXT` is set so cost stays bounded
- Integrate pass: depth-walk integration produces an accumulated luminance + transmittance per froxel; transmittance is multiplied (not added) across the walk
- HG phase function: anisotropy `g` clamped to (-0.999, 0.999) — `g = ±1` produces division-by-zero
- Output consumed: composite shader samples the integrated 3D image at fragment depth → world Z mapping; the gate `VOLUMETRIC_OUTPUT_CONSUMED: bool` (#928) must remain in lockstep — if composite path drops the sample, the dispatch must be skipped
- Disabled path: when volumetrics is off, integrate dispatch is skipped entirely (not dispatched + ignored — was the audit finding from #928)
- Resize: image-view rebind on composite resize (#905); verify both volumetric and composite descriptor sets get refreshed
- Sun-arc dependency: scattering color/intensity reads from the TOD palette; interior cells with no exterior sun must still produce neutral non-NaN output
- Performance: budget allows the inject+integrate pair to complete in <2 ms on RTX 4070 Ti at the documented exterior bench
**Output**: `/tmp/audit/renderer/dim_18.md`

### Dimension 19: Bloom Pyramid (M58)
**Entry points**: `crates/renderer/src/vulkan/bloom.rs` (5-mip down + 4-mip up, B10G11R11_UFLOAT), `crates/renderer/shaders/bloom_downsample.comp`, `crates/renderer/shaders/bloom_upsample.comp`, composite addition in `crates/renderer/shaders/composite.frag`
**Checklist**:
- Pyramid size: 5 down-mips + 4 up-mips; each mip is half the previous in X+Y; format `B10G11R11_UFLOAT` everywhere (no R16G16B16A16 mid-chain)
- Down-pass: 4-tap bilinear box filter — sample offsets at half-pixel centers, weights sum to 1.0
- Up-pass: 4-tap bilinear additive blend with previous up-mip; no clamp to [0,1] (HDR additive)
- Per-frame-in-flight slot owns its own mip chain — cross-frame WAR is gated by the per-frame fence (#931 audit: do NOT reintroduce the 9 redundant pre-barriers that were removed)
- Barrier count: 10 barriers per dispatch (down to 47% from pre-#931's 19); regression = audit finding even if correctness holds
- Intensity: composite multiplies bloom by 0.15 (tuned down from 0.20 on Prospector saloon); the constant lives in `composite.frag` — drift here is a visual regression
- Image-view rebind on composite resize (#905) — verify bloom + composite descriptor sets both refresh
- Disabled path: when bloom is off, neither down nor up is dispatched; composite must short-circuit the addition
- Tone-map order: bloom is added BEFORE ACES tone mapping (HDR addition), not after (LDR addition would clip)
- Source pyramid input: must read the un-tone-mapped HDR (`composite.frag` input), not the TAA output (TAA inputs to bloom is a regression pattern — verify the descriptor binding)
**Output**: `/tmp/audit/renderer/dim_19.md`

### Dimension 21: Disney BSDF / PBR Gating (#1248-#1252, 2026-05-21 → 2026-05-22)
**Entry points**: `crates/renderer/shaders/triangle.frag` (Disney lobe functions + 4 gate sites), `crates/renderer/src/shader_constants_data.rs` (`MAT_FLAG_BGSM_PBR`), `crates/renderer/src/vulkan/material.rs` (Disney material preset table from #1251)
**Checklist**:
- **Gate is `MAT_FLAG_BGSM_PBR` (bit 5) only** — verify the 3 consumer sites in `triangle.frag` ALL test this single flag (no path lights FNV / FO3 / Skyrim legacy materials with the Disney lobe). Gate-site grep target: `if ((mat.materialFlags & MAT_FLAG_BGSM_PBR) != 0u)`. Expected count: 3 — `:1652` (F0 derivation: authored specular for PBR vs metallic-workflow F0 for legacy), `:2447` (Disney diffuse consumer in the main `diffuseBrdf` branch), `:2669` (second Disney diffuse consumer in the deferred-specular path). Drift here is a per-game lighting regression — FNV's Lambert-only world would suddenly get Burley retro-reflection
- **`dielectricF0FromIor(eta)`** (#1248, `454b7a26`): Fresnel F0 derivation from per-material IOR (not the legacy hardcoded 0.04). Input-domain clamp on `eta` landed via #1253 (`e8e66b61`) — verify the clamp protects against `eta = 0` / `eta < 0` from corrupted material authoring. Symbol-anchored at `triangle.frag::dielectricF0FromIor`
- **`distributionGGXAniso(NdotH, HdotX, HdotY, ax, ay)`** (#1250, `c0374d00`): anisotropic GGX (Trowbridge-Reitz with ax ≠ ay) for brushed-metal / vinyl / satin authoring. **MUST degenerate exactly to `distributionGGX` when ax == ay** — that's the legacy compatibility contract, regression would break every isotropic material in the catalog. Symbol-anchored at `triangle.frag::distributionGGXAniso`
- **`deriveAxAy(roughness, anisotropic, out ax, out ay)`** (#1250 + #1253/#1254): remaps perceptual roughness + `anisotropic[0..1]` (ByroRedux follows the GLSL-PathTracer half-axis convention, NOT the Disney 2012 paper's full `[-1, 1]`; see comment at `triangle.frag:646-651`) to ax/ay. Input-domain clamp `clamp(anisotropic, 0.0, 1.0)` at `triangle.frag:652` landed in #1254 — verify the clamp returns a fully-degenerate-needle behavior at `anisotropic = 1.0` (Disney convention), not black/undefined fragments from `sqrt(1 - 0.9·a) < 0`
- **`disneyDiffuseSplit(...)`** (#1249 / #1252, `005eba25` + `32e768af`): Burley retro-reflection + sheen + Hanrahan-Krueger subsurface, **split into separate lobes** post-#1252 to prevent per-light × π sheen over-amplification. Returns `DisneyDiffuseSplit` struct. Verify sheen is NOT divided by π (Disney 2012 spec: layered material on Lambertian base — sheen is additive, not a Lambertian itself)
- **Disney material preset table** (#1251, `c09d63a6`): documented presets (glass IOR 1.45, copper, gold, plastic, etc.) live in `crates/renderer/src/vulkan/material.rs` as `pub fn` constructors. Verify each preset's authoring matches Disney 2012 documented values (cross-reference the GLSL-PathTracer reference at `/mnt/data/src/reference/GLSL-PathTracer/` per the `reference_glsl_pathtracer.md` memory; the disney lobe lives under `src/shaders/common/` in that external repo — backticked only when checking the local copy)
- **GLSL-PathTracer reference compliance**: Disney lobe math at `triangle.frag` cites GLSL-PathTracer line numbers in the doc comments (e.g. disney.glsl:56-57 for F0, :67-87 for diffuse — see external reference repo, not a local path). Verify the citations are still accurate — if GLSL-PathTracer upstream has changed, our port is now an undocumented divergence
- **#1147 Phase 2b gate sibling** (`fe22e64c`, 2026-05-23): PBR / SSS / model-space-normals are ALSO gated separately in `triangle.frag` (NOT all behind `MAT_FLAG_BGSM_PBR`). Verify:
  - `MAT_FLAG_BGSM_TRANSLUCENCY (1u << 6)` → SSS path (sibling modulators: `MAT_FLAG_BGSM_TRANSLUCENCY_THICK_OBJECT (1u << 8)` and `MAT_FLAG_BGSM_TRANSLUCENCY_MIX_ALBEDO (1u << 9)` select the SSS sub-mode)
  - `MAT_FLAG_BGSM_MODEL_SPACE_NORMALS (1u << 7)` (set by #972 for direct-TXST REFRs) → model-space sampling path
  - Each gate fires independently; no spurious cross-activation when only one flag is set
- **Per-game regression-guard checklist** (extends `audit-fnv` / `audit-fo3` / `audit-skyrim` regression guards):
  - **FNV**: zero materials set `MAT_FLAG_BGSM_PBR` (no BGSM authoring shipped). Disney lobe must be unreachable.
  - **FO3**: same as FNV.
  - **Oblivion**: same as FNV.
  - **Skyrim LE / SSE**: BGSM via mods only; vanilla Skyrim materials should not trip the gate without explicit BGSM intent.
  - **FO4 / FO76 / Starfield**: BGSM authoring is canonical — Disney lobe is the EXPECTED path. Regression pattern is the opposite: a FO4 BGSM that falls back to Lambert.
**Output**: `/tmp/audit/renderer/dim_21.md`

### Dimension 20: M-LIGHT v1 — Stochastic Soft Shadows
**Entry points**: `crates/renderer/shaders/triangle.frag` (sun shadow ray + cone-sample), `byroredux/src/render/sky.rs` (`sun_angular_radius` upload), `crates/renderer/src/vulkan/scene_buffer/`
**Checklist**:
- `sunAngularRadius` ships in the camera/scene UBO at the documented offset; current shipping value `0.020` (bumped from `0.0047`) — drift here changes shadow softness globally
- Per-fragment single-tap stochastic cone sample around the sun direction: random offset derived from frame index + pixel coords (deterministic per-pixel-per-frame), not a true RNG that breaks TAA history
- Shadow ray flags include `gl_RayFlagsTerminateOnFirstHitEXT` (no closest-hit needed; visibility query only)
- TAA accumulation absorbs the per-frame noise — verify the YCoCg clamp tolerance allows the noise to converge (too-tight clamp = persistent noise; too-loose = over-blur)
- Interior `radius=-1` gate (`triangle.frag` `isInteriorFill = radius < 0.0`, symbol-anchored) still bypasses the cone sample — soft-shadow code must not fire inside interior cells
- Disocclusion: when TAA mesh-id mismatches discard history, the un-converged single-sample frame is visible — verify the fallback isn't black (a black single-sample frame is the regression pattern)
**Output**: `/tmp/audit/renderer/dim_20.md`

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
