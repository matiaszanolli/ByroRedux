# REN-D18-01: a mid-session worldspace transition renders one frame of TOD_DAY sky + full-intensity sun at any game hour

## Description
`7a851ab9` made the bootstrap sun **direction** honour the live clock (`bootstrap_game_hour` → `compute_sun_arc`), but the palette half of the same seed was left at the fixed `TOD_DAY` slot and `sun_intensity` at the constant `SUN_INTENSITY = 4.0`. So the seed is internally inconsistent: sun vector = live hour, everything else = noon. At boot this is invisible — `byroredux/src/main.rs` runs the scheduler immediately after `setup_scene()`, so `weather_system` corrects the seed before the first rendered frame. On a mid-session worldspace change it is not: `step_debug_loads` / `step_cell_transition` run *after* `self.scheduler.run(&self.world, dt)` and *before* `self.render_one_frame(...)` in the same `about_to_wait` iteration, so the seeded values are what `build_render_data` uploads for that frame.

## Location
`byroredux/src/scene/world_setup.rs` — `apply_worldspace_weather` (the `insert_resource` pair in the WTHR arm); producers `byroredux/src/env_translate.rs` (`translate_exterior_cell_lighting`, `translate_sky`, `const SUN_INTENSITY`). Call sites: `byroredux/src/app_step.rs` (`step_cell_transition`, the `TransitionDestination::Exterior` arm) and `byroredux/src/debug_load.rs`.

## Evidence
The WTHR arm derives `sun_dir` from `bootstrap_game_hour(world)` but calls `translate_exterior_cell_lighting(wthr, sun_dir)` and `translate_sky(wthr, sun_dir, …)`, which read `sky_colors[…][TOD_DAY]` and `fog_day_near/far` unconditionally. Consumer chain for that frame: `byroredux/src/render/sky.rs::build_sky_params` copies `sun_intensity` straight through; `byroredux/src/render/lights.rs::collect_lights` snapshots it and feeds `compute_directional_upload`, whose exterior arm scales by `(sun_intensity / SUN_INTENSITY_PEAK).clamp(0,1)` — `4.0 / 4.0 = 1.0`.

## Impact
Door-walk into an exterior worldspace at, say, 01:00 and one frame is composited with the daytime zenith/horizon/lower gradient, daytime fog colour and distance, the TOD_DAY `SKY_SUNLIGHT` directional at full strength, and a `directional_dir` of `[0, -1, 0]` (the below-horizon sentinel `compute_sun_arc` correctly returns at night) — a full-brightness key light pointing straight down under a noon sky. This is the pre-#798 failure mode, fixed for the steady state, resurfacing for exactly one frame. Exterior-only; visual, no corruption. Whether it also poisons TAA/SVGF history beyond that frame is a capture question, not a `cargo test` one.

## Related
`7a851ab9` (the sun-direction half of this seed); #2511 (the adjacent transition-lifecycle fix, verified intact); `docs/engine/exal.md` §2, which calls the old seed "dead for one frame and misleading" — it is not dead.

## Suggested Fix
After `apply_worldspace_weather` returns on the transition path, resample once with the idiom that already exists for the console clock — `crate::systems::weather_system(world, 0.0)` (`byroredux/src/commands/time.rs::resample_lighting`). A `dt = 0.0` tick advances no clock and no cross-fade, so it renders the correct TOD sample of whichever weather the fade should start from. Alternatively hand the TOD slot pair into the two translate functions instead of hardcoding `TOD_DAY`. CPU-side; no render-pass change is implied.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (e.g. assert the seeded resource matches `weather_system`'s live-hour sample immediately post-transition)
- [ ] Whether this poisons TAA/SVGF history beyond the one frame is an open capture question, worth a follow-up note if confirmed

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D18-01).
