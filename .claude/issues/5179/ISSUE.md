# #5179: EXT-D4-2026-10-02-02: #4926's sibling: water's day/night factor still reads the source climate's TOD breakpoints through a cross-climate fade and steps at promotion

**Labels**: low,terrain-exterior,water,bug
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW
- **Dimension**: Sky, weather, sun (water's TOD consumer; `render/water.rs` is in no Dim 5 path)
- **Location**: `byroredux/src/render/water.rs:121-133`; `byroredux/src/systems/weather.rs:1009-1014`, `:1045`. Same pattern, harmless: `byroredux/src/commands/time.rs:62-65` (phase label).
- **Status**: NEW. Related: #4926 (closed) and #1018 (same class).
- **Tier Violated**: single-boundary (the effective TOD breakpoints exist only as a `weather_system` local)
- **Game Affected**: all, on a WTHR cross-fade between worldspaces whose CLMT TNAM differ (e.g. base game ↔ DLC worldspace)
- **Description**:
  - db0ec5467 blends the source and target `tod_hours` for `compute_sun_arc`, but only into a local (`weather.rs:1009-1014`); nothing publishes the effective breakpoints.
  - `render/water.rs` computes the GNAM day/night surface blend as `night_factor_for_hour(hour, WeatherDataRes.tod_hours)`. During the 8 s fade `WeatherDataRes` still holds the source weather, because the target lives in `WeatherTransitionRes.target` until promotion (`weather.rs:1343`).
  - Result: water follows the source climate for the whole fade, then jumps to the target climate's night factor on the promotion frame. Meanwhile the sky palette, fog and (since #4926) the sun all ease.
- **Evidence**: `render/water.rs:129`: `let tod_hours = weather.as_ref().map(|w| w.tod_hours);` then `:132` `night_factor_for_hour(hour, hours)`. `weather.rs:1343`: `wd.tod_hours = new_tod;` runs only in the promotion.
- **Impact**:
  - The water surface's day/night variant pops once at the end of a cross-climate fade.
  - The pop is visible only in hours where the two climates' night factors differ, i.e. the dawn/dusk bands.
  - Same-climate fades (the common case) are unaffected.
- **Suggested Fix**: Have `weather_system` publish the effective breakpoints or the night factor it already derives. For example, store the blended `tod_hours` on `SkyParamsRes` or a small resource, and have `render/water.rs` (and `time` command's phase label) read that instead of `WeatherDataRes.tod_hours`. Add a mid-fade water night-factor test beside `sun_arc_crossfade_tests`.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games sharing the decoder / other consumers of the same field / other copies of the stale text)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL `translate_*` (or `spatial_units::normalize`) boundary — never pushed into shaders/renderer, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix
