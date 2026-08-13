# REN-D18-01: a mid-session worldspace transition renders one frame of TOD_DAY sky + full-intensity sun at any game hour

- **Severity**: MEDIUM
- **Dimension**: 18 — Sky/Weather
- **Location**: `byroredux/src/scene/world_setup.rs` — `apply_worldspace_weather` (the `insert_resource` pair in the WTHR arm); producers `byroredux/src/env_translate.rs` (`translate_exterior_cell_lighting`, `translate_sky`, `const SUN_INTENSITY`). Call sites: `byroredux/src/app_step.rs` (`step_cell_transition`, `TransitionDestination::Exterior` arm) and `byroredux/src/debug_load.rs`.
- **Description**: `7a851ab9` made the bootstrap sun direction honour the live clock, but the palette half of the seed stayed at fixed `TOD_DAY` and `sun_intensity` at constant `SUN_INTENSITY = 4.0` — internally inconsistent. Invisible at boot (scheduler corrects it before the first render), but on a mid-session worldspace change `step_cell_transition` runs after `scheduler.run` and before `render_one_frame` in the same iteration, so the seeded values are what that frame uploads.
- **Evidence**: WTHR arm derives `sun_dir` from `bootstrap_game_hour` but calls `translate_exterior_cell_lighting`/`translate_sky` which read `sky_colors[…][TOD_DAY]` unconditionally. Consumer chain: `render/sky.rs::build_sky_params` copies `sun_intensity` through; `render/lights.rs::collect_lights` feeds `compute_directional_upload`, which scales `(sun_intensity/SUN_INTENSITY_PEAK).clamp(0,1)` = `4.0/4.0 = 1.0`.
- **Impact**: Door-walk into an exterior worldspace at, say, 01:00 renders one frame with daytime palette/fog + full-strength TOD_DAY directional pointed straight down (below-horizon sentinel `[0,-1,0]`) under a noon sky — the pre-#798 failure mode resurfacing for one frame. Exterior-only, visual, no corruption.
- **Related**: `7a851ab9` (sun-direction half of the seed), #2511 (adjacent transition-lifecycle fix, verified intact), `docs/engine/exal.md` §2 (calls the old seed "dead for one frame" — it is not).
- **Suggested Fix**: After `apply_worldspace_weather` returns on the transition path, resample once via `crate::systems::weather_system(world, 0.0)` (the idiom already used by the console clock, `byroredux/src/commands/time.rs::resample_lighting`). CPU-side; no render-pass change implied.

## Completeness Checks
- [ ] TESTS: assert the seeded resource matches weather_system's live-hour sample immediately post-transition
- [ ] Whether this poisons TAA/SVGF history beyond one frame is an open capture question

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2827
