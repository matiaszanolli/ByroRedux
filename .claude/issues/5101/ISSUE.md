# #5101: FO4-D1-01: merge_precombine_materials (the opaque-architecture blend restore) has no test on either route; the new drain-route guard passes mat_provider = None, so the merge never runs in it

**Labels**: bug, import-pipeline, low, legacy-compat, game:fo4, test-gap

**Source report**: `docs/audits/AUDIT_FO4_2026-09-29.md`
**Severity**: LOW (test-gap)
**Dimension**: M49 precombines

## Location
- `byroredux/src/cell_loader/precombined.rs` (`merge_precombine_materials`, and its main-thread job call site).
- `byroredux/src/cell_loader/partial.rs` (`finish_partial_import` stream-drain call site, behind `if let Some(provider) = mat_provider`).
- `byroredux/src/cell_loader/finish_partial_tests.rs` (`finish_partial_import_builds_precombine_geometry_entry`).

## Description
e593770f0 factored the #1619-follow-up blend restore into `merge_precombine_materials`: keep the BGSM's two_sided / decal / alpha_test flags but restore the NIF-side `has_alpha` / `src_blend_mode` / `dst_blend_mode`, because FO4 authors the "Standard" blend identically on lab glass and opaque Institute metal. The same commit calls the helper from the new streaming-drain branch of `finish_partial_import`.

`grep merge_precombine_materials` finds only the function and its two callers — no test. The new drain-route guard calls `finish_partial_import(&mut world, None, key, partial, …)`, so the provider branch that applies the merge never executes in it.

## Evidence
- `finish_partial_tests.rs`: `finish_partial_import(&mut world, None, key, partial, &|_| false);` (every call in the file passes `None`).
- `partial.rs`: the merge sits behind `if let Some(provider) = mat_provider`.

Validated at HEAD 9fcfdc3fc: `rg merge_precombine_materials byroredux/src` → definition + 2 call sites only; all `finish_partial_import` test calls pass `None` for the provider.

## Impact
Nothing is wrong today. But if either route drifted back to a bare `merge_external_material` loop (the efd3c41b regression shape), every test would still pass, and precombined Institute / lab walls would render as `MATERIAL_KIND_GLASS` (see-through, mirror-hazy) on that route only. Streamed exteriors and synchronous interiors now take different routes, so a one-sided drift shows only as a visual difference between load paths.

## Related
- #1619, efd3c41b (the original regression); `/audit-fo4` SKILL Dim 1 "Unguarded" note.

## Suggested Fix
Add a unit test for the helper with an in-memory `MaterialProvider` holding a "Standard"-blend BGSM (function 1, src 6, dst 7, two_sided, alpha_test) and a mesh whose NIF-side `has_alpha = false`; assert the blend triple is restored and the BGSM's two_sided/alpha_test survive. Add a provider-backed variant of the drain-route test.

## Completeness Checks
- [ ] **SIBLING**: Both routes (main-thread `PrecombinedSpawnJob` and stream drain) covered
- [ ] **CANONICAL-BOUNDARY**: The restore stays at the import/merge boundary, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix

