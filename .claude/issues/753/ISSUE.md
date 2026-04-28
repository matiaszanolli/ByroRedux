# #753: SF-D4-02: Starfield external `.mesh` companion-file format has no parser

URL: https://github.com/matiaszanolli/ByroRedux/issues/753
Labels: enhancement, nif-parser, import-pipeline, high, legacy-compat

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 4, SF-D4-02; Dim 6 SF-D6-02 same finding)
**Severity**: HIGH
**Status**: NEW

## Description

Vanilla Starfield separates vertex/index data into external `geometries/<sha1>/<sha1>.mesh` files inside the same BA2. `BSGeometry::BSGeometryMesh::External { mesh_name }` is a dead leaf in our codebase — there's no `crates/nif/src/mesh_file/`, no `MeshFile` parser, no decoder for the SHA1-named `.mesh` files.

```bash
$ grep -rn "BSGeometry\|bs_geometry" crates/nif/src/import/
(no matches)

$ grep -rn "\\.mesh\\|geometries/" crates/nif/src/ crates/bsa/src/
(only docstring + dispatch_tests literals; no parser code)
```

## Impact

Even after SF-D5-01 lands the `BSGeometry` importer with the internal-geom-data path, vanilla Starfield content (which exclusively uses external `.mesh` files — `FLAG_INTERNAL_GEOM_DATA = 0x200` is never set per `bs_geometry.rs:204-209`) imports zero geometry. The renderer needs **two parsers** to render any vanilla Starfield mesh: the NIF block (done) and the `.mesh` file (this issue).

## Format

Reference: nifly's `MeshFile.cpp` / `MeshFile.hpp` is the de facto authority. Same UDEC3 / i16 NORM / half-UV layout as inline `BSGeometryMeshData`, but as a standalone binary with its own header.

## Suggested Fix

1. New `crates/nif/src/mesh_file/` module (or sibling `crates/sfmesh/` crate). Decoder produces the same `BSGeometryMeshData` shape that SF-D5-01's importer already consumes.
2. Plumb the BA2 handle from the NIF importer to the `.mesh` reader. Asset extraction from BA2 is already solved (`crates/bsa/src/ba2.rs`); the wiring problem is passing the archive context through the importer chain.
3. Cache extracted `.mesh` files per session to avoid repeated BA2 reads (sha1 names dedupe naturally).

## Completeness Checks

- [ ] **TESTS**: Real-data integration gated on `BYROREDUX_STARFIELD_DATA`; pick a vanilla `BSGeometry` NIF that references an external `.mesh`, assert the `ImportedMesh` has the expected vertex/index counts.
- [ ] **SIBLING**: Verify the importer falls back gracefully when the external `.mesh` is missing (edge case for stripped archives).
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.

## Related

- SF-D5-01 (BSGeometry importer — Stage A; this is Stage B)
- ROADMAP must document that Starfield mesh support requires *two* parsers.
