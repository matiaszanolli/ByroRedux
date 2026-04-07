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
  `DEVICE_LOCAL` memory with HOST→AS_BUILD memory barriers
- **Multi-light SSBO**: point / spot / directional lights consumed by the
  fragment shader, with `VK_KHR_ray_query` shadow rays per light
- **Pipeline cache** persisted to disk (10–50 ms cold init → <1 ms warm)
- **Shared `VkSampler`** across all textures (one descriptor type, many bindings)
- **Per-image semaphore synchronization** with 2 frames in flight
- **Atomic swapchain handoff** during resize (no dropped frames, no torn submits)
- **Deferred texture destruction** so dynamic UI texture updates don't stall
  the graphics queue
- **Depth format autodetect** with fallback chain D32→D32S8→D24S8→D16
- **Backface culling** with confirmed NIF/D3D clockwise winding convention
- **DDS texture loading**: BC1 / BC3 / BC5 + DX10 with mipmaps

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
├── scene_buffer.rs     SSBO for multi-light data; uploaded per frame
├── acceleration.rs     BLAS + TLAS construction, scratch buffer reuse
├── texture.rs          DDS upload (RGBA + BC*), staging, layout transitions
├── dds.rs              DDS header parser (BC1/BC3/BC5 + DX10 extended, mips)
└── context/
    ├── mod.rs          VulkanContext struct definition + new() / Drop
    ├── draw.rs         draw_frame() — per-frame command recording + submission
    ├── resize.rs       recreate_swapchain() — atomic handoff on window resize
    ├── resources.rs    BLAS construction, register_ui_quad, log_memory_usage
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
4. Walk the ECS to collect render data: visible mesh handles, transforms,
   materials, light sources, decals
5. Update the scene SSBO with the per-frame light array (point/spot/dir)
6. Rebuild the TLAS over all visible BLASes (or skip if RT disabled)
7. Begin the render pass, bind pipeline + descriptor sets
8. For each mesh: push the model matrix as a push constant, bind the mesh
   vertex/index buffer, bind its texture descriptor set, draw indexed
9. End render pass, end command buffer
10. Submit to the graphics queue with semaphore sync, present to the swapchain

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
  mesh registry. Built from the vertex + index buffer in `DEVICE_LOCAL`
  memory. The build is queued on a transfer command buffer and waited on
  with a HOST→AS_BUILD memory barrier before the next frame's TLAS build.
- **TLAS per frame**: rebuilt every frame from the visible mesh handles
  and their world transforms. Scratch memory is reused between frames.
- **Ray query in the fragment shader**: each light tests visibility against
  the TLAS via `rayQueryEXT`, returning a hard-shadow boolean. Soft shadows
  are deferred (M22+ polish on the deferred roadmap).

The `VK_KHR_ray_query` extension is queried at device-pick time. When it's
not present, the fragment shader falls back to non-shadowed multi-light.

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
