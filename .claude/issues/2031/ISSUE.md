# PERF-D7-01: NPC spawn re-resolves the same active AI package 14 times

**Labels**: medium, performance, bug

**Severity**: MEDIUM
**Dimension**: Streaming & Cells
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## Location
`byroredux/src/npc_spawn.rs:1443-1626` (fourteen `active_package_is_*`/`active_*_location`/`active_*_target` call sites), `crates/plugin/src/esm/records/misc/ai.rs:288-296` (`active_package`, the shared walk every call re-invokes from scratch)

## Description
Since M42.2-M42.8 landed, `spawn_npc_entity`'s tail independently calls a package-is-X check plus a location/target getter for each of the seven procedures (Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol) — 14 calls total, each re-running `active_package()`'s `find()`-with-CTDA-evaluation walk over `npc.ai_packages` from scratch. An NPC's active package is a single winning `PackRecord` by construction (an invariant independently confirmed by `AUDIT_ECS_2026-07-16.md`), so all 14 calls converge on the same answer — the walk-and-CTDA-evaluate work runs up to 14x more than necessary, and `condition_met` is not cheap (closes over the M47.1 scripting evaluator per CTDA entry on every rejected package).

Verified current: `byroredux/src/npc_spawn.rs` still has all 14 separate `active_package_is_*`/`active_*_location`/`active_*_target` call sites (lines ~1468-1620), each independently invoking the shared `active_package` walk.

## Impact
Bounded by `ai_packages.len()` (small on vanilla NPCs) × CTDA length × 14. Already fully captured inside the existing `npc_spawn_wall` metric (#1798) — this finding directly compounds that still-open gap (no per-frame interior spawn throttle exists).

## Related
#1798 (the throttle/timing finding this compounds); `AUDIT_ECS_2026-07-16.md`'s mutual-exclusivity finding (confirms the premise that all 14 calls converge on one package).

## Suggested Fix
Resolve `active_package(...)` once at the top of the spawn tail, then match its `procedure_type` against the seven `PROCEDURE_*` constants to build the one `Behavior` component directly — collapses 14 walks into 1. ~40-60 LOC, behavior-preserving, low risk.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (e.g. asserting `active_package` is invoked exactly once per spawn)
