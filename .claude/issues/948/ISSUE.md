# Issue #948 — REN-D4-NEW-02: Packed depth-stencil fallback uses DEPTH-only view with combined-layout final_layout

**Severity**: LOW
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-11_DIM4.md
**Labels**: bug, renderer, vulkan, low

## Location

- `crates/renderer/src/vulkan/context/helpers.rs:336-342` — depth view aspect mask
- `crates/renderer/src/vulkan/context/helpers.rs:98` — render pass depth `final_layout`

## Evidence

`find_depth_format` accepts packed depth-stencil fallbacks. View uses `aspect_mask: DEPTH` only; render pass uses combined `DEPTH_STENCIL_READ_ONLY_OPTIMAL` final_layout. On non-NVIDIA devices that pick the packed fallback, stencil aspect is never transitioned.

## Fix sketch

(a) Restrict `find_depth_format` to `D32_SFLOAT` + `D16_UNORM` (pure depth) — 3-line change, easiest. OR
(b) Switch depth `final_layout` to `DEPTH_READ_ONLY_OPTIMAL` via VK_KHR_separate_depth_stencil_layouts when packed format is selected — ~10 lines, spec-strict.
