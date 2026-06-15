# #1600 — FO4-D8-LOW-01: SCOL transform composition only tested with identity outer rotation

**Severity**: LOW · **Dimension**: FO4 Cell Load End-to-End
**Source**: `docs/audits/AUDIT_FO4_2026-06-14.md` (FO4-D8-LOW-01)
**Location**: `byroredux/src/cell_loader/scol_expansion_tests.rs:34,79,140,186,247,306,347` (all `outer_rot = Quat::IDENTITY`); code under test `byroredux/src/cell_loader/refr.rs:498-499`

## Description
Every SCOL expansion test passes `Quat::IDENTITY` as the outer REFR rotation, so `outer_rot * (outer_scale * local_pos)` and `outer_rot * local_rot` are never exercised with a non-identity rotation. With identity outer_rot the rotation-of-position degenerates to identity and the rotation product to `local_rot` alone — a transform-order regression would slip through. The PKIN multi-CNAM test uses a non-identity rot but PKIN children inherit the outer position verbatim, so it doesn't cover SCOL composition either.

## Evidence
grep `outer_rot`/`from_rotation` in `scol_expansion_tests.rs` shows `Quat::IDENTITY` at every SCOL call site; no assertion checks a rotated child position or a composed non-identity quaternion.

## Impact
A future composition-order swap would mislocate/mis-orient every multi-axis-rotated mod-added SCOL instance and the suite would stay green. Vanilla FO4 ships all SCOLs with cached CM*.NIF so the expansion branch only runs on mod content / previs-bypass — limited blast radius today.

## Related
#585 (SCOL expansion), #1182 (SCOL recursion).

## Suggested Fix
Add one test with non-identity `outer_rot` + non-zero `outer_pos` and a SCOL part with non-zero local pos + non-identity local rot, asserting the composed `final_pos`/`final_rot` to a small epsilon — pinning the parent∘child order.

## Completeness Checks
- [ ] **SIBLING**: PKIN composition also gets a non-identity outer_rot test if it shares the parent∘child path
- [ ] **TESTS**: A regression test asserts composed `final_pos`/`final_rot` for a non-identity outer rotation + non-zero local rot
