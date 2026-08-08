# SF2D2-D2-03: BSGeometryMeshData lods/meshlets/cull_data and slot LOD index decoded and dropped by importer

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2631
**Finding ID**: SF2D2-D2-03

**Severity**: LOW
**Dimension**: 2 (BSGeometry Mesh Extraction)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:140-144,325`, `crates/nif/src/blocks/bs_geometry.rs:107-112`
**Status**: NEW

## Description
Three signals are parsed and discarded: (1) `mesh_data.lods` — full reduced
triangle lists per LOD level (importer reads only LOD 0); (2)
`meshlets`/`cull_data` — cluster-culling primitives; (3) the slot loop index
itself is lost at parse time (`BSGeometry::parse`'s `for _ in 0..4` loop
discards its own counter), so `meshes[0]` is the first *present* slot, not
necessarily LOD 0 — combined with the sentinel-slot skip, a future LOD
selector has no way to know which level it actually loaded.

## Evidence
`BSGeometry::parse`'s slot loop (`crates/nif/src/blocks/bs_geometry.rs:107-112`)
discards its own loop counter; `mesh_data.lods`/`meshlets`/`cull_data` are
parsed but never read by the importer (`bs_geometry.rs:140-144,325`).

## Impact
No LOD switching possible for Starfield content today (missing-feature,
nothing renders wrong) — but item (3) is cheap now, expensive to retrofit
later.

## Suggested Fix
Store the loop index as `BSGeometryMesh.lod_slot: u32` at parse time and
carry it into `ImportedMesh` alongside `bs_lod_cutoffs`. `lods`/`meshlets`
consumption itself is fine as EXAL follow-up work.

## Completeness Checks
- [ ] **TESTS**: A multi-slot fixture asserts `lod_slot` matches the actual authored slot index, not the post-skip array position
