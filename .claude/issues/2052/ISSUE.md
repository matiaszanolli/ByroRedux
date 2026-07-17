# TD1-003: npc_spawn.rs crossed 2000 LOC — spawn_npc_entity is a 1045-LOC function mixing 6 unrelated concerns

**GitHub Issue**: #2052
**Labels**: low,import-pipeline,tech-debt,bug

**Severity**: LOW
**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `byroredux/src/npc_spawn.rs:671-1815` (`spawn_npc_entity`), `:1813-2400` (`spawn_prebaked_npc_entity`)

## Description
Six numbered phases (placement root / skeleton / body / head+hair / equipment / idle animation / AI-package gating) live in one function body, threaded through shared local state. A second function duplicates a parallel, shorter phase sequence for the pre-baked-mesh path — the two have already partially diverged (kf path handles AI-package gating; prebaked path currently does not).

## Evidence
Confirmed live: `byroredux/src/npc_spawn.rs` is 2400 LOC total; `pub fn spawn_npc_entity(` starts at line 671, `pub fn spawn_prebaked_npc_entity(` starts at line 1813 — matching the report's claimed ranges exactly.

## Impact
Any change to one phase requires reviewing the entire 1045-line function for side effects on shared state.

## Suggested Fix
Extract each phase into a private helper; let `spawn_prebaked_npc_entity` share the equipment/skeleton helpers instead of re-implementing a parallel list.

**Age**: file created 2026-04-28, last touched 2026-07-16 — actively growing.
**Effort**: medium

## Completeness Checks
- [ ] **SIBLING**: `spawn_prebaked_npc_entity` currently diverges from `spawn_npc_entity` (missing AI-package gating) — the split should close this gap, not just organize code
- [ ] **TESTS**: A regression test pins that both spawn paths produce equivalent entities post-refactor (skeleton/body/head/equipment parity)
