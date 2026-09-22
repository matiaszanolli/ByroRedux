# REN-D8-2026-09-21-02: the #4545 water-caustic visibility ray traces mask `0xFF` with opaque traversal — effect cards and whole alpha-card foliage quads hide caustics the camera can see

**Labels**: low, renderer, shaders, water, bug

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: LOW (water caustics over-occluded) · **Dimension**: Water
**Location**: `crates/renderer/shaders/water.frag` (~:1385-1413): the `occlRq` camera-visibility ray, `rayQueryInitializeEXT(occlRq, topLevelAS, gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT, 0xFF, …)`
**Status**: NEW (introduced by `1375abf53`, the #4545 fix)
**Verified against**: HEAD `f97775ca8`

## Description

The #4545 fix gates each water-caustic deposit on a terminate-on-first-hit ray cast from the riverbed hit back toward the camera. The ray uses cull mask `0xFF` and `gl_RayFlagsOpaqueEXT`:
- The mask includes `VISIBILITY_LAYER_EFFECT` (32) and `VISIBILITY_LAYER_GLASS` (16).
- The opaque flag skips any alpha test.

As a result:
- **Effect cards occlude.** Effect-layer meshes (spray, mist, splash cards) between the bed and the camera count as occluders.
- **Foliage occludes as whole quads.** Alpha-tested foliage cards (reeds, lily pads, overhanging leaves) block with their full quad, not just their visible texels.
- **Glass occludes.** The compute twin `caustic_splat.comp` deliberately lets caustics show through glass, since its depth test reads opaque-only depth.

In each case the camera sees the bed through or around the occluder, but the caustic pattern vanishes.

**Publisher's validation note (fix scope).** The report proposes `VISIBILITY_MASK_SOLID`, but that fix removes only the EFFECT layer:
- SOLID is 31 = ARCHITECTURE | STATIC_PROP | DYNAMIC_ACTOR | FOLIAGE | GLASS, so glass still occludes.
- Foliage stays whole-quad under `gl_RayFlagsOpaqueEXT`.
- Since `f97775ca8`, FO3/FNV/Oblivion blended FX cards are no longer in the EFFECT layer at all (REN-D1-2026-09-21-01 (#4576)), so no mask choice excludes them.

## Evidence

- `water.frag`: `rayQueryInitializeEXT(occlRq, topLevelAS, gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT, 0xFF, floorWorld + toCamera * 1.0, 0.0, toCamera, max(cameraDist - 2.0, 0.0));`
- `shader_constants.glsl`: `VISIBILITY_LAYER_FOLIAGE 8u`, `VISIBILITY_LAYER_GLASS 16u`, `VISIBILITY_LAYER_EFFECT 32u`, `VISIBILITY_MASK_ALL_OPAQUE 15u`, `VISIBILITY_MASK_SOLID 31u`.

## Impact

Caustics disappear under spray and mist cards, around reeds and foliage cards, and behind glass, even where the bed itself is visible. Visual only, water caustics.

## Related

- #4545 (closed): the fix this refines.
- REN-D8-2026-09-21-01 (#4588): the depth gate on the glass-caustic twin.
- REN-D1-2026-09-21-01 (#4576): legacy blended FX cards now sit in opaque buckets, so this ray's mask cannot exclude them until that is fixed.

## Suggested Fix

Trace `VISIBILITY_MASK_ALL_OPAQUE` (which excludes glass and effect) and make foliage coverage-aware. There are two ways: drop `gl_RayFlagsOpaqueEXT` and run a candidate loop with `rayHitHasCoverage`, as `shadow_transport.glsl` does; or accept whole-quad foliage as a documented approximation. Pin the mask choice with a source-shape test.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D8-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: `caustic_splat.comp`'s glass gate and any other camera-visibility query use the same occluder policy
- [ ] **SIBLING**: `water.frag.spv` recompiled; `scripts/check-shader-artifacts.sh` green
- [ ] **TESTS**: a source-shape pin on the visibility ray's mask and flags
