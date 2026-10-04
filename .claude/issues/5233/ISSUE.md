# #5233 — FO4-D1-02: `precombined.rs` module doc still says "finest LOD only" and "cell-local identity", and cites a spec range that now points at the vertex table

https://github.com/matiaszanolli/ByroRedux/issues/5233

Source: `docs/audits/AUDIT_FO4_2026-10-03.md` (HEAD `32f4450d9`)

**Source**: `docs/audits/AUDIT_FO4_2026-10-03.md` (FO4-2026-10-03-D1-02)

- **Severity**: LOW
- **Dimension**: M49 precombines (doc rot)
- **Location**: `byroredux/src/cell_loader/precombined.rs:21-23`
- **Status**: NEW
- **Description**:
  - The header says "Meshes are decoded to Y-up, spawned at cell-local identity … LOD is selected by triangle count (finest LOD only, per `fo4-csg-format.md:138-142`)."
  - Since #4234, every populated LOD band is decoded (`precombine_lod_bands`). `fo4-csg-format.md:138-142` is now the on-disk vertex table.
  - "Cell-local identity" is the wrong premise behind FO4-D1-01 (#5228).
- **Evidence**: `git blame` traces lines 21-22 to e9df743f50 (2026-08-12) and line 23 to 0ace5caf19 (2026-06-03). Both predate #4234. `crates/nif/src/import/precombine.rs:45-50,156-171` documents the all-bands contract.
- **Impact**: cosmetic, but it is the first text an auditor reads, and it states two disproved premises.
- **Related**: #5228, #4234.
- **Suggested Fix**: rewrite the two sentences together with #5228's fix.

## Completeness Checks
- [x] **DOCS**: the module doc states all-bands decode (#4234) and the world-absolute exterior frame; the spec cite points at the real section
