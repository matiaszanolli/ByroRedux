# Vulkan Renderer

The renderer uses `ash` (raw Vulkan bindings for Rust), `ash-window` for
surface creation, and `gpu-allocator` for GPU memory management.

Source: `crates/renderer/src/vulkan/`

## Initialization Chain

`VulkanContext::new()` performs the full 11-step Vulkan initialization:

| Step | Module | What |
|------|--------|------|
| 1 | — | Load Vulkan entry points via `ash::Entry::load()` |
| 2 | `instance.rs` | Create `VkInstance` (API 1.3, platform extensions, validation layers) |
| 3 | `debug.rs` | Set up debug messenger routing to `log` crate |
| 4 | `surface.rs` | Create surface from raw window handles via `ash-window` |
| 5 | `device.rs` | Pick physical device (check extensions + queue families) |
| 6 | `device.rs` | Create logical device with graphics + present queues |
| 7 | `swapchain.rs` | Create swapchain (sRGB B8G8R8A8, mailbox present, clamped extent) |
| 8 | `context.rs` | Create render pass (CLEAR load op, PRESENT_SRC final layout) |
| 9 | `context.rs` | Create framebuffers per swapchain image |
| 10 | `context.rs` | Create command pool (per-buffer reset) + command buffers |
| 11 | `sync.rs` | Create sync objects (2 frames in flight, signaled fences) |

## Modules

### instance.rs

Creates the Vulkan instance with:
- Application info (name, version, API 1.3)
- Platform surface extensions (queried from display handle)
- `VK_EXT_debug_utils` extension (debug builds)
- `VK_LAYER_KHRONOS_validation` layer (debug builds, with availability check)

### debug.rs

Installs a `VK_EXT_debug_utils` messenger that routes Vulkan validation
messages through Rust's `log` crate:
- `ERROR` → `log::error!`
- `WARNING` → `log::warn!`
- `INFO` → `log::info!`
- `VERBOSE` → `log::trace!`

Covers general, validation, and performance message types.

### surface.rs

Thin wrapper around `ash_window::create_surface()`. Takes raw display and
window handles, returns a `VkSurfaceKHR`.

### device.rs

**Physical device selection** checks:
1. Required extension support (`VK_KHR_swapchain`)
2. Graphics queue family (supports `GRAPHICS` flag)
3. Present queue family (surface present support)

Logs the selected GPU name.

**Logical device creation** handles:
- Distinct queue families (graphics != present → concurrent sharing mode)
- Same queue family → exclusive sharing mode
- Creates device with requested queues

### swapchain.rs

**Format selection:** Prefers `B8G8R8A8_SRGB` with `SRGB_NONLINEAR` color space.

**Present mode:** Mailbox (triple-buffered) if available, otherwise FIFO.

**Extent:** Uses `currentExtent` if set, otherwise clamps window size to
surface capabilities.

**Image count:** `minImageCount + 1`, clamped to `maxImageCount`.

Creates image views for each swapchain image (2D, identity swizzle, color aspect).

### sync.rs

Creates per-frame synchronization objects for 2 frames in flight:
- `image_available` semaphores — signaled when swapchain image is acquired
- `render_finished` semaphores — signaled when command buffer finishes
- `in_flight` fences — CPU-side wait, created SIGNALED for first frame

### context.rs

**VulkanContext** owns all Vulkan state. Key operations:

`draw_clear_frame(color)`:
1. Wait for in-flight fence
2. Acquire next swapchain image
3. Reset fence
4. Record command buffer: begin render pass → end render pass (clear only)
5. Submit to graphics queue with semaphore synchronization
6. Present to swapchain
7. Handle `ERROR_OUT_OF_DATE_KHR` → signal swapchain recreate needed

`recreate_swapchain(size)`:
1. `device_wait_idle()`
2. Destroy framebuffers, render pass, swapchain image views, swapchain
3. Create new swapchain, render pass, framebuffers
4. Reallocate command buffers if image count changed

**Drop** waits for device idle, then destroys everything in reverse
initialization order.

## Current State

The renderer currently clears to a solid color (cornflower blue). It does
not yet render geometry, bind textures, or use pipelines. This is intentional
— the Vulkan foundation is proven correct, and rendering integration comes
after the game loop and ECS are solid.

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| ash | 0.38 | Raw Vulkan bindings |
| ash-window | 0.13 | Surface creation from window handles |
| gpu-allocator | 0.27 | GPU memory allocation (not yet used) |
| winit | 0.30 | Window handle types |
| raw-window-handle | 0.6 | Platform-agnostic handle traits |
