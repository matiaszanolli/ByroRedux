# REN-D8-NEW-03: composite_dep_in comment calls scene_image_views the swapchain image, contradicts module docstring

- **Severity**: LOW
- **Dimension**: 8
- **Labels**: low,renderer,documentation

## Description
The `composite_dep_in` comment calls attachment 0 "the swapchain image"; it is `scene_image_views[i]`, an offscreen `HDR_FORMAT` image. The dependency's *reasoning* is still correct — only the noun is stale — and the module docstring at the top of the same file is already right, so the file contradicts itself.

## Location
`crates/renderer/src/vulkan/composite.rs`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D8-NEW-03).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2799
