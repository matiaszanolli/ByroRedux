# Issues #301 and #302 — GPU memory / perf optimizations

## #301: write_mapped flushes entire allocation
- **Location**: `crates/renderer/src/vulkan/buffer.rs:569`
- **Problem**: passes `alloc.offset(), alloc.size()` to `aligned_flush_range` — flushes full allocation (e.g. 1.28 MB) when only `bytes.len()` (e.g. 32 KB) was written
- **Fix**: track actual written byte count, flush only that range
- **Callers** (6): scene_buffer (2), acceleration, composite, svgf, ssao — all write small payloads to larger buffers

## #302: Per-upload one-time fence create/destroy
- **Location**: `crates/renderer/src/vulkan/texture.rs:658-668`
- **Problem**: every `with_one_time_commands` creates a VkFence and destroys it. ~700 cycles × ~5us = ~3.5ms during cell load.
- **Fix**: persistent fence in VulkanContext, `vkResetFences` between uses
- **Callers** (11): texture uploads (2), buffer staging (1), BLAS builds (3), svgf/ssao/gbuffer init (3), scene_buffer init (via accel_manager), context init
- **Thread safety**: wrap fence in `Mutex<vk::Fence>` — submit+wait atomic per caller
