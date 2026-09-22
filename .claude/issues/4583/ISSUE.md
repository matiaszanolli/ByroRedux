# REN-D2-2026-09-21-02: the FACING_RATIO debug view paints every two-sided back face "inverted-normal" red

**Labels**: low, renderer, shaders, bug

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: LOW (a debug view that gives false positives) · **Dimension**: Debug/Telemetry
**Location**: `crates/renderer/shaders/triangle.frag`: the `geometricNormal` orientation (~:232-239), the `!gl_FrontFacing` flip that applies only to `N` (~:541-543), and the `viewFacingRatio` branch (~:1845-1860)
**Status**: NEW (introduced by `f97775ca8`)
**Verified against**: HEAD `f97775ca8`

## Description

The FACING_RATIO view colours solid red wherever `dot(geometricNormal, toCamera) < 0`, as the "inverted-normal class". But `geometricNormal` is oriented into the hemisphere of `fragNormalEffective`, the interpolated vertex normal, and that normal is not flipped for back faces. Only the shading normal `N` gets the `if (!gl_FrontFacing) N = -N;` flip. So on a two-sided draw (foliage, banners, cloth; the cull-off pipeline), every back-facing fragment has a geometric normal pointing away from the camera and reads red.

These pixels are not leak candidates. Shadow rays go through `offsetRayOriginForDirection`, which orients the offset normal to the ray direction (`dot(n, direction) >= 0 ? n : -n`), so two-sided back faces offset correctly.

## Evidence

- `triangle.frag`: `if (dot(geometricNormal, fragNormalEffective) < 0.0) geometricNormal = -geometricNormal;`, with no `gl_FrontFacing` term.
- `if (!gl_FrontFacing) { N = -N; }` flips only `N`.
- The view: `float facing = dot(geometricNormal, facingViewDir); … facing < 0.0 ? vec3(1.0, 0.04, 0.04) : …`.
- `include/ray_origin.glsl` `offsetRayOriginForDirection` is the direction-aware offset.

## Impact

The view was added for the single-sided-wall light-leak hunt. It floods every two-sided asset with false "inverted normal" red, which hides the real single-sided inversions it was built to find. Debug surface only.

## Related

- REN-D12-2026-09-21-01 (#4577): this view renders full-frame magenta until `composite.frag.spv` is rebuilt, so this defect only shows after that fix.
- REN-D2-2026-09-21-01 (#4582): the sibling RESTIR_LIGHT view from the same commit.

## Suggested Fix

In the view only, flip the tested normal on back faces (`gl_FrontFacing ? geometricNormal : -geometricNormal`), or give two-sided back faces a distinct neutral hue. Red then means a single-sided winding or normal inversion. Leave the production `geometricNormal` as is, because the ray-origin code depends on it.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D2-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: other `geometricNormal`-based debug views checked for the same two-sided back-face mislabel
- [ ] **SIBLING**: `triangle.frag.spv` recompiled (plain `-V`, keeps OpName for the reflection test)
