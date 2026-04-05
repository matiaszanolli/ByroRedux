# Investigation: Issue #49

## Root Cause
PipelineCache::null() at pipeline.rs:249 and :377. All 5 pipelines
compiled cold on every startup and swapchain recreation.

## Creation Sites
- create_triangle_pipeline: 4 graphics pipelines (opaque, alpha, two-sided × 2)
- create_ui_pipeline: 1 UI overlay pipeline
- Both called from context.rs new() and recreate_swapchain()

## Fix
1. Add pipeline_cache field to VulkanContext
2. Load from disk (pipeline_cache.bin) at startup, empty if not found
3. Pass to both pipeline creation functions
4. Save to disk in Drop before destroying the cache
5. Destroy the cache in Drop

## Scope
3 files: context.rs (create/store/save/destroy), pipeline.rs (accept cache
parameter), pipeline cache file (~/.cache/byroredux/ or next to binary).
