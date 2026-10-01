# Issue #4928

**Title:** EXT-D4-2026-09-27-06: A duplicated 4-slot TOD fold (`cloud_tod_slot` vs `fold_to_four_tod_slots`) and a stale lock-order comment
**State:** OPEN
**Labels:** bug, low, tech-debt, terrain-exterior

**Severity**: LOW
**Dimension**: Sky, weather, sun
**Tier Violated**: n/a
**Game Affected**: n/a
**Status**: NEW
**Location**:
`byroredux/src/systems/weather.rs:473-484` vs `:704-711`; the comment is at `:1211-1213` (it says `wd` is live, but it was dropped at `:1099`)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Suggested Fix
Call `fold_to_four_tod_slots` from the cloud path, and fix the comment.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **LOCK_ORDER**: `WeatherDataRes` / `WeatherTransitionRes` / `SkyParamsRes` / `CellLightingRes` acquisition order preserved (#3263, #1410)
- [ ] **TESTS**: A regression test pins this specific fix

