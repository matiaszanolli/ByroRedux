# REN-D2-2026-08-07-02: Glass refraction passthru loop drops rayTMin to 0.0 after the first iteration

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2462
**Finding ID**: REN-D2-2026-08-07-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 2 — Ray Queries / RT Shading
**Location**: `crates/renderer/shaders/triangle.frag` — IOR refraction block, `REFRACT_PASSTHRU_BUDGET` loop (`rayTMin` initialisation and its reassignment in the passthru-continue arm)
**Status**: NEW

## Description
The loop initialises `float rayTMin = 0.05;` — matching the convention `raytrace.glsl` documents at length ("Same 0.05 tMin convention every other ray-query site in this shader uses"). But the passthru-continue arm silently resets it:
```glsl
rayOrigin = exitPoint + refractDir * 0.05;
rayTMin = 0.0;                       // <- no comment, no rationale
accumulatedDist += hDist;
continue;
```
From iteration 2 onward the query runs with `tMin = 0.0` and only a 0.05 origin nudge along the *newly refracted* direction. When the refracted direction is near-tangent to the interface just crossed (grazing exit, common near total internal reflection), the 0.05 nudge projects to well under 0.05 of perpendicular clearance, and a `tMin` of 0.0 makes the just-crossed triangle a committable candidate at `t ≈ 0`.

## Evidence
`raytrace.glsl`'s `traceReflection` documents the failure mode this convention exists to prevent — pre-#1017 it used tMin 0.01 against a 0.05 bias, "which let perturbed-normal flips at grazing angles fire the ray back through the surface and self-hit, producing black speckle on metals." The refraction loop is cited in that same comment as one of the sites honouring 0.05, but it only honours it on the first of up to three iterations. The loop's own downstream guards (`terminusOnSelf`, `terminusOnGlass`, `terminusOnFallback`) catch a self-*terminus*, but a mid-loop self-hit is consumed as a passthru and burns budget instead.

## Impact
Wasted passthru budget and, at grazing exits, a refraction ray that re-enters the surface it just left — the loop then terminates one interface early and the fragment falls to the ambient escape path. Symptom class: intermittent flat/ambient patches on curved glass at grazing angles. Bounded (max 3 iterations, always converges). Confined to `glassIORAllowed` fragments. **Needs RenderDoc verification** to confirm the self-hit actually commits on real content.

## Related
#1017 (tMin normalisation on `traceReflection`), #789 (the passthru loop's origin), #820 (Frisvad basis at the same site — verified intact).

## Suggested Fix
Keep `rayTMin = 0.05` for every iteration, or — if the 0.0 is deliberate to avoid skipping genuinely thin stacked panes — document that rationale inline so the next audit does not re-flag it.

## Completeness Checks
- [ ] **TESTS**: Needs RenderDoc verification of a grazing-exit refraction case; not unit-testable
- [ ] **SIBLING**: Confirm `traceReflection`/`traceShadowTransmittance` still honour their own tMin convention consistently across iterations
