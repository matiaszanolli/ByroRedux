# #573 SY-2: Main render pass outgoing dep carries BOTTOM_OF_PIPE in dst_stage_mask — Sync2 spec violation

**Severity**: MEDIUM  
**Audit**: AUDIT_RENDERER_2026-04-22  
**File**: `crates/renderer/src/vulkan/context/helpers.rs:153-158`

## Summary

Main G-buffer render pass outgoing subpass dependency has `BOTTOM_OF_PIPE` in `dst_stage_mask`. Vulkan spec §7.6.1 forbids this unless both access masks are 0. Rejected by Synchronization2 validation.

## Fix

```rust
.dst_stage_mask(
    vk::PipelineStageFlags::FRAGMENT_SHADER
        | vk::PipelineStageFlags::COMPUTE_SHADER,
    // Remove BOTTOM_OF_PIPE
)
```
