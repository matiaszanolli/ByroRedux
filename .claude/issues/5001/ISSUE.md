# Issue #5001

**Title:** SF-2026-09-29-D4-02: Starfield WTHR FNAM power / max-opacity / height-fog tail is never decoded — parse_wthr gates it on FO4 | FO76 only
**State:** OPEN
**Labels:** bug, medium, legacy-compat, game:starfield, terrain-exterior, esm-plugin

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: ESM → cell bring-up (EXAL weather handoff)
**Location**: `crates/plugin/src/esm/records/weather.rs` — FNAM arm of `parse_wthr`

## Description
The FNAM arm reads the four fog distances for every non-Skyrim game. The power/max pair is gated on `matches!(game, GameKind::Fallout4 | GameKind::Fallout76) && len >= SKYRIM_FNAM_SIZE` (32), and the ten height-fog floats on the same match with `len >= FO4_FNAM_SIZE` (72). xEdit's shared `wbWeatherFogDistance` (`wbDefinitionsCommon.pas:9795-9846`) gives power for `> gmTES4R`, max for `> gmFNV`, and the height block under `IsFO4Plus` from form version 119/120. SF1 uses the same struct (`wbDefinitionsSF1.pas:18870`), and every Starfield FNAM is 72 bytes. Starfield therefore keeps the defaults `fog_day/night_power = 1.0`, `fog_day/night_max = 1.0` and `fog_height = None`.

## Evidence
Raw FNAM of all 3 `Starfield.esm` WTHRs:
- `DefaultWeather` 0x15E: `10 3000 10 3000 | 0.4 0.4 | 0.9 0.9 | 10 120 10 120 0.05 0.05 10 220 10 900`
- `NewAtlantisWeather50` 0x27CF9B: same shape (far 220/220)
- `SpaceWeather` 0x249FA6: `… 1 1 0 0 0 10000 0 10000 …`

`translate_weather` / `weather_data_from_record` (`byroredux/src/env_translate.rs`) consume `fog_day_max` in `FogMedium::from_legacy_ramp` and `fog_height` in `with_authored_height_range`, so the authored 0.9 opacity cap and the 120 m near-height band are replaced by defaults. SpaceWeather's authored `max = 0` (no fog in space) becomes 1.0.

## Impact
Starfield exterior fog ignores its authored opacity ceiling and vertical profile; space weather gets a full-opacity fog cap it explicitly authored as zero. Currently masked by the WTHR fog-unit defect (fog 70× too short, EXT-D1-2026-09-29-01 in `AUDIT_EXTERIOR_2026-09-29.md`); becomes the visible defect once that lands. All 21 CLMT `WLST` entries point at the two 3000-far weathers.

## Related
EXT-D1-2026-09-29-01 (units; its fix must also lift the height mids/ranges once decoded); SF-2026-09-29-D4-01; #3956 (closed, FO4/FO76 height-fog boundary).

## Suggested Fix
Extend both `matches!` guards to `Fallout4 | Fallout76 | Starfield`, or gate on byte length per xEdit's form-version rule. Then have `spatial_units::normalize` lift the four distances and the eight height mid/range values (not the density scales). Add a Starfield case beside `parse_fo4_fnam_retains_all_eighteen_floats`.

Validated at HEAD 9fcfdc3fc: both `matches!(game, GameKind::Fallout4 | GameKind::Fallout76)` gates in `parse_wthr`'s FNAM arm exclude Starfield; `spatial_units::normalize` has no weather arm.

## Completeness Checks
- [ ] **SIBLING**: FO3/FNV FNAM bytes 16–23 (day/night power per xEdit) are also ignored — check with the FNV/FO3 owners
- [ ] **CANONICAL-BOUNDARY**: unit lift stays in `spatial_units::normalize`, never in the renderer/shaders
- [ ] **TESTS**: A regression test pins this specific fix (Starfield 72-byte FNAM fixture)

