# EXT-D1-2026-09-27-04: env.health FAILs `is_interior/is_exterior` on every interior entered after an exterior

**Issue**: #4915
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,bug

**Severity**: LOW
**Dimension**: EXAL boundary discipline
**Tier Violated**: n/a
**Game Affected**: all
**Status**: NEW (check dates from `f90e4eec1`)
**Location**:
`byroredux/src/commands/env_health.rs:349-358`; `byroredux/src/commands/env_health_tests.rs:175-181`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- `SkyParamsRes` survives into interiors by design (#1199), and every producer sets `is_exterior: true`.
- The "consistent interior pair" fixture uses a sky that production never creates.

## Impact
False `env: FAIL` on interiors. It must land with EXT-D1-02's fix.

## Suggested Fix
Fire only when `!lit.is_interior && !sky.is_exterior`, and replace the fixture.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
