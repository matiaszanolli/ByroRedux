---
issue: 0
title: REN-D1-NEW-01: current_frame=0 reset after resize can submit on un-waited fence
labels: bug, renderer, medium, vulkan, sync
---

**Severity**: MEDIUM (deadlock potential after resize)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 1)

## Location

- `crates/renderer/src/vulkan/context/resize.rs:469` — post-resize `current_frame = 0` reset
- `crates/renderer/src/vulkan/context/draw.rs:108-120` — both-fences wait pattern

## Why it's a bug

`device_wait_idle` does NOT transition UNSIGNALED fences back to SIGNALED. The both-fences wait pattern at `draw.rs:108-120` can deadlock on a slot whose last `reset_fences` was issued mid-recording before resize.

## Fix sketch

Re-create or explicitly re-signal the in_flight fences after the `device_wait_idle` in `recreate_swapchain`. Either:
- Destroy + recreate the fences (mirror what `recreate_for_swapchain` does for semaphores), OR
- After `device_wait_idle`, walk all `in_flight_fences` and re-signal any that are UNSIGNALED.

## Completeness Checks

- [ ] **UNSAFE**: Sync primitive recreation; verify destroy ordering.
- [ ] **SIBLING**: Verify `images_in_flight` reset at the same site is not affected.
- [ ] **DROP**: No Drop impact.
- [ ] **LOCK_ORDER**: No RwLock changes.
- [ ] **FFI**: No cxx changes.
- [ ] **TESTS**: Manual repro with VK_LAYER_KHRONOS_validation + rapid resize storm.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
