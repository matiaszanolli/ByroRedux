# #6309: EXT-D1-2026-10-21-05: A script-forced weather change made while the player is in an interior never reaches that interior's sky

**Labels**: low,terrain-exterior,scripting,bug

**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-21.md · **Severity**: LOW · **Dimension**: EXAL boundary discipline · **Location**: `byroredux/src/app_step.rs (step_streaming -> apply_forced_weather)`

- **Tier Violated**: n/a
- **Game Affected**: Oblivion, FO3, FNV (ObScript `ForceWeather` / `SetWeather` / `ReleaseWeatherOverride`)
- **Location**: `byroredux/src/app_step.rs:114` (the only `apply_forced_weather` call, inside `step_streaming`); `byroredux/src/cell_loader/load.rs:744` (Show-Sky install, at load only)
- **Status**: NEW (residual of closed #6028, `553428296`)
- **Description**: #6028 made every *install* honour `ForcedWeather` through `forced_or`. A *change* of the forced weather is only acted on by `apply_forced_weather`, and that runs only from `step_streaming`, which returns early unless an exterior `WorldStreamingState` exists. A door transition into an interior drains that state. So a `ForceWeather` issued inside a Show-Sky interior leaves the interior sky on the climate default until the next cell load. The same holds for an ordinary interior's window-portal sky (`portal_outdoor_sky` from `SkyParamsRes`), and for the release. GECK documents `ForceWeather` as an immediate switch.
- **Evidence**: `apply_show_sky_interior_climate` has one caller (`load.rs:744`). `step_streaming` returns when `self.streaming.is_none()`.
- **Impact**: Quest scripts that force weather while the player is indoors in a Show-Sky cell do not show the change until the next cell load. Exterior behaviour is correct.
- **Suggested Fix**: Track the applied forced weather outside the streaming state, for example on a resource. When it changes in an interior, re-run the Show-Sky install, or re-apply the worldspace environment for the portal palette, through the same `forced_or` plus `collapse_weather_transition`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
