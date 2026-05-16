# SAFE-D1-NEW-02: SKINNED_BLAS_FLAGS / UPDATABLE_AS_FLAGS bit composition not pinned by unit tests

**GitHub**: #1144
**Severity**: LOW
**Audit**: AUDIT_SAFETY_2026-05-16.md
**Status**: CONFIRMED

## Location
- `crates/renderer/src/vulkan/acceleration/constants.rs:78-98`
- `crates/renderer/src/vulkan/acceleration/tests.rs` (absent — no flag-pin tests)

## Summary
`UPDATABLE_AS_FLAGS` (PREFER_FAST_TRACE | ALLOW_UPDATE) and `SKINNED_BLAS_FLAGS`
(PREFER_FAST_BUILD | ALLOW_UPDATE) have no unit tests pinning their bit composition.
A future edit adding `ALLOW_COMPACTION` to the skinned arm would compile + violate
VUID-03667 at runtime; a FAST_TRACE typo on skinned would regress FPS silently.

## Fix
Add two `#[test]` functions to `tests.rs`:
- `updatable_as_flags_is_fast_trace_plus_allow_update`
- `skinned_blas_flags_is_fast_build_plus_allow_update`

Pattern mirrors `gpu_material_size_is_260_bytes` at `material.rs:647`. ~16 LOC.
Should land with #1145 (SAFE-D6-NEW-01) in the same commit.
