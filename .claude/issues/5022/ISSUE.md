# REN-D3-2026-09-29-01: GpuCamera.dof_params.w now carries a 0/1/2 history mode (#4942) but every description still says camera_static (1.0 = parked)

**Labels**: low,documentation,doc-rot,renderer,shaders

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-29.md`
**Severity**: LOW (doc-rot)
**Dimension**: GPU-Struct Layout
**Location**:
- Writer: `restir_history_mode` in `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs`.
- Readers: `triangle.frag` `dofParams.w > 1.5` (ReSTIR EMA) and `> 0.5` (GI seed).
- Stale text: the `GpuCamera::dof_params` rustdoc in `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (also says "Written in `vulkan/context/draw.rs`"); the five GLSL `CameraUBO` mirror comments (`include/bindings.glsl`, `triangle.vert`, `water.vert`, `cluster_cull.comp`, `caustic_splat.comp`); `docs/engine/shader-pipeline.md` GpuCamera row at offset 304; the `triangle.frag` GI-seed comment "(dofParams.w = camera_static)".

## Description
Since #4942 `GpuCamera.dof_params.w` carries a 0/1/2 history mode (0 moving, 1 parked, 2 parked + scene-static), but every description of the lane still says "camera_static (1.0 = parked)". Layout tests cannot see a semantic change to an existing lane. The rustdoc itself requires all five mirrors to carry the same comment, so all sites now describe a binary flag. A future reader keyed on `== 1.0` would silently miss the scene-static parked state.

## Impact
None today; a trap for the next reader of the lane.

## Related
#4942 (closed); #4870 (open GPU-struct doc-drift bundle, does not name this lane).

## Suggested Fix
Rewrite the texts to "w = history mode: 0 moving, 1 parked, 2 parked + scene-static (`restir_history_mode`)", and correct the writer location in the rustdoc.

Validated at HEAD 9fcfdc3fc: `restir_history_mode(camera_static, scene_static) -> f32` exists; `triangle.frag` reads `dofParams.w > 1.5`; all five GLSL mirror comments and `shader-pipeline.md` row 304 still say `camera_static (1.0 = parked)`.

## Completeness Checks
- [ ] **SIBLING**: all five GLSL `CameraUBO` mirrors updated in lockstep with the rustdoc
- [ ] **TESTS**: optionally extend the mirror-comment pin to cover the `w` lane text
