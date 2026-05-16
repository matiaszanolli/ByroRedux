# SAFE-D6-NEW-01: BlasEntry does not store built_flags — VUID-03667 flag-match half has no runtime pin

**GitHub**: #1145
**Severity**: LOW
**Audit**: AUDIT_SAFETY_2026-05-16.md
**Status**: CONFIRMED

## Location
- `crates/renderer/src/vulkan/acceleration/types.rs:11-56` (BlasEntry struct — missing built_flags)
- `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:579-589` (refit validation site)
- `crates/renderer/src/vulkan/acceleration/predicates.rs` (validate_refit_counts — flag half unchecked)

## Summary
`BlasEntry` stores `built_vertex_count` + `built_index_count` to pin the count half of
VUID-03667 at UPDATE time. The flag-set half is enforced only by convention (both BUILD
and UPDATE reference the same constant). With two constants (`SKINNED_BLAS_FLAGS` vs
`UPDATABLE_AS_FLAGS`), a future BUILD site using the wrong constant compiles silently.

## Fix
1. Add `built_flags: vk::BuildAccelerationStructureFlagsKHR` to `BlasEntry`
2. Populate at all 5 BUILD sites in `blas_skinned.rs` + TLAS BUILD sites in `tlas.rs`
3. Extend `validate_refit_counts` → `validate_refit_inputs` to assert `entry.built_flags == current_flags`
4. Update 4 existing tests in `tests.rs:156-188` for new field
5. Add `validate_refit_inputs_rejects_flag_drift` test

~40 LOC. Should land with #1144 (SAFE-D1-NEW-02 constant-pin tests) in the same commit.
Related: #907 (the built_vertex_count precedent).
