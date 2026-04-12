# Issue #238 — PERF-04-11-H2

**Title**: Transform propagation BFS drops and re-acquires locks per child
**Severity**: HIGH
**Dimension**: ECS Query Patterns
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `crates/core/src/ecs/systems.rs:93-128`

## Summary
Inside the BFS loop each child visit acquires and drops four RwLocks. 500-entity hierarchy = ~4000 lock cycles/frame. Lift the queries out of the loop, mirror `world_bound_propagation_system`'s pass 2 pattern.

## Fix with
`/fix-issue 238`
