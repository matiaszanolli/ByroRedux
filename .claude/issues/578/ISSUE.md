# #578 PIPE-1: Composite pipeline bakes static viewport/scissor redundantly alongside dynamic state

**Severity**: LOW  
**Audit**: AUDIT_RENDERER_2026-04-22  
**File**: `crates/renderer/src/vulkan/composite.rs:637-681`

## Summary

`new_inner` bakes width/height into `viewports`/`scissors` at creation while also declaring DYNAMIC_STATE_VIEWPORT/SCISSOR. The baked values are ignored at runtime (dynamic state overrides them). Dead code that misleads readers.

## Fix

Use empty slices with explicit counts for dynamic viewport/scissor state in `PipelineViewportStateCreateInfo`.
