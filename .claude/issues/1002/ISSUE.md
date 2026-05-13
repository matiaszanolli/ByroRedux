# #1002 — SPT-D5-02: TREE BNAM unused in SpeedTree importer

- **Severity**: LOW
- **Domain**: legacy-compat / import-pipeline
- **Audit**: `docs/audits/AUDIT_SPEEDTREE_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1002

## TL;DR
`TreeRecord.billboard_size` (BNAM) is parsed but `parse_and_import_spt` only wires `bounds` from OBND. FO3/FNV records that ship BNAM but lack OBND fall back to the 256×512 default instead of the authored billboard size.

## Fix
Add `billboard_size: Option<(f32, f32)>` to `SptImportParams`, prefer it over OBND in `compute_billboard_size` (BNAM is authored *for the billboard*; OBND is the tree's physical AABB).

## Bundle with
#1001 (SPT-D4-04 default scaling) — both touch `compute_billboard_size`.
