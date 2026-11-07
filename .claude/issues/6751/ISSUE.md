# REN-D8-03: nothing pins the local grid's skip and completion wiring (the #4780 class for the new state)

Labels: low,renderer,test-gap,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6751

**Severity**: LOW · **Dimension**: Volumetrics

## Location
`VolumetricsPipeline::record_skipped_frame`, `VolumetricsPipeline::mark_frame_completed`, `record_volumetrics_pass` (the `if !ran { vol.clear_local_grid_params(..) }` arm) in `crates/renderer/src/vulkan/volumetrics.rs` / `context/post_passes.rs`

## Description
The wiring is correct today. A skipped or failed dispatch calls `grid.invalidate_history()` (through `record_skipped_frame`, which `record_neutral_frame` also runs) and rewrites the slot's table with `clear_local_grid_params`, so composite cannot march a stale atlas. `mark_frame_completed` promotes box history only after submit. None of the three lines is named by a test: `every_skipped_frame_drops_the_temporal_history_not_just_the_first` lists seven lanes and not the grid; `record_volumetrics_pass_routes_skip_clears_through_the_shared_latch` checks only the latch and `record_neutral_frame`.

## Evidence
`grep -rn "clear_local_grid_params\|mark_frame_completed" crates/renderer/src` finds no assertion on them. The `LocalBoxSet` unit tests cover `invalidate_history` and `commit_history` in isolation only.

## Impact
Deleting any of the three lines silently brings back a stale-history or stale-atlas march (flicker, or fire frozen from a skipped frame) with every test green.

## Suggested Fix
Add the three needles to the #4780 source-shape test (`production_text`), as it does for the existing lanes.

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Neighbouring unpinned contracts checked
- [ ] **TESTS**: The new pin fails when the wiring is removed (mutation-check it)
