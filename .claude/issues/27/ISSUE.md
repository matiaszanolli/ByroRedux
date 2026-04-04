# Issue #27: Renderer: duplicate pipeline rasterizers — two-sided pipelines are no-ops

- **State**: OPEN
- **Labels**: bug, renderer, medium, pipeline
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:105-121`

Both `rasterizer` and `rasterizer_no_cull` use CULL_MODE_NONE. Four
pipeline objects exist but only two unique configs.
