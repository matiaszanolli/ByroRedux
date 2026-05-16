# PERF-D8-NEW-02: TLAS build allocates Vec<String> missing-BLAS samples every frame

**GitHub**: #1142
**Severity**: LOW
**Audit**: AUDIT_PERFORMANCE_2026-05-16.md
**Status**: CONFIRMED

## Location
- `crates/renderer/src/vulkan/acceleration/tlas.rs:87`

## Summary
`let mut missing_samples: Vec<String> = Vec::new();` at tlas.rs:87 allocates every frame even
when no BLAS is missing (steady-state: Vec dropped empty). The `tlas_instances_scratch` one
line above IS amortised via `mem::take` — `missing_samples` doesn't follow the same pattern.

## Fix
Option A: add `tlas_missing_samples_scratch: Vec<String>` to AccelerationManager, `mem::take`
at function top.
Option B: `[Option<String>; 5]` fixed array (MISSING_BLAS_SAMPLE_LIMIT = 5).
~10 LOC either way.
