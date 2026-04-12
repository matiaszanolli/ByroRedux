# Issue #250 — PERF-04-11-L5

**Title**: world_bound_propagation re-acquires Children/LocalBound queries in pass 2
**Severity**: LOW
**Dimension**: ECS Query Patterns
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `byroredux/src/systems.rs:611-612`

## Summary
Pass 2 re-acquires queries already held by pass 1. Lift the acquires out and share between passes.

## Fix with
`/fix-issue 250`
