# #5424: EXT-D1-2026-10-08-03: The per-cell `XCCM` override path does not apply the #5363 WTHS `DefaultWeather` stand-in — two default-weather rules, and `resolve_default_weather`'s "same rule" doc is now false

**Labels**: low,terrain-exterior,bug,game:starfield
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5424

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-08.md` — `EXT-D1-2026-10-08-03` (HEAD `00f580e09`)

- **Severity**: LOW. It is vanilla-latent: the Starfield.esm census found 0 exterior cells authoring `XCCM`, and the 36 `XCCM` cells it found are all interior.
- **Dimension**: EXAL boundary discipline
- **Location**:
  - `byroredux/src/scene/world_setup.rs:486-514` (`apply_cell_climate_override` → bare `resolve_default_weather`; "keeps the current sky" when it is `None`).
  - `byroredux/src/cell_loader/exterior.rs:1925-1962` (the worldspace path with the stand-in).
  - Doc: `byroredux/src/env_translate.rs:480-487`.
- **Status**: NEW (introduced by `424dfe0e1`)
- **Tier Violated**: single-boundary
- **Game Affected**: Starfield (mods / future content)
- **Description**:
  - On a WTHS-only worldspace climate, entering an `XCCM` pocket and leaving it re-resolves the worldspace climate.
  - `resolve_default_weather` returns `None` there, so the override path keeps the pocket's weather for the rest of the session instead of restoring the `DefaultWeather` stand-in. An `XCCM` that targets a WTHS-only climate is never applied, so its TNAM clock is lost.
- **Suggested Fix**:
  - Move the stand-in into `resolve_default_weather` (or a shared `resolve_climate_weather`) so both call sites use one rule.
  - Add a test: override in, then out, on a WTHS-only climate.

## Completeness Checks
- [ ] **SIBLING**: Both default-weather call sites (worldspace path and `apply_cell_climate_override`) use the one shared rule
- [ ] **TESTS**: A regression test pins this specific fix
