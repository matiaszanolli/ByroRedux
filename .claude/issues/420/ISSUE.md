# REN-RT-H1: Reflection + glass through-ray omit gl_RayFlagsTerminateOnFirstHitEXT — wasted ray cost

**Issue**: #420 — https://github.com/matiaszanolli/ByroRedux/issues/420
**Labels**: bug, renderer, high, performance

---

## Finding

Two `rayQueryEXT` call sites in `crates/renderer/shaders/triangle.frag` omit `gl_RayFlagsTerminateOnFirstHitEXT` even though both consume only first-hit data:

- `triangle.frag:213-218` — `traceReflection` helper
- `triangle.frag:531-534` — glass through-ray

Both paths issue `rayQueryInitializeEXT(rq, topLevelAS, gl_RayFlagsOpaqueEXT, 0xFF, origin, tMin, direction, maxDist)` and then:

- Single `rayQueryProceedEXT` call
- Read `rayQueryGetIntersectionT / InstanceCustomIndex / PrimitiveIndex / Barycentrics` with the `committed=true` selector
- Return

A single `rayQueryProceedEXT` is enough for opaque-only traversal, but without `TerminateOnFirstHitEXT` the driver may still pay the "find closest hit" cost on hardware that keeps searching for a CLOSER opaque intersection before committing.

## Impact

Measurable ray cost regression on:
- Long reflection rays (`maxDist = 5000` units from `triangle.frag:610`)
- Glass through-rays (`maxDist = 2000` units)

Both paths only need ANY hit (they read the committed instance then exit), so using terminate-on-first is an **unbiased** speedup.

For comparison, the shadow path at `triangle.frag:839` and window-portal path at `:460` correctly set the flag.

## Fix

Add `gl_RayFlagsTerminateOnFirstHitEXT |` to the `rayFlags` argument:

```glsl
// triangle.frag:215 (traceReflection)
rayQueryInitializeEXT(rq, topLevelAS,
    gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT,
    0xFF, origin, tMin, direction, maxDist);

// triangle.frag:533 (glass through-ray)  — same change
```

Recompile SPIR-V.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit every other `rayQueryInitializeEXT` call in the shader tree (GI hemisphere at line 887, shadow at 839, window portal at 460, reflection at 213, glass at 533). Confirm each either has the flag or has a documented reason to need closest-hit.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Before/after frame-time on an interior scene with reflective metal + glass. Expect measurable drop on the reflection-heavy frame.

## Source

Audit: `docs/audits/AUDIT_RENDERER_2026-04-18.md`, Dim 9 H1.
