# Issue #242 — PERF-04-11-M5

**Title**: build_geometry_ssbo bypasses StagingPool
**Severity**: MEDIUM
**Dimension**: GPU Memory
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `crates/renderer/src/mesh.rs:152-165`
**Sibling**: #239

## Summary
Sister issue to #239 / #99. `MeshRegistry::build_geometry_ssbo` passes `staging_pool = None`. One large fire-and-forget staging alloc per cell load. Thread the pool through.

## Fix with
`/fix-issue 242`
