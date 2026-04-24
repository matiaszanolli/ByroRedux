# #572 SY-1: Composite render pass dep_in omits COMPUTE_SHADER in src_stage_mask

**Severity**: MEDIUM  
**Audit**: AUDIT_RENDERER_2026-04-22  
**File**: `crates/renderer/src/vulkan/composite.rs:368-374`

## Summary

The composite render pass incoming subpass dependency only covers `COLOR_ATTACHMENT_OUTPUT` in `src_stage_mask`. The compute passes before composite (SVGF, TAA, caustic, SSAO) write images sampled by composite fragment shader. Currently masked by per-pass explicit barriers, but latent foot-gun.

## Fix

```rust
.src_stage_mask(
    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
        | vk::PipelineStageFlags::COMPUTE_SHADER,
)
```
