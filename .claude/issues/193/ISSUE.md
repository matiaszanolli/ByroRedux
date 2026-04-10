# #193: SYNC-1: SSAO reads destroyed depth image view after swapchain recreation
- **Severity**: CRITICAL
- **Dimension**: Vulkan Synchronization / Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/resize.rs`, `crates/renderer/src/vulkan/ssao.rs:254-257`
- **Source**: `docs/audits/AUDIT_RENDERER_2026-04-10b.md`
- **Related**: #195 (LIFE-2), #198 (SYNC-4)
- **Fix**: Destroy+recreate SSAO pipeline in `recreate_swapchain()` with new depth image view and dimensions
