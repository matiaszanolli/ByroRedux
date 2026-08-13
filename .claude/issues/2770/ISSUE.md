# REN-D1-03: Magic material kind 11 (MultiLayerParallax) hand-copied at four sites instead of imported from the shared table

- **Severity**: LOW
- **Dimension**: 1
- **Labels**: low,renderer,tech-debt,bug

## Description
Magic material kind `11` (MultiLayerParallax) hand-copied at four live sites, one gating the TLAS instance shadow mask; `MATERIAL_KIND_GLASS` beside it is imported from the shared table. Test declares a 4th copy, so it cannot detect `is_refractive_glass` drifting.

## Location
`crates/renderer/src/vulkan/acceleration/predicates.rs`, `crates/renderer/src/vulkan/context/draw.rs`, `crates/renderer/src/vulkan/acceleration/tests.rs`, `crates/renderer/shaders/triangle.frag`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D1-03).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2770
