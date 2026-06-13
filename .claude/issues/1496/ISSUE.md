## Finding REN2-11 — Renderer Audit 2026-06-11

- **Severity**: LOW (improvement opportunity, not a regression)
- **Dimension**: RT Ray Queries / Tangent-Space (Dims 9 + 16)
- **Location**: `crates/renderer/shaders/triangle.vert:190`; derivative consumers in `triangle.frag`: `:1312` (flat-shading normal), `:1231-1234` (derivative TBN), `:1122-1125` (POM), `:1643` (rtLOD footprint) — POM/TBN receive `fragWorldPos` via callers at `:1353`/`:1501`
- **Status**: NEW. Validated CONFIRMED at HEAD `1e8a25ab`.

## Description

The `fragWorldPos` varying is absolute (`rel + origin` added at `triangle.vert:190`, **before** interpolation), so `dFdx`/`dFdy` consumers see ~0.0156 u quantization in far worldspaces (up to ~20% relative derivative noise close-up at |world| ≥ 131k). Passing the relative position as the varying and reconstructing the absolute in the fragment shader would move quantization after the derivative stage at zero extra varying cost.

Validation note: in exact arithmetic `dFdx/dFdy` cancel a uniform `+renderOrigin` offset — the residual effect here is purely the f32 quantization of the large interpolated values, which is why this is LOW and needs visual confirmation. The audit recommends a RenderDoc capture of flat-shaded close-up content in a |coord| > 131k cell to confirm visibility **before** acting.

## Suggested Fix

Only if confirmed visible: switch the varying to render-origin-relative and add `renderOrigin` in the fragment shader where the absolute position is required (RT ray origins).

## Completeness Checks
- [ ] **SIBLING**: All four derivative consumers updated together if the varying changes
- [ ] **TESTS**: Layout/lockstep pins re-verified if the varying interface changes (.spv recompiles across all users of the varying)

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
