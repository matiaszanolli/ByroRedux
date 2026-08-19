# REN-D16-04: sampleLocalMedium's cluster lookup has only a single shader early-out protecting against a stale cell's smoke replaying

**Dimension**: 16 (Volumetrics)
**Location**: `crates/renderer/shaders/volumetrics_inject.comp` (`sampleLocalMedium`, line 165)
**Status**: NEW

**Description**: `sampleLocalMedium`'s only guard against reading a stale cluster (e.g. one referencing fog volumes from a cell that was already unloaded, before the cluster buffer for the new cell has been fully rebuilt) is the single `fogVolumeCount == 0u || params.local_volume_grid.w <= 0.0` early-out at the top of the function. There is no explicit invalidation/versioning of the cluster buffer itself between cell loads, so this early-out is the sole line of defense against replaying a previous cell's local fog.

**Impact**: A timing window between a cell unload and the next cell's cluster-buffer rebuild could replay stale local-fog data if the single early-out condition doesn't happen to trip (e.g. `fogVolumeCount` is nonzero from the old cell and the grid bounds haven't been reset yet).

**Suggested Fix**: add an explicit cluster-buffer generation/version check (or a forced clear on cell transition) rather than relying solely on the count/grid-bounds early-out.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

