# PERF-D7-01: Worldspace-persistent-cell load bypasses the new resumable/budgeted streaming architecture entirely

Filed from: `docs/audits/AUDIT_PERFORMANCE_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2275
Labels: high, performance, bug

**Severity**: HIGH
**Dimension**: World Streaming & Cell Transitions (7)

**Location**: `byroredux/src/scene/world_setup.rs:511-523` → `byroredux/src/cell_loader/exterior.rs:82` (`load_worldspace_persistent_cell`, `load_references` call at :111) → `byroredux/src/cell_loader/references/mod.rs:158` (`FrameTimeBudget::unlimited()`)

## Description
`world_setup.rs`'s "foreground-first" bootstrap comment (lines 501-510) claims `stream_initial_radius` "blocks only for the center cell," but before it even computes the load deltas, it synchronously calls `load_worldspace_persistent_cell` for every globally-persistent REFR within `radius_load` cells of the spawn point. That call reaches `load_references(..)`, which still hard-codes `FrameTimeBudget::unlimited()` — the same unbudgeted primitive the new resumable-NPC-assembly work (`9bf4c493`) was built specifically to replace. Persistent-cell REFRs include NPCs (Whiterun's guards, tavern keepers, etc.), which now route through the brand-new cooperative `NpcSpawnJob` state machine, but driven to completion with zero yields.

## Evidence
- `world_setup.rs:511-514`: `if state.persistent_root.is_none() { ... state.persistent_root = crate::cell_loader::load_worldspace_persistent_cell(...); }` — runs synchronously before `compute_streaming_deltas`.
- `cell_loader/exterior.rs:111`: `load_worldspace_persistent_cell` calls `load_references(&local_refs, ...)` directly (no budget parameter, no async dispatch).
- `cell_loader/references/mod.rs:158`: `pub(super) fn load_references(...) { let mut budget = FrameTimeBudget::unlimited(); ... }` — confirmed still hard-coded.
- `resumable.rs`'s own module doc calls this class of stall ("the largest remaining EXAL apply outlier") the exact motivation for the new machinery, yet this specific call site was not migrated to it.

## Impact
Cost scales with the streaming radius footprint (persistent NPC density across N cells), not the single arrival cell — the render thread stalls for the full synchronous cost of every persistent-cell NPC assembly at boot/fast-travel, on exactly the class of exterior scene (dense Bethesda settlement) the "foreground-first" work targeted.

## Related
Session 62-63 streaming rearchitecture (`67081437`/`484893de`/`9926fa50`/`9bf4c493`). Conceptually adjacent to #1798 (interior `load_references` call site has the same "budget primitive exists, this call site doesn't use it" gap) — note #1798 is currently closed on GitHub despite the code path it describes still being unbudgeted per this session's re-verification; worth a look independent of this issue.

## Suggested Fix
Thread a real `FrameTimeBudget` through `load_worldspace_persistent_cell`, or dispatch it through the same async worker/apply pipeline used for regular streamed cells instead of running it inline during bootstrap.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the interior `load_cell_with_masters` call site (`cell_loader/load.rs:404`, tracked separately as #1798)
- [ ] **TESTS**: A regression test pins this specific fix (e.g. asserting `load_worldspace_persistent_cell` yields/budgets rather than running to completion inline)
