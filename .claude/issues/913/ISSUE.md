---
issue: 0
title: REN-D7-NEW-07: recreate_swapchain does NOT reset frame_counter — stale TAA jitter post-resize
labels: renderer, medium, vulkan
---

**Severity**: MEDIUM (one frame of mis-aligned TAA reprojection after every resize)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 7)

## Location

- `crates/renderer/src/vulkan/context/resize.rs` — `recreate_swapchain` resets `current_frame = 0` but NOT `frame_counter`

## Why it's a bug

`recreate_swapchain` resets `current_frame = 0` but NOT `frame_counter`, which feeds the Halton TAA jitter sequence. One frame of mis-aligned reprojection after every resize. Visible as a one-frame ghost / smear on the resized first frame.

## Fix sketch

Two options:
1. Reset `frame_counter` alongside `current_frame` in `recreate_swapchain`.
2. Call `signal_temporal_discontinuity` (if it exists) to bump the SVGF/TAA recovery window so the first post-resize frame is a clean accumulation start.

Option 2 is more robust because it also clears the SVGF history, but option 1 is the minimal fix.

## Completeness Checks

- [ ] **SIBLING**: Verify SVGF + TAA both consume `frame_counter` consistently.
- [ ] **TESTS**: Manual visual check: resize during continuous motion, verify no first-frame ghost.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
