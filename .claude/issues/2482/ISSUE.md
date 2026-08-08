# REN-D2-2026-08-07-03: Refraction passthru loop does not decrement tMax, so effective reach is 3x the documented 2000

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2482
**Finding ID**: REN-D2-2026-08-07-03 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 2 — Ray Queries
**Location**: `crates/renderer/shaders/triangle.frag` — IOR refraction block, the `rayQueryInitializeEXT(refrRQ, ...)` call inside the `REFRACT_PASSTHRU_BUDGET` loop
**Status**: NEW

## Description
Every iteration of the passthru loop re-issues the query with a hard-coded `2000.0` tMax while advancing `rayOrigin` past each skipped interface. Unlike the sibling loops — `traceReflection` (`remaining -= advance`), `traceShadowTransmittance` (`opaqueRemaining -= advance` and `remaining -= advance`), `traceWaterRay` (`remaining = maxDist - travelled`) — the refraction loop never decrements its reach.

## Evidence
`accumulatedDist` is tracked for the distance-attenuation term (`refrColor *= 1.0 / (1.0 + accumulatedDist * 0.002)`) but is never fed back into the query's tMax. With `REFRACT_PASSTHRU_BUDGET = 2` the ray can travel up to ~6000 world units across three segments while the code and comments describe a 2000-unit reach.

## Impact
Cosmetic/consistency, plus a mild cost overrun on stacked-glass views: a refraction ray can resolve a terminus three times farther away than the intended budget, then be heavily attenuated anyway by the distance term. No correctness break, no unbounded walk (iteration count is fixed at 3).

## Related
Sibling reach bookkeeping in `raytrace.glsl::traceReflection` and `shadow_transport.glsl::traceShadowTransmittance`.

## Suggested Fix
Track `refrRemaining = 2000.0` alongside `accumulatedDist` and subtract each `hDist + 0.05` per passthru, matching the three sibling traversal loops — or amend the comment to state the 3-segment reach is intended.

## Completeness Checks
- [ ] **TESTS**: N/A shader-side (no live device in `cargo test`); document the fix rationale inline
