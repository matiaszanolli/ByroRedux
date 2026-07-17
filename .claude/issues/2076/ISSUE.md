# TD8-102: spawn_water_plane's blas_specs output parameter is dead inside the function; interior call site fabricates a throwaway Vec

**GitHub Issue**: #2076
**Labels**: low,import-pipeline,tech-debt,bug

**Severity**: LOW
**Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
**Location**: `byroredux/src/cell_loader/water.rs:77-182` (`let _ = blas_specs;`); `byroredux/src/cell_loader/load.rs:416,439`

## Description
Water meshes are deliberately excluded from BLAS/TLAS, so `spawn_water_plane` discards the `blas_specs` parameter unconditionally. Reasonable for the exterior call site (which has a real shared accumulator), but the interior path allocates a `_blas_dummy` Vec purely to have something to pass, never read again.

## Evidence
Confirmed live: `byroredux/src/cell_loader/water.rs:77` — `pub(super) fn spawn_water_plane(..., blas_specs: &mut Vec<(u32, u32, u32)>, ...)` — with `let _ = blas_specs;` at line 182 inside the function body, confirming the parameter is dead within the function. `byroredux/src/cell_loader/load.rs` interior call site declares `let mut _blas_dummy: Vec<(u32, u32, u32)> = Vec::new();` solely to pass to `spawn_water_plane`.

## Impact
None functionally; misleads a future reader into thinking `_blas_dummy` matters.

## Suggested Fix
Drop the `blas_specs` parameter from `spawn_water_plane` entirely; update both call sites.

**Effort**: small

## Completeness Checks
- [ ] **SIBLING**: Confirm the exterior call site's `blas_specs` accumulator isn't secretly read elsewhere before removing the parameter workspace-wide
- [ ] **TESTS**: Existing interior/exterior water-plane spawn tests cover both call sites — purely mechanical parameter removal
