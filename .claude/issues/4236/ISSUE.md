# FO4-2026-09-11-D4-02: ImportedMesh::bs_lod_cutoffs is write-only — three producers, zero consumers

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4236

**Severity**: LOW
**Dimension**: 4 — NIF BSVER 130 + Half-Float + FO4 Collision
**Location**: `crates/nif/src/import/mesh/bs_tri_shape.rs:273-276`, `crates/nif/src/import/walk/mod.rs:574,950`, `crates/nif/src/import/types.rs:941`
**Status**: NEW

**Description**: `bs_lod_cutoffs` is populated from FO4 `BSMeshLODTriShape` and Skyrim `NiLodTriShape` but has no reader anywhere outside its own tests. Not a correctness bug per the sibling `select_finest_lod` finding (drawing all bands is the correct max-detail render), but a missing distance-LOD optimization whose input data is parsed then discarded.

**Evidence**: `grep -rn bs_lod_cutoffs crates/nif/src` shows three producers (`bs_tri_shape.rs:273`, `walk/mod.rs:574`, `walk/mod.rs:950`) and the field definition (`types.rs:941`), but every other hit is inside `crates/nif/src/import/tests/` or `crates/nif/src/import/mesh/bs_tri_shape_kind_passthrough_tests.rs` — zero non-test consumers.

**Impact**: 12,096 FO4 shapes (plus Skyrim's population) pay full triangle cost at all distances with no way to shed detail.

**Related**: The `select_finest_lod` HIGH finding (same dimension); #2283 (fixed the cutoffs being discarded at parse time, one layer below this).

**Suggested Fix**: Either wire a distance test trimming the index range per the NifSkope rule, or add a one-line doc note that the field is parsed-but-unconsumed pending that pass.

## Completeness Checks
- [ ] **TESTS**: If wired, a regression test pins the distance-trim behavior
