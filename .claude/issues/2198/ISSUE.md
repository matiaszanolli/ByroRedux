# TD1-NEW-03: npc_spawn.rs re-crossed 2000 LOC after #2052's function-level fix (legitimate new AI-behavior code)

**Labels**: low, tech-debt, bug
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-07-25.md` (TD1-NEW-03)
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2198

## Severity
LOW

## Dimension
1 (File/Function/Module Complexity)

## Location
`byroredux/src/npc_spawn.rs` (2777 LOC total), `apply_ai_package_behavior` (228 LOC, at line 1593)

## Description
#2052 extracted `spawn_npc_entity` down to 828 LOC — that fix holds. The file re-crossed 2000 LOC anyway (2400→2777) because `apply_ai_package_behavior` (228 LOC) was added, consolidating a re-resolve-per-procedure pattern into a single-resolve dispatcher for Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol (itself the fix for PERF-D7-01/#2031). No function in the file is newly oversized.

## Evidence
`wc -l byroredux/src/npc_spawn.rs` → 2777; `apply_ai_package_behavior` at line 1593 (228 LOC); `spawn_npc_entity` confirmed at line 716 (828 LOC).

## Impact
None beyond the file taxing full-file reviews; no single function needs decomposition today.

## Related
Existing: #2052 (CLOSED, function-level, not regressed).

## Suggested Fix
No urgent action. If the file grows further, extract `apply_ai_package_behavior` and its seven `active_package_is_*` arms into a sibling module (e.g. `npc_spawn/ai_package.rs`).

## Completeness Checks
- [ ] **SIBLING**: If/when split, follow the same per-family module pattern used for `systems/{sandbox,wander,travel,follow,escort,guard,patrol}.rs`
- [ ] **TESTS**: N/A for this tracking note — existing tests already cover the consolidated dispatcher
