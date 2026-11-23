# #7388 — REN-D8-2026-11-23-01: `BYRO_FREEZE_FROXEL_PATTERN=1` also stops the combustion simulation and the flame-source animation, not just the noise phase

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D8-01). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW
- **Dimension**: Volumetrics (diagnostic switch contract)
- **Location**: `VolumetricsParams` assembly in `record_volumetrics_pass` (`context/post_passes.rs`, the `temporal_params` lane z), `freeze_froxel_pattern` (`volumetrics.rs`), `VolumetricsPipeline::dispatch` (`simulation_time` / `simulation_dt`), `combustion_sources.glsl` (`timeSeconds`), `combustion_transport.glsl` (`phase`, the wave-number terms)
- **Status**: NEW
- **Description**: 0c61c4e7c implements the switch by uploading `0.0` instead of `volumetric_time_seconds` in `temporal_params[2]`, and documents it as "holding procedural density noise at its initial phase". The same lane is the clock of the whole combustion model: `dispatch` derives `simulation_time` from `temporal_params[2]` and `simulation_dt = (simulation_time - last_simulation_time_seconds).clamp(0, 1/15)`, which is 0 on every frame after the first once the clock is pinned; `transport_armed` is `combustion_active && simulation_dt > 0.0`, and `transport_simulation_dt(combustion_active, simulation_dt)` feeds the transport step. The shaders also read `params.temporal_params.z` as `timeSeconds` for the emitter flicker (`combustion_sources.glsl`) and for the transport's turbulence phase and wave terms (`combustion_transport.glsl`). With the switch set, a cell with fire or smoke therefore stops evolving after its first seeded frame; it is not a "same scene, frozen noise" comparison.
- **Evidence**: `temporal_params: [DEFAULT_TEMPORAL_HISTORY_WEIGHT, DEFAULT_DENSITY_REJECTION, if freeze_froxel_pattern() { 0.0 } else { volumetric_time_seconds }, fog_coverage.clamp(0.01, 1.0)]`; `let simulation_time = if frame_params.temporal_params[2].is_finite() { frame_params.temporal_params[2].max(0.0) } ...`.
- **Impact**: Off by default; the shipped path is untouched (`freeze_froxel_pattern()` is false unless the variable is exactly "1"). Only a comparison capture that sets it in a cell with a live fire source would be misled: the fire looks frozen or absent, and the local grid's box (driven by the same sim time) holds its first state.
- **Related**: ac558feb7 / 0c61c4e7c capture diagnostics; combustion local grid #6531/#6533.
- **Suggested Fix**: either document the switch as "freezes all volumetric time, combustion included", or carry the noise clock in its own lane and leave `simulation_time` on the real clock (host and shaders then agree on which time each term reads).

Observations (no ID, not filed; the rest of the dimension traced clean):
- `fog_height_reference` (`byroredux/src/render/mod.rs`): the exterior ground probe is a physics ray (`PhysicsWorld::cast_ray_down`) of `DEFAULT_RENDER_DISTANCE` (400,000 BU), so it answers only for loaded colliders. Beyond the streamed terrain (or over open water with no collider) it still falls back to the camera's own Y exactly as before ac558feb7; the commit narrows the failure from "camera above 4096 BU" to "no collider under the camera at any height". It also hits the first collider (a roof or tree top) rather than terrain proper. Both are consistent with the docstring ("Falls back to the camera's own Y when no ground is found below") and with the existing #2859 capsule exclusion; the value feeds the SVGF key as a continuous lane (`fog_height_reference` is the 11th `SVGF_FOG_INPUTS` entry, so a streaming-driven jump larger than 1 % resets history rather than smearing). No action.
- The `interior_portal_sun_active` change in 0c61c4e7c now keys the interior portal sweep to the *uploaded* radiance (post cloud transmittance, post `BYRO_FROXEL_SUN`), so an overcast or night interior skips the portal lists. Every consumer of those lists is on the sun term (`localSkyAperture`, shaft dust `INTERIOR_SHAFT_DUST_PER_METER`), so nothing observable is lost; `portal_sweep_tracks_uploaded_sun_radiance` pins it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
