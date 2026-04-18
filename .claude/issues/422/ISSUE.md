# REN-COMP-H2: SVGF bilinear history age uses max() instead of weighted average — ghosting at disocclusion

**Issue**: #422 — https://github.com/matiaszanolli/ByroRedux/issues/422
**Labels**: bug, renderer, high

---

## Finding

`crates/renderer/shaders/svgf_temporal.comp:109` aggregates history age across the 4 bilinear reprojection taps using `max`:

```glsl
histAge = max(histAge, sMom.z);
```

The SVGF reference (Schied 2017 — "Spatiotemporal Variance-Guided Filtering", §4.2) uses the **weighted average** of history length with the same bilinear weights as color. Many reference implementations use `min` to be conservative.

## Impact — ghosting at disocclusion boundaries

Using `max` overcommits to old history. At disocclusion boundaries:
- Some neighbours were recently disoccluded (age = 1)
- One stale neighbour survived (age = 255)

That lingering neighbour drives `alphaC = max(0.2, 1/(255+1)) = 0.2` (the slow blend) instead of the fast recovery blend (`alphaC ≈ 1/2 = 0.5` from the newly-disoccluded taps). Freshly-disoccluded pixels inherit history they shouldn't have.

Symptom: ghosting on fine-scale moving occluders — pillar edges, character silhouettes, chair/table edges as the camera orbits.

## Fix

Replace the `max` with weighted average, matching how `histInd` and `histMom` are aggregated in the same loop:

```glsl
// Replace:
histAge = max(histAge, sMom.z);
// With:
histAge += w * sMom.z;
// ... then after the loop, normalize:
if (wTotal > 0.01) {
    histInd /= wTotal;
    histMom /= wTotal;
    histAge /= wTotal;   // <-- add this
    hasHistory = true;
}
```

Recompile SPIR-V.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check that moments (`histMom`) and indirect (`histInd`) stay on the weighted-average path (already correct per audit).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Before/after screenshot diff on a scene with a pillar silhouette moving across the camera frustum. Expect reduced trailing/ghosting at the pillar's leading edge.

## Source

Audit: `docs/audits/AUDIT_RENDERER_2026-04-18.md`, Dim 10 H2. References Schied et al. 2017.
