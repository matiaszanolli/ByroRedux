---
issue: 0
title: REN-D1-NEW-02: render_finished per-image semaphore can be re-used before present engine releases it
labels: bug, renderer, high, vulkan, sync
---

**Severity**: HIGH (validation error / race under image-index aliasing)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 1)

## Location

- `crates/renderer/src/vulkan/sync.rs:80,103` — `render_finished` and `images_in_flight` sized to `swapchain_image_count`
- `crates/renderer/src/vulkan/sync.rs:43-46` — doc comment claims the opposite of what's true
- `crates/renderer/src/vulkan/context/draw.rs:~1936,~1959` — submit and present sites index by `image_index`

## Why it's a bug

The `images_in_flight` fence guard covers GPU work completion, not present-engine processing. With multiple swapchain images and frame-pacing variance, a per-image `render_finished` semaphore can be signaled by frame N+1 while the present engine is still waiting on it from frame N.

Per Vulkan spec (and the Khronos sample's canonical pattern), `render_finished` should be sized per **frame-in-flight**, not per swapchain image. The present wait should key off `frame`, not `image_index`.

Current sync.rs:43-46 doc:
```
/// `render_finished` semaphores are per swapchain image — signaled when
```
This is the canonical anti-pattern; the comment correctly describes what the code does, but the code is wrong.

## Fix sketch

1. Resize `render_finished` from `swapchain_image_count` to `MAX_FRAMES_IN_FLIGHT`.
2. Update the submit site at `draw.rs:~1936` to use `render_finished[frame]` instead of `render_finished[image_index]`.
3. Update the present wait site at `draw.rs:~1959` to use `render_finished[frame]`.
4. Update sync.rs:43-46 doc to describe the per-frame semantics.
5. Verify `recreate_for_swapchain` recreates `render_finished` to the correct count.

## Repro

VK_LAYER_KHRONOS_validation under MAILBOX present mode with 3+ swapchain images and variable frame pacing (e.g. CPU-bound frame followed by GPU-bound frame). Look for VUID-vkQueueSubmit-pSignalSemaphores-00067 or VUID-vkQueuePresentKHR-pWaitSemaphores-01294.

## Completeness Checks

- [ ] **UNSAFE**: Sync primitive resizing — verify destroy ordering in `recreate_for_swapchain` and Drop.
- [ ] **SIBLING**: Verify `image_available` (already per-FIF) is unchanged.
- [ ] **DROP**: Verify Drop iterates the new (smaller) `render_finished` count.
- [ ] **LOCK_ORDER**: No RwLock changes.
- [ ] **FFI**: No cxx changes.
- [ ] **TESTS**: Hard to unit-test; rely on validation layers + manual MAILBOX repro.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
