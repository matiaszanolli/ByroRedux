# #576 PIPE-2: All rasterization pipelines unconditionally rebuilt on resize despite dynamic viewport/scissor

**Severity**: MEDIUM  
**Audit**: AUDIT_RENDERER_2026-04-22  
**File**: `crates/renderer/src/vulkan/context/resize.rs:49-64`

## Summary

`recreate_swapchain` destroys and recreates all pipelines unconditionally. Since viewport/scissor are dynamic state, pipelines are format-independent. A swapchain format comparison guard would let extent-only resizes skip pipeline rebuild entirely.

## Fix

Compare `swapchain_format` + `depth_format` before destroying render pass and pipelines. Skip rebuild if only extent changed.
