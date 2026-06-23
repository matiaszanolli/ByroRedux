# LC-D5-01: Second exterior no-weather default producer outside the EXAL boundary with divergent constants

**Issue**: #1722
**Source audit**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-06-23.md`
**Severity**: MEDIUM · **Labels**: medium, legacy-compat, bug
**Dimension**: 5 — EXAL (no render-time fallback)
**Location**: `byroredux/src/systems/weather.rs:208-235` (`apply_neutral_exterior_fallback` + `NEUTRAL_*` consts) vs `byroredux/src/env_translate.rs:331-429` (`procedural_fallback_*` + `FB_*` consts)

## Description

The EXAL contract requires the "no climate/weather" case to be one canonical default at the translate boundary, never a runtime/render branch. There are two producers of the exterior-default lighting with divergent values:
- Boundary (`env_translate.rs`): `FB_AMBIENT [0.15,0.14,0.12]`, `FB_FOG_COLOR [0.65,0.7,0.8]`, `FB_FOG_NEAR 15000`, `FB_FOG_FAR 80000`, `FB_SUNLIGHT [1.0,0.95,0.8]`.
- Runtime (`weather.rs::apply_neutral_exterior_fallback`): `NEUTRAL_AMBIENT [0.4,0.4,0.4]`, `NEUTRAL_FOG_COLOR [0.5,0.55,0.6]`, `NEUTRAL_FOG_NEAR 1000`, `NEUTRAL_FOG_FAR 50000`, `NEUTRAL_SUNLIGHT [1.0,1.0,1.0]`.

## Evidence

`weather_system` (`weather.rs:373-377`) takes the neutral path when `WeatherDataRes` is absent and writes `NEUTRAL_*` into `CellLightingRes` (consts `:208-214`, write `:229-234`). `world_setup.rs:424` inserts `procedural_fallback_weather()` (the `FB_*` set). The same state resolves to two different looks.

## Impact

Inconsistent exterior fallback lighting (ambient ~2.6×, fog distances 10–15×). Visible only on the narrow window where `WeatherDataRes` is missing at `weather_system` time; blast radius small. The violation is the duplicated default living in a system.

## Related

EXAL "no render-time fallback" (`exal.md` §3); #463 / #1034.

## Suggested Fix

Have `apply_neutral_exterior_fallback` consume `procedural_fallback_cell_lighting` (or its `FB_*` constants), collapsing to one source of truth.
