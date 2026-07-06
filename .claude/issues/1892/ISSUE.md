# REN-2026-07-05-L01: STATIC_BLAS_FLAGS comment claims the compaction pass is dead, but it is live

**Issue**: #1892 · **Severity**: LOW · **Labels**: low, renderer, documentation
**Dimension**: AS Correctness · **Filed from**: docs/audits/AUDIT_RENDERER_2026-07-05.md (rt-deep suite)
**Location**: crates/renderer/src/vulkan/acceleration/constants.rs (`STATIC_BLAS_FLAGS` doc-comment, ~L127–131)

## Description
The doc-comment above `STATIC_BLAS_FLAGS` asserts `ALLOW_COMPACTION` is set "even though no
caller currently runs the compact pass." Stale: `build_blas_batched` (`blas_static.rs`) runs a
full live compaction pass today.

## Evidence
- constants.rs:127-131 — the "no caller currently runs the compact pass" comment.
- blas_static.rs:796+ — Phase 3 `ACCELERATION_STRUCTURE_COMPACTED_SIZE_KHR` query pool, Phase 4
  size queries after an AS_BUILD→AS_BUILD WRITE→READ barrier, Phases 5–7 read back + COMPACT copy.
  Driven every cell/scene load via resources.rs::build_blas_batched.

## Suggested Fix
Update the comment to state the compaction pass is live in `build_blas_batched`; the
lockstep-across-call-sites rationale for the shared constant remains valid.
