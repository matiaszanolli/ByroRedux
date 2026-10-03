# #5178: EXT-D4-2026-10-02-01: #4928's sibling sweep missed `weather_sky_state`: a third TOD-slot reduction (`.min(3)`) and a cloud-alpha rule that disagrees with the steady-state sampler

**Labels**: low,terrain-exterior,bug
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW. The code is dormant today.
- **Dimension**: Sky, weather, sun
- **Location**: `byroredux/src/env_translate.rs:1388-1402` (`weather_sky_state`). Callers: `:1307` (`TOD_DAY`) and `:1586` (literal `1`). Compare with `byroredux/src/systems/weather.rs:476-506` (`sample_weather_sky`) and `env_translate.rs:1531-1539`.
- **Status**: NEW. Related: #4928 (closed; its SIBLING checkbox) and #4494 (closed; constants in the same function).
- **Tier Violated**: single-boundary (two derivations of one quantity)
- **Game Affected**: all
- **Description**:
  - `weather_sky_state` seeds `WeatherSkyState.cloud_tints` using `let slot = tod_slot.min(3)`. That is a third 6→4 TOD reduction beside the shared `fold_to_four_tod_slots` that #4928 consolidated onto. It is also wrong for `TOD_HIGH_NOON`: 4 becomes 3, which is NIGHT, where the shared fold gives DAY.
  - The seed's alpha is `color.a / 255 × JNAM alpha` (`:1400`). The steady-state sampler drops PNAM's fourth byte: `cloud_layer_colors` goes through `to_rgb_f32` at `:1531-1539`, and alpha is the JNAM value alone (`weather.rs:496-503`).
  - UESP's Skyrim WTHR page lists the PNAM entries as `rgb` per TOD, so the fourth byte is padding, not alpha.
- **Evidence**:
  - `env_translate.rs:1392`: `let slot = tod_slot.min(3);`
  - `env_translate.rs:1400`: `(color.a as f32 / 255.0 * wthr.cloud_layer_alphas[layer][slot]).clamp(0.0, 1.0)`
  - `weather.rs:496-503`: `lerp1(alpha_a, alpha_b, t)` with no PNAM byte.
  - `/mnt/data/src/reference/uesp-wiki/Skyrim Mod/Mod File Format/WTHR.wiki:27-31`: PNAM = `:rgb Sunrise / Day / Sunset / Night`.
  - The test fixture at `env_translate.rs:4711-4716` authors `a: 200`, which encodes the "byte 3 is alpha" reading.
- **Impact**:
  - Both callers pass DAY, so the `.min(3)` misfold is unreachable today.
  - `weather_system` rebuilds `cloud_tints` from `wd.weather` on every tick (`weather.rs:483-505`), so the seed alpha is visible only before the first tick after `apply_environment`.
  - The seed remains a near-copy that will drift. A future caller seeding at another TOD (for example `seeded_at_wrong_tod_resample`'s scenario) would read HIGH_NOON as NIGHT, and the first frame would carry padding-scaled cloud alpha.
- **Suggested Fix**: Seed through the same rule as the sampler. Call `fold_to_four_tod_slots` (or make the seed `sample_weather_sky` on the just-translated `WeatherDataRes`), drop the PNAM fourth byte from the alpha, and replace the literal `1` at `:1586` with `TOD_DAY`.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games sharing the decoder / other consumers of the same field / other copies of the stale text)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL `translate_*` (or `spatial_units::normalize`) boundary — never pushed into shaders/renderer, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix
