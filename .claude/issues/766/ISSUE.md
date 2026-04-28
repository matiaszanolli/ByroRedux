# #766 — NIF-D5-NEW-01: SE Meshes1 Havok long-tail (`bhkBallSocketConstraintChain`, `bhkPlaneShape`)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/766
- **Severity**: LOW
- **Labels**: enhancement, low, nif-parser
- **Source**: docs/audits/AUDIT_NIF_2026-04-28.md (NIF-D5-NEW-01)

## Summary

Two Havok types from Skyrim SE DLC content fall through to `NiUnknown`:
- `bhkBallSocketConstraintChain` — 6 hits in `Skyrim - Meshes1.bsa` (rope/chain physics)
- `bhkPlaneShape` — 1 hit in `Skyrim - Meshes1.bsa` (kinematic floor)

Block-size recovery handles them today on FO3+; would cascade on Oblivion (no block-size recovery).

## Fix

Stub each as a NiObject with `block_size`-driven trailing-byte skip. ~20 LOC each. Layouts in nif.xml at `/mnt/data/src/reference/nifxml/nif.xml`.

## Test plan

- Per-type byte-exact dispatch test with captured fixture
- Skyrim SE Meshes1 integration sweep that pins post-fix unknown count to 0
