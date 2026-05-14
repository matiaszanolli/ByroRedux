# Issue #1031

**Title**: REN-D10-NEW-11: SvgfPipeline::recreate_on_resize skips initialize_layouts on freshly-created history images

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D10-NEW-11
**Severity**: LOW
**File**: `crates/renderer/src/vulkan/svgf.rs:568`

## Issue

`SvgfPipeline::recreate_on_resize` allocates fresh history images at `initial_layout: UNDEFINED` but never re-runs `initialize_layouts` to walk them to GENERAL before the next dispatch. The history-reset gate means contents are unused, but **validation will fire VUID-vkCmdDispatch-None-04115 on the post-resize first dispatch** (image bound as STORAGE descriptor at UNDEFINED layout).

## Fix

Call `initialize_layouts` after the image-create loop, OR set `initial_layout: GENERAL` on history images and add a `VK_IMAGE_LAYOUT_UNDEFINED → GENERAL` transition in the first-frame post-resize barrier path. Same shape as caustic / TAA / bloom resize paths.

## Completeness Checks
- [ ] **SIBLING**: Cross-check caustic.rs `recreate_on_resize` for the same gap
- [ ] **TESTS**: Resize integration with validation layers enabled

