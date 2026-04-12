# Issue #249 — PERF-04-11-L4

**Title**: NameIndex rebuilds on any entity spawn, not just Name inserts
**Severity**: LOW
**Dimension**: ECS Query Patterns
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `byroredux/src/systems.rs:138-159`

## Summary
NameIndex uses `world.next_entity_id()` as its generation. Any spawn (named or not) triggers a full HashMap rebuild. Gate the generation bump on `Name` insertion specifically.

## Fix with
`/fix-issue 249`
