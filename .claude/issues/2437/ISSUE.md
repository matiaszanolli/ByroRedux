# COORD-4: Four independent copies of the C·R·Cᵀ basis change, coupled only by comments

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2437
**Finding ID**: COORD-4 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 1 — Coordinate-system correctness
**Location**: `crates/nif/src/import/coord.rs:41-45`; `crates/nif/src/import/mesh/skin.rs:479-492`; `crates/nif/src/import/collision/mod.rs:518-522` and `:487-494` (quaternion flavour)
**Status**: NEW (structural/regression-risk only — all four currently correct, hand-verified numerically)

## Description
The array-form position swap has one home; the rotation flavour does not. #1617 routed the translation halves of the Havok/skin sites into the SoT but deliberately left the matrix/quat math duplicated. This is the precise shape of the pre-#1044 bug class (five copies, one missing the #333 normalise fix, drifted for months).

## Impact
None currently. A future fix (handedness/determinant guard) applied to one copy will not propagate to the other three, with no test to catch the divergence.

## Related
#1044 (CLOSED, the original 5-copy drift class), #1617 (CLOSED, routed translation halves only, left rotation duplicated).

## Suggested Fix
Promote `zup_to_yup_rot_mat3` into `crates/core/src/math/coord.rs`; at minimum add a cross-checking unit test that feeds one random rotation through all four paths and asserts agreement.

## Completeness Checks
- [ ] **TESTS**: A cross-checking test feeds one random rotation through all four paths and asserts numerical agreement
- [ ] **SIBLING**: Confirm no fifth copy exists elsewhere (repeat the #1044 sweep)
