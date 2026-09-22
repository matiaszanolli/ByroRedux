# PHYS-D1-2026-09-21-01: The #2543 non-finite extent clamp is pinned only by a release-only test that no CI lane runs

**Issue**: #4686
**Filed**: 2026-09-22 (audit-publish, AUDIT_PHYSICS_2026-09-21.md)

**Severity**: LOW
**Dimension**: Shape Translation
**Location**: `crates/physics/src/convert.rs:605-616` (`#[cfg(not(debug_assertions))]` test), `:26-32` (`clamp_shape_extent`); `.github/workflows/ci.yml:169-170`, `:197-198`

## Description
`non_finite_cuboid_extent_clamps_instead_of_reaching_rapier` is compiled out whenever debug assertions are on. No debug-lane test feeds NaN/±Inf through Ball, Capsule, Cylinder or Cuboid. CI runs only `cargo test --workspace` in the default profile; the only `--release` test jobs are the cornell oracle and byroredux-nif data gates.

## Evidence
`clamp_shape_extent` is the only non-finite backstop. `grep f32::NAN|INFINITY convert.rs` finds only the release test and the Compound twin, which does run by default.

## Impact
A future "simplification" to `value.clamp(1e-3, MAX)` is NaN-transparent (the #3194/#3529 class) and CI would stay green.

## Related
#2543, #3194, #3529.

## Suggested Fix
Add a debug-lane test sending NaN/±Inf through Ball, Capsule and Cylinder, or unit-test `clamp_shape_extent` directly. Keep the release test.
