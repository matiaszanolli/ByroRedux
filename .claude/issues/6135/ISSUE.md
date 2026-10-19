# PHYS-SD-04: Interior water volume depth flips from 200 BU to 4096 BU when the cell's REFR spread exceeds about 1984 BU

Labels: low,physics,water,game:oblivion,game:fo3,game:fnv,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6135

**Severity**: LOW · **Dimension**: Water / Buoyancy (Dim 5 contract with `cell_loader/water.rs`) · **Games**: Oblivion, FO3, FNV (interiors with a real `XCLW`; Skyrim/FO4 author an inert 0.0 that is filtered)

## Location
`byroredux/src/cell_loader/water.rs` (`spawn_water_plane` `volume_depth`; `interior_water_placement`, `MAX_HALF_EXTENT = 2048`), `byroredux/src/cell_loader/load.rs` (interior caller)

## Trigger Conditions
An interior whose placed references span more than about 1984 BU in X or Z.

## Description
`spawn_water_plane` decides interior versus exterior from `half_extent > 1024.0`, and says so in a comment ("captures the spawn caller contract without an explicit flag"). `interior_water_placement` can return up to `MAX_HALF_EXTENT = 2048` for exactly the large cells (`half_extent = span/2 + 32`, clamped), so those interiors take the exterior branch: `EXTERIOR_CELL_UNITS` (4096) of volume below the surface, where small pools get `DEFAULT_INTERIOR_VOLUME_DEPTH` (200). The constant's own comment names the 4096 default as the cause of an earlier "spuriously submerged" regression.

## Evidence
`let volume_depth = if half_extent > 1024.0 { EXTERIOR_CELL_UNITS } else { DEFAULT_INTERIOR_VOLUME_DEPTH };` (verified at publish time). No test pins the depth selection; the only interior-placement tests check the extent and the empty fallback.

## Impact
The same pool gets a 20x different swim/buoyancy column depth depending on how far apart unrelated REFRs sit. Which side is closer to the original engine was not established; the inconsistency itself is the defect. Not measured against a real cell census.

## Related
#3548 (the interior `XCLW` gate), `docs/engine/watal.md` (interior volume).

## Suggested Fix
Pass an explicit `interior: bool` (or the depth) from the two callers (`load.rs`, `exterior.rs`) instead of inferring it from `half_extent`.

Source: docs/audits/AUDIT_PHYSICS_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
