# REN-D4-03: egui_pass in_dep comment describes pre-FSR-tail dependency chain, no longer accurate

- **Severity**: LOW
- **Dimension**: 4
- **Labels**: low,renderer,documentation

## Description
The `in_dep` comment says it chains after composite's swapchain write and that composite's outgoing dep sets `dstStage = NONE`; since the FSR tail, `PresentationPipeline` writes the swapchain and its outgoing dep uses `COLOR_ATTACHMENT_OUTPUT | TRANSFER`.

## Location
`crates/renderer/src/vulkan/egui_pass.rs`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D4-03).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2786
