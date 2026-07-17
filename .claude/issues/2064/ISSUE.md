# TD2-105: ImportedMesh to Vertex + local-AABB conversion copy-pasted between object_lod.rs and placement_lod.rs

**GitHub Issue**: #2064
**Labels**: low,renderer,tech-debt,bug

**Severity**: LOW
**Dimension**: 2 (Logic Duplication)
**Location**: `byroredux/src/cell_loader/object_lod.rs:264-279,306-315` vs. `byroredux/src/cell_loader/placement_lod.rs:444-459,483-491`

## Description
The format-specific streaming logic (`.bto` vs `.lod`) is deliberately separate — that part is fine. This specific mesh-to-vertex/AABB conversion has nothing to do with the format difference and was evidently copied.

## Evidence
Confirmed live: both files contain a near-identical `ImportedMesh` → `Vec<Vertex>` mapping closure (same color/normal/uv/tangent fallback defaults) immediately followed by a near-identical local-AABB min/max/center/radius accumulation loop, at the claimed line ranges.

## Suggested Fix
Extract `imported_mesh_to_vertices()` and `local_aabb_center_radius()` into a shared LOD-support module.

**Effort**: small

## Completeness Checks
- [ ] **TESTS**: A regression test pins that the extracted helpers produce byte-identical vertex/AABB output for both callers
