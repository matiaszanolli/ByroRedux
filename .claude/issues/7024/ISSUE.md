# REN-D6-03: MultiLayerParallax `inner_layer_thickness` is clamped to 0..1 as a weight, but the authored field is a distance (5..500)

Labels: low, renderer, shaders, nifal, game:skyrim, documentation
URL: https://github.com/matiaszanolli/ByroRedux/issues/7024

**Severity**: LOW
**Source**: `docs/audits/AUDIT_RENDERER_2026-11-16.md` (renderer audit, delta since 2026-11-09)
**Location**: the MultiLayerParallax block in `crates/renderer/shaders/triangle.frag` (`innerWeight = clamp(multiLayerInnerThickness,0,1) * innerSample.a`, `refractUV`); `crates/renderer/shaders/include/ray_hit.glsl` (`innerWeight`); pack site in `byroredux/src/render/static_meshes.rs`; `MLP_REFRACTION_UV_PER_UNIT`
**Status**: NEW (validated against HEAD; no matching open issue)

## Description
`nif.xml` defines the field as "how far from the surface the inner layer appears to be", range 5..500; all 662 vanilla kind-11 meshes author 5..500 (median 22.9), so the clamp is always exactly 1.0 and `innerWeight` equals the inner layer's own alpha; 22.9 and 500 shade identically. Only `refraction_scale` varies the primary path (`0.0659 x viewTS` for ice). nifly and OpenMW parse but never consume the field. `ray_hit.glsl` applies the inner layer without the refraction UV offset the primary path uses (<= ~0.066 x viewTS at ice values), undocumented.

**Impact**: interpretive-transfer gap; no shipped reference pins the vanilla transfer function, so this is a documentation task, not a behaviour change.

## Suggested Fix
Do not invent a mapping. Record in the `shader_constants.glsl` comment and `docs/engine/nifal.md` that the lane saturates for vanilla and that the secondary (`ray_hit.glsl`) path skips the offset. If a transfer function is ever measured, apply it at the `translate_material` boundary as a resolved weight.

**Closed, only touching: #5961 / #619 / #343 / #430.**
## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
