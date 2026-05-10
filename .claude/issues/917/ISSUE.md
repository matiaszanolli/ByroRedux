---
issue: 0
title: REN-D10-NEW-03: SVGF frames_since_creation increments on dispatch call, not GPU completion
labels: renderer, medium, vulkan
---

**Severity**: MEDIUM (defence-in-depth; self-heals through svgf_failed latch + resize cycle today)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 10)

## Location

- `crates/renderer/src/vulkan/svgf.rs` — `frames_since_creation` increment site

## Why it's a bug

`frames_since_creation` increments at command-recording time, not GPU-completion time. If the GPU work fails (validation reject, queue lost, etc.) before the SVGF dispatch runs, the counter still advances and the next frame's reprojection assumes valid history that wasn't actually written.

Self-heals through the `svgf_failed` latch + resize cycle today, so not user-visible. But fragile for any future hot-recovery path.

## Fix sketch

Move the `frames_since_creation += 1` increment to either:
1. After the per-frame fence wait at the **start** of the next frame (proves last frame's GPU work completed), OR
2. Into a callback after `vkQueueSubmit` returns success.

## Completeness Checks

- [ ] **SIBLING**: TAA has the same accumulator pattern; verify same fix applies.
- [ ] **TESTS**: Inject a forced SVGF dispatch failure; verify history is treated as invalid on next frame.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
