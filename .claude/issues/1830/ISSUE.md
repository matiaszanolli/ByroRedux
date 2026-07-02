# SF2-03: BSGeometryMesh tri_size/num_verts hints parsed then never validated against resolved geometry

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1830
**Labels**: bug, nif-parser, low, legacy-compat

**Severity**: LOW
**Dimension**: BSGeometry mesh (Starfield audit Dim 2)
**Location**: `crates/nif/src/blocks/bs_geometry.rs:188-198` (fields declared); consumer `crates/nif/src/import/mesh/bs_geometry.rs`

## Description

Each `BSGeometryMesh` slot carries `tri_size` (triangle-index byte-size hint) and `num_verts` (vertex-count hint), "always present regardless of internal/external". The importer never cross-checks these against the actually-parsed `mesh_data.vertices.len()` / `triangles.len()`. A slot's hint disagreeing with its resolved `.mesh` body is a strong signal of a wrong-file resolve (hash collision, stale archive) or a truncated companion — currently undiagnosable.

## Evidence

Fields declared `crates/nif/src/blocks/bs_geometry.rs:188-192` (`tri_size` at 190, `num_verts` at 192), read into the struct at parse time (`crates/nif/src/blocks/bs_geometry.rs:219-220`), never read again outside `Debug`. No comparison site anywhere in `crates/nif/src/import/mesh`.

## Impact

Defense-in-depth gap only; no incorrect render on its own.

## Suggested Fix

After Stage B parse succeeds, `log::debug!` (or `debug_assert`) when `data.vertices.len() != num_verts as usize` or the `tri_size`-derived triangle count disagrees, to surface bad resolves during bring-up.

## Completeness Checks
- [ ] **SIBLING**: Same hint-validation gap checked on the Stage A (internal) parse path
- [ ] **TESTS**: A regression test pins this specific fix (mismatched hint vs. resolved body triggers the debug signal)

