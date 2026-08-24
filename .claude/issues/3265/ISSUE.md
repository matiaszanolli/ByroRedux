# 3265: CONC-D5-2026-08-24-02: player_water_state re-locks TotalTime+WindField inside the per-plane loop while holding three storage guards

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-24.md` (CONC-D5-2026-08-24-02)

## Description

This is the only site in the physics path holding storage read guards (`WaterPlane`/`WaterVolume`/`WaterFlow`) across a resource acquisition — the reverse of the "snapshot storages, drop guards, then take the resource" discipline every sibling follows. It also re-acquires `TotalTime`/`WindField` and recomputes `weather_wave_adjustment` once per water plane per frame, where `apply_buoyancy_with_scratch` hoists both out of its loop.

## Location

`byroredux/src/systems/character.rs:893-940` (guards at 898-901, resource acquisitions at 916-920)

## Impact

Not a live deadlock today — `WindField`'s only writer (`weather_system`) is a deliberate `Stage::Early` exclusive (#3111). A future registration change moving `weather_system` to parallel is all that separates this from a genuine cross-thread ABBA.

## Related

#3111; `apply_buoyancy_with_scratch` is the reference hoist-before-loop shape. Adjacent to #3263 (CONC-D3-2026-08-24-05, same weather-exclusive dependency).

## Suggested Fix

Hoist `time_secs` + `weather_wave_adjustment` above the `query::<WaterPlane>()` acquisitions in `player_water_state`, matching `apply_buoyancy_with_scratch:590-600`.

## Completeness Checks
- [ ] **LOCK_ORDER**: Storage-before-resource discipline restored
- [ ] **SIBLING**: Matches `apply_buoyancy_with_scratch`'s hoist-before-loop shape
- [ ] **TESTS**: Confirm `weather_wave_adjustment` computed once per call, not per plane
