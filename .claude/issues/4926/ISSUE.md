# 4926: EXT-D4-2026-09-27-04: The WTHR cross-fade does not blend the sun arc — the sun snaps on completion when the climates' TNAM differ

labels: bug, low, terrain-exterior
state: OPEN

**Severity**: LOW
**Dimension**: Sky, weather, sun
**Tier Violated**: n/a
**Game Affected**: worldspace-boundary cross-fades between differing climates
**Status**: NEW (#1018 fixed this class for fog only)
**Location**:
`byroredux/src/systems/weather.rs:1019`; promotion at `:1290`, `:1312`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
The arc uses the source's `tod_hours` for the whole 8 s fade, then jumps. It was not confirmed on real data that a door crosses between two such climates.

## Suggested Fix
Blend both arcs by `transition_t`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **LOCK_ORDER**: `WeatherDataRes` / `WeatherTransitionRes` / `SkyParamsRes` / `CellLightingRes` acquisition order preserved (#3263, #1410)
- [ ] **TESTS**: A regression test pins this specific fix

