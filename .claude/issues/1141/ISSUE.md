# CONC-D5-NEW-02: build_skinned_blas (sync) is dead code — latent scratch race if revived

**GitHub**: #1141
**Severity**: INFO
**Audit**: AUDIT_CONCURRENCY_2026-05-16.md
**Status**: CONFIRMED (unreachable in current call graph)

## Location
- `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:46-231` (sync path)

## Summary
`build_skinned_blas` (sync, uses `submit_one_time`) has zero production callers as of `1775a7e6`.
Production uses `build_skinned_blas_batched_on_cmd`. If the sync path is ever revived, it shares
`blas_scratch_buffer` with the per-frame batched builder: the host fence-wait between submissions
does NOT establish a device-side memory dependency, and the first BUILD in the batched path
lacks the `AS_WRITE→AS_WRITE` serialize barrier (`if i > 0` gate misses it).

## Fix
Preferred: delete `build_skinned_blas` (sync). It is dead weight per `#911 / REN-D5-NEW-02`.
Alternative: add `debug_assert!(false, "deprecated; use build_skinned_blas_batched_on_cmd")` at
function entry to catch future revivals in tests/debug builds.
