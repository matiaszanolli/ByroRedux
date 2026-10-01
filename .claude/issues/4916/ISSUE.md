# 4916: EXT-D1-2026-09-27-05: env.health gates neither #4839's `sunlight_color` nor #4416's `image_space`

labels: bug, low, terrain-exterior, test-gap
state: OPEN

**Severity**: LOW
**Dimension**: EXAL boundary discipline
**Tier Violated**: no-leak (gate coverage)
**Game Affected**: all
**Status**: NEW
**Location**:
`byroredux/src/commands/env_health.rs:286-338`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
Both are new canonical floats that reach shaders unchanged. The #4731 completeness pin covers `WaterMaterial` only.

## Suggested Fix
Check the `WeatherSkyState` colours and scalars and the 4 image-space slots, and reuse #4731's serde-leaf completeness test.

## Related
#4483, #4731, #4840

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix

