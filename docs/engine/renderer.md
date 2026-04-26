# Vulkan Renderer

The renderer is a Vulkan 1.3 implementation built on `ash` (raw Vulkan
bindings), `ash-window` (surface creation), and `gpu-allocator` (GPU
memory). It supports `VK_KHR_ray_query` for hardware ray-traced shadows
on RTX-class hardware and falls back to non-RT rendering on devices
without the extension.

Source: `crates/renderer/src/vulkan/`

## High-level capabilities

- **Full Vulkan init chain** with validation layers in debug builds
- **RT acceleration structures**: BLAS per mesh + TLAS rebuilt per frame in
  `DEVICE_LOCAL` memory with HOST→AS_BUILD memory barriers, LRU eviction
  (1 GB budget), `ALLOW_COMPACTION` + async occupancy query + compacted
  copy (M36: 20–50% BLAS memory reduction), batched build submission
- **Multi-light SSBO**: point / spot / directional lights consumed by the
  fragment shader, with `VK_KHR_ray_query` shadow rays per light
- **Streaming RIS direct lighting** (M31.5): 8 independent weighted reservoirs
  per fragment, each sampled from the full cluster proportional to luminance;
  unbiased W = resWSum / (K · w_sel) clamped at 64×
- **G-buffer** (6 attachments): normal, motion vector, mesh ID, raw indirect,
  albedo, plus the HDR color target
- **SVGF temporal denoiser** for indirect lighting — motion-vector reprojection,
  mesh-id disocclusion, albedo-demodulated accumulation
