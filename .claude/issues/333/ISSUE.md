# D4-05: zup_matrix_to_yup_quat fast path skips unit-quaternion normalization

## Finding: D4-05 (LOW, diagnostic)

**Source**: `docs/audits/AUDIT_NIF_2026-04-15.md` / Dimension 4
**Games Affected**: Any NIF with a slightly scaled rotation matrix (export-tool drift, hand-authored rotations). Rare.
**Location**: `crates/nif/src/import/coord.rs:33-42`, `matrix3_to_quat:48-88`

## Description

Fast-path gate is `(det - 1.0).abs() < 0.1` — accepts matrices with determinant in roughly [0.93, 1.07]. Shepperd's formula only produces a unit quaternion when the input is a proper rotation; scaled matrices within the gate produce non-unit quats (up to ~3.5% off unity at the edges).

Downstream consumers (`scene.rs`, `cell_loader.rs`) build `Quat::from_xyzw` without `.normalize()`, so the non-unit error propagates into ECS `Transform` rotation. Existing `zup_to_yup_*` tests use exact rotation matrices and don't catch this.

## Impact

Subtle shear/scale drift on affected meshes. Probably invisible but contradicts rotation invariants.

## Suggested Fix

Either tighten the fast-path gate to `1e-3` (force SVD for marginal cases) or normalize the quaternion at the end of `matrix3_to_quat`. Normalize cost: 1 sqrt + 4 muls — negligible vs SVD.


## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_NIF_2026-04-15.md`._
