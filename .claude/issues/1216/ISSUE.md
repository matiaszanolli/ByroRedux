**Severity**: LOW (observability)
**Dimension**: NIF Format Readiness
**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-05-19.md` Dim 2 FIND-2

The parser already gates the `data_size != expected_data_size` warning on `num_vertices != 0` in `crates/nif/src/blocks/tri_shape.rs` (per #836 / SK-D5-NEW-02) because SSE skinned bodies legitimately ship `num_vertices=0` (data lives on a sister `NiSkinPartition` per #559).

There is no symmetric warning for the **opposite** case: `num_vertices == 0` outside of (a) the SSE-skinned-reconstruction path AND (b) the FO4-precombined Shared-variant path. Either the file is a CSG-only shape that needs a reader we haven't shipped, or it's a malformed shape — and we can't tell from the parser side.

The walker (`crates/nif/src/import/walk/mod.rs:50-65`) skips the whole subtree when a `BSPackedCombinedGeomDataExtra` extra-data ref resolves under the host NiNode, so the Shared case is identifiable at walk time. But the BSTriShape parser itself doesn't carry the host-context bit.

### Suggested Fix
Add a `tracing` counter in `BsTriShape::parse` when `num_vertices == 0 && data_size == 0 && (vertex_attrs & VF_SKINNED) == 0`, surfaced through `nif_stats` so the parse harness reports `zero_vertex_bs_tri_shape_count` per archive.

### Completeness Checks
- [ ] **UNSAFE**: N/A.
- [ ] **SIBLING**: BSMeshLODTriShape parser at the same path.
- [ ] **TESTS**: `nif_stats` regression on Fallout4 - MeshesExtra.ba2 should report ≥124k zero-vertex BSTriShape counts.
