# EXT-D6-2026-10-19-02: The `Starved` verdict never settles the boundary telemetry; the grid-cross bench clock freezes and the `lod-coverage` gate still reads `settled=1`

Labels: medium,terrain-exterior,tech-debt,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6093

**Severity**: MEDIUM (dev / harness path only, but it is the acceptance harness, and the hang is introduced by the fix for #6054) · **Dimension**: Distant LOD and trees (and the Dim 7 gate semantics) · **Games**: FNV, FO4 (any saturating radius); bench harness only

## Location
- `byroredux/src/app_step.rs` (only the `Settled` arm calls `state.telemetry.settle_lod`)
- `byroredux/src/streaming/telemetry.rs` (`boundary_in_progress`, `settle_lod`, `clear_completed_boundary`)
- `byroredux/src/bench_camera.rs`, `byroredux/src/app_events.rs` (the grid-cross logical clock)
- `byroredux/src/streaming_helpers.rs` (`update_lod_coverage(world, state, complete)` / `coverage.settled = complete`)

## Description
`begin_boundary` opens `ActiveBoundaryTelemetry { full_detail_settled: false, lod_settled: false }`. It is cleared only when both flags are true; `lod_settled` is set only by `settle_lod`, which `app_step.rs` calls solely in the `Settled` arm. `Starved` clears `lod_reconcile_pending`, so no later frame reaches the reconcile: `lod_settled` stays false for ever, so `boundary_in_progress()` stays true until another `grid_changed` replaces it.

The `--bench-camera grid-cross` / `grid-soak` clock holds the path frame while a boundary is in progress, and `grid_cross_complete` requires `!boundary_in_progress`. The camera therefore never moves to the next boundary and the bench never prints its summary.

The same ring is also invisible to the `lod-coverage` line: `reconcile_lod_rings` computes `complete` before it knows the verdict and writes `coverage.settled = complete` (`true` for a starved ring), so `m-exteriors.sh`-style gates keep reading `settled=1` for a horizon that lost 352 quads.

## Evidence
- `grep -rn 'settle_lod\|lod_settled' byroredux/src` outside `telemetry.rs`: only `app_step.rs` (Settled arm) and a test.
- The test `a_capacity_starved_lod_reconcile_is_not_reported_settled` pins `lod_settle_verdict` only, not what the caller does with `Starved`.
- The door-exit arrival has no active boundary (`last_player_grid` is pre-seeded), which is why #6054's own repro did not hang; a crossing with an active boundary does.

## Impact
A radius-12 FNV / FO4 `grid-cross` or `grid-soak` run that saturates the pool never finishes (until killed). The bench line reports `unsettled_lod=1`, and the next real crossing bumps `superseded_lod` for a boundary that was merely stuck.

## Related
EXT-D6-2026-10-19-01, #6054.

## Suggested Fix
Give the starved verdict an explicit telemetry outcome (settle it with a `starved` flag, or count it as its own terminal state) so the boundary closes; carry `Starved` into `LodCoverageStats` (a `starved`/`rejected` counter the gate verdict can fail on). Add a caller-level test that a `Starved` progress closes the telemetry boundary.

Source: docs/audits/AUDIT_EXTERIOR_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
