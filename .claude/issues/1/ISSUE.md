# Issue #1: XYZ euler rotation keys not supported

## Metadata
- **Type**: enhancement
- **Severity**: low
- **Labels**: enhancement, animation, nif-parser, M21
- **State**: OPEN
- **Created**: 2026-04-02
- **Milestone**: M21 follow-up
- **Affected Areas**: NIF animation import, keyframe interpolation

## Problem Statement
`NiTransformData` rotation keys with `KeyType::XyzRotation` (type 4) store rotations as three separate float key groups for X, Y, Z euler angles instead of quaternion keys. The current import logs a warning and drops the rotation channel.

## Affected Files
- `crates/nif/src/anim.rs` — `convert_quat_keys()` skips XyzRotation
- `crates/nif/src/blocks/interpolator.rs` — `NiTransformData.xyz_rotations` already parsed

## Acceptance Criteria
- [ ] XYZ euler key groups sampled per-axis and combined into quaternion keys
- [ ] Per-axis key types respected (each axis can differ)
- [ ] Unit test: known euler angles produce expected quaternions

## Notes
Rare in FNV, more common in Morrowind/Oblivion content.
