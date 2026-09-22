# EXT-D4-2026-09-21-01: WTHR transition promotion is still a hand-kept field copy — the drop has shipped four times

**Issue**: #4733
**Filed**: 2026-09-22 (audit-publish, AUDIT_EXTERIOR_2026-09-21.md)

**Severity**: LOW (hardening; correct today)
**Dimension**: Sky, weather, sun
**Game Affected**: all WTHR games
**Location**: `byroredux/src/systems/weather.rs:1199-1254` (`promote_weather_transition_target`); struct `WeatherDataRes` at `byroredux/src/components.rs:1378`

## Description
All 14 `WeatherDataRes` fields are promoted today via 14 hand-written copies (verified by sweeping the struct), but the same omission has shipped four times: #1101 `wind_speed`, #1102 `skyrim_dalc_per_tod`, #4481 both HNAM dimmers, and `cloud_layer_velocities_authored`. The guards are per-field regression tests written after each drop, not a structural guarantee.

## Evidence
The promotion body compared field-by-field against the struct. Confirmed unchanged at HEAD `ee6d3fb39`.

## Impact
The next `WeatherDataRes` field is dropped at every weather transition, with no failing test until someone notices and writes one.

## Suggested Fix
Destructure `&tr.target` exhaustively (no `..`) before the #3263 lock-order drop, so adding a field becomes a compile error at the promotion site instead of a silent runtime drop.

## Related
#1101, #1102, #4481, #3985

## Source
docs/audits/AUDIT_EXTERIOR_2026-09-21.md (EXT-D4-2026-09-21-01)
