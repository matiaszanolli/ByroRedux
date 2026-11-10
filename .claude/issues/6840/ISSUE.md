# PERF-2026-11-09-D5-03: the local sim hashes twelve words and issues three same-address global atomics per active cell, for a number only the debug log and the bench line read (unmeasured)

Labels: low,renderer,shaders,performance,bug
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: LOW — **unmeasured** (filed as a measure-first item). **Status**: NEW.

**Location**: `crates/renderer/shaders/volumetrics_local_sim.comp` (the `if (carriesCombustion(..))` block: 12 x `hashWord`, `atomicAdd(localGridStats[base+0], 1)`, `atomicXor(.., h)`, `atomicAdd(.., h)`); consumers `LocalGrid::begin_frame` / `fold_stats`, `note_volumetrics_state`, `byroredux/src/app_events.rs` and `byroredux/src/systems/debug.rs` log lines.

## Description
`local_grid_cells` and `local_grid_hash` exist to verify determinism and to print on the bench/stats line (`grep -rn local_grid_hash crates byroredux` finds only those logging sites and the struct). They are computed by every active cell of every box on every frame in release builds. The brick stores in the same block are real (composite reads them); the hash and atomics are not.

## Evidence
The atomics target three fixed words per box. Their values are divergent (`h` differs per cell), so they cannot be warp-aggregated at compile time the way a constant `atomicAdd(.., 1)` can. Recorded sim cost per active cell falls as the cell count rises (s1: 0.081 ms / 7,228 cells = 11 ns; s2a: 0.262 / 37,277 = 7 ns; stage-1b s3a: 1.252 / 272,145 = 4.6 ns), which fits a fixed serialised component but does not prove one. #5950 measured a different site (the inject's per-bin combustion-moment atomics, spread over 256 bins) as free; this does not re-open it.

## Impact
Not quantified; bounded by the sim's recorded cost (0.07-0.15 ms on current scenes), so at most a fraction of that.

## Measure first
`volumetrics_local_sim_ms` on s3a and s2a with the hash block stripped (keep the bricks), interleaved medians of 5. If below noise, close with the number.

## Suggested Fix (only if it matters)
Move the hash behind a `LOCAL_GRID_DEBUG` bit already carried in `localGrid.info.w`, or compute `active_cells` from the brick buffer on the host.

Report: `docs/audits/AUDIT_PERFORMANCE_2026-11-09.md`.

## Completeness Checks
- [ ] **MEASURED**: Before/after numbers from an interleaved A/B (tier pinned) are attached; close with the number if the delta is below noise
- [ ] **SIBLING**: Same pattern checked in related files (the determinism test/bench consumers of `local_grid_hash`)

