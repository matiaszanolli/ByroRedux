# #5418: ECS-2026-10-08-D7-02: `detach_victims_from_surviving_parents` takes one `Parent` lock per victim and builds a full victim set on every cell unload; its doc says it costs one query

**Labels**: low,ecs,performance,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5418

**Source**: `docs/audits/AUDIT_ECS_2026-10-08.md` — `ECS-2026-10-08-D7-02` (HEAD `00f580e09`)

- **Severity**: LOW. The cost is per streaming boundary, not per frame. Owner overlap: `/audit-performance`.
- **Dimension**: 7 — Erased removal cost
- **Location**: `byroredux/src/cell_loader/unload.rs:527-570`
- **Status**: NEW (introduced by `ec0e0c8b4`, #5310)
- **Description**: The function is correct, but its cost does not match its doc:
  - It allocates a `HashSet` of all victims unconditionally.
  - It then calls `world.get::<Parent>(victim)` once per victim. Each call is a full read-lock acquire and release, including lock-tracker bookkeeping.

  The doc says *"Whole-chain teardowns (cell unload) pay one `Parent` query and no-op."* In a real cell, most victims are subtree nodes that do have a `Parent`, so every unload pays O(victims) lock round-trips plus the set build. The 2026-09 boundary work (#3689, #4616) treated exactly this class of per-victim cost as a regression.
- **Suggested Fix**: Take `world.query::<Parent>()` once and iterate the victims under that single guard. Build `victim_set` only after the first victim whose parent is not obviously in the sweep, or test membership with a binary search on the sorted victim slice (`drain_cell_victims` already sorts it). Correct the doc.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
