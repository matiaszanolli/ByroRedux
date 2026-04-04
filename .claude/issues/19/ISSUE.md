# Issue #19: Renderer: depth store DONT_CARE precludes future depth readback

- **State**: OPEN
- **Labels**: enhancement, renderer
- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/context.rs:760`

Depth attachment uses `StoreOp::DONT_CARE`. Needs `STORE` for deferred shading, SSAO, or shadow mapping.

**Fix**: Change depth `store_op` to `STORE`.
