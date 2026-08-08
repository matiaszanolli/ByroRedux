# REN-D16-2026-08-07-02: Per-froxel shadow-ray budget is up to 10 rays, not the documented single ray

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2509
**Finding ID**: REN-D16-2026-08-07-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 16 — Volumetrics
**Location**: `crates/renderer/shaders/volumetrics_inject.comp:main`
**Status**: NEW (documentation/cost drift, not a correctness bug)

## Description
The design contract (and the file's own header comment, "shadow visibility is the standard 'trace toward light, miss = lit'") describes one `TerminateOnFirstHit` ray per froxel. Current code casts: 1 opaque sun ray, +1 glass-mask sun ray for interiors, and then up to `MAX_FROXEL_LIGHTS = 4` local lights × up to 2 rays each (opaque mask, then glass mask) = up to **10 ray-query traversals per froxel**. At the default grid (160×90×64 = 921,600 froxels) that is a worst case near 9.2M ray queries per frame from the injection pass alone.

## Evidence
`volumetrics_inject.comp:503-519` (sun opaque + interior glass), `:582-601` (`needsVisibility` opaque `traceShadowBinary` then `shadowPolicyUsesGlass` glass `traceShadowBinary`), `:552` `const uint MAX_FROXEL_LIGHTS = 4u`.

## Impact
No visual defect; a GPU-cost cliff in dense-light interiors that the checklist/design docs do not budget for. Also means any future "cost of volumetrics" estimate derived from the docs is off by ~10×.

## Related
M-LIGHT v2 shadow-policy work; #2205 (spot-cone guard in the same loop).

## Suggested Fix
Update the `volumetrics_inject.comp` header comment and the `VOLUMETRIC_OUTPUT_CONSUMED` doc block in `volumetrics.rs` to state the real per-froxel ray budget, and consider gating the second (glass) ray behind a cheap "did the opaque-architecture mask miss AND is a glass-capable light" precheck so the common case stays at 1 ray.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change) unless the optional gating optimization is also implemented, in which case a perf regression test pins the cheap-precheck behavior
