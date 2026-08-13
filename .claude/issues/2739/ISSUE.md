# REN-D4-01: recreate_for_swapchain's fence loop destroys before a fallible recreate with no null-out

**Severity**: HIGH
**Dimension**: 4 — Sync/Barriers
**Location**: `crates/renderer/src/vulkan/sync.rs` — `recreate_for_swapchain`, the `for fence in &mut self.in_flight` loop; `crates/renderer/src/vulkan/context/resize.rs` — `recreate_screen_passes`

## Description

The `in_flight` loop does `destroy_fence(*fence)` and then a fallible `create_fence(...)?` **without nulling the handle first**, while the `render_finished` loop directly above it correctly `clear()`s before rebuilding. If `create_fence` fails mid-loop, the error propagates with `self.in_flight` holding one or more destroyed handles that later code — including `VulkanContext::drop` — will use or destroy again. Compounding it, `recreate_screen_passes` assigns `self.framebuffers = create_main_framebuffers(...)` **before** calling `recreate_for_swapchain(...)?`, so the `#1211` `framebuffers.is_empty()` sentinel (which is what makes a partially-failed resize survivable) is already satisfied by the time the fence step can fail.

## Impact

Use of a destroyed `VkFence` after a failed swapchain recreate — a spec violation with driver-defined consequences, and a double-destroy at teardown. Reachable only when `vkCreateFence` fails, i.e. under host-memory pressure during a resize.

## Suggested Fix

Mirror the `render_finished` loop — `clear()` or null each handle before the fallible recreate — and move the `framebuffers` assignment after `recreate_for_swapchain` so the `#1211` sentinel covers the whole function.

## Related

#1211, #910, #952, #1188, #2741 (same no-null-after-destroy hygiene class, one layer up)

Filed from `docs/audits/AUDIT_RENDERER_2026-08-12b.md` (finding REN-D4-01).
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2739
