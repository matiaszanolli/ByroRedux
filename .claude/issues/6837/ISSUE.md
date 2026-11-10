# PERF-2026-11-09-D3-01: the BLAS residency reservation and the memory-budget per-preset table bill 44 B per froxel per slot, but the local-grid irradiance field adds 16 B per froxel per slot and the atlas a fixed 48 MiB

Labels: low,renderer,memory,performance,bug
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: LOW (accounting gap; no reachable effect on the RT-minimum card — see Impact). **Status**: NEW.

**Location**: `screen_scaled_reservation_bytes` (`crates/renderer/src/vulkan/acceleration/predicates.rs`, `froxels * FROXEL_BYTES_PER_SLOT * MAX_FRAMES_IN_FLIGHT`); `FROXEL_BYTES_PER_SLOT = 44` (`crates/renderer/src/vulkan/volumetrics.rs`); `irradiance_resident_bytes` / `local_grid_resident_bytes` (`volumetrics/local_grid.rs`, called only by the boot log and tests); `docs/engine/memory-budget.md` "Volumetrics (M55)" tables.

## Description
`LocalGrid::build` allocates, per frame in flight, an RGBA16F irradiance image of `2 x froxel width` and, once, three RGBA16F `64 x 64 x 256` atlas volumes. Both are resident whenever the volumetrics pipeline exists, live boxes or not. Neither is in `screen_scaled_reservation_bytes` (which reads `FROXEL_BYTES_PER_SLOT` only) nor in `render_extent_bytes_per_pixel_x1024`, and `grep -rn -i "irradiance\|local_grid" acceleration/` returns nothing. The doc states the costs in prose but the per-preset table ("1080p native ~183 MB", "4K native ~730 MB") and the "Fixed floor" column omit them.

## Evidence (derived from the constants, default divisor 8, 64 slices)
Irradiance bytes = froxels x 32 (16 B x 2 FIF): 1280x720 = 29.5 MB; 1920x1080 = 66.4 MB; 2560x1440 = 118.0 MB; 3840x2160 = 265.4 MB. Atlas: 48 MiB = 50.3 MB fixed. Missing from the reservation: 116.7 MB at 1080p native and 315.7 MB at 4K native. The BLAS budget is `(heap - reservation) / 3` clamped to [256 MiB, 1 GiB], so the overstatement of the budget is a third of that (~39 MB at 1080p, ~105 MB at 4K).

## Impact
The budget only moves when `(heap - reservation) / 3 < 1 GiB`. On the 12 GB dev card and the 6 GB RT minimum, at 1080p and 4K native, the clamp wins, so nothing changes today. It becomes real on a heap near 5 GiB at 4K native, and it is the same "new screen-sized allocation missing from the reservation" regression class #3992 / #6035 closed twice. The doc table is wrong now.

## Suggested Fix
Add an `IRRADIANCE_BYTES_PER_FROXEL_PER_SLOT = 16` constant beside `FROXEL_BYTES_PER_SLOT`, bill it (and `local_grid_resident_bytes()`) in `screen_scaled_reservation_bytes`, extend `froxel_grid_cost_matches_the_memory_budget_doc` to pin the new row, and add the irradiance and atlas rows to the per-preset table.

Related: #3992, #6035, #6827 (same image's 3D-dimension limit), #6533. Report: `docs/audits/AUDIT_PERFORMANCE_2026-11-09.md`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (any other resident volumetrics/local-grid allocation absent from the reservation)
- [ ] **TESTS**: A regression test pins this specific fix

