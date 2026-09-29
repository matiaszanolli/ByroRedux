# #5134: EXT-D1-2026-09-29-01: Starfield WTHR fog distances reach `translate_weather` in metres — every Starfield exterior with a resolved climate fogs out at ~43 m

**Labels**: high, bug, terrain-exterior, esm-plugin, game:starfield

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-09-29.md`
**Severity**: HIGH (a wrong canonical value out of an EXAL `translate_*`)
**Dimension**: EXAL boundary discipline (tier violated: no-fabrication — a unit is mistranslated before the boundary)

## Location
- `crates/plugin/src/esm/records/spatial_units.rs` `normalize` — never touches `index.weathers`
- `byroredux/src/env_translate.rs` `translate_exterior_cell_lighting` and `translate_weather`
- `crates/plugin/src/esm/records/weather.rs` (`parse_wthr` FNAM arm)

## Description
- `b9e961eeb`'s `spatial_units::normalize` lifts Starfield's metric scene data by `BETHESDA_UNITS_PER_METER` (70) at the parse boundary: CELL XCLL fog, LGTM fog, REFR, SCOL, WRLD and NAVM distances/heights — but not WTHR.
- `translate_exterior_cell_lighting` and `translate_weather` read `fog_day_near/far` and `fog_night_near/far` as engine units and feed them to `FogMedium::from_legacy_ramp` → `fit_legacy_fog_extinction` (`byroredux/src/fog.rs`), which divides by 70 to get metres.
- A Starfield ramp of 10 / 3000 m is therefore fitted as 0.14 / 42.9 m — roughly 70× the extinction.

## Evidence
Units settled from data:
- `Starfield.esm` has exactly 3 WTHR: `DefaultWeather` 0x15E FNAM = 10 / 3000 / 10 / 3000; `NewAtlantisWeather50` 0x27CF9B the same; `SpaceWeather` 10 / 500000.
- Same plugin, LGTM fog is unambiguously metric and `normalize` already lifts it (`ShipInteriorLT` 1 / 5; `VecteraMineLT` 10 / 50; `DefaultLightingTemplate` 750 / 12000).
- Magnitude match with FO4 (Bethesda units): FO4 WTHR fog-far median 180,000 BU (p90 250,000; n = 71); Starfield's 3000 m = 210,000 BU. FO4 day height range 10,000 BU vs Starfield 120 m = 8,400 BU. Read as BU, Starfield would be a 43 m fog wall and a 1.7 m height band.
- Reach: all 21 Starfield CLMTs with a `WLST` point at one of the two 3000-far weathers; 19 WRLDs author a `CNAM` climate; nothing in `scene/world_setup.rs` / `cell_loader/exterior.rs` gates on Starfield, so `resolve_default_weather` → `translate_weather` runs for every one.

## Impact
Any Starfield exterior whose climate resolves renders fully fogged beyond ~43 m. The same too-dense medium drives the composite height fog and the froxel volumetrics (both use the `fog_extinction_per_meter` producer). Starfield exteriors are a named policy skip in `docs/smoke-tests/m-exteriors.sh`, so no gate would catch this.

## Related
#5001 (SF-2026-09-29-D4-02: Starfield WTHR FNAM power/max-opacity/height tail never decoded — once decoded, the height fields also need this lift). Sibling, not verified: Starfield WATR DNAM distance-like fields (`underwater_fog_near/far`, `noise_falloff`, `depth_amount`) look metric too; FO76 shares `decode_dnam_starfield` and is not metric, so a lift must key on `GameKind::Starfield` (see #4837). AUDIT_ESM_2026-09-29 § Disproved handoff.

## Suggested Fix
In `spatial_units::normalize`, lift `index.weathers[*].fog_{day,night}_{near,far}` (and the `WeatherHeightFog` fields once Starfield decodes them, #5001). Add a spatial_units test like the XCLL/LGTM ones. Keep `translate_weather` unchanged — units must be settled before the boundary.

Validated at HEAD 9fcfdc3fc: `normalize` iterates cells, statics, scols, worldspaces, lighting_templates and navmeshes only — no `weathers` reference in `spatial_units.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (Starfield WATR distance fields, WTHR height fog once decoded)
- [ ] **TESTS**: A regression test pins this specific fix
