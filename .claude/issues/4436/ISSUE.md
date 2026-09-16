# #4436: SF-2026-09-16-D3-03: `MaterialProvider::sf_cdb_count`'s Phase-2 plan prescribes the re-parse the spike ruled out, and the `.mat` fallthrough cites a closed tracker

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4436
- **Labels**: low,documentation,doc-rot,game:starfield,legacy-compat
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW
- **Dimension**: 3
- **Location**:
  - `byroredux/src/asset_provider/material/provider.rs:160-167`
  - `byroredux/src/asset_provider/material/merge.rs:326-329`
- **Status**: NEW
- **Description**:
  - **Phase-2 plan**: the field doc, which is what a Phase-2 implementer reads
    first, says: "Phase 2 (future, SF-D3-01 #1289): re-`parse` each CDB on
    demand and walk the instance trees … so per-material metalness /
    roughness / texture paths flow into `ImportedMesh`". The spike §3 measured
    `parse` at 9.19 GB per full-size CDB (~18 GB across the 13 discovered)
    and concluded "Calling `parse` on the cell-load path is not viable; an
    indexed reader is the project".
  - **Wrong target and tracker**: the same doc names the wrong sink
    (`ImportedMesh`, where the boundary takes `&mut ImportedMaterial`) and a
    closed tracker (#1289, where #3398 owns the work).
  - **Stale `.mat` pointer**: `merge.rs:326-329` says ".mat format is not yet
    parsed (tracked in SF-D6-03)". SF-D6-03 is the closed #762. The open
    tracker for the loose `.mat` resolver gap is #4277.
- **Evidence**: The quoted doc text above, against spike §3 lines 189-203.
- **Impact**: The doc steers the next implementer toward the approach already
  measured as non-viable.
- **Suggested Fix**: Rewrite the Phase-2 paragraph to "indexed/streaming
  reader (#3398)" and target `ImportedMaterial`. Repoint `merge.rs` at #4277.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
