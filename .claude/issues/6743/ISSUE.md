# REN-D3-02: lane and struct-sync comments contradict live code in six places

Labels: low,renderer,shaders,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6743

**Severity**: LOW · **Dimension**: GPU-Struct Layout (semantic lane changes the layout tests cannot see)

## Location
1. `crates/renderer/shaders/include/camera_ubo_members.glsl`, the `renderDebug` comment ("z = LOD telemetry enable"; live bits 0 = RT-LOD telemetry, 1 = `--rt-test-no-gi-clamp`, 2 = `--rt-test-no-reservoir-clamp`)
2. `GpuCamera.render_debug` rustdoc in `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (bits 0-1 only)
3. `VulkanContext.render_debug_flags` rustdoc in `crates/renderer/src/vulkan/context/mod.rs` (catalogue stops at `0x200` and calls it "reserved"; it is `DBG_VIZ_AO`)
4. `crates/renderer/shaders/include/volumetrics_bindings.glsl`, the `vec4 wind_gust` comment ("Remaining lanes are reserved"; `.z` = `BYRO_SUN_ALPHA`, `.w` = `BYRO_SUN_DUST` strength are live)
5. `GpuLocalGridParams.info` rustdoc ("w unused"; it carries `LOCAL_GRID_DEBUG_*` bits)
6. `docs/engine/renderer.md` "Shader Struct Sync (CRITICAL)" (still "six GLSL sites"; since #6681/#6682 `GpuInstance` and `CameraUBO` members are declared once)

## Description
Six comments / docs describe lane layouts or declaration counts that the live code no longer has (listed under Location).

## Evidence
Each read at the symbol named; item 6 is `grep -n "six GLSL" docs/engine/renderer.md`.

## Impact
Doc rot of exactly the "semantic lane change" kind the layout tests cannot catch. A future author reading the GLSL comment could repurpose `renderDebug.z` bit 2, `wind_gust.z/.w` or `info.w` as free.

## Suggested Fix
Update the six sites. Item 1 once, in `camera_ubo_members.glsl` (the `dof_w` history-mode substring test matches a different member and is unaffected). Either extend the `render_debug_flags` catalogue through `0x80000000` or point it at `DBG_BITS`.

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Same wording checked in related files (docs, other comments)
- [ ] **TESTS**: Covered by existing guards / `.claude/commands/_audit-validate.sh`; no new test needed for pure comment edits
