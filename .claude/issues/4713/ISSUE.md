# GAME-D7-2026-09-21-04: ItemEventBatch is written and drained but read by nothing; its doc names consumers that do not exist

**Issue**: #4713
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: LOW
**Dimension**: 7 — written-never-read
**Location**: `crates/scripting/src/events.rs`; producers `crates/scripting/src/equipment.rs`; `crates/scripting/src/cleanup.rs`

## Description
Emitted on both sides of every loot/pickup, drained at Late; no system reads it. Doc claims "notification UI, quest fragments, and future crime systems all observe it" — none do.

## Evidence
`rg ItemEventBatch` shows definition, emit, cleanup, tests only.

## Impact
Dead plumbing presented as a live script event.

## Related
#4414 (same "documented live event, zero readers" class).

## Suggested Fix
Wire a real consumer, or correct the doc to say "no consumer yet."
