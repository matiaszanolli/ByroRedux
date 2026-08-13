# REN-D8-NEW-02: record_ssao_pass doc claims current-frame AO with no lag, actual AO sample is two frames old

- **Severity**: LOW
- **Dimension**: 8
- **Labels**: low,renderer,documentation

## Description
Doc says AO is "current-frame (no lag)" because SSAO runs before composite — but `composite.frag` has no AO binding at all; the sole reader is `triangle.frag` in the **main render pass**, which runs earlier. With per-FIF AO images the sampled AO is **two frames old**, not zero and not one. `triangle.frag`'s own "computed last frame" is closer but still off by one slot.

## Location
`crates/renderer/src/vulkan/context/post_passes.rs` (`record_ssao_pass` doc)

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D8-NEW-02).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2798
