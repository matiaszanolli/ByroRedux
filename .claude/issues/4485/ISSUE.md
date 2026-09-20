# EXT-D4-2026-09-19-03: HNAM sunlight_dimmer consumer multiply is unpinned — every fixture builds dimmer 1.0

- **ID**: EXT-D4-2026-09-19-03
- **Labels**: medium,terrain-exterior,bug,test-gap,game:oblivion
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4485

**Severity**: MEDIUM · **Dimension**: Weather/sun (test-gap) · **Tier Violated**: single-boundary (boundary value unpinned through its consumer) · **Game Affected**: Oblivion (latent elsewhere)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D4-2026-09-19-03; the skill's 2026-09-19 seed, verified)

**Location**: `byroredux/src/systems/weather.rs:805-807` (the multiply); fixtures `:2060-2061`, `:2309-2310`, `:2418-2419`

**Description**
The only guard, `sunlight_dimmer_translates_from_the_hnam_block` (`env_translate.rs:3385`), pins the *translation*. The consumer-side multiply that carries the dimmer onto `CellLightingRes.directional_color` is exercised exclusively with `sunlight_dimmer: 1.0` — all three `WeatherDataRes` fixtures hard-code 1.0 — so the multiply is an identity in every test and no test can detect a dropped, double-applied, or misplaced dimmer. This is the gap EXT-D4-2026-09-19-02 slipped through (landed 2026-09-18 with the multiply unpinned).

**Impact**
A regression of the dimmer chain passes the suite silently on all games; on Oblivion it surfaces as wrong exterior sun brightness.

**Suggested Fix**
One `weather_system` test with `sunlight_dimmer: 0.5` and SKY_SUNLIGHT = 1.0 asserting `cell_lit.directional_color == 0.5`, plus the post-promotion pin from EXT-D4-2026-09-19-02's fix.

## Completeness Checks
- [ ] **TESTS**: The new fixture also covers `grass_dimmer` reaching `GroundCoverDimmer`
