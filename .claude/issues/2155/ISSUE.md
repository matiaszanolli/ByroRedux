# 2155: CONC-D4-NEW-03: ABBA detector coverage is bounded by test reachability — the long tail cannot be declared closed

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2155
**Labels**: bug, low, sync

---

## Severity
LOW

## Dimension
Scheduler Access Declarations — `/audit-concurrency` 2026-07-25

## Location
`crates/core/src/ecs/lock_tracker.rs:194-300`

## Description
The cross-thread ABBA graph only records an edge when a lock is acquired while another is already held on a thread a test actually drives. Code paths with no test coverage contribute zero edges — neither cleared nor flagged. A static workspace-wide scan found 849 distinct ordered lock-acquisition pairs (140 appearing in both directions somewhere in the tree) — mostly false positives since the scan doesn't model guard lifetimes, but it shows the acquisition surface is far wider than what the green test run proves.

## Evidence
Concrete uncovered-or-thinly-covered regions with multi-lock functions: `byroredux/src/cell_loader/` (needs real ESM/BSA data), `byroredux/src/render/` collection passes, `byroredux/src/save_io.rs`, `byroredux/src/npc_spawn.rs`, `byroredux/src/scene.rs::setup_scene`, and most of `byroredux/src/commands/` (only 2 of ~10 modules were touched by `b5e38c22`).

## Impact
Low today — those paths are predominantly single-threaded (cell/scene load on main thread outside the scheduler; console commands inside the exclusive `DebugDrainSystem`), so an inconsistent order there is latent rather than live. Becomes real the moment any is promoted into the parallel batch or moved to a loader thread. This is the same structural gap that let CONC-D5-01/-02/-03 (#2134, #2135, and the sibling filed above) go undetected.

## Related
#1410 (closed, TS-02), `b5e38c22`, CONC-D4-NEW-01 (#2136), CONC-D5-01 (#2134), CONC-D5-02 (#2135).

## Suggested Fix
Fixing CONC-D4-NEW-01 (#2136 — detector on during the live 5-frame bench) is the highest-yield next step, since it covers loader/render/scene paths unit tests cannot reach. Optionally record, in a comment near `global_order`, that clearance is coverage-bounded so a future audit doesn't read a green job as proof of absence.

## Completeness Checks
- [ ] **TESTS**: Standing note, not directly test-pinnable — closes as a side effect of #2136 landing
