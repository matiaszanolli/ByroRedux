# #4797: PERF-D1-2026-09-23b-03: `WaterSurfaceMesh::surface_y_at` scans every surface triangle linearly, per frame for the camera and player and per dynamic body per physics step

**Severity**: LOW
**Labels**: low, performance, water, physics, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D1-2026-09-23b-03)

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**:
  - `crates/core/src/ecs/components/water.rs:599-661`;
  - callers `byroredux/src/systems/water.rs:198-205`, `systems/character.rs:1062-1071`, `crates/physics/src/water.rs:880-885`;
  - producer `byroredux/src/material_translate.rs:437-475`.
- **Status**: NEW (`17c01a4e5`)
- **Description**: The whole placed mesh's world-space triangles are attached, and each query runs a barycentric test over all of them. There is no XZ grid or BVH, and the AABB reject passes everything over the footprint.
- **Impact**: (camera + player + bodies in footprint) × triangles, per frame or per step. Stream-mesh triangle counts are uncensused, so there is no estimate.
- **Suggested Fix**: Build a coarse XZ grid (cell → triangle indices) at component construction.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
