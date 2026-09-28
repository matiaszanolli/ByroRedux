# #4945: REN-D8-2026-09-27-01: Regression of #4858 — `traceArchitecturalWindowGlass`'s candidate loop can never execute. The ray is `gl_RayFlagsOpaqueEXT` over all-OPAQUE geometry, so architectural window glass never establishes a sun portal

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4945
- **Labels**: medium,renderer,shaders,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D8-2026-09-27-01**._

- **Severity**: MEDIUM. Visual; the default volumetric path in sealed interiors with glazed windows, in every game. It is partly masked at ray tier ≥ 1 by the rim fallback.
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp` `traceArchitecturalWindowGlass` (~l.292–317); only caller is `main`'s sealed-interior sun arm (`bool sawGlass = traceArchitecturalWindowGlass(`, ~l.2989). Pinned by `crates/renderer/src/vulkan/volumetrics.rs` `architectural_glass_portal_checks_every_candidate_layer`.
- **Status**: Regression of #4858 (closed by `ab255cfd2`). That fix made the function unconditionally false, which is worse than the first-hit false-negative it replaced.
- **Description**:
  - #4858 said the terminate-on-first-hit query could commit a clutter pane instead of a farther architectural one.
  - `ab255cfd2` dropped TerminateOnFirstHit and loops `while (rayQueryProceedEXT(rq)) { if (rayQueryGetIntersectionTypeEXT(rq, false) != gl_RayQueryCandidateIntersectionTriangleEXT) continue; … if (layer == RENDER_LAYER_ARCHITECTURE) return true; } return false;`.
  - But the query still passes `gl_RayFlagsOpaqueEXT`, and every BLAS geometry is `vk::GeometryFlagsKHR::OPAQUE` (`blas_static.rs`, `blas_skinned.rs`). No instance sets `FORCE_NO_OPAQUE`: the only instance flag is `TRIANGLE_FACING_CULL_DISABLE` in `tlas.rs`, and no shader uses `gl_RayFlagsNoOpaqueEXT`.
  - Opaque triangle hits are committed automatically during traversal and are never returned as candidates. `rayQueryProceedEXT` returns true only for non-opaque triangle or AABB candidates, so it returns false on the first call and the loop body never runs.
  - The function also never inspects the committed hit, so it always returns false.
  - This is the only candidate-type loop in the shader tree (grep `CandidateIntersection`).
- **Evidence**:
  ```glsl
  rayQueryInitializeEXT(rq, topLevelAS, gl_RayFlagsOpaqueEXT, VISIBILITY_LAYER_GLASS, origin, 0.05, direction, tMax);
  while (rayQueryProceedEXT(rq)) {            // never true: all geometry opaque
      if (rayQueryGetIntersectionTypeEXT(rq, false) != gl_RayQueryCandidateIntersectionTriangleEXT) continue;
      ...
  }
  return false;
  ```
  - The call site feeds `visibility = sawGlass || (fogRayQualityTier > 0u && … && hasArchitectureRimAroundSkyRay(...))`.
  - The primary sun ray uses `VISIBILITY_MASK_ALL_OPAQUE`, so a glass pane does not block it. The glass pass is exactly what should turn "clear ray through a pane" into "lit".
- **Impact**:
  - In sealed interiors (no Show Sky bit), a froxel whose sun ray leaves through a glazed window is lit only if an authored LightShaft/SkyAperture covers it, or if the 4-probe architecture rim fallback happens to find a coherent plane.
  - The rim fallback is shed at ray tier 0 (`fogRayQualityTier > 0u`, #4793). There, every glazed window without an authored aperture is dark.
  - Large glazed openings (a pane wider than the ~272 BU probe diagonal, e.g. cathedral windows or glass walls) are dark at any tier, because all four probes pass through the glass. They use `VISIBILITY_LAYER_ARCHITECTURE` and miss.
  - Where the rim fallback does rescue a window, it costs 4 closest-hit rays per froxel that the single glass query was meant to short-circuit (a #4793 overlap).
  - Nothing catches this: the guard test asserts the loop's text.
- **Related**: #4858 (the fix that introduced this), #4793 (rim-ray cost), `0572bfd5a` (interior godrays), `traceArchitectureDistance` (its sibling uses the same flag correctly, because it only drains and reads the *committed* hit).
- **Suggested Fix**: Pick one:
  - Initialize with `gl_RayFlagsNoOpaqueEXT` so every glass triangle surfaces as a candidate. Return true on the first architecture-layer candidate and never confirm the others.
  - Keep the opaque closest-hit and re-trace from `t + ε` while the committed layer is not architecture, with a small bound.

  Replace the source-shape pin with one that also asserts the non-opaque flag. Confirm by a `VOLUMETRIC_TERM` A/B on a glazed-window interior at tier 0 and tier 2.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
