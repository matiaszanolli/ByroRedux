# #1196 — PERF-DIM7-02: BLAS refit unconditional even when dispatch is a no-op

**Source**: docs/audits/AUDIT_PERFORMANCE_2026-05-19.md (Dim 7, MEDIUM)
**Severity**: medium
**Labels**: bug, medium, renderer, performance, M29, memory
**State**: OPEN (filed 2026-05-19)
**Paired**: #1195 (must gate on the same bool)
**Blocked on**: #1194 (instrumentation prerequisite)

## Cause

crates/renderer/src/vulkan/context/draw.rs:994-1024 — refit loop has no gate paired to the dispatch gate. BLAS whose vertex buffer wasn't written this frame doesn't need refit.

## Fix

Same skip-flag plumbing as #1195. Critical sub-fix: bump `last_used_frame` on the skip path so LRU at draw.rs:1077 doesn't reap a quiescent slot. Mirrors the existing dispatch-side bump at draw.rs:899.

## Risk

HIGH — split decision between dispatch and refit. Must gate on same bool. Transform changes without bone changes must still force refit.

## Estimated impact

Combined with #1195: ~3 ms / frame upper bound on Prospector.
