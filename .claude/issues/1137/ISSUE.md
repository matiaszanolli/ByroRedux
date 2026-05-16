# CONC-D2-NEW-02: promote STATIC_BLAS_FLAGS to module constant

**GitHub**: #1137
**Severity**: LOW
**Audit**: AUDIT_CONCURRENCY_2026-05-16.md
**Status**: CONFIRMED

## Location
- `crates/renderer/src/vulkan/acceleration/blas_static.rs:210-213` (single-shot fn-local const)
- `blas_static.rs:549-556` (size-query inline literal in `build_blas_batched`)
- `blas_static.rs:671-682` (record inline literal in `build_blas_batched`)

## Summary
`build_blas` has a function-local `STATIC_BLAS_FLAGS` const. `build_blas_batched` repeats the
same flag pair (`PREFER_FAST_TRACE | ALLOW_COMPACTION`) as two inline literals. Same flag-drift
risk as the `6059e2ab` skinned-BLAS issue that motivated `1775a7e6`. `constants.rs:69` already
notes it as a pending counterpart.

## Fix
Promote to `pub(super) const STATIC_BLAS_FLAGS` in `constants.rs` alongside `SKINNED_BLAS_FLAGS`
and `UPDATABLE_AS_FLAGS`. Replace both inline literals in `build_blas_batched`.
