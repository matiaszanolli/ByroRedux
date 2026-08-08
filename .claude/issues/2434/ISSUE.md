# COORD-1: KF XYZ-Euler rotation keys use the CCW convention, contradicting Gamebryo's CW-positive rule every other Euler consumer honours

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2434
**Finding ID**: COORD-1 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 1 — Coordinate-system correctness
**Location**: `crates/nif/src/anim/keys.rs:102-132` (call), `:188-200` (`euler_to_quat_wxyz`)
**Status**: NEW

## Description
`convert_xyz_euler_keys` handles `NiTransformData`/`NiKeyframeData` rotation keys stored as `Rotation Type == 4` (XYZ_ROTATION_KEY). It samples the three axes and composes with `euler_to_quat_wxyz(x, y, z)`, which builds standard CCW-positive elementary quaternions (`qx ⊗ qy ⊗ qz`, hand-expanded against the code). Gamebryo is clockwise-positive per the vendor header (`efd/Matrix3.h`: "positive angles are associated with clockwise rotations"), so a Gamebryo Euler triple must be negated before composition — every other Euler consumer in the tree (`euler_zup_to_quat_yup`, the REFR dispatcher, XCLL lighting) does negate. Conjugating the code's product through the Z-up→Y-up swap gives `Rx(rx)·Rz(-ry)·Ry(rz)` — character-for-character `--rotation-mode 3`, which `byroredux/src/cell_loader/euler.rs:79` itself labels a non-shipping diagnostic.

## Evidence
Reachability confirmed live (`crates/nif/src/anim/keys.rs:60-61` dispatches on `KeyType::XyzRotation`, fed from `crates/nif/src/blocks/interpolator.rs:308-330`). Existing tests (`crates/nif/src/anim/tests/coord_keys.rs:37-80`) only assert unit length and axis dominance — sign-blind, pass under either convention. Confirmed directly: `euler_zup_to_quat_yup` (`crates/core/src/math/coord.rs:135`) negates `rx`/`rz`; `euler_to_quat_wxyz` (`crates/nif/src/anim/keys.rs:188`) does not negate anything.

## Impact
Any animated node whose rotation is authored as XYZ Euler key groups rotates in the wrong direction (exact inverse for single-axis channels, a different rotation entirely for multi-axis ones) — limbs/doors/machinery counter-rotate or skew. Bethesda KF overwhelmingly ships quaternion keys, so scope is narrow but nonzero, and the failure is silent.

## Suggested Fix
Negate the three samples before composition (build the Z-up quat from `(-x,-y,-z)`), or add a `euler_zup_to_quat_yup_xyz` sibling to the core SoT rather than a second private formula in the animation crate. Add a sign-discriminating regression pin before shipping the flip; validate against a real asset that exercises `Rotation Type == 4`.

## Completeness Checks
- [ ] **TESTS**: A sign-discriminating regression test pins the fix (existing tests are sign-blind)
- [ ] **SIBLING**: Confirm no other private Euler-composition formula exists outside the core SoT
