# #6242: PAR-D5-01: The lossless clip decoder hands raw quaternions to the pose: a zero-length rotation reaches `Quat::from_array` unrejected, where the spline path's `normalize_quaternion` refuses it

**Labels**: medium,bug,import-pipeline,animation,game:fo4

**Source**: docs/audits/AUDIT_PARSERS_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Decode-Consumer Wiring · **Location**: `crates/hkx/src/animation.rs` (`decode_lossless_in`, `rotation_at`)

- **Location**:
  - `crates/hkx/src/animation.rs:903-916` (`decode_lossless_in`, `rotation_at`).
  - Spline path: `normalize_quaternion` at 1637–1661.
  - Consumer: `byroredux/src/asset_provider/animation.rs:585-602`.
  - Sampler: `crates/core/src/animation/interpolation.rs:260` (`q0.slerp(q1, t)`, no normalise).
- **Status**: NEW (in code added by 2649febc7, #5897)
- **Trigger Input**: an `hkaLosslessCompressedAnimation` whose static or dynamic rotation array stores `(0,0,0,0)`, or any finite non-unit quaternion such as `(0,0,0,2)`.
- **Description**:
  - `decode_lossless_in` reads four `f32`s per rotation and only flags non-finite values (912). The spline decoder runs every quaternion through `normalize_quaternion`, which:
    - errors on zero length and on length-squared overflow (#4396);
    - normalises everything else.
  - The two decoders of the same `HkxAnimation` therefore apply different policies. A lossless clip's zero quaternion becomes a zero rotation matrix (the bone and its skinned vertices collapse to a point). A length-2 quaternion scales the bone by 4.
  - Nothing warns about either.
- **Evidence**: 912 checks `is_finite` only. `convert_hkx_clip` does `Quat::from_array(rotation)` with no normalise, and the linear rotation sampler `slerp`s the raw keys.
- **Impact**: modded FO4 lossless clips (first-person poses, furniture) render with collapsed or scaled bones and no diagnostic. Vanilla is unaffected: the #5897 census found all 827,268 stored quaternions unit length.
- **Related**: #5897, #4396, #5662 (the non-finite track-drop policy this decoder does share).
- **Suggested Fix**: run each lossless rotation through `normalize_quaternion`. Treat its `Err` like a non-finite value, so the track is dropped into `dropped_bones`. Add a fixture with a zero quaternion.

**Validation note (audit-publish, HEAD 4a3e89d56)**: the *non-unit* half of the original claim does not hold end-to-end. `convert_hkx_clip` (`byroredux/src/asset_provider/animation.rs`) passes every sample through `byroredux_core::math::coord::zup_to_yup_quat_wxyz`, which calls `normalize_quat`, so a finite length-2 quaternion is normalised before `Quat::from_array` and does not scale the bone. The *zero-length* half holds: `normalize_quat` returns a zero-length input unchanged (`if len_sq == 0.0 { return q; }`), so a `(0,0,0,0)` lossless rotation still reaches the pose as a zero quaternion, unrejected and unwarned, while the spline path's `normalize_quaternion` errors on it. The divergent decoder policy is still the defect; the Suggested Fix above is unchanged.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
