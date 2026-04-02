# Investigation: Issue #1 — XYZ Euler Rotation Keys

## Code Path
1. `NiTransformData::parse()` in `interpolator.rs:208` — when `rotation_type == XyzRotation`, reads 3 `KeyGroup<FloatKey>` into `xyz_rotations: Option<[KeyGroup<FloatKey>; 3]>`. No quaternion keys are read.
2. `convert_quat_keys()` in `anim.rs:200` — checks `rotation_type`, hits the XyzRotation branch, logs warning, returns empty vec.
3. Result: rotation channel dropped entirely.

## Fix Strategy
The fix is in `convert_quat_keys()` in `anim.rs`. When `rotation_type == XyzRotation`:
1. Collect all unique timestamps across all 3 axis key groups
2. For each timestamp, sample each axis independently (reuse the same linear/quadratic/TBC interpolation the float keys already support)
3. Convert the 3 euler angles to a quaternion via coordinate conversion
4. Store as `RotationKey` with `KeyType::Linear` (quaternion slerp between the composed samples)

Key detail: Gamebryo euler angles are in **radians**, Z-up coordinate system. Need to:
- Sample X, Y, Z euler angles at each unique time
- Apply Z-up to Y-up euler reordering: Gamebryo (X, Y, Z) around Z-up axes → Y-up euler
- Convert to quaternion

The per-axis float sampling logic doesn't exist in `anim.rs` yet — it's in `core/animation.rs` (`sample_scale` does float interpolation). But since we're doing this at import time (not runtime), we can write a simple float sampler inline.

## Files to Change
- `crates/nif/src/anim.rs` — `convert_quat_keys()`: handle XyzRotation branch
- No other files needed

## Scope: 1 file, < 5 file threshold. Proceed.
