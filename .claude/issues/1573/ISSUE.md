# bug, renderer, low, tech-debt

## AS-D1-01: STATIC_BLAS_FLAGS has no value-pinning test (siblings do)

**Severity**: LOW
**Dimension**: AS Correctness
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-06-14.md`
**Status**: NEW

## Description
`UPDATABLE_AS_FLAGS` and `SKINNED_BLAS_FLAGS` both have value-pin tests; `STATIC_BLAS_FLAGS` (`FAST_TRACE | ALLOW_COMPACTION`) does not. Test-coverage symmetry gap, not a live bug — the constant is shared across all static-BLAS call sites so the VUID lockstep it protects can't regress; only the perf/compaction intent could silently drift.

## Evidence
- `crates/renderer/src/vulkan/acceleration/constants.rs` (`STATIC_BLAS_FLAGS`).
- `crates/renderer/src/vulkan/acceleration/tests.rs` — sibling pin tests for `UPDATABLE_AS_FLAGS` (line ~1346) and `SKINNED_BLAS_FLAGS` (line ~1358) present; `grep` for a `static_blas_flags` test → none.

## Impact
None at runtime; a future flag-set change to `STATIC_BLAS_FLAGS` would be unguarded by a test.

## Suggested Fix
Add `static_blas_flags_is_fast_trace_allow_compaction` mirroring the two sibling tests.

## Completeness Checks
- [ ] **TESTS**: new test asserts the exact `FAST_TRACE | ALLOW_COMPACTION` bit-set, matching the sibling tests' style
