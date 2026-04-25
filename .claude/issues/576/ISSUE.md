# PIPE-2: All rasterization pipelines unconditionally rebuilt on resize despite dynamic viewport/scissor

State: OPEN

## Finding

**Source**: AUDIT_RENDERER_2026-04-22 · Dimension 3 — Pipeline State
**File**: `crates/renderer/src/vulkan/context/resize.rs:49-64`
**Severity**: MEDIUM (avoidable compilation stall on resize)

`recreate_swapchain` destroys and recreates all rasterization pipelines unconditionally:

```rust
// Destroy old pipelines before the render pass they reference.
self.device.destroy_pipeline(self.pipeline, None);
self.device.destroy_pipeline(self.pipeline_two_sided, None);
for (_, pipe) in self.blend_pipeline_cache.drain() {
    self.device.destroy_pipeline(pipe, None);
}
self.device.destroy_pipeline(self.pipeline_ui, None);
self.device.destroy_render_pass(self.render_pass, None);
```

Since viewport and scissor are declared as dynamic state, these pipelines are format-independent. The render pass is also recreated unconditionally (line 63), which is what drives the pipeline invalidation. On window drag resize events, this causes avoidable pipeline recompilation stalls (even with pipeline cache, the VkPipeline object must be destroyed and re-created).

## Fix

Add a format comparison guard before render pass recreation:

```rust
let new_format = new_swapchain_state.format;
let new_depth_format = self.find_depth_format();
let format_changed = new_format != self.swapchain_state.format
    || new_depth_format != self.depth_format;

if format_changed {
    // Only rebuild render pass and pipelines when format changes
    self.device.destroy_pipeline(self.pipeline, None);
    // ... etc
    self.render_pass = self.create_render_pass(new_format, new_depth_format);
    self.pipeline = self.create_pipeline(self.render_pass);
    // ...
}
```

On extent-only resizes (the common case during window dragging), only framebuffers and G-buffer images need recreation.

## Completeness Checks
- [ ] **SIBLING**: Composite pipeline (`composite.rs`) also recreated on resize — apply same guard
- [ ] **SIBLING**: SVGF and SSAO compute pipelines — verify if they are also unconditionally recreated
- [ ] **DROP**: Pipeline destruction must still happen when format changes — ensure no leak in the conditional path
- [ ] **TESTS**: Resize window rapidly → no visible stutter; validation layers clean on resize
