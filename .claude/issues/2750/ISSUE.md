# REN-D3-2026-08-12-02: GpuCamera::dof_params.zw carries live data but is documented reserved(0) in the Rust struct and 4 of 5 GLSL mirrors

- **Severity**: MEDIUM
- **Dimension**: 3 — GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuCamera::dof_params`); `crates/renderer/shaders/triangle.vert`, `water.vert`, `cluster_cull.comp`, `caustic_splat.comp` (their `CameraUBO` `dofParams` declarations); writer `crates/renderer/src/vulkan/context/draw.rs`; readers `crates/renderer/shaders/include/lighting.glsl`, `crates/renderer/shaders/triangle.frag`

## Description
The Rust doc reads *"x = aperture half-radius, y = focal distance, **zw = reserved (0)** … Available to shaders for future screen-space DOF … without an extra UBO binding."* Both lanes are live: `.z` = `light_atten_knee` (the #1451 point/spot attenuation knee, live-tunable via the `light.atten` console command), `.w` = `camera_static`. Byte layout is correct everywhere — this is a **semantic** lockstep break, and the wording is not merely stale but an active invitation to repurpose a lane that has two consumers. NEW as a code-side issue: a 2026-07-09 audit already fixed the same rot in `docs/engine/shader-pipeline.md`; the in-code sites were never corrected.

## Evidence
Writer: `dof_params: [active_dof.aperture, active_dof.focus_dist, self.light_atten_knee, if camera_static { 1.0 } else { 0.0 }]`. Readers: `crates/renderer/shaders/include/lighting.glsl` — `float kneeFrac = (dofParams.z > 0.0001) ? dofParams.z : 0.5;`; `triangle.frag` — `bool cameraStatic = dofParams.w > 0.5;` and `float giSeed = dofParams.w > 0.5 ? frameCount : floor(frameCount * 0.25);`. Declaration-site audit: `crates/renderer/shaders/include/bindings.glsl` is **correct**; `triangle.vert`, `water.vert`, `cluster_cull.comp`, `caustic_splat.comp` and `gpu_types.rs` all say `zw = reserved`.

Spot-checked against live code during publish: confirmed `bindings.glsl:232` says "z = atten knee frac, w = camera_static" while `triangle.vert:92`, `water.vert:82`, `cluster_cull.comp:68`, `caustic_splat.comp:75` and `gpu_types.rs` `dof_params` doc all still say "zw = reserved".

## Impact
No runtime effect today. The failure mode is a future author trusting the Rust struct doc or 4 of 5 GLSL mirrors, treating `.z`/`.w` as free, and silently breaking point/spot attenuation shaping plus the parked-camera GI-seed decorrelation that makes indirect lighting converge. The exact trap the codebase already burned on with `_pad_id0` → `ior`.

## Related
#2164, #1928 (`VolumetricsParams::render_origin.w` overload — same class), #1451, #2483 / #2433 / #2415.

## Suggested Fix
Replace `zw = reserved (0)` in `gpu_types.rs` with the `crates/renderer/shaders/include/bindings.glsl` wording plus the consumer list, and propagate the same one-line comment to the four standalone `CameraUBO` mirrors. Comment-only — no `.spv` recompile, so `scripts/check-shader-artifacts.sh` is unaffected.

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2750
