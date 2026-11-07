# REN-D8-04: the local grid's descriptor layouts are reflection-checked only at device boot, not in the static `reflect.rs` layout test

Labels: low,renderer,shaders,test-gap,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6752

**Severity**: LOW · **Dimension**: Volumetrics

## Location
`local_grid_layout_bindings` (set 1 of `volumetrics_local_sim.comp` and `combustion_phase_bake.comp`) in `crates/renderer/src/vulkan/volumetrics/local_grid.rs`; the `assert_layout!` list in `crates/renderer/src/vulkan/reflect.rs`

## Description
The static test lists `volumetrics`, `volumetrics_integrate`, `caustic` and `composite`, not the local grid. `inject_local_grid_bindings` (26..=31) is covered through the inject layout. Set 1 of the sim and bake shaders, and the sim's use of set 0, are validated only by `validate_set_layout(1, ..)` inside `LocalGrid::build`, which needs a Vulkan device.

## Evidence
`grep -rn "local_grid_layout_bindings" crates/renderer/src` returns only the definition and `build`; `reflect.rs` has no local-grid case. `LocalGrid::build` errors with "local grid layout drifted against volumetrics_local_sim.comp".

## Impact
A binding or type drift between the GLSL and the Rust layout passes `cargo test` and fails at engine start; a missing grid is fatal because composite requires `local_fire_views`.

## Suggested Fix
Add an `assert_layout!` for the local grid (set 1, both shaders), as for `exposure_meter` and `caustic`. This is a `cargo test`-visible shader contract, not a Vulkan change. `LocalSimPush` is a second hand-built block pinned only by string / offset tests (`push_block_in_the_shader_matches_the_encoder`, `push_constants_encode_boxes_then_counts_in_shader_order`), not by a size comparison with the compiled push block.

Related: REN-D3-01.

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Neighbouring unpinned contracts checked
- [ ] **TESTS**: The new pin fails when the wiring is removed (mutation-check it)
