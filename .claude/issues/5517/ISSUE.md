# #5517: EXT-D1-2026-10-09-02: `wths_only_climate_resolves_the_defaultweather_stand_in`'s "both callers share the rule" assertion compares `resolve_default_weather` with itself

**Labels**: bug, low, terrain-exterior, test-gap

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-09.md` — finding `EXT-D1-2026-10-09-02` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW (test-gap)
- **Dimension**: EXAL boundary discipline
- **Location**: `byroredux/src/env_translate.rs:5447` and `:5459-5465` (`default_weather_rule_tests`)
- **Status**: NEW (introduced by `302394f94`). It has the same shape as #5344.
- **Tier Violated**: n/a
- **Game Affected**: Starfield (the WTHS stand-in)
- **Description**:
  - `worldspace_pass` is `resolve_default_weather(&climate, &weathers)`, the identical call with the identical arguments as `resolved` three statements earlier. The equality can never fail.
  - What #5424 needed pinned is that both production callers route through the rule:
    - `build_exterior_world_context` (`cell_loader/exterior.rs:1902`)
    - `apply_cell_climate_override` (`scene/world_setup.rs:497`)
  - Nothing pins either call site. A re-inlined pick in either one, which is how #5424 arose, would still pass.
- **Impact**: The #5424 regression guard is vacuous for its stated purpose. The stand-in's own value is still pinned by the other two assertions.
- **Related**: #5424, #5344.
- **Suggested Fix**: Replace the self-comparison with a `source_scan::production_text` pin. It should assert that both caller files call `resolve_default_weather` and that neither names `default_weather_by_edid`. Alternatively, drive `apply_cell_climate_override`'s pure part through a fixture.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
