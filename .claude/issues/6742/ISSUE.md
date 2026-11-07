# REN-D3-01: `GpuLocalGridParams` (the `LocalGridParams` UBO) has no layout or lane guard against its GLSL block

Labels: low,renderer,test-gap,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6742

**Severity**: LOW · **Dimension**: GPU-Struct Layout

## Location
`crates/renderer/src/vulkan/volumetrics/local_grid.rs` (`GpuLocalGridParams`), `crates/renderer/shaders/include/local_grid_params.glsl` (`uniform LocalGridParams`)

## Description
The local fire grid added a hand-mirrored std140 block (`boxes: [[f32;4]; MAX_LOCAL_BOXES]`, `info: [u32;4]`, `sun_dir`, `sun_radiance`, `phase`, `render_origin`; 144 B), bound by the inject pass (binding 26) and composite (binding 12). `every_remaining_uniform_block_size_matches_its_host_struct` (`crates/renderer/src/vulkan/reflect.rs`) has 8 cases and omits it; `every_shader_struct_is_classified` scans only `struct` and push-constant blocks, so a `uniform` block is outside it.

## Evidence
By hand the layout matches today (4x16 + 5x16 = 144 B; `MAX_LOCAL_BOXES = LOCAL_GRID_MAX_BOXES` is single-sourced). No `src` test mentions `LocalGridParams`. All members are vec4-class, the lane-transposition class `reflect.rs` already guards for `VolumetricsParams`.

## Impact
No bug now. A lane edit on one side silently misroutes box origins, masks or sun phase in both consumers.

## Suggested Fix
Add `LocalGridParams` to the `reflect.rs` size table and a `uniform_block_member_offsets_by_name` pin against `offset_of!(GpuLocalGridParams, ..)`, as the `VolumetricsParams` pin does (both helpers exist).

Related: REN-D8-04 (the local grid's descriptor layouts).

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Neighbouring unpinned contracts checked
- [ ] **TESTS**: The new pin fails when the wiring is removed (mutation-check it)
