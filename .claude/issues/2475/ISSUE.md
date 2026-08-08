# REN-D20-NEW-01: EguiPass render pass survives a swapchain format change (framebuffers only are rebuilt)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2475
**Finding ID**: REN-D20-NEW-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 20 — Debug/Telemetry
**Location**: `crates/renderer/src/vulkan/egui_pass.rs::EguiPass::recreate_framebuffers` / `crates/renderer/src/vulkan/context/resize.rs:878-889`
**Status**: NEW

## Description
`recreate_swapchain` explicitly treats a surface-format change as a reachable case: it computes `let format_changed = self.swapchain_state.format != old_swapchain_format;` and, when true, tears down + rebuilds the main render pass and every rasterization pipeline. The `presentation` pipeline is destroyed and reconstructed unconditionally on every resize, so it also picks up the new format. `EguiPass` does neither. It only gets `recreate_framebuffers`, whose doc comment asserts the opposite of what the sibling code assumes: "The render pass itself stays — the swapchain format is the same after resize." Two consequences on a format change: (a) `create_framebuffers` attaches the new image views to a render pass whose attachment `format` is the *old* one (VUID-VkFramebufferCreateInfo-pAttachments-00880), and (b) `Options::srgb_framebuffer`, computed once in `EguiPass::new`, is never recomputed, so the overlay's gamma curve silently flips wrong even if (a) were tolerated.

## Evidence
```rust
// resize.rs:186 — the codebase's own admission that this is reachable
let format_changed = self.swapchain_state.format != old_swapchain_format;
...
// resize.rs:883 — egui gets framebuffers only, no format re-check
if let Some(ref mut pass) = self.egui_pass {
    pass.recreate_framebuffers(&self.device, &self.swapchain_state.image_views, self.swapchain_state.extent)?;
}
```
vs. `egui_pass.rs:121-123`: "The render pass itself stays — the swapchain format doesn't change on resize."

## Impact
Only fires when the surface format actually changes across a recreate (HDR/SDR display switch, monitor move, driver-side format renegotiation). When it does, the `?` on `recreate_framebuffers` propagates out of `recreate_swapchain`, which is a hard failure of the whole resize, not a graceful overlay-off. Blast radius is the entire frame loop, not just the overlay. Frequency is low; severity when hit is high.

## Related
#576 (PIPE-2, the format-gated pipeline rebuild this path was modelled on); #1433 (egui incoming dependency); `resize.rs:932-971` (presentation's unconditional rebuild is the pattern to copy).

## Suggested Fix
Pass the new format into the egui resize hook and, when it differs from the one `EguiPass::new` was built with, drop + rebuild the whole `EguiPass` (as `presentation` does) rather than only its framebuffers. **Needs a format-change repro (HDR toggle / monitor move) or RenderDoc to observe** — the failure mode does not appear in `cargo test`.

## Completeness Checks
- [ ] **TESTS**: Needs a format-change repro (HDR toggle / monitor move) to confirm the fix
