# Vulkan Renderer

The renderer is a Vulkan 1.3 implementation built on `ash` (raw Vulkan
bindings), `ash-window` (surface creation), and `gpu-allocator` (GPU
memory). It supports `VK_KHR_ray_query` for hardware ray-traced shadows,
reflections, and 1-bounce GI on RTX-class hardware and falls back to
non-RT rendering on devices without the extension. The Vulkan 1.3
`Synchronization2` device feature is enabled.

Source: `crates/renderer/src/vulkan/`

> **Status note (2026-05-28).** This doc was last fully reconciled at
> Session 42. The Session-12 (2026-04) material/RT narrative is preserved
> below and extended forward through the work that landed since: the
> material-table R1 refactor (#785), the Disney BSDF port (#1248–#1257),
> water + water-side caustics (M38 / #1210), bloom (M58) and volumetrics
> (M55) scaffolds, GPU bone-palette skinning (M29.5/M29.6), the egui debug
> overlay, per-pass GPU timers (#1194), and the Session-35 `acceleration/`
> + `scene_buffer/` submodule splits. Per-frame benchmark numbers are
> tracked in [ROADMAP.md](../../ROADMAP.md), not duplicated here.

## High-level capabilities

- **Full Vulkan init chain** with validation layers in debug builds
- **RT acceleration structures**: BLAS per mesh + per-skinned-entity BLAS
  refit + TLAS rebuilt per frame in `DEVICE_LOCAL` memory with HOST→AS_BUILD
  memory barriers, LRU eviction (budget = `device_local / 3`, floored at
  256 MB), `ALLOW_COMPACTION` + async occupancy query + compacted copy
  (M36: 20–50% BLAS memory reduction), batched build submission
- **Multi-light SSBO**: up to `MAX_LIGHTS = 512` point / spot / directional
  lights consumed by the fragment shader, with `VK_KHR_ray_query` shadow
  rays per light and a per-light `falloff_exponent` attenuation contract
- **Streaming RIS direct lighting** (M31.5): 16 independent weighted reservoirs
  per fragment (`NUM_RESERVOIRS = 16`), each sampled from the full cluster
  proportional to luminance;
  unbiased W = resWSum / (K · w_sel) clamped at 64×
- **G-buffer** (5 attachments + the HDR color target): octahedral normal,
  motion vector, mesh ID, raw indirect, albedo
- **SVGF temporal denoiser** for indirect lighting — motion-vector reprojection,
  mesh-id disocclusion, albedo-demodulated accumulation
- **Composite pass**: direct + denoised indirect reassembly (multiplies
  indirect by local albedo per #268's invariant) + water/glass caustic
  accumulators + bloom add, with ACES tone mapping
- **TAA** (M37.5): `taa.comp` with Halton(2,3) projection jitter (period 16
  per #1093), motion-vector reprojection, Catmull-Rom 9-tap history resample,
  3×3 YCoCg neighborhood variance clamp (γ = 1.25), mesh-id disocclusion,
  luma-weighted α = 0.1 blend
- **Clustered lighting** (`cluster_cull.comp`) — 16×9×24 froxel grid
  (`CLUSTER_TILES_X/Y` × `CLUSTER_SLICES_Z`), up to 128 lights per cluster,
  frustum assignment for direct shading candidates
- **SSAO** (`ssao.comp`) compute pass with noise texture + kernel samples
- **Disney BSDF** (#1248–#1257): per-material IOR → Fresnel F0, Burley
  retro-reflective diffuse + Hanrahan-Krueger fake-SSS subsurface + Fresnel
  sheen, anisotropic GGX (`ax`/`ay` from perceptual roughness + anisotropy);
  gated on the game-agnostic `MAT_FLAG_PBR_BSDF` material flag. Reference
  impl + canonical values from knightcrawler25's GLSL-PathTracer (MIT)
- **Water** (M38): `water.vert`/`water.frag` surface pipeline sharing the
  main render pass; RT reflection/refraction + sun-driven caustic synthesis
  (#1210), underwater tint
- **Bloom** (M58): separable down/up pyramid (`bloom_downsample.comp` +
  `bloom_upsample.comp`, `BLOOM_MIP_COUNT = 5`) added to scene HDR before
  tone-mapping
- **Volumetric lighting** (M55): froxel 3D-texture scaffold
  (`volumetrics_inject.comp` + `volumetrics_integrate.comp`); allocation +
  layout + dispatch plumbing live, scattering output not yet consumed
  (`VOLUMETRIC_OUTPUT_CONSUMED = false`)
- **egui debug overlay**: dedicated `LOAD`-op render pass over the swapchain,
  driven by `egui-ash-renderer` (shares the engine allocator)
- **Per-pass GPU timers** (#1194): one `VkQueryPool` per frame-in-flight slot
  bracketing every major GPU pass for measured (not guessed) per-pass cost
- **Pipeline cache** persisted to disk (10–50 ms cold init → <1 ms warm)
- **Shared `VkSampler`** across all textures, 16× anisotropic filtering
  when the device exposes it
- **Per-image semaphore synchronization** with 2 frames in flight
- **Atomic swapchain handoff** during resize (no dropped frames, no torn submits)
- **Deferred texture/mesh destruction** (two-frame countdown) so dynamic UI
  texture updates don't stall the graphics queue
- **Depth format autodetect** with fallback chain D32→D32S8→D24S8→D16
- **Backface culling** with confirmed NIF/D3D clockwise winding convention
- **DDS texture loading**: BC1 / BC3 / BC5 + DX10 with mipmaps
- **Global geometry SSBO** (#294): per-draw vertex/index buffer rebinds
  eliminated — one SSBO indexed by `mesh_handle` for all meshes
- **Multi-draw indirect**: identical meshes collapse into one
  `cmd_draw_indexed_indirect` per batch (#309), instead of per-batch
  per-instance `cmd_draw_indexed`

### Historical changelog (date-stamped)

- **Session 11 sync/cache hardening** (#313, #316, #317, #392, #415, #416,
  #420, #422, #426): `VkPipelineCache` threaded through every pipeline
  create site, per-(src, dst, two_sided) blend pipeline cache, TLAS build
  barrier widened to COMPUTE_SHADER, `TRIANGLE_FACING_CULL_DISABLE` on
  TLAS instances gated on the `two_sided` flag, `gl_RayFlagsTerminateOnFirstHitEXT`
  on reflection + glass rays, SVGF history age as a weighted average
  (Schied 2017 §4.2), BLAS compaction leak fixed on partial OOM,
  empty-TLAS `VUID-VkBufferCopy-size-01988` / `-size-01188` suppression
  via size=0 guard, and an opt-in global lock-order graph for cross-thread
  ABBA detection.
- **Session 12 material plumbing + RT correctness** (2026-04 sweep):
  BSShaderTextureSet slots 3/4/5 (parallax / env cube / env mask)
  routed to the per-instance shader struct with `parallaxHeightScale` /
  `parallaxMaxPasses` (#452 + #453), parallax-occlusion fragment branch
  guarded on a non-zero parallax index using screen-space derivatives,
  window portal ray fires along `-N` not `-V` with a grazing-angle gate
  so oblique windows actually transmit (#421). SPIR-V reflection via
  `rspirv` cross-checks every descriptor layout against the shader's
  declared bindings at pipeline create time (#427); mismatch = hard fail
  instead of silent undefined behaviour at dispatch.
- **GPU skinning** (M29 Phase 1+2, sessions 18–20; bone-palette M29.5/M29.6):
  see the [GPU skinning](#gpu-skinning) section below.
- **Caustic scatter** (M22 / #321 Option A): `caustic_splat.comp` projects
  refracted-light splat per refractive-source pixel into a scalar
  R32_UINT accumulator; composite samples and adds it to direct lighting.
  RT-enabled gates land in #640 — both shader-side (`sceneFlags.x < 0.5`
  early-out) and CPU-side (skip dispatch when no TLAS handle for the
  frame), plus `OPAQUE | TerminateOnFirstHit` ray flags.
- **Material-table R1 refactor** (#785, post-Session-12): the per-material
  fields that previously lived on every per-instance struct were collapsed
  into a deduped per-frame `MaterialTable` SSBO indexed by
  `material_id: u32`. `GpuInstance` shrank to 112 bytes; `GpuMaterial` is
  300 bytes. See [Material table](#material-table-r1).
- **Disney BSDF** (#1248–#1257, 2026-05): IOR-derived Fresnel F0, Burley +
  HK + sheen diffuse lobe, anisotropic GGX, input-domain clamps.
- **Water + water-side caustics** (M38 / #1210 Phases A–E, 2026-05-22/23):
  `water_caustic_accum` per-FIF image cleared before the main render pass,
  written by `water.frag`'s `imageAtomicAdd`, sampled by composite.
- **Material-feature flag rename** (Stage 3, `ae364e29`): the per-material
  feature bits dropped their `BGSM_` prefix; the shader reads game-agnostic
  `MAT_FLAG_*` names. The five effect flags come from the build-generated
  shader include; the PBR/Disney bits are hand-`#define`d in `triangle.frag`.

## Module map

```
crates/renderer/src/vulkan/
├── mod.rs              Re-exports
├── instance.rs         Create VkInstance with extensions + validation layer
├── debug.rs            VK_EXT_debug_utils → log crate
├── surface.rs          ash_window::create_surface wrapper
├── device.rs           Physical + logical device selection (queue families,
│                       extensions, RT + Synchronization2 capability detection)
├── swapchain.rs        Format / present mode / extent selection, image views
│                       (FIFO/MAILBOX with a `present_mode` override)
├── sync.rs             Per-frame semaphores + fences (2 frames in flight)
├── allocator.rs        gpu-allocator wrapper for VkBuffer / VkImage
├── buffer.rs           Vertex / index / uniform buffer creation, staging
├── pipeline.rs         Graphics pipeline + pipeline cache persistence
├── descriptors.rs      Descriptor set layouts (UBO, SSBO, samplers, AS)
├── compute.rs          Compute pipeline utilities (shared by SSAO/SVGF/TAA/
│                       cluster_cull/skin/bloom/volumetrics)
├── material.rs         GpuMaterial + MaterialTable::intern (R1 dedup), the
│                       MAT_FLAG_* / MATERIAL_KIND_* feature-flag constants
├── scene_buffer/       Per-frame scene SSBO/UBO (Session 35 split into:
│                       constants / gpu_types / buffers / upload / descriptors)
├── acceleration/       BLAS + TLAS lifecycle (Session 35 split into:
│                       constants / types / predicates / blas_static /
│                       blas_skinned / tlas / memory). Per-skinned-entity
│                       BLAS refit, scratch buffer reuse + serialise barrier,
│                       LRU eviction, compaction (ALLOW_COMPACTION + query + copy)
├── skin_compute.rs     M29 GPU pre-skinning + M29.5/M29.6 bone-palette
│                       compute pipelines, per-entity SkinSlot output SSBO,
│                       persistent bind-inverses SSBO + slot pool
├── gbuffer.rs          G-buffer attachments: octahedral normal, motion
│                       vector, mesh ID, raw indirect, albedo
├── svgf.rs             SvgfPipeline — temporal accumulation denoiser for
│                       indirect lighting (motion reprojection + mesh-id
│                       disocclusion + albedo-demodulated history)
├── taa.rs              TaaPipeline — Halton jitter, Catmull-Rom resample,
│                       YCoCg variance clamp, per-FIF ping-pong history
├── caustic.rs          CausticPipeline — refracted-light splat compute
│                       pass driving the glass/MultiLayerParallax accumulator (#321)
├── water.rs            WaterPipeline — water-surface draw variant of the
│                       main render pass (M38), RT reflection/refraction
├── water_caustic.rs    Per-FIF R32_UINT accumulator for water-side caustics
│                       (#1255 / Phase C of #1210), cleared before the render pass
├── composite.rs        CompositePipeline — direct + denoised indirect +
│                       caustic + bloom reassembly, ACES tone mapping
├── ssao.rs             SSAO compute pipeline (noise texture, kernel)
├── bloom.rs            Bloom pyramid (M58) — separable down/up compute passes
├── volumetrics.rs      Froxel volumetric scaffold (M55) — 3D-texture
│                       allocation + inject/integrate dispatch (output not
│                       yet consumed)
├── egui_pass.rs        egui debug overlay render pass (LOAD-op over swapchain)
├── gpu_timers.rs       Per-pass VkQueryPool timestamp brackets (#1194)
├── reflect.rs          SPIR-V reflection cross-check (`rspirv`) — every
│                       descriptor layout validated against shader bindings
│                       at pipeline create time (#427)
├── texture.rs          DDS upload (RGBA + BC*), staging, layout transitions
├── dds.rs              DDS header parser (BC1/BC3/BC5 + DX10 extended, mips)
└── context/
    ├── mod.rs          VulkanContext struct definition + new() / Drop
    ├── draw.rs         draw_frame() — per-frame command recording + submission
    ├── resize.rs       recreate_swapchain() — atomic handoff on window resize
    ├── resources.rs    build_blas_for_mesh, register_ui_quad, log_memory_usage,
    │                   swapchain_extent, rebind_hdr_views
    ├── helpers.rs      find_depth_format, create_render_pass, create_framebuffers
    └── screenshot.rs   ScreenshotBridge / readback copy for debug captures
```

The split between `vulkan/*.rs` (low-level building blocks and per-pass
pipelines) and `vulkan/context/*.rs` (the orchestration layer that owns all
of them) keeps each file manageable. `VulkanContext::new()` walks the init
chain; `VulkanContext::draw_frame()` records and submits a frame.

## Initialization Chain

`VulkanContext::new()` performs the full init chain. Each step is in its
own module so the call sequence reads as a recipe:

| Step | Module | What |
|---|---|---|
| 1 | — | Load Vulkan entry points via `ash::Entry::load()` |
| 2 | `instance.rs` | Create `VkInstance` (API 1.3, platform extensions, validation layer if debug) |
| 3 | `debug.rs` | Set up `VK_EXT_debug_utils` messenger routing to the `log` crate |
| 4 | `surface.rs` | Create `VkSurfaceKHR` from raw window handles via `ash-window` |
| 5 | `device.rs` | Pick physical device — checks swapchain extension, queue families, `VK_KHR_ray_query`, and `Synchronization2` |
| 6 | `device.rs` | Create logical device with graphics + present queues + RT extensions when available |
| 7 | `allocator.rs` | Create the `gpu-allocator` instance (Vulkan backend) |
| 8 | `swapchain.rs` | Create the swapchain (sRGB B8G8R8A8, present mode chosen with a `present_mode` override, clamped extent) |
| 9 | `context::helpers` | Pick depth format (D32→D32S8→D24S8→D16) and create the depth image |
| 10 | `context::helpers` | Create the render pass (color + depth, CLEAR load op, PRESENT_SRC final layout) |
| 11 | `context::helpers` | Create framebuffers per frame-in-flight slot (HDR + G-buffer + depth attachments) |
| 12 | `pipeline.rs` | Load the pipeline cache from disk (if present) and create the graphics pipeline |
| 13 | `descriptors.rs` | Create descriptor set layouts for the UBO/SSBO/sampler/AS bindings |
| 14 | `context::mod` | Create the command pool (per-buffer reset) and command buffers |
| 15 | `sync.rs` | Create per-frame sync objects (2 frames in flight, fences signaled) |

The `VulkanContext` struct holds the device, queues, swapchain images,
framebuffers, descriptor sets, pipelines, sync objects, mesh registry,
texture registry, scene SSBOs, RT acceleration manager, and all the
per-pass pipelines (SVGF, TAA, caustic, water, bloom, volumetrics, SSAO,
composite, egui). [`Drop`](../../crates/renderer/src/vulkan/context/mod.rs)
tears everything down in reverse order under `device_wait_idle()`, with a
panic-unwind guard (#1128) so the teardown `debug_assert`s don't fire
during a panic. **Important ordering constraint (#1406):** `AllocatorResource`
must be removed from the ECS `World` *before* `VulkanContext::drop()` is
reached — the allocator's own `Drop` holds a live `Arc<Device>` and will
call into the Vulkan driver; if the `World` outlives the `VulkanContext`
the allocator fires after the logical device has already been destroyed.

## Per-frame draw

`VulkanContext::draw_frame(world, ...)` in [`vulkan/context/draw.rs`](../../crates/renderer/src/vulkan/context/draw.rs) does (roughly, in order):

1. Wait for the in-flight fence for this frame slot **and** the previous
   slot (cross-slot wait — the shared scratch / SSBO invariant).
2. Read back any pending screenshot from the prior frame.
3. Acquire the next swapchain image (handle `ERROR_OUT_OF_DATE_KHR` → swap).
   Bracketed by a GPU timer so a FIFO-present block is attributable.
4. Reset the fence and command buffer; run the deferred-destroy tick
   (after the wait, so freed handles are guaranteed GPU-idle — #418).
5. Walk the ECS via `build_render_data` to collect visible mesh handles,
   transforms, the per-frame `MaterialTable`, light sources, decals, and
   skinned-mesh bone palettes.
6. Update the camera UBO (`GpuCamera`, 304 bytes) — view + prev-view-proj
   + inverse(viewProj) + **Halton jitter** for TAA + `rt_flag` (1.0 only
   when ray_query is supported AND the TLAS is written for this frame;
   `patch_camera_rt_flag` flips it in-place after `write_tlas` succeeds,
   killing the cell-load warmup flash — #1227). The motion-vector
   attachment is computed from **un-jittered** positions.
7. Update the scene SSBO with the per-frame `GpuLight` array (point/spot/dir)
   and upload the `MaterialTable` SSBO (binding 13).
8. **Bone-palette skin chain** (M29.5/M29.6) — dispatch `skin_palette.comp`
   to write per-entity bone-world matrices into the persistent palette SSBO,
   then `skin_vertices.comp` (M29) to write pre-skinned vertices into each
   entity's `SkinSlot` output SSBO. Gated per-entity on `pose_dirty`
   (#1195/#1196) — idle NPCs skip both dispatch and the BLAS refit.
9. **Skinned BLAS refit** — for every dirty skinned entity, refit its BLAS
   (`mode = UPDATE`, `src == dst`) against the new vertex data, with
   `record_scratch_serialize_barrier` between iterations so actors don't
   race the shared scratch buffer (#642). Barrier chain:
   `COMPUTE_WRITE → AS_BUILD_INPUT_READ`, then `AS_BUILD_WRITE → AS_BUILD_INPUT_READ`.
10. Rebuild/refit the TLAS over visible BLASes — `build_tlas` overrides the
    per-mesh BLAS lookup with `skinned_blas[entity_id]` whenever
    `DrawCommand.bone_offset != 0`. HOST→AS_BUILD memory barrier before the
    ray-query consumers.
11. Dispatch `cluster_cull.comp` to assign lights to froxel clusters.
12. Clear the `water_caustic_accum` image **before** the main render pass
    (so `water.frag`'s in-pass `imageAtomicAdd` accumulates) with a
    TRANSFER→FRAGMENT_SHADER barrier.
13. Begin the main render pass into the HDR + G-buffer attachments. Instanced
    draw batching merges identical `mesh_handle` draws sharing pipeline /
    two-sided / layer state; the global geometry SSBO means no per-draw VB/IB
    rebind — only push constants (`model_index`, `bone_offset`, `material_id`)
    change per draw. After opaque + alpha-blend draws but before
    `cmd_end_render_pass`, the `WaterPipeline` records water-surface draws
    (depth write off, G-buffer attachments masked off so water never
    pollutes SVGF inputs).
14. End the main render pass.
15. Dispatch `svgf_temporal.comp` for indirect-light denoise.
16. Dispatch `caustic_splat.comp` to project refracted-light splats for
    glass / MultiLayerParallax into the scalar caustic accumulator (skipped
    when no TLAS handle is available — #640).
17. Dispatch the volumetric inject + integrate passes (M55 — scaffold; the
    output is multiplied by 0.0 in composite until Phase 2 lands).
18. Dispatch `taa.comp` for temporal AA; ping-pong history images.
19. Dispatch `ssao.comp` for screen-space ambient occlusion.
20. Dispatch the bloom down/up pyramid over the active HDR view.
21. Dispatch the composite pass to assemble
    `direct + indirect * albedo + caustic + bloom` with ACES tone mapping,
    writing the swapchain image.
22. If the egui overlay is active, record its LOAD-op render pass over the
    swapchain image.
23. Record the optional screenshot copy.
24. End command buffer, submit to the graphics queue with semaphore sync
    (`reset_fences` moved to immediately-before-submit per #952), present
    to the swapchain.

Every fallible Vulkan call propagates as a Rust `Result` and aborts the
submit cleanly so the swapchain stays consistent. Each major pass is
bracketed with a `gpu_timers` timestamp pair when the timer pool is active.

## Resize

`recreate_swapchain(size)` in [`vulkan/context/resize.rs`](../../crates/renderer/src/vulkan/context/resize.rs):

1. `device_wait_idle()` so no in-flight frames touch the old swapchain
2. Destroy framebuffers, depth image, render pass, swapchain image views, swapchain
3. Create new swapchain (new extent), depth image, render pass, framebuffers,
   and resize all per-pass per-FIF images (G-buffer, HDR, SVGF/TAA history,
   caustic accumulators, bloom mips, volumetric froxels)
4. Reallocate command buffers if the image count changed

The handoff is atomic from the application's point of view: the old
swapchain is fully torn down before the new one is created. `draw_frame`
guards against an empty-framebuffer state if a recreate failed mid-way
(#1211). Pending fences get reset; the next `draw_frame` runs through the
standard acquire path.

## RT acceleration structures

Located in [`vulkan/acceleration/`](../../crates/renderer/src/vulkan/acceleration/). Split across `mod.rs` (struct + lifecycle), `blas_static.rs` (mesh-keyed BLAS builds + eviction), `blas_skinned.rs` (per-entity BLAS refits), `tlas.rs` (TLAS rebuild), `memory.rs` (`shrink_*_to_fit` + telemetry), `predicates.rs` (pure decision fns), `constants.rs` (slack margins + eviction thresholds), and `types.rs` (`BlasEntry`, `TlasState`).

AS flag composition is hoisted to three module-level constants —
`STATIC_BLAS_FLAGS`, `SKINNED_BLAS_FLAGS`, `UPDATABLE_AS_FLAGS` — with
deliberate per-tier choices (rigid + TLAS prefer `FAST_TRACE`; skinned
prefers `FAST_BUILD`). `built_flags` is recorded on every `BlasEntry` to
guard `VUID-03667` at refit time (#1144 / #1145).

- **BLAS per mesh**: built once when the mesh is uploaded, owned by the
  `AccelerationManager` keyed by `MeshHandle`. Builds use
  `PREFER_FAST_TRACE | ALLOW_COMPACTION`. Builds are **batched** into a
  single submission per cell load (one fence, one scratch buffer shared
  across the batch) rather than fencing per mesh.
- **BLAS compaction (M36)**: after each batched build, an async occupancy
  query reports the compacted size; a compact copy is allocated at that
  exact size and the original BLAS is queued for `deferred_destroy`. 20–50%
  memory reduction on typical cells.
- **BLAS LRU eviction**: budget is `device_local / 3`, floored at
  `MIN_BLAS_BUDGET_BYTES = 256 MB`. When a new build would exceed budget,
  the LRU entries are evicted and their instances drop out of the next TLAS
  rebuild. `missing_blas` is counted split by cause — skinned / rigid /
  ssbo_evicted (#1228) — and throttled to 1 log/sec.
- **Per-skinned-entity BLAS** (M29): keyed by `EntityId`, built sync at
  cell load with `ALLOW_UPDATE | PREFER_FAST_BUILD`, then refit per frame
  via `mode = UPDATE` (`src == dst`) against the skin-compute output. The
  TLAS-build path looks up `skinned_blas[entity_id]` whenever
  `DrawCommand.bone_offset != 0`, so static draws keep the per-mesh
  `blas_entries` lookup unchanged.
- **TLAS per frame**: rebuilt every frame from the visible mesh handles and
  their world transforms, with frustum culling against the camera. Scratch
  memory is amortized across frames; `tlas_scratch_should_shrink` is a
  TLAS-calibrated predicate distinct from the BLAS `scratch_should_shrink`
  (#1226). Instance data is staged through a **device-local instance buffer**
  (#289) with a two-stage barrier chain (HOST_WRITE→TRANSFER_READ→AS_READ).
  `MAX_INSTANCES = 0x40000` (262144); `MAX_INDIRECT_DRAWS` is sized identically.
- **Ray query in the fragment shader + caustic splat compute**: shadow,
  reflection, 1-bounce GI, window-portal, and refracted-light caustic rays —
  all against the same TLAS. Shadow query is driven by the streaming-RIS
  reservoir pipeline (M31.5). Reflection rays get exponential distance
  falloff + roughness-driven angular jitter (#320). 1-bounce GI samples
  cosine-weighted hemisphere directions with `tMin = 0.05` matching the bias
  (#669); far hits fall back to a simplified cost model beyond the GI ray
  horizon, with a smoothed fade across the `rtLOD` boundary (#9873add6).
  Caustic ray uses `OPAQUE | TerminateOnFirstHit` (#640). Every consumer is
  gated on `sceneFlags.x > 0.5` (rt_flag = ray_query supported AND TLAS
  written this frame).

The `VK_KHR_ray_query` extension is queried at device-pick time. When it's
not present, the fragment shader falls back to non-shadowed multi-light.

## G-buffer

Located in [`vulkan/gbuffer.rs`](../../crates/renderer/src/vulkan/gbuffer.rs).

Five render targets (plus the HDR color image) written by the main geometry
pass (`triangle.frag`). One image per frame-in-flight slot for each
attachment:

| Attachment | Format | Purpose |
|------------|--------|---------|
| HDR color | R16G16B16A16_SFLOAT | Direct lighting + emissive + sky |
| Normal | R16G16_SNORM | Octahedral-encoded world-space normal (#275) |
| Motion vector | R16G16_SFLOAT | Screen-space delta for reprojection |
| Mesh ID | R32_UINT | Disocclusion detection (SVGF + TAA); bit 31 = alpha-blend marker |
| Raw indirect | B10G11R11_UFLOAT_PACK32 | Albedo-demodulated indirect (#268) |
| Albedo | B10G11R11_UFLOAT_PACK32 | Re-modulation target in composite |

The normal attachment was narrowed from RGBA16 to RG16_SNORM via octahedral
encoding (#275); the raw-indirect + albedo attachments use the packed
`B10G11R11` format to save bandwidth.

The albedo demodulation invariant (#268): the main shader writes indirect
lighting with the local albedo factored out so SVGF accumulates energy
across neighbors with different albedos; composite re-multiplies by the
local albedo. The metal/glossy reflection path routes through the direct
target (#315) specifically because its contribution already carries the hit
surface's albedo.

## SVGF temporal denoiser

Located in [`vulkan/svgf.rs`](../../crates/renderer/src/vulkan/svgf.rs)
with `shaders/svgf_temporal.comp`.

Temporal accumulation pass for the noisy 1-SPP indirect-light target.
Reprojects the previous frame's history via the motion vector attachment,
rejects samples where the reprojected mesh ID disagrees with the current
sample's mesh ID (disocclusion; the nearest-tap fallback masks bit 31 the
same way the bilinear loop does — #1159), and blends into ping-pong history
images with an α schedule that tightens after a few stable frames. History
age is tracked as a weighted average (Schied 2017 §4.2). Moments data
(first + second raw moments) is written for the future spatial (A-trous)
pass.

## TAA (M37.5)

Located in [`vulkan/taa.rs`](../../crates/renderer/src/vulkan/taa.rs)
with `shaders/taa.comp`.

Structure mirrors `SvgfPipeline`: per-frame-in-flight RGBA16F history
images, ping-pong descriptor sets, first-frame guard, resize hooks.

Per-frame flow:

1. The vertex shader applies a Halton(2,3) sub-pixel projection jitter
   driven by `CameraUbo.jitter` (period 16 — #1093 — chosen as the nearest
   power of two above the natural LCM-6 period). The motion-vector attachment
   is computed from **un-jittered** positions so reprojection stays correct.
2. `taa.comp` samples current HDR color, reprojects history through the
   motion vector via a Catmull-Rom 9-tap resample, clamps it against the
   current-frame 3×3 YCoCg neighborhood min/max (γ = 1.25), rejects it
   outright when mesh IDs disagree, and blends with α = 0.1 weighted by luma
   to damp bright-pixel ghosting.
3. `CompositePipeline::rebind_hdr_views()` swaps composite's input to the
   active TAA output each frame. On a TAA dispatch error, composite falls
   back to the raw HDR view (`fall_back_to_raw_hdr`).

## Composite pass

Located in [`vulkan/composite.rs`](../../crates/renderer/src/vulkan/composite.rs)
with `shaders/composite.vert` + `shaders/composite.frag`.

Fullscreen quad. Reads the direct HDR, the SVGF-denoised indirect, the
albedo attachment, the glass and water caustic accumulators, and the bloom
output; computes `direct + indirect * albedo + caustic + bloom` (re-applying
the #268 demodulation invariant), runs ACES tone mapping, and writes to the
swapchain color attachment (or to the TAA input when TAA is enabled). The
volumetric term is folded in via `final = scene * vol.a + vol.rgb`, but its
contribution is currently multiplied by 0.0 (`VOLUMETRIC_OUTPUT_CONSUMED`
gate) until the M55 inject/integrate passes produce real scattering.

## Material table (R1)

Located in [`vulkan/material.rs`](../../crates/renderer/src/vulkan/material.rs)
(`GpuMaterial`, `MaterialTable`, the `MAT_FLAG_*` / `MATERIAL_KIND_*`
feature flags) with the per-frame upload in
[`scene_buffer/upload.rs`](../../crates/renderer/src/vulkan/scene_buffer/upload.rs).

Before R1 (#785), every per-material field — texture indices, PBR scalars,
alpha state, Skyrim+ shader-variant payloads, BSEffect falloff, BGSM UV
transform, NiMaterialProperty diffuse/ambient — was duplicated onto every
per-instance struct, so a cell that places one material 10–30 times carried
the same ~35 fields that many times. R1 factored them into a deduped
**`GpuMaterial`** (300 bytes) and a per-frame **`MaterialTable`** SSBO
(binding 13, `MAX_MATERIALS = 16384`). `GpuInstance` (112 bytes) now
references its material via `material_id: u32`, and identical materials
collapse to the same id via `MaterialTable::intern`. `triangle.frag` reads
`materials[inst.materialId].foo` for every per-material field.

The material **feature flags** are game-agnostic: the parser→Material
boundary resolves all per-game shader-property differences into a small
flag set, and the shader reads only those flags. The full set lives in
[`material.rs`](../../crates/renderer/src/vulkan/material.rs). Only the five
effect flags (`MAT_FLAG_VERTEX_COLOR_EMISSIVE`, `_EFFECT_SOFT`,
`_EFFECT_PALETTE_COLOR`, `_EFFECT_PALETTE_ALPHA`, `_EFFECT_LIT`) are mirrored
into the build-generated `shaders/include/shader_constants.glsl`; the
PBR/Disney bits (`MAT_FLAG_PBR_BSDF`, `MAT_FLAG_TRANSLUCENCY` + variants,
`MAT_FLAG_MODEL_SPACE_NORMALS`) are hand-`#define`d directly in
`triangle.frag`, and `material_flag::BGSM_AUTHORED` is host-side only:

| Flag | Meaning |
|------|---------|
| `MAT_FLAG_VERTEX_COLOR_EMISSIVE` | Vertex color drives emissive |
| `MAT_FLAG_EFFECT_SOFT` / `_PALETTE_COLOR` / `_PALETTE_ALPHA` / `_LIT` | BSEffectShaderProperty soft-particle / greyscale-to-palette / lit-effect paths |
| `MAT_FLAG_PBR_BSDF` | Material authors Disney-style PBR fields → Disney BSDF path |
| `MAT_FLAG_TRANSLUCENCY` (+ `_THICK_OBJECT` / `_MIX_ALBEDO`) | Subsurface translucency |
| `MAT_FLAG_MODEL_SPACE_NORMALS` | Model-space (vs tangent-space) normal map |
| `material_flag::BGSM_AUTHORED` | Host-side only (NOT mirrored to the shader): material came from a BGSM/BGEM file (drives spec-glossiness → metallic-roughness translation) |

`material_kind` carries the special-case render path selector
(`MATERIAL_KIND_GLASS = 100`, `MATERIAL_KIND_EFFECT_SHADER = 101`,
`MATERIAL_KIND_NO_LIGHTING = 102`). Per-instance bits (geometry-level, not
material-level) stay on `GpuInstance.flags`:
`INSTANCE_FLAG_NON_UNIFORM_SCALE`, `_ALPHA_BLEND`, `_CAUSTIC_SOURCE`,
`_TERRAIN_SPLAT`, `_PRESKINNED`, `_FLAT_SHADING`.

> **Shader Struct Sync (CRITICAL).** `GpuInstance` is duplicated across
> four GLSL copies (`triangle.vert`, `triangle.frag`, `water.vert`,
> `ui.vert`); `GpuCamera` across `triangle.vert/frag`, `water.vert/frag`,
> `cluster_cull.comp`, `caustic_splat.comp`. Post-R1 the contract narrowed
> so only `triangle.frag` mirrors the full `GpuMaterial`. The
> `gpu_instance_is_112_bytes_std430_compatible`, `gpu_camera_is_288_bytes`
> (the test name keeps `288` for grep continuity but asserts the live
> 304-byte `GpuCamera` layout), and `GpuMaterial`-size tests pin the byte
> layout; the `feedback_shader_struct_sync` note records the update protocol.

## Disney BSDF

The PBR shading lobe in `triangle.frag` is a port of knightcrawler25's
GLSL-PathTracer (MIT) Disney implementation, gated on `MAT_FLAG_PBR_BSDF`
(#1248–#1257):

- **Fresnel F0 from IOR** (`dielectricF0FromIor`, #1248) — dielectric F0 is
  derived from the per-material `ior` field rather than a hardcoded 0.04.
- **Disney diffuse lobe** (#1249) — Burley retro-reflection + Hanrahan-Krueger
  fake-SSS subsurface + Fresnel-weighted sheen, returned as a struct so the
  diffuse term is `/PI`'d (Lambertian) while sheen is NOT (Disney 2012
  layered-material convention). #1252 split the helper to stop per-light
  sheen × π over-amplification.
- **Anisotropic GGX** (#1250) — `deriveAxAy(roughness, anisotropic)` maps
  perceptual roughness + an `anisotropic` strength into `ax`/`ay`;
  `distributionGGXAniso` reduces exactly to the isotropic `distributionGGX`
  when `ax == ay` (anisotropic = 0). `aspect = sqrt(1 - anisotropic * 0.9)`
  caps the lobe stretch so anisotropic = 1 doesn't produce a degenerate
  needle.
- **Input-domain clamps** (#1253/#1254) — defensive clamps on the IOR and
  anisotropy inputs so out-of-range authored values can't produce
  black/undefined fragments.
- A documented Disney material preset table (#1251) provides canonical
  values for common surface classes.

These follow the project's no-guessing rule: the math + canonical values
come from the GLSL-PathTracer reference, not invented constants.

## Water (M38) and water-side caustics

`WaterPipeline` ([`vulkan/water.rs`](../../crates/renderer/src/vulkan/water.rs))
is a draw variant of the main scene render pass:

- shares subpass 0 so it draws into the same HDR + G-buffer attachments;
- reuses the bindless-texture descriptor set (set 0), the scene set (set 1),
  and the engine `Vertex` layout (so water meshes travel through the global
  vertex/index SSBOs);
- `water.vert` + `water.frag` shaders, a 128-byte push-constant block
  (`WaterPush`) carrying time + flow + per-plane material params;
- SRC_ALPHA / ONE_MINUS_SRC_ALPHA blend on HDR attachment 0; attachments 1–5
  (normal, motion, mesh_id, raw_indirect, albedo) are masked off so water
  never pollutes the SVGF / motion-vector inputs;
- depth test on, depth write off; cull NONE (seen from both sides).

Water flow class is one of `WATER_CALM` / `WATER_RIVER` / `WATER_RAPIDS` /
`WATER_WATERFALL`.

Water-side caustics (#1210, Phases A–E) use a dedicated per-FIF R32_UINT
accumulator in [`vulkan/water_caustic.rs`](../../crates/renderer/src/vulkan/water_caustic.rs)
— cleared **before** the main render pass (the inverse ordering of the
glass-caustic pipeline) so `water.frag`'s `imageAtomicAdd` accumulates
in-pass; composite then samples it alongside the glass `causticTex`. The sun
direction + intensity is plumbed through `CameraUBO` so `water.frag` can
shoot a shadow ray to the sun and synthesize caustics on a miss.

## Bloom (M58)

[`vulkan/bloom.rs`](../../crates/renderer/src/vulkan/bloom.rs) with
`shaders/bloom_downsample.comp` + `shaders/bloom_upsample.comp`.

Separable compute pyramid: a down-pyramid of `BLOOM_MIP_COUNT = 5` half-res
levels (4-tap bilinear box filter), then an up-pyramid of `BLOOM_MIP_COUNT - 1`
levels that sum each upsampled level with the same-resolution down-mip. The
final `up_mips[0]` is what composite adds to scene HDR before tone-mapping
(`BLOOM_INTENSITY = 0.15`). Plain box filters were chosen over Jimenez's
13-tap/9-tap weights deliberately — lifting those weights verbatim from the
talk slides would be required to avoid violating the no-guessing rule, and
the box filter lands ~80% of the visual win unambiguously.

## Volumetric lighting (M55)

[`vulkan/volumetrics.rs`](../../crates/renderer/src/vulkan/volumetrics.rs)
with `shaders/volumetrics_inject.comp` + `shaders/volumetrics_integrate.comp`.

Frostbite-style froxel volumetrics: a 3D texture indexed by
`(screenUV.x, screenUV.y, sliceZ)` with denser slices near the camera
(`VOLUME_FAR = 200`). **Phase 1 (current)** allocates the per-FIF 3D images,
transitions them to `GENERAL`, and clears them to a composite no-op
(`rgb = 0`, `a = 1`); the inject/integrate dispatch plumbing is wired but
the output is gated off (`VOLUMETRIC_OUTPUT_CONSUMED = false`) until Phase 2
adds density+lighting injection (TLAS shadow raymarch + Henyey-Greenstein
phase) and ray-march integration.

## Multi-light SSBO

Located in [`vulkan/scene_buffer/`](../../crates/renderer/src/vulkan/scene_buffer/). Split across `mod.rs` (re-exports), `constants.rs` (capacity ceilings + flag bits), `gpu_types.rs` (`#[repr(C)]` shader-contract structs), `buffers.rs` (`SceneBuffers` struct + `new()` + accessors), `upload.rs` (per-SSBO upload-and-flush), and `descriptors.rs` (descriptor-set writes for AO / GBuffer / cluster / TLAS).

The renderer uses an SSBO (not a UBO) so the shader can iterate a variable
number of lights without recompiling the pipeline (`MAX_LIGHTS = 512`). Each
`GpuLight` is a 64-byte struct of four `vec4`s: `position_radius`
(xyz = world position, w = radius), `color_type` (rgb = color, w = type:
0 point / 1 spot / 2 directional), `direction_angle` (xyz = direction,
w = spot outer-angle cosine), and `params` (x = `falloff_exponent` from the
LIGH DATA record, rest reserved). The fragment shader iterates the array and
accumulates contributions with a standardized attenuation contract
(`fc338d90`); for RT hardware it shoots a ray per light against the TLAS for
hard shadows.

The SSBO is double-buffered between frames-in-flight and updated on the host
with `HOST_VISIBLE` memory.

Cell interior lighting (the `XCLL` sub-record from CELL records) becomes two
entries: one ambient (modeled as a bottom-hemisphere directional) and one
directional (the cell's "key light" rotation/color). The ambient fill is
decoupled from the live light count so a sparse interior doesn't crush the
fill floor (`636bcc96`). LIGH records become point lights with their declared
radius and color. See [Cell Lighting](lighting-from-cells.md) for the full
pipeline.

## Pipeline cache

The graphics pipeline is created with a `VkPipelineCache` object that
persists to disk across runs. On a cold start the cache file is missing and
`vkCreateGraphicsPipelines` takes 10–50 ms; on subsequent runs the cache
hits and the same call drops to <1 ms. The cache is written back to disk on
clean shutdown. The cache is threaded through every pipeline create site,
including the lazily-created per-(src, dst, two_sided) blend pipeline cache
and the water pipeline.

## Sync model

Per-frame:

- `image_available` semaphore — signaled by `vkAcquireNextImageKHR`,
  consumed by the queue submit
- `render_finished` semaphore — **per swapchain image** (flipped back from
  per-frame to resolve `VUID-vkQueueSubmit-pSignalSemaphores-00067` across
  both FIFO and MAILBOX — `548c1b69`), signaled by the queue submit,
  consumed by `vkQueuePresentKHR`
- `in_flight` fence — CPU-side wait, created `SIGNALED` so the first frame
  doesn't deadlock; `reset_fences` is placed immediately before the submit
  (#952) so a mid-frame `?` error can't leave the fence unsignaled with no
  pending submit

Two frames in flight: while frame N is being presented, frame N+1 is being
recorded on a separate command buffer with a separate fence. `draw_frame`
waits on both this slot's fence **and** the previous slot's, because the
shared scratch buffer and persistent SSBOs are touched across slots.

For the Havok / RT path there's an additional **HOST→AS_BUILD** memory
barrier that fences staging-buffer uploads before the BLAS build, and the
skinned-refit barrier chain (`COMPUTE → AS_BUILD → AS_BUILD → FRAGMENT`)
described in the per-frame draw section.

## Backface culling and winding order

Backface culling is enabled. NIF files use D3D-style **clockwise** winding,
which matches Vulkan's `VK_FRONT_FACE_CLOCKWISE`. After M17's coordinate
system fix this is consistent end-to-end:

```
Gamebryo Z-up CW         → Y-up CCW            → Vulkan CW
(NIF source data)        → (engine internal)   → (renderer winding)
```

See [Coordinate System](coordinate-system.md) for the Z-up→Y-up conversion
and the SVD repair for degenerate NIF rotation matrices.

## Texture upload

[`vulkan/texture.rs`](../../crates/renderer/src/vulkan/texture.rs) handles
DDS textures end-to-end:

1. Parse the DDS header in [`vulkan/dds.rs`](../../crates/renderer/src/vulkan/dds.rs)
   (handles the FourCC layout, the DX10 extended header, BC1/BC3/BC5)
2. Allocate a destination `VkImage` in `DEVICE_LOCAL` memory via `gpu-allocator`
3. Allocate a host-visible staging buffer of the right size
4. Memcpy the DDS pixel data into the staging buffer
5. Record a command buffer that:
   a. Transitions the image to `TRANSFER_DST_OPTIMAL`
   b. Copies the staging buffer into the image, mip by mip
   c. Transitions the image to `SHADER_READ_ONLY_OPTIMAL`
6. Submit on the graphics queue and wait
7. Insert into the texture registry, return the handle

The texture registry caches by path so the same DDS isn't uploaded twice
across cell loads, and exposes a bindless descriptor array (set 0). Deferred
destruction queues the old `VkImage` until two frames have elapsed before
actually freeing the GPU memory, so dynamic UI texture updates (Ruffle SWF
rendering, egui textures) don't need a `device_wait_idle`.

## Asset reading helpers

The renderer's mesh registry exposes a small `upload(...)` API used by both
the cell loader and the loose-NIF demo path. It takes vertex and index
slices, hands off the GPU upload to a one-time command buffer, queues the
BLAS build, and returns a `MeshHandle`. See the [Asset Pipeline](asset-pipeline.md)
doc for the full NIF→ECS→GPU upload flow.

## Dependencies

| Crate             | Version | Purpose                                    |
|-------------------|---------|--------------------------------------------|
| ash               | 0.38    | Raw Vulkan bindings                        |
| ash-window        | 0.13    | Surface creation from window handles       |
| gpu-allocator     | 0.28    | Vulkan memory allocator                    |
| rspirv            | 0.12    | SPIR-V reflection (descriptor cross-check) |
| winit             | 0.30    | Window handle types                        |
| raw-window-handle | 0.6     | Platform-agnostic handle traits            |
| image             | 0.24    | PNG fallback for non-DDS textures          |
| egui              | 0.33    | Debug-overlay UI (CPU side)                |
| egui-ash-renderer | 0.11    | Debug-overlay GPU pipeline (shares allocator) |

The shaders are compiled offline with `glslangValidator` and embedded into
the binary via `include_bytes!` from `crates/renderer/shaders/`. The full
set: `triangle.vert/frag` (main geometry + PBR + RT ray queries),
`composite.vert/frag`, `ui.vert/frag`, `water.vert/frag`, plus the compute
shaders `cluster_cull`, `ssao`, `svgf_temporal`, `taa`, `caustic_splat`,
`skin_vertices`, `skin_palette`, `bloom_downsample`, `bloom_upsample`,
`volumetrics_inject`, `volumetrics_integrate`. Shared `#define`s
(workgroup sizes, `MAT_FLAG_*`, cluster dims, etc.) are generated by
`crates/renderer/build.rs` into `shaders/include/shader_constants.glsl` from
`shader_constants_data.rs`, so the GLSL and Rust constants can never drift.
