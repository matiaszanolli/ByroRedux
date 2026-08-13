# REN-D8-NEW-04: svgf_temporal_clamps_fireflies test doc names a dead TAA sibling test removed by e5d02f83

- **Severity**: LOW
- **Dimension**: 8
- **Labels**: low,renderer,documentation

## Description
`svgf_temporal_clamps_fireflies_before_history_branch`'s doc names a TAA sibling test (`taa_comp_floors_alpha_for_moving_pixels_under_parked_camera`) added by `c6342845` and removed by `e5d02f83` — a dead symbol in the doc of a regression guard whose whole purpose is surviving refactors. Live nearest sibling: `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces`.

## Location
`crates/renderer/src/vulkan/svgf.rs`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D8-NEW-04).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2800
