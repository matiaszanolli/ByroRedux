# Issue #3454: REN-2026-08-27-D18-01: weather_system still carries a pre-#1199 comment claiming unload_cell removes SkyParamsRes on every cell unload — the invariant #3323 now depends on

**Filed**: 2026-08-27 via /audit-publish from `docs/audits/AUDIT_RENDERER_2026-08-27.md`

**Severity**: LOW
**Dimension**: Sky / weather / exterior lighting
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-27.md` (REN-2026-08-27-D18-01)
**Status**: NEW

## Location
`byroredux/src/systems/weather.rs:725-727` (the `#803` cloud-scroll comment)

## Description
The comment reads *"cloud scroll lives on `CloudSimState`, which survives cell transitions (unlike `SkyParamsRes`, which `unload_cell` removes on every cell unload)"*. That parenthetical was true before #1199 and is false now: `byroredux/src/cell_loader/unload.rs:166-178` documents at length that `SkyParamsRes` / `CellLightingRes` / `WeatherDataRes` are worldspace-scoped with World lifetime and are deliberately **not** released per cell.

## Evidence
`unload.rs:166-178` — *"`SkyParamsRes` / `CellLightingRes` / `WeatherDataRes` / `WeatherTransitionRes` … are worldspace-scoped — acquired once by `apply_worldspace_weather` … at streaming bootstrap, not per cell load. The pre-#1199 pattern released them on every cell unload … Their lifetime now matches the World."* No `remove_resource::<SkyParamsRes>()` exists on any unload path.

## Impact
Documentation only, but newly load-bearing: #3323's entire correctness argument for `GpuCamera.exterior_sky_tint` rests on `SkyParamsRes` surviving the transition into an interior so `weather_system` can keep advancing the exterior TOD colour the window-portal escape reads (`byroredux/src/render/sky.rs:58-71`). A reader who trusts the stale parenthetical would conclude #3323 is a no-op on the exact path it was written for, and could "fix" the wrong end.

## Related
#1199, #3323, #803

## Suggested Fix
Drop the parenthetical or replace it with the current contract ("`SkyParamsRes` is likewise World-lifetime since #1199; this accumulator is separate because …").

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other comments asserting the pre-#1199 per-cell release of worldspace-scoped resources)
