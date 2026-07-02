# D2-NEW-04: Static-mesh hot loop pays a redundant GlobalTransform re-probe and a late IsFxMesh gate

**Issue**: #1805
**Labels**: low,renderer,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (D2-NEW-04)

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D2-NEW-04)

## Location
`byroredux/src/render/static_meshes.rs:147,175,280`

## Description
Two skip-ordering inefficiencies in the per-entity draw enumeration: the #1377 hoist probes `tq.get(entity).is_none()` (`:147`) then re-fetches the same component at `:175`, two storage lookups per drawn entity where one binding would do; and the `IsFxMesh` skip fires only after ~12 optional-component gets and the frustum-sphere test, all discarded for FX entities.

## Evidence
`static_meshes.rs:147` `if tq.get(entity).is_none()`; `:175` `if let Some(transform) = tq.get(entity)` — same component, second lookup.

## Impact
One extra storage get per rendered entity per frame (tens of µs at FO4 MedTek scale) plus ~12 wasted gets per FX entity. Micro-scale but this is the single hottest CPU loop in `build_render_data`.

## Related
#1377, #1136.

## Suggested Fix
Replace the two-step probe with a single `let Some(transform) = tq.get(entity) else { continue; };`, and hoist the `fx_q` gate immediately after the visibility skip.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other hot-path loops / other dirty gates)
- [ ] **TESTS**: A regression test pins this specific fix

