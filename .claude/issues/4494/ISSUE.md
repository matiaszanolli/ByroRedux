# EXT-D1-2026-09-19-04: uncited fallback constants in weather_sky_state / WeatherSkyState::default

- **ID**: EXT-D1-2026-09-19-04
- **Labels**: low,terrain-exterior,bug
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4494

**Severity**: LOW · **Dimension**: EXAL boundary · **Tier Violated**: no-fabrication (citation missing; values are engine choices applied uniformly)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D1-2026-09-19-04; same class as open #4314)

**Location**: `byroredux/src/env_translate.rs:1210-1216` (moon_glare fallback `0.35`), `:1231-1235` (`sun_glare == 0 → 1.0`); `byroredux/src/components.rs:1195-1210` (`WeatherSkyState::default`: `cloud_coverage: 0.35`, `stars_color`, `sun_glare: 1.0`, `moon_glare: 0.35`)

**Description**
The boundary's other engine-chosen constants all carry citations (`SUN_SOUTH_TILT` → exal.md §9 Q1; cloud coverage → skyal.md §2.3 value-exact; `FB_*` → pre-M40 defaults; `WIND_TO_SCROLL_RATE` → calibration note — all verified). The four WeatherSkyState constants above have no comment and no spec cite, and the 0.35 moon-glare fallback is duplicated verbatim in two files with neither citing the other.

**Impact**
Documentation-only today; the duplication can drift the same way previously triplicated constants did.

**Suggested Fix**
One doc note each (skyal.md §2 anchor + a "aliased by `WeatherSkyState::default`" cross-reference), mirroring the `FB_TOD_HOURS`/`DEFAULT_TOD_HOURS` pattern.

## Completeness Checks
- [ ] **TESTS**: N/A (docs), but confirm no third copy of 0.35 exists after the fix
