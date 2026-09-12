# FO4-2026-09-11-D4-01: select_finest_lod picks one of three disjoint precombine LOD bands, dropping 12.45% of baked triangles

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4234

**Severity**: HIGH
**Dimension**: 4 — NIF BSVER 130 + Half-Float + FO4 Collision
**Location**: `byroredux/src/cell_loader/precombined.rs:704-777` (`select_finest_lod` + call site); premise mirrored in `crates/nif/src/import/precombine.rs:110-114`
**Status**: NEW (#3641 introduced `select_finest_lod` and is the code being questioned, not a prior report of this)

**Description**: The FO4 precombined-geometry path treats a shared-geometry object's three LOD triangle bands as alternative triangulations of the same surface and renders only the largest by triangle count (`select_finest_lod`). That premise is wrong: the three bands are **disjoint sub-meshes** whose union is the full model, matching NifSkope's own renderer for the identical in-NIF construct (`BSMeshLODTriShape`, additive `switch` fall-through in `tools/nifskope/src/gl/bsshape.cpp:426-450`).

**Evidence**: Measured against retail FO4 data. (1) `lod0+lod1+lod2 == num_triangles` for 12,096/12,096 `BSMeshLODTriShape` blocks (100%) — the bands partition the index buffer. (2) Vertex-set intersection across bands is exactly zero for 5,292/5,292 multi-band shapes — alternative triangulations would share vertices; these do not. (3) Across `Fallout4 - MeshesExtra.ba2`'s 28,190 shared-geometry objects, 6,668 (23.7%) have ≥2 populated bands, and picking the max band discards 1,468,471 of 11,793,978 triangles (12.45%), with individual objects losing up to 58% of their triangles. Confirmed in current code: `select_finest_lod` (`precombined.rs:718-726`) picks a single band by max count, and its own comment plus `crates/nif/src/import/precombine.rs:110-114`'s comment both assert the "alternative triangulations / z-fighting" premise.

**Impact**: Baked FO4 architecture renders with visible geometry missing. This is the default render path for FO4 exteriors and most interiors, so it affects essentially every FO4 cell with precombines, silently (no log, no parse-rate signal, no test). The z-fighting the removed code's comment cites as justification cannot occur, since the bands share no vertices.

**Related**: #3641 (introduced `select_finest_lod`); the in-NIF sibling finding for `bs_lod_cutoffs` (write-only field, filed separately).

**Suggested Fix**: Render the union of all three bands as max-detail (`tri_start = 0`, `tri_count = lod_counts.iter().sum()`), deleting `select_finest_lod` and the "alternative triangulations" comment. Distance LOD, if wanted, must *drop* tail bands (Level1 = LOD0+LOD1, Level0 = LOD0), never pick one. Add a regression test asserting a 3-band fixture yields `sum(lod_counts)` triangles.

## Completeness Checks
- [ ] **SIBLING**: Check `crates/nif/src/import/precombine.rs:110-114`'s matching comment/logic for the same fix
- [ ] **TESTS**: A regression test pins a 3-band fixture to `sum(lod_counts)` triangles, not `max(lod_counts)`
