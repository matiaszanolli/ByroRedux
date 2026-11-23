# #7384 — REN-D10-2026-11-23-02: `light_level_at` (new CPU lighting consumer) diverges from the GPU light model in three ways, and nothing pins the parity

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D10-02). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW
- **Dimension**: Light Animation (CPU consumer of the light model)
- **Location**: `byroredux/src/detection/light.rs` `light_level_at`, `ambient_light`; contrast `byroredux/src/render/mod.rs` `compute_directional_upload`, `byroredux/src/render/lights.rs` `collect_lights` / `gpu_light_from_emitter`, `byroredux/src/systems/weather.rs` (`cell_lit.directional_color = sunlight`), `include/lighting.glsl` `shadowableLightRadiance` (spot cone, `exteriorCloudSunTransmittance`)
- **Status**: NEW (`gh issue list --state all --search "light_level_at"` and `"light level night sun"` empty). Introduced by 995ed089b / 4870d7772 (M65), lock order changed by 10f7c16fc.
- **Description**: The PRD (SNK-DET-009, Q3 decided 2026-11-14) defines the light level as computed "from the lights the renderer is given". What the renderer is given is `collect_lights`' output. The CPU model matches the GPU on the point-light distance law (I checked `Emitter::distance_attenuation` against `pointSpotAtten`: same knee `max(range, source_radius)`, same smoothstep cull window from authored range to `LEGACY_LIGHT_CULL_RANGE_MULTIPLIER * range`, same inverse-square softening; the only difference is the GPU's 1 BU knee floor), on `dimmer * intensity`, and on the entity set (every `LightSource` with a `GlobalTransform`, same lock order as `collect_lights`). It diverges in:
  1. Exterior directional: it adds `luminance(cell.directional_color)` raw. `weather_system` writes `directional_color = sunlight` (the TOD palette times the HNAM dimmer) with no sun-arc factor, while the GPU light is `compute_directional_upload(directional_color, false, sun_intensity, None)` = colour x `clamp(sun_intensity / SUN_INTENSITY_PEAK)`, which `compute_sun_arc` ramps to exactly 0 outside `sunrise_begin..sunset_end`. So at night and across the dawn/dusk ramps the CPU level carries a sun term the renderer does not light the world with. Magnitude depends on the night `sunlight` palette entry per WTHR (not measured here: needs data).
  2. Spot lights: `Emitter::distance_attenuation` is documented as the distance law only; the cone (`outer_cone_cos`, `EmitterKind::Spot`) is not applied, so a ceiling spot or flashlight lights its whole sphere on the CPU and a cone on the GPU.
  3. Cloud transmittance of the sun (`exteriorCloudSunTransmittance`) is not applied. PRD row 1 documents blindness to ray-traced shadowing; clouds are not RT and are not listed.
- **Evidence**: `light_level_at` body (`level += luminance(cell.directional_color)` under `!cell.is_interior`); `compute_directional_upload` (exterior arm `(sun_intensity / SUN_INTENSITY_PEAK).clamp(0.0, 1.0)`); `compute_sun_arc` (`else { 0.0 }` outside the arc); `weather_system` (`cell_lit.directional_color = sunlight;`). The single test only asserts `near > far` for one lamp.
- **Impact**: stealth light term over-reports outdoor light at night and dusk and over-reports under spot lamps. Gameplay-visible, no rendering effect.
- **Related**: `docs/prd/sneak-and-detection.md` SNK-DET-009 / Q3; `docs/engine/sneak-detection.md`.
- **Suggested Fix**: route the exterior directional through `compute_directional_upload` with the live `SkyParamsRes::sun_intensity` (the call `collect_lights` and `build_sky_params` already make), apply the cone factor for `EmitterKind::Spot`, and add a parity test that feeds one `LightSource` set through both `light_level_at` and `gpu_light_from_emitter` + the Rust mirror of `pointSpotAtten`. Whether to include cloud transmittance is an owner call (CPU-only data would be needed).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **TESTS**: A regression test pins this specific fix
