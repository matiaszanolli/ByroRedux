# Issue #251 — PERF-04-11-L6

**Title**: AnimationStack builds transient Vec<&str> for channel dedup per tick
**Severity**: LOW
**Dimension**: CPU Allocations
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `byroredux/src/systems.rs:351-360`

## Summary
Per-tick `Vec<&str>` for channel-name sort/dedup. Use `SmallVec<[&str; 32]>` or a reusable buffer cached on the component.

## Sibling
#252 (same pattern, different Vec)

## Fix with
`/fix-issue 251`
