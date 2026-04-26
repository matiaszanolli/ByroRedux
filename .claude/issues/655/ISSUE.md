# Issue #655: LIFE-M2: SwapchainState::destroy takes &self — image_views vector left populated, masking double-destroy hazard

**File**: `crates/renderer/src/vulkan/swapchain.rs:202-208`
**Dimension**: Resource Lifecycle

`destroy` takes `&self`, so `self.image_views` is not cleared. If `destroy` is ever called twice (e.g. error path that calls it then drops VulkanContext which also calls swapchain destroy — currently doesn't happen, but would on any future panic cleanup), every view handle is destroyed twice and the swapchain handle is destroyed twice. Vulkan permits double-destroy on `VK_NULL_HANDLE` but not on the same valid handle.

**Fix**: Change signature to `&mut self`, clear `self.image_views` and set `self.swapchain = vk::SwapchainKHR::null()` after destruction. The single caller in Drop (mod.rs:1384) already has `&mut self.swapchain_state` available.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
