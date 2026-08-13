# REN-D2-01: ReSTIR spatial reuse provably inert past ~66 841 BU due to depth-clamp mismatch

- **Severity**: LOW
- **Dimension**: 2
- **Labels**: low,renderer,performance,bug

## Description
ReSTIR spatial reuse is provably inert past ~66 841 BU: the depth lane is clamped to 65504 on write, compared against unclamped `worldDist` on read. Five wasted reservoir fetches per pixel there. **Cluster D-2.**

## Location
`crates/renderer/shaders/triangle.frag` (reservoir write vs. `spatialDepthCompatible` read)

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D2-01).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2777
