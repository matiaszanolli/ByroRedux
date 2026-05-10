# #903 — REN-D11-NEW-01: NaN propagation through TAA history relies on undefined GLSL min/max semantics

**Severity**: LOW (fragile-but-works)
**Domain**: Vulkan renderer · TAA temporal blend · shader-only
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-05-08_DIM11.md` § Dimension 11
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/903
**Status**: NEW · CONFIRMED at HEAD `53f4f64`

## Locations

- `crates/renderer/shaders/taa.comp:147-154` — Catmull-Rom history sample + YCoCg variance clamp
- `crates/renderer/shaders/taa.comp:166` — final `imageStore` after `max(outRgb, vec3(0.0))`

## Summary

Temporal blend has no explicit `isnan(histRgb)` guard. NaN poisoning of `uPrevHistory` is dormant today (no live RT NaN source) but the YCoCg `clamp(histYc, yMin, yMax)` at line 153 is the only thing standing between NaN and self-perpetuating history poison — and `clamp` relies on undefined GLSL `min`/`max` semantics for NaN propagation. Most desktop drivers (NVIDIA / AMD / Intel) return the non-NaN argument so the clamp acts as an implicit NaN filter; a future driver emitting strict IEEE 754 propagation would break this.

## Fix path (shader-only, safe under speculative-vulkan-fix policy)

```glsl
if (any(isnan(histRgb)) || any(isinf(histRgb))) {
    histRgb = currRgb;
}
```

Insert at `taa.comp:151` between the Catmull-Rom sample and the YCoCg clamp. After GLSL change: `glslangValidator -V taa.comp -o taa.comp.spv` + commit the regenerated `.spv`.

**Sibling**: SVGF temporal blend has the same shape — apply same guard in same patch.

## Related

- #820 (closed) — Frisvad basis NaN at normal incidence on IOR refraction (different defect, same NaN-poisoning risk class)
- #801 (closed) — cell-streaming history reset (verified intact during this audit)
- #904 — sibling Dim 11 finding; both shader-only, can land together
