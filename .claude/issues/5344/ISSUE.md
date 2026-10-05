# #5344: EXT-D4-2026-10-05-01: #5178's "seed == per-frame sampler" assertion compares `weather_sky_state` with itself; the seed clamps alpha where the sampler does not

**Labels**: low,terrain-exterior,test-gap,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5344

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-05.md` — `EXT-D4-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: LOW (test hygiene; the fold half is pinned)
- **Dimension**: Sky, weather, sun
- **Location**:
  - `byroredux/src/env_translate.rs:4927-4965` (`weather_sky_state_seed_folds_high_noon_onto_day`).
  - Seed alpha at `env_translate.rs:1428` (`.clamp(0.0, 1.0)`); sampler at `byroredux/src/systems/weather.rs:491-497` (`lerp1(alpha_a, alpha_b, t)`, unclamped).
- **Status**: NEW. Related: #5178 (closed).
- **Tier Violated**: single-boundary (the parity claim is unpinned)
- **Game Affected**: all
- **Description**:
  - The test builds `wd = translate_weather(&w, …)` and asserts `wd.weather.cloud_tints[0] == seeded.cloud_tints[0]` "the seed and the per-frame sampler must derive one quantity one way".
  - `wd.weather` is itself `weather_sky_state(wthr, TOD_DAY)`, so this is seed@DAY == seed@HIGH_NOON. That is a fold check, not a sampler check. `sample_weather_sky` is never run.
  - The two rules still differ: the seed clamps the JNAM alpha to [0, 1] and the sampler does not. A JNAM outside the unit range (mod or corrupt data) gives a first frame that differs from every later frame.
- **Suggested Fix**:
  - Run `weather_system` one tick on the translated `WeatherDataRes` at DAY and compare its `SkyParamsRes.weather.cloud_tints` with the seed.
  - Apply the same clamp in both places, or move it to `translate_weather`, so the table carries canonical [0, 1] alphas.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
