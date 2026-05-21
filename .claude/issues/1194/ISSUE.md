# #1194 — PERF-DIM7-INSTR: GPU timer + dispatches_skipped (gates DIM7-01/02/03)

**Source**: docs/audits/AUDIT_PERFORMANCE_2026-05-19.md (Dim 7, prioritization step 3)
**Severity**: medium
**Labels**: bug, medium, renderer, performance, M29
**State**: OPEN (filed 2026-05-19)
**Blocks**: #1195 (DIM7-01), #1196 (DIM7-02), #1197 (DIM7-03)

## Why

Every MEDIUM-tier Dim-7 fix is currently "needs measurement". Without per-pass GPU timing + a skip counter we can't tell a 1 ms saving from a 1 ms regression.

## Plan

1. `dispatches_skipped: u32` on `SkinCoverageFrame` (crates/renderer/src/vulkan/skin_compute.rs:102)
2. VkQueryPool TIMESTAMP brackets around skin dispatch loop, skin BLAS refit loop, TAA dispatch
3. Surface via `tex.skin` and `bench-stats --break-down skin`

## Completeness checks → see GH issue body
