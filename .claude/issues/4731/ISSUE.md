# EXT-D1-2026-09-21-01: env.health's WaterMaterial walk is a hand-kept field list with no struct-completeness pin

**Issue**: #4731
**Filed**: 2026-09-22 (audit-publish, AUDIT_EXTERIOR_2026-09-21.md)

**Severity**: LOW (test gap; the gate is complete today)
**Dimension**: EXAL boundary discipline
**Game Affected**: all
**Location**: `byroredux/src/commands/env_health.rs:149-228` (`check_water_plane`); struct at `crates/core/src/ecs/components/water.rs:158-360`

## Description
`check_water_plane` walks `WaterMaterial` as a hand-kept field list. #4483's fix names 10 radiance + 48 finite fields — confirmed that is all 58 `f32`-typed fields today, but nothing ties the list to the struct, so a newly added float field reaches `water.frag`'s push constants ungated.

## Evidence
`git log -S` shows five current fields arrived within a two-day burst (2026-08-19/20), so the struct grows in bursts.

## Impact
The next WATAL field lands unchecked by the `env: FAIL` gate the exterior smoke matrix consumes.

## Suggested Fix
Add a test that serialises `WaterMaterial::default()` (derives `Serialize` under `inspect`) and asserts every numeric leaf name appears in `check_water_plane`'s list, or pin a field-count const beside the list.

## Related
#4483 (closed)

## Source
docs/audits/AUDIT_EXTERIOR_2026-09-21.md (EXT-D1-2026-09-21-01)
