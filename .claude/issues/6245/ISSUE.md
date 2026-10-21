# #6245: EXT-D4-2026-10-21-03: Interior caustics dim the room's XCLL directional by the outdoor cloud column

**Labels**: medium,terrain-exterior,shaders,renderer,bug

**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Sky, weather, sun · **Location**: `crates/renderer/src/vulkan/context/post_passes.rs (caustic.sun_transmittance)`

- **Tier Violated**: no-leak (outdoor weather leaks into interior lighting)
- **Game Affected**: all games, in interiors with a directional key and a refractor or water caustic
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs:544`; `crates/renderer/shaders/caustic_splat.comp:448-455`; the bake source is `crates/renderer/src/vulkan/context/frame_params.rs::build_sky_cube_params`
- **Status**: NEW (introduced by `c231f5b5f`)
- **Description**: `caustic.sun_transmittance = self.camera_cloud_sun_transmittance;` is set unconditionally. `caustic_splat.comp` multiplies every `Ltype == 2u` light's `lightReach` by `causticSun.x`. In an interior, the directional `GpuLight` is the room's XCLL key (`render/lights.rs` emits it as type 2.0). The cloud-shadow bake still runs every interior frame (PERF-D5-02). When a `SkyParamsRes` exists it bakes from `portal_outdoor_sky`, which `build_sky_params` always sets for interiors. Otherwise it bakes from the default `SkyParams`, which has the sun above the horizon and coverage 0.35, so procedural puffs still drift over the camera column. Either way the room key's caustics follow the outdoor cloud field. `exteriorCloudSunTransmittance` deliberately returns 1.0 in interiors ("their key light is the room's XCLL directional, not the sun"), so the caustic disagrees with the surfaces it lands on.
- **Evidence**: `begin_frame_recording.rs:47-53` (readback every frame), `post_passes.rs:544` (no `is_exterior` gate), `bindings.glsl:414-418` (the stated interior contract). The froxel arm is different and correct: in interiors it multiplies `portal_sun_radiance`, which really is the outdoor sun.
- **Impact**: Glass and water caustics under an interior's directional key weaken under overcast weather outside (stratiform term down to about 8%) and pulse as clouds drift over the camera column.
- **Related**: EXT-D4-2026-10-21-01; PERF-D5-02 (the bake running in interiors).
- **Suggested Fix**: Pass `1.0` for the caustic sun term when the frame is not exterior (`SkyParams::is_exterior`). If interior caustics should ever follow a portal sun, carry that as its own lane.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
