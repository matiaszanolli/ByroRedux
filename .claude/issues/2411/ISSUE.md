# TD1-010: asset_provider/tests.rs — pure test file (2011 LOC) with clear per-topic section markers ready for a directory split

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2411
**Finding ID**: TD1-010 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 1 — File / Function / Module Complexity
**Location**: `byroredux/src/asset_provider/tests.rs` (whole file, 2011 LOC)
**Status**: NEW

## Description
Pure test file at 2011 LOC. Already has 8 explicit topic-divider comments that split the file logically (M35 sibling archive, BGSM merge #493 — the largest at ~1200 lines, Starfield `.mat`, etc), so the split boundaries are already marked in-file.

## Related
Same pattern as #2311 (the sibling `crates/nif/src/import/tests/` split), which proved the approach.

## Suggested Fix
Convert to a `tests/` directory mirroring the `import/tests/` precedent: `mod.rs`, `archive_siblings.rs`, `material_path.rs`, `bgsm_merge.rs`, `starfield_mat.rs`. Zero logic change.

## Completeness Checks
- [ ] **TESTS**: All tests still compile and pass unchanged after the directory conversion
