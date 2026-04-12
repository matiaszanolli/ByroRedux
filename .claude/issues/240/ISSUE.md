# Issue #240 — PERF-04-11-M2

**Title**: sample_* keyframe sampling allocates temporary Vec<f32> per call
**Severity**: MEDIUM
**Dimension**: CPU Allocations
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `crates/core/src/animation/interpolation.rs:98, 217, 304, 372, 393`

## Summary
Every `sample_translation` / `sample_rotation` / `sample_scale` / `sample_float_channel` / `sample_color_channel` call allocates `let times: Vec<f32> = keys.iter().map(|k| k.time).collect();` to satisfy `find_key_pair`'s signature. 10 characters × 8 channels = ~80 allocs/frame. Rewrite `find_key_pair` as a closure-based binary search.

## Fix with
`/fix-issue 240`
