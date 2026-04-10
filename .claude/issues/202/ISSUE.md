# #202: SYNC-03: SSAO param UBO host write lacks barrier before compute dispatch
- **Severity**: MEDIUM — **Domain**: renderer — **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/ssao.rs:329-368`
- **Fix**: Add HOST→COMPUTE_SHADER memory barrier for UBO before dispatch
