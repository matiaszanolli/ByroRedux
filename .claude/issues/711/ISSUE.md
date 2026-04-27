# NIF-D5-05: FO4 BSMeshLODTriShape parser errors on MeshesExtra — 45,521 LOD blocks demoted

URL: https://github.com/matiaszanolli/ByroRedux/issues/711
Labels: bug, nif-parser, high

---

## Severity: HIGH

## Game Affected
FO4 (distant LOD)

## Location
- `crates/nif/src/blocks/tri_shape.rs` `parse_lod()` (called from `crates/nif/src/blocks/mod.rs:237`)

## Description
`BSMeshLODTriShape` IS dispatched via `parse_lod`, but in FO4 `MeshesExtra.ba2` (cell-LOD archive) **45,521 blocks fail mid-parse** and fall back to NiUnknown via the err-recovery path at `lib.rs:383`. That archive's `BSTriShape` parsing also bombs **18,073 times** — the two share a code path. The NiUnknown histogram counts both.

This is a **parser-error** (not a missing dispatch arm) — the NiUnknown bucket conflates dispatch-fallback with err-recovery; per #601 `nif_stats` separates them.

## Evidence
`d5_unk_ba2 Fallout4 - MeshesExtra.ba2`:
- `BSMeshLODTriShape`: 45,521 in NiUnknown bucket
- `BSTriShape`: 18,073 in NiUnknown bucket (cascade from same code path)

Other FO4 archives (`Fallout4 - Meshes.ba2`) have **zero** NiUnknown for these types, so the issue is LOD-specific. Likely a vertex-format flag combination unique to merged-LOD geometry.

## Impact
All FO4 distant terrain / building LODs render as empty bounding boxes. Headline FO4 outdoor scenes show pop-in at every cell boundary because the LOD substitute is missing.

## Suggested Fix
Reproduce with `crates/nif/examples/trace_block.rs` on one failing block and inspect. Suspects:
1. `VF_FULL_PRECISION` clear + LOD flags interaction (cf. #621)
2. `BSLODTriShape::parse_lod`'s 3-u32 LOD trailer running past the block boundary on small LOD batches

Cross-link: this is bundle-adjacent to #621 (parse_dynamic + data_size).

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D5-05)
- Bundle-adjacent: #621 (BsTriShape parse_dynamic VF_FULL_PRECISION)
- Cross-references: #601 (nif_stats per-type recovered-vs-fallback split)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify `BSTriShape::parse` cascade path — fix should resolve both 45K + 18K simultaneously
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Capture failing block via `trace_block.rs`; add byte-exact regression in `dispatch_tests.rs`
- [ ] **CORPUS**: Reproduce zero NiUnknown for `BSMeshLODTriShape` + `BSTriShape` on `Fallout4 - MeshesExtra.ba2`
