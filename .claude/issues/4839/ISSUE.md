# #4839 — REN-D10-2026-09-24-01: interior portal-sky path feeds `sun_illuminance = [0,0,0]`, so clouds seen through Show-Sky interiors, authored apertures and the window-portal cube carry no sun term

**Labels**: bug,renderer,medium,terrain-exterior

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D10-2026-09-24-01**._

- **Severity**: MEDIUM. Visual, wrong shading input on a newly visible path. No NaN or black, so the "neutral fallback" rule holds.
- **Dimension**: Sky/Weather (consumption)
- **Location**: `byroredux/src/render/sky.rs` — `build_sky_params` (interior branch, `outdoor_sky_params(world, &sky_res, None)`) and `outdoor_sky_params` (`sun_illuminance: cell_directional.map_or([0.0; 3], …)`). Consumer `crates/renderer/shaders/include/clouds.glsl` `cloud_march` (`dome.sun_illuminance.xyz`), reached via `composite.frag` `build_sky_dome()` and the sky-cube bake (`build_sky_cube_params` → `from_composite`).
- **Status**: NEW (introduced by `0572bfd5a`; the composite could not show interior sky before it).
- **Description**: `0572bfd5a` gives every interior a `portal_outdoor_sky` built with `cell_directional = None`; `sun_illuminance` is the only field derived from `cell_directional`, so `None` yields `[0.0; 3]`. `SkyParams::sun_illuminance`'s own doc says "Zero when there is no exterior sun", but here the exterior sun exists (`portal_sun()` computes it and volumetrics use it). Exteriors light the cloud body with `compute_directional_upload(WTHR sunlight, …) × TOD ramp × cloud transmittance`. The interior copy of the same sky gets `sun_radiance = 0`, leaving only the sky-ambient term.
- **Evidence**: `outdoor_sky_params(world, &sky_res, None)` in both interior arms. `cloud_march` runs whenever `weather_aurora.z` (coverage, default 0.35) > 0.001 and `dir.y` > ~0.015; `sun_radiance = sun_illuminance * diffuse_calibration * octave_sum`, so zero sun leaves `ambient * sigma_t * transport`. No test asserts interior `sun_illuminance` (`interior_only_boot_uses_live_clock_for_portal_sun` asserts `sun_intensity` and `portal_sun_radiance` only). `weather_system` gates `directional_color` writes on `!is_interior`, so the exterior sunlight colour is not recoverable from `CellLightingRes` inside an interior.
- **Impact**: Clouds seen from inside (Show-Sky interiors, aperture planes, skylights, bounded openings looking up) read as flat, unlit sky-ambient blobs with no sunlit faces or silver lining. They do not match the same weather outdoors, on the one path this feature exists to show. Horizon-facing window panes sample `-N`, where the horizon fade zeroes clouds.
- **Suggested Fix**: Capture the exterior `sun_illuminance` (sunlight colour × ramp × cloud transmittance) into `SkyParamsRes` during the exterior weather tick and feed it to the portal palette. Do not use the XCLL directional, and do not reuse `portal_sun_radiance` (units: sun-disc colour × 0–4 intensity). Add an interior `sun_illuminance > 0` assertion at noon and `== 0` at night to `render::sky::tests`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
