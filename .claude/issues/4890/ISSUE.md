# REN-D5-2026-09-26-12: `recreate_swapchain_core` has two `?` windows between creating the new swapchain and retiring the old one

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4890

**Labels**: low,renderer,vulkan,bug

- **Severity**: LOW (every caller treats a resize failure as fatal and exits; the leak is on an exit path only)
- **Dimension**: Memory/Lifecycle (resize error paths)
- **Location**: `crates/renderer/src/vulkan/context/resize.rs` — `recreate_swapchain_core`
- **Status**: NEW
- **Description / Evidence**: I confirmed the order in code. `old_image_views` and `old_swapchain` are held in locals, then `swapchain::create_swapchain(..)?`, `FrameExtentSet::for_output(..)?` and `FsrTemporalState::new(..).context(..)?` all run *before* the old-view destroy loop and `destroy_swapchain(old_swapchain)`. (A) If `create_swapchain` fails, `old_image_views` is dropped (handles leaked) and `Drop` destroys the still-recorded old swapchain. (B) If either later `?` fails, `self.swapchain.state` is already the new swapchain, so both `old_image_views` and the retired `old_swapchain` leak. `Drop` never destroys the retired swapchain, which violates VUID-vkDestroySurfaceKHR-surface-01266 at exit.
- **Suggested Fix**: Move the old-view destroy loop and `destroy_swapchain(old_swapchain)` to immediately after `create_swapchain` succeeds and before the extent and FSR computations. It is a host-only reorder that keeps the LIFE-M1 constraint. Extend `old_image_views_destroyed_between_new_swapchain_creation_and_old_destroy`.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl and teardown ordering are still correct
- [ ] **TESTS**: A regression test pins this specific fix
