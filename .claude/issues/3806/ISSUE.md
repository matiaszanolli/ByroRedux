# #3806 — EX-16: boundary/soak test harness — actor path crossing a cell edge during unload/reload

State: OPEN
Labels: enhancement, ai, terrain-exterior, test-gap

Split from #2372 (EX-16 plan, acceptance criterion 6).

## Criterion (verbatim from #2372)
"Boundary and soak tests cover unload/reload while an actor path crosses a
cell edge."

## Current status (verified 2026-08-31): NOT DONE — no substrate to test against yet

`streaming_tests.rs` has cell load/unload tests
(`moving_one_cell_loads_three_unloads_zero`,
`moving_two_cells_loads_and_unloads`), but none involve actors or NAVM
paths. `byroredux/src/systems/navmesh_path.rs` has only synthetic unit
tests of `residency_generation` invalidation with fabricated generation
counters — not a real `unload_cell`/reload integration harness with an
actor mid-path. No test file combines boundary + soak + actor + cell-edge
+ NAVM semantics.

## Blocked on
This criterion has no substrate to test against until both of these land:
- #3802 — cross-tile NAVM path connectivity (an actor "crossing a cell
  edge" mid-path is meaningless without cross-tile pathing existing)
- #3803 — actor/package suspend/migrate/resume across stream boundaries
  (the thing this test suite is meant to catch regressions in)

## Scope for this issue (once unblocked)
1. A boundary-crossing integration test: an actor with a live NAVM path
   spanning two adjacent cells, one of which unloads mid-path (simulating
   the player moving away and back) — must resume without a dangling path,
   duplicate entity, or lost package state.
2. A soak variant: repeated load/unload cycles across many boundary
   crossings (mirroring the existing streaming soak-test shape, if one
   exists — check `streaming_tests.rs` / any `--bench` harness for a
   precedent) to catch slow leaks (entity count, GPU handle count,
   `NifImportRegistry` growth) that a single-crossing test wouldn't.
3. Regression-pin `LodCoverageStats`/`TerrainSeamStats`-shaped invariants
   specific to actor/NAVM residency, if #3803/#3802 introduce any new
   trackable state worth a live audit gate (mirroring the precedent
   `LodCoverageStats::vwd_full_model_overlaps` set for VWD culling, #3307).

## Related
#2372 (parent plan issue). Blocked on #3802 and #3803 — do not start this
until at least one of them has landed enough that there's real behavior to
pin.
