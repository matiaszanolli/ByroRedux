# Issue #252 — PERF-04-11-L7

**Title**: AnimationStack allocates Vec<(EntityId, Vec3, Quat, f32)> per tick
**Severity**: LOW
**Dimension**: CPU Allocations
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `byroredux/src/systems.rs:362-371`

## Summary
Pose-update batch Vec allocated fresh each tick. Stash on the component as a scratch buffer, clear + reserve on tick entry.

## Sibling
#251 (same pattern)

## Fix with
`/fix-issue 252`
