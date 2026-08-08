# TD1-007: cell_loader/spawn.rs crossed 2000 LOC — spawn_mesh_instance and spawn_placed_instances are genuine production bloat

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2410
**Finding ID**: TD1-007 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 1 — File / Function / Module Complexity
**Location**: `byroredux/src/cell_loader/spawn.rs:1205-1750` (`spawn_mesh_instance`), `:385-624` (`spawn_placed_instances`)
**Status**: NEW

## Description
`byroredux/src/cell_loader/spawn.rs` newly crossed 2000 LOC (2018 total). `spawn_mesh_instance` (546 LOC) and `spawn_placed_instances` (240 LOC) are genuine production bloat, not test code. Neither appears in the clippy cognitive-complexity report — the bulk is straight-line per-attribute vertex assembly (positions/normals/UVs/tangents/skin-weights with fallback defaults), not deep branching.

## Related
Same subsystem, same week's commits as `cell_loader/references/mod.rs` (TD1-006).

## Suggested Fix
Extract `build_vertex_buffer(mesh: &ImportedMesh) -> Vec<Vertex>` covering the per-attribute fallback logic. Check against the loose-NIF loader's equivalent for Dimension-2 duplication before splitting.

## Completeness Checks
- [ ] **SIBLING**: Compare the extracted `build_vertex_buffer` against the loose-NIF loader's own vertex-assembly path for duplication
- [ ] **TESTS**: A regression test pins vertex-attribute output before/after extraction
