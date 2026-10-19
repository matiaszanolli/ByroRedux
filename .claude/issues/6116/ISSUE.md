# NIF-SD-03: FO4 precombine cache entries retain one deep copy of the decoded geometry per instance

Labels: low,nif-parser,nif,memory,game:fo4,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6116

**Severity**: LOW · **Dimension**: Geometry Handoff / Allocation Hygiene · **Games**: Fallout 4 (bsver 130+), shared-variant `_oc.nif`

## Location
`byroredux/src/cell_loader/precombined.rs` (`build_precombine_meshes`: `decoded.clone().into_imported_mesh(..)` per instance; `geometry_only_cached`); `byroredux/src/cell_loader/partial.rs`

## Description
`build_precombine_meshes` decodes each shared object once and clones the whole decoded mesh per `BSPackedGeomDataCombined` instance. #3510's `geometry_dedup` makes the upload and BLAS paths use one representative, but every clone's `positions/normals/uvs/colors/tangents/indices` vectors stay alive in `CachedNifImport.meshes` for the entry's registry lifetime.

## Evidence
Sampling 1,505 `_oc.nif` evenly from `Fallout4 - MeshesExtra.ba2` (14,744 objects, 25,993 instances): CPU geometry with per-instance copies is 1.50x the distinct geometry. Per entry: mean 0.46 MiB, median 0.14 MiB, p90 0.96 MiB, max 46.5 MiB (estimated at 64 B/vertex + 12 B/triangle from the `ImportedMesh` field widths, not dhat-measured). Filling the 2,048-entry cache holds roughly 0.3 GiB of duplicated vertex data beyond 0.6 GiB of distinct data.

## Impact
Host memory only; the entry count cap (not bytes) bounds it, and a single worst-case entry is 46 MiB.

## Related
#3510 (the GPU half was fixed), NIF-SD-01 (the same entries dominate the cache on FO4).

## Suggested Fix
Share the decoded vertex arrays between aliases (an `Arc` inside `ImportedMesh`, or drop the alias vertex vectors once `geometry_dedup` is built) so the entry holds one copy plus N transforms.

Source: docs/audits/AUDIT_NIF_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