- **Composite pass**: direct + denoised indirect reassembly (multiplies
  indirect by local albedo per #268's invariant) with ACES tone mapping
- **TAA** (M37.5): `taa.comp` with Halton(2,3) projection jitter,
  motion-vector reprojection, Catmull-Rom 9-tap history resample,
  3×3 YCoCg neighborhood variance clamp (γ = 1.25), mesh-id disocclusion,
  luma-weighted α = 0.1 blend
- **Clustered lighting** (`cluster_cull.comp`) — frustum assignment for
  direct shading candidates
- **SSAO** (`ssao.comp`) compute pass with noise texture + kernel samples
- **Pipeline cache** persisted to disk (10–50 ms cold init → <1 ms warm)
- **Shared `VkSampler`** across all textures, 16× anisotropic filtering
  when the device exposes it
- **Per-image semaphore synchronization** with 2 frames in flight
- **Atomic swapchain handoff** during resize (no dropped frames, no torn submits)
- **Deferred texture destruction** (two-frame countdown) so dynamic UI texture
  updates don't stall the graphics queue
- **Depth format autodetect** with fallback chain D32→D32S8→D24S8→D16
- **Backface culling** with confirmed NIF/D3D clockwise winding convention
- **DDS texture loading**: BC1 / BC3 / BC5 + DX10 with mipmaps
- **Global geometry SSBO** (#294): per-draw vertex/index buffer rebinds
  eliminated — one SSBO indexed by `mesh_handle` for all meshes
- **Multi-draw indirect**: identical meshes collapse into one
  `cmd_draw_indexed_indirect` per batch (#309), instead of per-batch
  per-instance `cmd_draw_indexed`
- **Session 11 sync/cache hardening** (#313, #316, #317, #392, #415, #416,
  #420, #422, #426): `VkPipelineCache` threaded through every pipeline
  create site, per-(src, dst, two_sided) blend pipeline cache, TLAS build
  barrier widened to COMPUTE_SHADER, `TRIANGLE_FACING_CULL_DISABLE` on
  TLAS instances gated on the `two_sided` flag, `gl_RayFlagsTerminateOnFirstHitEXT`
  on reflection + glass rays, SVGF history age as a weighted average
  (Schied 2017 §4.2), BLAS compaction leak fixed on partial OOM,
  empty-TLAS `VUID-VkBufferCopy-size-01988` / `-size-01188` suppression
  via size=0 guard, and an opt-in global lock-order graph for cross-thread
  ABBA detection
- **M29 GPU skinning** (Phase 1+2, sessions 18-20): per-frame `skin_compute`
  pass writes pre-skinned vertices into a per-entity output SSBO; the
  raster path still reads the static vertex buffer today, while the
  per-skinned-entity BLAS is **refit** in-place (`mode = UPDATE`,
  `src == dst`) once per frame against the new vertex data so RT shadows
  + reflections see animated geometry. First-sight setup (compute prime
  + initial sync BUILD) is one-shot per cell load. See M29 in
  [ROADMAP.md](../../ROADMAP.md) for the Phase 3 raster integration that's
  gated on M41 stability.
- **Scratch-serialise barrier** (#642): every BLAS build / refit in this
  manager shares one `blas_scratch_buffer`; consecutive
  `cmd_build_acceleration_structures` calls now emit
  `AS_WRITE → AS_WRITE` at `ACCELERATION_STRUCTURE_BUILD_KHR` between
  iterations via `AccelerationManager::record_scratch_serialize_barrier`.
  The static batched build path and the per-frame skinned refit loop
  both call the same helper to keep the barrier style in lockstep.
- **Caustic scatter** (M22 / #321 Option A): `caustic_splat.comp` projects
  refracted-light splat per refractive-source pixel into a scalar
  R32_UINT accumulator; composite samples and adds it to direct
  lighting. RT-enabled gates land in #640 — both shader-side
  (`sceneFlags.x < 0.5` early-out) and CPU-side (skip dispatch when
  no TLAS handle for the frame), plus `OPAQUE | TerminateOnFirstHit`
  ray flags so the driver doesn't run full closest-hit traversal.
- **Session 12 material plumbing + RT correctness** (2026-04 sweep):
  BSShaderTextureSet slots 3/4/5 (parallax / env cube / env mask)
  routed to `GpuInstance` with `parallaxHeightScale`/`parallaxMaxPasses`
  (#452 + #453), parallax-occlusion fragment branch guarded on
  `parallaxIdx != 0` using screen-space derivatives (same TBN recipe as
  the existing `perturbNormal`), window portal ray fires along `-N`
  not `-V` with a grazing-angle gate so oblique windows actually
  transmit (#421). `GpuInstance` grew 176 → 192 bytes with the
  Shader Struct Sync test asserting lockstep across four GLSL
  copies (`triangle.vert`, `triangle.frag`, `ui.vert`,
  `caustic_splat.comp`). SPIR-V reflection via `rspirv` cross-checks
  every descriptor layout against the shader's declared bindings at
  pipeline create time (#427); mismatch = hard fail instead of silent
  undefined behaviour at dispatch.

## Module map

```
crates/renderer/src/vulkan/
├── mod.rs              Re-exports
├── instance.rs         Create VkInstance with extensions + validation layer
├── debug.rs            VK_EXT_debug_utils → log crate
├── surface.rs          ash_window::create_surface wrapper
├── device.rs           Physical + logical device selection (queue families,
│                       extensions, RT capability detection)
├── swapchain.rs        Format / present mode / extent selection, image views
├── sync.rs             Per-frame semaphores + fences (2 frames in flight)
├── allocator.rs        gpu-allocator wrapper for VkBuffer / VkImage
├── buffer.rs           Vertex / index / uniform buffer creation, staging
├── pipeline.rs         Graphics pipeline + pipeline cache persistence
├── descriptors.rs      Descriptor set layouts (UBO, SSBO, samplers, AS)
├── compute.rs          Compute pipeline utilities (shared by SSAO/SVGF/TAA/cluster_cull)
├── scene_buffer.rs     SSBO for multi-light data; uploaded per frame
├── acceleration.rs     BLAS + TLAS construction, per-skinned-entity BLAS
│                       refit, scratch buffer reuse + serialise barrier,
│                       LRU eviction, compaction (ALLOW_COMPACTION + query + copy)
├── skin_compute.rs     M29 GPU pre-skinning compute pipeline (1 workgroup
│                       per skinned entity), per-entity SkinSlot output SSBO
├── gbuffer.rs          G-buffer attachments: normal, motion vector, mesh ID,
│                       raw indirect, albedo
├── svgf.rs             SvgfPipeline — temporal accumulation denoiser for
│                       indirect lighting (motion reprojection + mesh-id
│                       disocclusion + albedo-demodulated history)
├── taa.rs              TaaPipeline — Halton jitter, Catmull-Rom resample,
│                       YCoCg variance clamp, per-FIF ping-pong history
├── caustic.rs          CausticPipeline — refracted-light splat compute
│                       pass driving the scalar accumulator (#321)
├── composite.rs        CompositePipeline — direct + denoised indirect +
│                       caustic reassembly, ACES tone mapping
├── ssao.rs             SSAO compute pipeline (noise texture, kernel)
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
    └── helpers.rs      find_depth_format, create_render_pass, create_framebuffers
```

The split between `vulkan/*.rs` (low-level building blocks) and `vulkan/context/*.rs` (the orchestration layer that owns all of them) keeps each file under ~600 lines. `VulkanContext::new()` walks the init chain;
`VulkanContext::draw_frame()` records and submits a frame.

## Initialization Chain

`VulkanContext::new()` performs the full init chain. Each step is in its
own module so the call sequence reads as a recipe:

| Step | Module | What |
|---|---|---|
| 1 | — | Load Vulkan entry points via `ash::Entry::load()` |
| 2 | `instance.rs` | Create `VkInstance` (API 1.3, platform extensions, validation layer if debug) |
| 3 | `debug.rs` | Set up `VK_EXT_debug_utils` messenger routing to the `log` crate |
| 4 | `surface.rs` | Create `VkSurfaceKHR` from raw window handles via `ash-window` |
| 5 | `device.rs` | Pick physical device — checks swapchain extension, queue families, and `VK_KHR_ray_query` capability |
| 6 | `device.rs` | Create logical device with graphics + present queues + RT extensions when available |
| 7 | `allocator.rs` | Create the `gpu-allocator` instance (Vulkan backend) |
| 8 | `swapchain.rs` | Create the swapchain (sRGB B8G8R8A8, mailbox present mode, clamped extent) |
| 9 | `context::helpers` | Pick depth format (D32→D32S8→D24S8→D16) and create the depth image |
| 10 | `context::helpers` | Create the render pass (color + depth, CLEAR load op, PRESENT_SRC final layout) |
| 11 | `context::helpers` | Create framebuffers per swapchain image (color + depth attachment) |
| 12 | `pipeline.rs` | Load the pipeline cache from disk (if present) and create the graphics pipeline |
| 13 | `descriptors.rs` | Create descriptor set layouts for the UBO/SSBO/sampler/AS bindings |
| 14 | `context::mod` | Create the command pool (per-buffer reset) and command buffers |
| 15 | `sync.rs` | Create per-frame sync objects (2 frames in flight, fences signaled) |

The `VulkanContext` struct has 50+ fields holding the device, queues,
swapchain images, framebuffers, descriptor sets, pipeline, sync objects,
mesh registry, texture registry, scene SSBO, RT context, and so on.
[`Drop`](../../crates/renderer/src/vulkan/context/mod.rs) tears
everything down in reverse order under `device_wait_idle()`.

## Per-frame draw

`VulkanContext::draw_frame(world, ...)` in [`vulkan/context/draw.rs`](../../crates/renderer/src/vulkan/context/draw.rs) does:

1. Wait for the in-flight fence for this frame slot
2. Acquire the next swapchain image (handle `ERROR_OUT_OF_DATE_KHR` → swap)
3. Reset the fence and the command buffer
4. Walk the ECS via `build_render_data` to collect visible mesh handles,
   transforms, materials, light sources, decals, skinned-mesh bone offsets
5. Update the camera UBO (view + proj + **Halton jitter** for TAA;
   `flags.x` = 1.0 only when ray_query is supported AND the TLAS is
   written for this frame, used by the shaders' RT-enable gates)
6. Update the scene SSBO with the per-frame light array (point/spot/dir)
7. Dispatch `cluster_cull.comp` to assign lights to froxel clusters
8. **Skin chain** (M29) — for every skinned entity in the visible draw
   list:
    - Dispatch `skin_compute.comp` to write pre-skinned vertices into
      the entity's `SkinSlot` output SSBO (read by step 9's BLAS refit)
    - Emit `COMPUTE_WRITE → AS_BUILD_INPUT_READ` barrier
    - Refit the per-skinned-entity BLAS (`mode = UPDATE`, `src == dst`)
      against the new vertex data; `record_scratch_serialize_barrier`
      between iterations so multiple skinned actors don't race the
      shared scratch buffer (#642)
    - Emit `AS_BUILD_WRITE → AS_BUILD_INPUT_READ` barrier
9. Rebuild/refit the TLAS over visible BLASes — `build_tlas` overrides
   the per-mesh BLAS lookup with `skinned_blas[entity_id]` whenever
   `DrawCommand.bone_offset != 0`. HOST→AS_BUILD memory barrier
   before the ray-query consumers.
10. Begin the G-buffer render pass. Instanced draw batching merges
    identical `mesh_handle` draws; `last_mesh_handle` cache avoids
    redundant descriptor binds. Global geometry SSBO means no per-draw
    VB/IB rebind — only push constants (`model_index`, `bone_offset`,
    `material_index`) change per draw.
11. Dispatch `ssao.comp` for screen-space ambient occlusion.
12. Dispatch `svgf_temporal.comp` for indirect-light denoise.
13. Dispatch `caustic_splat.comp` to project refracted-light splats
    into the scalar caustic accumulator (skipped when no TLAS handle
    is available for the frame — #640).
14. Dispatch the composite pass to assemble
    `direct + indirect * albedo + caustic` with ACES tone mapping.
15. Dispatch `taa.comp` for temporal AA; ping-pong history images.
16. Blit/copy into the swapchain color attachment.
17. End command buffer, submit to the graphics queue with semaphore
    sync, present to the swapchain.

Errors during step recording propagate as Rust `Result`s and abort the
submit cleanly (see #85 in the changelog) so the swapchain stays consistent.

## Resize

`recreate_swapchain(size)` in [`vulkan/context/resize.rs`](../../crates/renderer/src/vulkan/context/resize.rs):

1. `device_wait_idle()` so no in-flight frames touch the old swapchain
2. Destroy framebuffers, depth image, render pass, swapchain image views, swapchain
3. Create new swapchain (new extent), depth image, render pass, framebuffers
4. Reallocate command buffers if the image count changed

The handoff is atomic from the application's point of view: the old
swapchain is fully torn down before the new one is created. Pending fences
get reset; the next `draw_frame` runs through the standard acquire path.

## RT acceleration structures

Located in [`vulkan/acceleration.rs`](../../crates/renderer/src/vulkan/acceleration.rs).

- **BLAS per mesh**: built once when the mesh is uploaded, owned by the
  `AccelerationManager` keyed by `MeshHandle`. Builds use
  `PREFER_FAST_TRACE | ALLOW_COMPACTION`. Builds are **batched** into a
  single submission per cell load (one fence, one scratch buffer shared
  across the batch) rather than fencing per mesh.
- **BLAS compaction (M36)**: after each batched build, an async occupancy
  query reports the compacted size; a compact copy is allocated at that
  exact size and the original BLAS is queued for `deferred_destroy`. 20–50%
  memory reduction on typical cells.
- **BLAS LRU eviction**: 1 GB budget. When a new build would exceed
  budget, the LRU entries are evicted and their instances drop out of the
  next TLAS rebuild.
- **Per-skinned-entity BLAS** (M29): keyed by `EntityId`, built sync at
  cell load with `ALLOW_UPDATE | PREFER_FAST_BUILD`, then refit per
  frame via `mode = UPDATE` (`src == dst`) against the M29 skin
  compute output. The TLAS-build path looks up
  `skinned_blas[entity_id]` whenever `DrawCommand.bone_offset != 0`,
  so static draws keep the per-mesh `blas_entries` lookup unchanged.
- **TLAS per frame**: rebuilt every frame from the visible mesh handles
  and their world transforms, with frustum culling against the camera.
  Scratch memory is amortized across frames. Instance data is staged
  through a **device-local instance buffer** (#289) with a two-stage
  barrier chain (HOST_WRITE→TRANSFER_READ→AS_READ). `MAX_INSTANCES = 8192`.
- **Ray query in the fragment shader + caustic splat compute**: shadow,
  reflection, 1-bounce GI, window-portal, and refracted-light caustic
  rays — all against the same TLAS (set 1, binding 2). Shadow query is
  driven by the streaming-RIS reservoir pipeline (M31.5). Reflection
  rays get exponential distance falloff + roughness-driven angular
  jitter (#320). 1-bounce GI samples cosine-weighted hemisphere
  directions with `tMin = 0.05` matching the bias (#669); far hits fall
  back to a simplified cost model beyond the GI ray horizon. Caustic
  ray uses `OPAQUE | TerminateOnFirstHit` (#640) — refracted rays
  through small glass volumes only need any-hit semantics. Every
  consumer is gated on `sceneFlags.x > 0.5` (rt_flag = ray_query
  supported AND TLAS written this frame).

The `VK_KHR_ray_query` extension is queried at device-pick time. When it's
not present, the fragment shader falls back to non-shadowed multi-light.

## G-buffer

Located in [`vulkan/gbuffer.rs`](../../crates/renderer/src/vulkan/gbuffer.rs).

Six render targets written by the main geometry pass (`triangle.frag`):

| Attachment | Format | Purpose |
|------------|--------|---------|
| HDR color | R16G16B16A16_SFLOAT | Direct lighting + emissive + sky |
| Normal | R16G16B16A16_SFLOAT | View-space normals for SVGF + TAA |
| Motion vector | R16G16_SFLOAT | Screen-space delta for reprojection |
| Mesh ID | R32_UINT | Disocclusion detection (SVGF + TAA) |
| Raw indirect | R16G16B16A16_SFLOAT | Albedo-demodulated indirect (#268) |
| Albedo | R8G8B8A8_UNORM | Re-modulation target in composite |

The albedo demodulation invariant (#268): the main shader writes indirect
lighting with the local albedo factored out so SVGF accumulates energy
across neighbors with different albedos; composite re-multiplies by the
local albedo. The metal/glossy reflection path routes through the direct
target (#315) specifically because its contribution already carries the
hit surface's albedo.

## SVGF temporal denoiser

Located in [`vulkan/svgf.rs`](../../crates/renderer/src/vulkan/svgf.rs)
with `shaders/svgf_temporal.comp`.

Temporal accumulation pass for the noisy 1-SPP indirect-light target.
Reprojects the previous frame's history via the motion vector attachment,
rejects samples where the reprojected mesh ID disagrees with the current
sample's mesh ID (disocclusion), and blends into ping-pong history images
with an α schedule that tightens after a few stable frames. Moments data
(first + second raw moments) is written for the future M37 spatial
(A-trous) pass.

## TAA (M37.5)

Located in [`vulkan/taa.rs`](../../crates/renderer/src/vulkan/taa.rs)
with `shaders/taa.comp`.

Structure mirrors `SvgfPipeline`: per-frame-in-flight RGBA16F history
images, ping-pong descriptor sets, first-frame guard, resize hooks.

Per-frame flow:

1. Vertex shader applies a Halton(2,3) sub-pixel projection jitter
   driven by `CameraUbo.jitter` (the motion-vector attachment is
   computed from **un-jittered** positions so reprojection stays
   correct).
2. `taa.comp` samples current HDR color, reprojects history through
   the motion vector via a Catmull-Rom 9-tap resample, clamps it
   against the current-frame 3×3 YCoCg neighborhood min/max
   (γ = 1.25), rejects it outright when mesh IDs disagree, and blends
   with α = 0.1 weighted by luma to damp bright-pixel ghosting.
3. `CompositePipeline::rebind_hdr_views()` swaps composite's input
   to the active TAA output each frame.

## Composite pass

Located in [`vulkan/composite.rs`](../../crates/renderer/src/vulkan/composite.rs)
with `shaders/composite.vert` + `shaders/composite.frag`.

Fullscreen quad. Reads the direct HDR, the SVGF-denoised indirect, and
the albedo attachment; computes `direct + indirect * albedo` (re-applying
the #268 demodulation invariant), runs ACES tone mapping, and writes to
the swapchain color attachment (or to the TAA input when TAA is enabled).

## Multi-light SSBO

Located in [`vulkan/scene_buffer.rs`](../../crates/renderer/src/vulkan/scene_buffer.rs).

The renderer uses an SSBO (not a UBO) so the shader can iterate a variable
number of lights without recompiling the pipeline. Each light is a 64-byte
struct: `position` (vec3), `radius` (f32), `color` (vec3), `intensity` (f32),
`type` (u32), `direction` (vec3), `inner_cos`, `outer_cos`. The fragment
shader iterates the array once and accumulates contributions; for RT
hardware it shoots a ray per light against the TLAS for hard shadows.

The SSBO is double-buffered between frames-in-flight and updated on the
host with `VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT`.

Cell interior lighting (the `XCLL` sub-record from CELL records) becomes
two entries: one ambient (modeled as a bottom-hemisphere directional) and
one directional (the cell's "key light" rotation/color). LIGH records
become point lights with their declared radius and color. See
[Cell Lighting](lighting-from-cells.md) for the full pipeline.

## Pipeline cache

The graphics pipeline is created with a `VkPipelineCache` object that
persists to disk across runs. On a cold start the cache file is missing
and `vkCreateGraphicsPipelines` takes 10–50 ms; on subsequent runs the
cache hits and the same call drops to <1 ms. The cache is written back to
disk on clean shutdown.

## Sync model

Per-frame:

- `image_available` semaphore — signaled by `vkAcquireNextImageKHR`,
  consumed by the queue submit
- `render_finished` semaphore — signaled by the queue submit, consumed by `vkQueuePresentKHR`
- `in_flight` fence — CPU-side wait, created `SIGNALED` so the first frame
  doesn't deadlock

Two frames in flight: while frame N is being presented, frame N+1 is
being recorded on a separate command buffer with a separate fence.
Acquiring image N+2 blocks until frame N's fence is signaled.

For the Havok / RT path there's an additional **HOST→AS_BUILD** memory
barrier that fences staging-buffer uploads before the BLAS build, and an
**AS_BUILD→TRANSFER** barrier on the TLAS scratch buffer between frames.

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
across cell loads. Deferred destruction queues the old `VkImage` until two
frames have elapsed before actually freeing the GPU memory, so dynamic UI
texture updates (Ruffle SWF rendering) don't need a `device_wait_idle`.

## Asset reading helpers

The renderer's mesh registry exposes a small `upload(...)` API used by both
the cell loader and the loose-NIF demo path. It takes vertex and index
slices, hands off the GPU upload to a one-time command buffer, queues the
BLAS build, and returns a `MeshHandle`. See the [Asset Pipeline](asset-pipeline.md)
doc for the full NIF→ECS→GPU upload flow.

## Dependencies

| Crate           | Version | Purpose                                    |
|-----------------|---------|--------------------------------------------|
| ash             | 0.38    | Raw Vulkan bindings                        |
| ash-window      | 0.13    | Surface creation from window handles       |
| gpu-allocator   | 0.27    | Vulkan memory allocator                    |
| winit           | 0.30    | Window handle types                        |
| raw-window-handle | 0.6   | Platform-agnostic handle traits            |
| image           | 0.24    | PNG fallback for non-DDS textures          |

The shaders are compiled offline with `glslangValidator` and embedded
into the binary via `include_bytes!` from `crates/renderer/shaders/`.
