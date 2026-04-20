# Issue #463

FO3-6-05: Climate TNAM sunrise/sunset hours parsed but not consumed by weather_system

---

## Severity: Low

**Location**: `crates/plugin/src/esm/records/climate.rs:89-95`, `byroredux/src/systems.rs` (weather_system)

## Problem

`ClimateRecord` exposes TNAM fields (`sunrise_begin`, `sunrise_end`, `sunset_begin`, `sunset_end` — each a 10-minute-unit byte), but `weather_system` at `scene.rs:186-215` populates `WeatherDataRes` with hardcoded time-of-day slots and interpolates sky_colors without a climate-driven schedule.

FO3 Capital Wasteland and FNV Mojave ship slightly different values — Wasteland has an earlier sunrise.

## Impact

Minor aesthetic drift — post-war DC sky transitions on Mojave clock, ~0.3 hr off on sunrise. Not a gameplay blocker.

## Fix

Thread `ClimateRecord` into `WeatherDataRes` so the TOD interpolator uses per-climate hour breakpoints. Replace hardcoded TOD slots with `climate.sunrise_begin..=climate.sunset_end` with linear interpolation across the 6 existing weather slots.

## Completeness Checks

- [ ] **TESTS**: Load FO3 cell, verify `WeatherDataRes.sunrise_hour` matches CLMT data (not hardcoded default)
- [ ] **SIBLING**: FNV + Oblivion + Skyrim all have CLMT — single fix benefits all
- [ ] **DOCS**: Climate system notes in engine docs

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-6-05)
