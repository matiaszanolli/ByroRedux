# #897 — REN-D15-01: night_factor for fog distance uses hardcoded hours; color breakpoints are CLMT-driven

**Severity**: LOW
**Dimension**: Sky / Weather / Exterior Lighting
**Location**: `byroredux/src/systems.rs:1531-1541`
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-05-07_DIM15.md` § REN-D15-01
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/897

## Summary

`night_factor` for fog-distance lerp uses hardcoded hours (6, 18, 20, 4) while color breakpoints are CLMT TNAM-driven via `build_tod_keys(wd.tod_hours)` at `:1456`. On non-default-hour CLMTs (FO3 Capital Wasteland) palette and fog disagree by 0.3-2 h on day/transition.

## Fix sketch

Derive `night_factor` from `slot_b` of the TOD keys table:

```rust
let night_factor = match slot_b {
    s if s == TOD_NIGHT || s == TOD_MIDNIGHT => 1.0,
    s if s == TOD_DAY || s == TOD_HIGH_NOON => 0.0,
    _ => 0.5, // SUNRISE / SUNSET — half-night
};
```

~10 lines.

## Test

`weather_system` regression at FO3-style sunrise (5.7h) asserting `fog_near` matches day-fog distance.

## Status

NEW. CONFIRMED via line-walk during the 2026-05-07 Dim 15 focused audit.
