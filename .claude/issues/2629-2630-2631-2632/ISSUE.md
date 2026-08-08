# Issues 2629, 2630, 2631, 2632

All four are LOW-severity Starfield-audit findings. Two domains:
- #2629, #2630 → **bsa** (`byroredux-bsa`) — BA2 v2/v3 archive header
- #2631, #2632 → **nif** (`byroredux-nif`) — BSGeometry mesh extraction (Starfield)

---

## #2629 — SF-D1-04: log_v2_v3_extra_bytes doc claims name-table-size field that is always constant 1, dead heuristic

**Severity**: LOW · **Dimension**: 1 (BA2 v2/v3 LZ4 Block Decompression)
**Location**: `crates/bsa/src/ba2.rs:431-474` (`log_v2_v3_extra_bytes`)
**Status**: NEW — sibling of #2360 (SF-BA2-02, OPEN, LOW), different defect in the same helper

### Description
`log_v2_v3_extra_bytes` documents a "compressed name-table size" field that is always the constant `1` on every real archive — the malformed-header heuristic built on it is dead code. All 129 archives have `hdr[24..32] == 0100000000000000` byte-identical. A value of 1 is not a size; the `stream_pos + size > name_table_offset` malformed-header branch derived from reading it as one can never fire on real data.

### Evidence
129/129 archives byte-identical on this field.

### Impact
Documentation/diagnostic only.

### Suggested Fix
Rename to `unknown_1`/`unknown_2` (or `name_table_format`), recording the observed constant; drop or replace the dead heuristic.

### Related
#2360 (SF-BA2-02).

### Completeness Checks
- [ ] **TESTS**: N/A — doc/diagnostic-only change

---

## #2630 — SF-D1-05: no test covers v3-zlib path, LZ4 under-run, or real-data-derived BA2 header fixture

**Severity**: LOW · **Dimension**: 1 (BA2 v2/v3 LZ4 Block Decompression)
**Location**: `crates/bsa/src/ba2.rs:1009-1470` (`mod tests`)
**Status**: NEW

### Description
`compression_method == 0` on a v3 archive (v3+zlib) has zero test coverage and does not occur in vanilla; no under-run test exists (see SF-D1-01); the header-offset tests build their fixture to mirror the parser's own layout assumption, so a wrong offset would move in lockstep with the bug rather than being caught.

### Evidence
No v3+zlib fixture; no LZ4 under-run test; header-offset fixtures are generated from the parser's own assumed layout rather than an independent byte-literal spec.

### Impact
A future header-layout edit could pass the whole suite while breaking every v3 archive, surfacing only on a manual run against game data.

### Suggested Fix
Synthesize a v3+method-0 fixture; add the LZ4 under-run test (see SF-D1-01); add a byte-literal fixture built from the documented v3 header layout with post-parse content assertions.

### Related
SF-D1-01.

### Completeness Checks
- [ ] **TESTS**: This finding IS the test-coverage fix — add the three fixtures described above

---

## #2631 — SF2D2-D2-03: BSGeometryMeshData lods/meshlets/cull_data and slot LOD index decoded and dropped by importer

**Severity**: LOW · **Dimension**: 2 (BSGeometry Mesh Extraction)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:140-144,325`, `crates/nif/src/blocks/bs_geometry.rs:107-112`
**Status**: NEW

### Description
Three signals are parsed and discarded: (1) `mesh_data.lods` — full reduced triangle lists per LOD level (importer reads only LOD 0); (2) `meshlets`/`cull_data` — cluster-culling primitives; (3) the slot loop index itself is lost at parse time (`BSGeometry::parse`'s `for _ in 0..4` loop discards its own counter), so `meshes[0]` is the first *present* slot, not necessarily LOD 0 — combined with the sentinel-slot skip, a future LOD selector has no way to know which level it actually loaded.

### Evidence
`BSGeometry::parse`'s slot loop (`crates/nif/src/blocks/bs_geometry.rs:107-112`) discards its own loop counter; `mesh_data.lods`/`meshlets`/`cull_data` are parsed but never read by the importer (`bs_geometry.rs:140-144,325`).

### Impact
No LOD switching possible for Starfield content today (missing-feature, nothing renders wrong) — but item (3) is cheap now, expensive to retrofit later.

### Suggested Fix
Store the loop index as `BSGeometryMesh.lod_slot: u32` at parse time and carry it into `ImportedMesh` alongside `bs_lod_cutoffs`. `lods`/`meshlets` consumption itself is fine as EXAL follow-up work.

### Completeness Checks
- [ ] **TESTS**: A multi-slot fixture asserts `lod_slot` matches the actual authored slot index, not the post-skip array position

---

## #2632 — SF2D2-D2-04: UDEC3-decoded normals feed unnormalized into tangent synthesis Gram-Schmidt

**Severity**: LOW · **Dimension**: 2 (BSGeometry Mesh Extraction)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:150-162,217`, `crates/nif/src/blocks/bs_geometry.rs:569-580`, `crates/nif/src/import/mesh/tangent.rs:442-505`
**Status**: NEW

### Description
UDEC3-decoded normals feed unnormalized into `synthesize_tangents_yup`'s Gram-Schmidt, which assumes unit N. `unpack_udec3_xyzw`'s raw remap has no normalization (unit-length only to 10-bit quantization); the Gram-Schmidt projection is only correct for `|n| == 1`, and the degenerate fallback branch (`t_y = [n[1], n[2], n[0]]`) is neither normalized nor orthogonalized against `n`.

### Evidence
`unpack_udec3_xyzw` (`crates/nif/src/blocks/bs_geometry.rs:569-580`) performs no normalization; `synthesize_tangents_yup`'s Gram-Schmidt (`crates/nif/src/import/mesh/tangent.rs:442-505`) assumes unit-length input.

### Impact
Quantization error (~0.1%) is visually negligible on the non-degenerate path (shader renormalizes); the degenerate branch's non-orthogonality is a pre-existing shared divergence (AUDIT_INCREMENTAL_2026-05-22 ID-4), sub-pixel in practice.

### Suggested Fix
`normalize_inplace` the copy fed to `synthesize_tangents_yup`; orthogonalize the degenerate `t_y` against `n` with Gram-Schmidt + normalize before the cross product.

### Related
AUDIT_INCREMENTAL_2026-05-22 ID-4 (the pre-existing shared divergence this overlaps with).

### Completeness Checks
- [ ] **TESTS**: A degenerate-normal fixture asserts the fallback tangent is normalized and orthogonal to N
