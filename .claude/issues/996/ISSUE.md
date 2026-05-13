# #996 — SPT-D5-01: `SptImportParams.wind` docstring says BNAM but BNAM is billboard size

- **Severity**: LOW
- **Domain**: documentation / legacy-compat
- **Audit**: `docs/audits/AUDIT_SPEEDTREE_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/996

## TL;DR
Docstring on `SptImportParams.wind` (`crates/spt/src/import/mod.rs:65-67`) says wind comes from BNAM. The TREE parser and the cell-loader comment correctly say BNAM is billboard width/height; wind comes from CNAM. Stale doc that will mislead Phase 2 wiring.

## Fix
Update docstring to reference CNAM (Oblivion 5×f32; FO3/FNV 8×f32).
