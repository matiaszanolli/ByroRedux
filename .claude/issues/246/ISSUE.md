# Issue #246 — PERF-04-11-M11

**Title**: build_render_data acquires 8 independent component queries
**Severity**: MEDIUM (design)
**Dimension**: ECS Query Patterns
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `byroredux/src/render.rs:92-99`

## Summary
Render-data collection acquires 8 separate RwLock-backed queries for optional components. Pattern is correct but indicates a missing `query_bundle`/`query_n_mut` ECS API. Short-term: document the sorted acquisition order. Long-term: add a sorted-batch acquire macro.

## Fix with
`/fix-issue 246`
