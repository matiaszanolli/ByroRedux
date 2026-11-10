# SAFE-2026-11-09-02: host workgroup sizes for the local-grid compute passes are unpinned literals (`LOCAL_SIM_WORKGROUP`, `PHASE_BAKE_WORKGROUP`)

Labels: low,safety,renderer,shaders,test-gap,bug
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: LOW. **Status**: NEW (the class was #5620 / #2768, both closed and cured by generated `WORKGROUP_*` constants; these two shaders re-introduced bare literals).

**Location**: `crates/renderer/src/vulkan/volumetrics/local_grid.rs:52` (`PHASE_BAKE_WORKGROUP = 64`), `:66` (`LOCAL_SIM_WORKGROUP = 4`), dispatch sites `:991`, `:1243`; shaders `volumetrics_local_sim.comp:29` (`local_size_x/y/z = 4`), `combustion_phase_bake.comp:19` (`local_size_x = 64`).

## Description
The froxel passes take `WORKGROUP_X/Y/Z` from generated shader constants and divide by the same Rust constants. The local sim and the phase bake hard-code `local_size` in GLSL and re-type it as private Rust constants (`groups = LOCAL_GRID_CELLS / LOCAL_SIM_WORKGROUP`, `PHASE_TABLE_ROWS` for the Y count). No test links them: the reflection test pins descriptor bindings, not `local_size`; `push_block_in_the_shader_matches_the_encoder` pins only the push block.

## Evidence
`grep -rn "LOCAL_SIM_WORKGROUP\|PHASE_BAKE_WORKGROUP" crates` shows only the definitions and the two dispatch sites; `shader_constants_gen.rs` has no entry for either.

## Impact
Not memory-unsafe (the shader bounds-checks `global.x/y >= dim` and `box >= push.info.x`). Drift shows as an under-covered dispatch — part of an atlas slab or the phase table silently never written — which looks like a rendering bug.

## Suggested Fix
Emit both constants through the shader-constants generator and use them in both places, or add a SPIR-V reflection assertion on the `LocalSize` execution mode.

Related: #5620, #2768. Report: `docs/audits/AUDIT_SAFETY_2026-11-09.md`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (any other compute pass with a literal `local_size`)
- [ ] **TESTS**: A regression test pins this specific fix

