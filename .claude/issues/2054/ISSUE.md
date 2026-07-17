# TD1-005: records/misc/ai.rs crossed 2000 LOC — bundles 6 unrelated record families + 1013 lines of tests

**GitHub Issue**: #2054
**Labels**: low,import-pipeline,legacy-compat,tech-debt,bug

**Severity**: LOW
**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `crates/plugin/src/esm/records/misc/ai.rs`

## Description
Every other `misc/` sibling holds exactly one record family. `ai.rs` bundles PACK (605 LOC, incl. the 7 `active_package_is_*` selectors), QUST (240 LOC), DIAL/INFO/MESG (325 LOC), CSTY, IDLE, plus a combined 1013-line test module.

## Evidence
Confirmed live: `crates/plugin/src/esm/records/misc/ai.rs` is 2260 LOC total, matching the report's figure.

## Impact
A change to quest-stage parsing requires navigating a 2260-line file that also holds the hot, frequently-touched AI-package selector logic.

## Suggested Fix
Split into `misc/pack.rs`, `misc/quest.rs`, `misc/dialogue.rs`; fold CSTY+IDLE into `misc/character.rs`.

**Age**: file created 2026-05-12, last touched 2026-07-16 (same day as npc_spawn.rs).
**Effort**: medium

## Completeness Checks
- [ ] **SIBLING**: Match the established one-family-per-file `misc/` convention every other sibling file already follows
- [ ] **TESTS**: Split preserves all existing per-record test coverage; no test content changes, only module location
