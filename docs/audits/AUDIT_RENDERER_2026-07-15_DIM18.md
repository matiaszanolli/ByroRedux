# Renderer Audit — 2026-07-15 (Dimension 18: Sky / Weather / Exterior Lighting, M33/M34)

Scope: `--focus 18` — single-dimension run of `/audit-renderer`, `--depth deep`.
Covers `byroredux/src/systems/weather.rs`, `byroredux/src/render/sky.rs`,
`byroredux/src/render/lights.rs`, `byroredux/src/env_translate.rs`,
`byroredux/src/scene/world_setup.rs`, `crates/plugin/src/esm/records/weather.rs`,
`crates/plugin/src/esm/records/climate.rs`, and the sky/cloud/fog sections of
`triangle.frag`/`composite.frag`.

This is **first coverage** of this dimension under its current definition.
An older report titled "Dimension 18" (2026-06-04) exists but covers what is
now Dimension 16 (volumetrics) — the dimension numbering was reorganized
since then. No prior audit has scored this exact scope, so there is no
"already clean, re-verify" baseline to cross-check against; everything below
is fresh analysis.

## Executive Summary

Sky/weather/exterior lighting is in good shape: monotonic time advance, a
data-driven (not hardcoded) sun arc from CLMT TNAM hours, correctly-ordered
TOD-then-weather-fade blending, wind-scaled 4-layer cloud parallax, a single
non-drifting zenith-color source shared between the RT ray-miss fill and the
raster sky gradient, correct weather-side fog interpolation, a symbol-anchored
interior-fill gate, and a non-NaN/non-black no-WTHR fallback. Same-worldspace
cell-transition stability holds for the common path (game clock and weather
data both persist across cell streaming).

One gap survived scrutiny:

- **REN-D18-01 (MEDIUM)** — the procedural-fallback worldspace-transition path
  (`insert_procedural_fallback_resources`) unconditionally resets the global
  game clock (`GameTimeRes`) on every call, unlike its sibling WTHR branch and
  its own neighboring `CloudSimState` insert, both of which correctly guard on
  "first load only." This produces a visible time-of-day pop whenever the
  engine transitions into a climateless worldspace after the clock has
  already advanced. Blast radius is narrow — vanilla worldspaces all resolve
  a CLMT and never hit this branch after initial load — but it's reachable on
  corrupt/partial ESMs, climate-less mod worldspaces, and synthetic/test cells.

No CRITICAL/HIGH findings. No bench-of-record comparison — this was a static
code trace, not a live render.

## Findings

### REN-D18-01: Procedural-fallback worldspace transition resets the global game clock (TOD pop)
- **Severity**: MEDIUM
- **Dimension**: Sky/Weather
- **Location**: `byroredux/src/scene/world_setup.rs` (`insert_procedural_fallback_resources`, called from the `else` branch of `apply_worldspace_weather`)
- **Status**: NEW
- **Description**: The WTHR (climate-present) branch of `apply_worldspace_weather` correctly guards the `GameTimeRes` insert behind a first-load check: `if world.try_resource::<WeatherDataRes>().is_some() { install a WeatherTransitionRes crossfade } else { insert new_weather; insert initial_game_time() }`. Its sibling, `insert_procedural_fallback_resources` (the climateless-worldspace path), instead calls `world.insert_resource(initial_game_time())` **unconditionally** — even though the `CloudSimState` insert two lines above it in the *same function* is correctly guarded with `if world.try_resource::<CloudSimState>().is_none() { .. }`. Since `weather_system` derives the TOD palette, sun arc, and fog entirely from `GameTimeRes.hour`, every transition into (or between) climateless worldspaces snaps the clock back to `initial_game_time()`'s fixed hour (10.0 by default, or the frozen `BYRO_HOUR` override) — a visible lighting pop from whatever time it actually was.
- **Evidence**:
  ```rust
  // world_setup.rs — WTHR branch (correctly guarded)
  if world.try_resource::<WeatherDataRes>().is_some() {
      world.insert_resource(WeatherTransitionRes { target: new_weather, elapsed_secs: 0.0, duration_secs: 8.0, done: false });
  } else {
      world.insert_resource(new_weather);
      world.insert_resource(initial_game_time());
  }
  ```
  ```rust
  // world_setup.rs::insert_procedural_fallback_resources — fallback branch (unguarded)
  pub(crate) fn insert_procedural_fallback_resources(world: &mut World, sun_dir: [f32; 3]) {
      world.insert_resource(crate::env_translate::procedural_fallback_cell_lighting(sun_dir));
      world.insert_resource(crate::env_translate::procedural_fallback_sky(sun_dir));
      if world.try_resource::<CloudSimState>().is_none() {   // <- correctly guarded
          world.insert_resource(CloudSimState::default());
      }
      world.insert_resource(crate::env_translate::procedural_fallback_weather());
      world.insert_resource(initial_game_time());            // <- NOT guarded: always resets the clock
  }
  ```
  `apply_worldspace_weather` (and therefore this fallback path) is invoked on cross-worldspace/exterior-destination transitions from `app_step.rs`, `scene.rs`, and `debug_load.rs` — not just at process boot — so the reset can fire mid-session.
