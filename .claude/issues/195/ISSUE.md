# #195: LIFE-2: Scene descriptor AO binding not updated after swapchain resize
- **Severity**: HIGH
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/resize.rs`, `crates/renderer/src/vulkan/scene_buffer.rs:614-633`
- **Source**: `docs/audits/AUDIT_RENDERER_2026-04-10b.md`
- **Related**: #193 (SYNC-1), #198 (SYNC-4)
- **Fix**: Call `scene_buffers.write_ao_texture()` for each frame slot after SSAO recreation in resize path
