# #1000 — SPT-D4-03: Mesh normal vs billboard rotation-arc convention

- **Severity**: LOW
- **Domain**: legacy-compat / import-pipeline
- **Audit**: `docs/audits/AUDIT_SPEEDTREE_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1000

## TL;DR
Placeholder mesh sets normal to `[0, 0, 1]`. Billboard system uses `from_rotation_arc(-Z, look_dir)` and expects mesh facing `-Z`. Hidden by `two_sided: true` AND by #994 (Billboard never attached). Will surface once #994 is fixed — half-frame flicker during fast camera-around-tree motion.

## Fix
Flip normals to `[0, 0, -1]`, swap index order `[0, 3, 2, 2, 1, 0]` to preserve winding. Verify NIF importer's billboard convention for parity.
