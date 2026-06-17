# REN-D10-01: Soft-particle depth fade mixes relative/absolute precision conventions

- **Issue**: #1642
- **Severity**: HIGH
- **Dimension**: Camera-Relative Precision
- **Source audit**: docs/audits/AUDIT_RENDERER_2026-06-16.md
- **Labels**: high, renderer, bug
- **Location**: `crates/renderer/shaders/triangle.frag` — `MAT_FLAG_EFFECT_SOFT` soft-particle depth-fade block (~L601-614)

## Description
The soft-particle depth fade (introduced by `1ddeae28`) reconstructs occluder
(`sceneWorld`) and fragment (`fragSceneWorld`) via the render-origin-**relative**
`invViewProj`, but measures the gap against the **absolute** `cameraPos.xyz`.
Mixing the two precision conventions the renderer otherwise keeps separate.

## Evidence
`triangle.frag:613-614` differences `length(... - cameraPos.xyz)` terms with the
absolute camera. `ssao.comp` (which the comment claims to mirror) is fed a
relative `ssao_cam_rel = camera_pos - render_origin` in `context/draw.rs`; the
soft-fade path reuses the absolute `CameraUBO.cameraPos`.

## Impact
Works in interiors (`render_origin ≈ 0`); degenerates to zero/noise in
large-coordinate exteriors (MarkarthWorld, FO4 Commonwealth), breaking the FO4
exterior-FX use case the feature was built for. Bounded to effect-shader soft
particles; not a crash.

## Suggested Fix
Compute the gap using `cameraPos.xyz - renderOrigin.xyz` (relative camera vs
relative scene points), mirroring the `ssao_cam_rel` pattern. Or add
`renderOrigin.xyz` to both scene points to reconstruct absolute before
differencing against absolute `cameraPos`.

## Completeness Checks
- [ ] SIBLING: other recently-added `invViewProj` reconstruction sites checked
- [ ] TESTS: regression test pins gap as origin-invariant
