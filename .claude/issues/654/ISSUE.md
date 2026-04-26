# Issue #654: LIFE-M1: recreate_swapchain destroys old image views before new swapchain is created

**File**: `crates/renderer/src/vulkan/context/resize.rs:61-74`
**Dimension**: Resource Lifecycle

Lines 61-63 destroy `self.swapchain_state.image_views` (children of the old swapchain). Line 74 then passes `old_swapchain` as the `oldSwapchain` parameter to `vkCreateSwapchainKHR`. Per spec VUID-VkSwapchainCreateInfoKHR-oldSwapchain-01933, the old swapchain must be in a state where it can be retired — this is satisfied. But destroying the image views before the swapchain create call places the old swapchain in an inconsistent state for validation layer's "swapchain image not in expected state" check.

The Vulkan spec is ambiguous (image views are independent objects; spec doesn't forbid destroying them before passing the swapchain to oldSwapchain), so this isn't a hard violation, but most reference implementations defer image-view destruction until after the new swapchain is created.

**Fix**: Move the `for &view in &self.swapchain_state.image_views` loop (lines 61-63) to execute AFTER `swapchain::create_swapchain` returns (line 83), but before `destroy_swapchain(old_swapchain, …)` at line 122-127.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
