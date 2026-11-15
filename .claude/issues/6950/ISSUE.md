# #6950: TD1-2026-11-15-04: `advance_runtime_unit` is a 383-line, 8-deep nine-phase state machine in one `match`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6950
- **Labels**: low,gameplay,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (fn >200, nesting >5)
- **Age**: unchanged since #5091 (2026-10-01) and last edited 2026-10-28 (477124477). It was never named by a tech-debt report.
- **Effort**: small
- **Labels**: `tech-debt`
- **Location**: `byroredux/src/npc_spawn/resumable/runtime.rs:642-1030`
- **Status**: NEW
- **Description**: `match state.phase` has nine arms (`Skeleton` `:656`, `Body(i)` `:707`, `Head` `:792`, `Hair` `:807`, `Brow` `:841`, `Eye(i)` `:860`, `HeadSubPart(i)` `:909`, `Armor(i)` `:935`, `Finalize` `:959`). Two of them (`Skeleton` ~50 lines, `Body` ~85) are large; the deepest nesting I saw is inside the `Body` arm (`:818`, 36-column indent).
- **Impact**: Every new NPC-assembly stage or per-game race rule lands in this one function.
- **Suggested Fix**: One `fn advance_<phase>(state, world, ctx, ..) -> UnitOutcome` per arm in the same file (or `resumable/phases.rs`); `advance_runtime_unit` becomes the 20-line dispatcher. The step-per-frame resumable contract and the per-phase `UnitOutcome` return are unchanged. Pin the dispatcher (`scheduler_access_tests.rs` already pins `advance_actor_synth` at 151).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
