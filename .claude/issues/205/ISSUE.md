# #205: SYNC-06: recreate_swapchain does not reset current_frame
- **Severity**: LOW — **Domain**: renderer — **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/context/resize.rs`
- **Fix**: Add `self.current_frame = 0;` at end of recreate_swapchain
