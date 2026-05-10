---
issue: 0
title: REN-D5-NEW-01: acquire_next_image error path between acquire and submit leaks image_available semaphore signal
labels: bug, renderer, medium, vulkan, sync
---

**Severity**: MEDIUM (semaphore leak on rare error path → next acquire trips validation)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 5)

## Location

- `crates/renderer/src/vulkan/context/draw.rs` — acquire_next_image call site

## Why it's a bug

`acquire_next_image` early-returns on `OUT_OF_DATE` cleanly, but `?`-propagated errors *between* a successful acquire and `queue_submit` leak the `image_available[frame]` semaphore signal. The next acquire on the same slot trips VUID-vkAcquireNextImageKHR-semaphore-01779.

## Fix sketch

Two options:
1. **Stub-submit**: on error path, submit an empty command buffer that waits on `image_available[frame]` to consume the signal.
2. **Recreate**: destroy and recreate `image_available[frame]` on the error path before returning.

Option 1 is the canonical pattern (see Khronos sample's error-handling).

## Completeness Checks

- [ ] **UNSAFE**: Verify error path covers all `?`-propagated sites between acquire and submit.
- [ ] **SIBLING**: Audit screenshot copy + UI overlay paths for the same pattern.
- [ ] **TESTS**: Inject a simulated error between acquire and submit; verify validation passes on next frame.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
