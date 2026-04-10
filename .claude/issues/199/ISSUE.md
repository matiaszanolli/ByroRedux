# #199: RL-4: recreate_swapchain double-destroys stale handles on error
- **Severity**: HIGH — **Domain**: renderer — **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:20-181`
- **Fix**: Null out handles after destroying them (vkDestroy* accepts null as no-op)
