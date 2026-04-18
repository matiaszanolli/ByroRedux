# REN-RT-H3: Window portal ray uses -V instead of -N — oblique window fragments render opaque

**Issue**: #421 — https://github.com/matiaszanolli/ByroRedux/issues/421
**Labels**: bug, renderer, high

---

## Finding

`crates/renderer/shaders/triangle.frag:456-466` fires the window portal ray along `-V` (camera view direction) instead of the surface outward normal `-N`:

```glsl
vec3 throughDir = -V;  // continue along the camera's line of sight
rayQueryInitializeEXT(rq, topLevelAS,
    gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT,
    0xFF, fragWorldPos - N * 0.15, 0.05, throughDir, 2000.0);
```

Since `V = normalize(cameraPos - fragWorldPos)`, `-V` points from camera TOWARD the fragment. For an off-axis fragment on a window pane, the through-ray continues into the interior sidewall/ceiling, not through the window's outward normal direction.

## Impact

Windows seen at oblique angles:
- Through-ray hits interior geometry (sidewall, ceiling, frame)
- Portal "escape" test fails
- Falls to the else branch → renders as opaque alpha-blended surface with no sky transmission

Only windows the camera looks at perpendicularly light up correctly. Visible regression in Bethesda interiors (inns, huts) where windows are commonly viewed at oblique angles during traversal.

The comment at line 456 acknowledges "continue along the camera's line of sight" as the intent — but portal semantics require the ray to fire along the surface OUTWARD normal, not along view.

## Fix

Two acceptable approaches:

**(a) Fire along `-N`**:
```glsl
vec3 throughDir = -N;  // outward through the glass pane
```

**(b) Snell-refracted V** (more physically accurate for glass IOR):
```glsl
vec3 throughDir = refract(-V, N, 1.0 / 1.5);  // air → glass IOR ≈ 1.5
```

**(c) Defensive fallback** (orthogonal): gate with `if (dot(-V, N) > 0.1)` so oblique incidence falls back to the alpha-blend path gracefully without firing a ray.

Option (a) is the cleanest minimal fix; option (b) is the long-term correct answer for refractive glass.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: None — window portal logic is specific to this site. Verify the 2000-unit tMax is still appropriate for exterior escape once the direction changes.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Visual test in an Oblivion inn or Skyrim shack — oblique-angle windows should transmit sky correctly. Currently render as opaque alpha-blended panes.

Also re-check the negative origin bias concern in `triangle.frag:462-464` (L3 in the audit — `fragWorldPos - N * 0.15` can start the ray inside a mullion frame BLAS).

## Source

Audit: `docs/audits/AUDIT_RENDERER_2026-04-18.md`, Dim 9 H3.