- **Impact**: Visible time-of-day / sun-direction / fog pop on any transition *into* a climateless worldspace after the clock has advanced past its initial value. Recoverable (the clock resumes advancing normally from the reset hour); visual only, no crash, no state corruption. Vanilla FNV/FO3/Oblivion/Skyrim+ worldspaces all resolve a CLMT and take the guarded WTHR branch, so shipped content never triggers this after first load — the gap is reachable on corrupt/partial ESMs, mod worldspaces authored with no CLMT, and synthetic/test cells.
- **Related**: Mirrors the `is_none()` guard pattern already correctly used for `CloudSimState` in the same function, and for `GameTimeRes` itself in the sibling WTHR branch — this is an inconsistency between two code paths that should behave identically, not a novel bug pattern.
- **Suggested Fix**: Guard the `GameTimeRes` insert in `insert_procedural_fallback_resources` with `if world.try_resource::<GameTimeRes>().is_none() { world.insert_resource(initial_game_time()); }`, mirroring both the `CloudSimState` guard directly above it and the WTHR branch's equivalent first-load check.

## Verified Clean

- **Monotonic time advance**: `weather_system` is the sole writer of `GameTimeRes.hour` (registered once at boot), advances it exactly once per frame (`hour += dt * time_scale / 3600.0`) followed by a single `-= 24.0` wrap — no backward jump, no double-advance, and the wrap can't under/overshoot at any realistic `dt`/`time_scale`.
- **Sun arc from data**: `compute_sun_arc(hour, wd.tod_hours)` derives the sun's visible window and arc from `tod_hours`, populated from CLMT `TNAM` sunrise/sunset bytes (validated range, converted to hours) via `climate_tod_hours`, with a documented `[6,10,18,22]` fallback only when climate data is absent/invalid. The old hardcoded `[6h,18h]` arc is gone; covered by dedicated regression tests.
- **TOD color easing**: piecewise-linear interpolation across the 6 authored TOD slots (plus synthetic noon/midnight anchors) matches Gamebryo's linear color-key model; fog distance uses the identical `(slot_a, slot_b, t)` tuple as the color palette, keeping the two in lockstep.
- **Weather-fade ordering**: confirmed TOD-resolved-per-weather happens first, then the cross-weather blend (`lerp3(.., transition_t)`) — both source and target weather are independently TOD-sampled before blending, and fog distance follows the same per-weather-then-blend order.
- **Cloud layers**: all four scroll accumulators advance every frame scaled by a wind-derived rate (`cloud_scroll_rate_from_wind(wd.wind_speed)`), not a fixed literal; all four reach the composite shader's 4-layer world-XY parallax mix.
- **Sky gradient single source**: the RT ray-miss fills in `triangle.frag` (refraction miss, GI miss) use a flat zenith approximation (`skyTint`) as a deliberate simplification, and both that value and the full raster gradient's zenith color in `composite.frag` are written from the same `sky_params.zenith_color` upload — one source, no drift risk between the two consumers.
- **Fog scope (weather-side)**: day/night fog near/far are correctly index-paired and interpolated by the same night-factor that drives the color palette; the weather-side computation feeding `CellLightingRes`/`GpuCamera` is correct (direct-vs-indirect application is Dimension 8's concern, out of this dimension's scope).
- **Interior fill correctness**: the interior directional gets `0.6×` color scale and a `radius = -1.0` sentinel; the shader's `isInteriorFill` flag is symbol-anchored (`bool isInteriorFill = radius < 0.0`) and gates both the isotropic-fill shading branch and the RT shadow-ray loop (`!isInteriorFill`), pinned by dedicated integration tests independent of any persisted sky-intensity state.
- **Fallback safety**: the no-WTHR exterior branch's constants are all finite and non-black; the sun-arc math guards its only division sites against degenerate inputs; covered by fallback-specific tests.
- **Cell-transition stability (common path)**: TOD is driven by the global clock plus per-worldspace weather data, both of which persist across same-worldspace cell streaming (streaming never calls `apply_worldspace_weather`); the WTHR worldspace-change path correctly preserves the clock and cross-fades weather over 8 seconds. The one gap in this area is the fallback-path clock reset, filed as REN-D18-01 above.

## Doc Consistency

`docs/engine/shader-pipeline.md`'s `GpuCamera` field table (336 B total; `sky_tint` at offset 272 with `w` = sun angular radius; `fog` at offset 240; `sun_direction` at offset 288 with `w` = intensity) matches `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` exactly. No doc drift found for this dimension's fields.

---
Suggest: `/audit-publish docs/audits/AUDIT_RENDERER_2026-07-15_DIM18.md`
