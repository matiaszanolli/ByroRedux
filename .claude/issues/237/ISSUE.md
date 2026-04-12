# Issue #237 — PERF-04-11-H1

**Title**: Frustum culling not wired into build_render_data
**Severity**: HIGH
**Dimension**: Draw Call Overhead
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `byroredux/src/render.rs:90-182`
**Follow-up to**: #217

## Summary
`build_render_data` iterates every MeshHandle entity and emits a DrawCommand unconditionally. #217 added WorldBound population + propagation, but nothing reads it during render-data collection. 3000-entity cells emit 3000 draws; ~200 are actually visible. Wire `WorldBound` + camera frustum planes into the query.

## Fix with
`/fix-issue 237`
