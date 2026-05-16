# CONC-D5-NEW-01: cross-submission scratch-serialize barrier invariant has no pinning unit test

**GitHub**: #1140
**Severity**: MEDIUM
**Audit**: AUDIT_CONCURRENCY_2026-05-16.md
**Status**: CONFIRMED (coverage in place, no test pinning it)

## Location
- `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:545-555` (self-emitted barrier)
- `crates/renderer/src/vulkan/acceleration/blas_static.rs:622-647` (cell-load batched path)

## Summary
The `refit_skinned_blas` self-emits `record_scratch_serialize_barrier` at line 555 (#983 /
REN-D8-NEW-15). This correctly covers the cross-submission scratch race where cell-load builds
run in a fenced `submit_one_time` before the per-frame cmd. However there is no unit test
pinning this invariant — a future refactor moving the barrier back to caller-side would
silently regress with no test coverage. Validation layers cannot catch cross-submission
scratch races.

## Fix
Add a regression test under `crates/renderer/src/vulkan/acceleration/tests.rs` asserting the
barrier emit count via a predicate in `predicates.rs` — e.g.
`requires_pre_refit_serialize_barrier(prior_submission_kind) -> bool`. A pure unit-test
predicate co-located with the existing predicates family is sufficient without a live Vulkan device.
