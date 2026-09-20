# EXT-D5-2026-09-19-02: water sentinel-game-invariance guard pins 5 of ~15 sentinel fields via a no-parse literal

- **ID**: EXT-D5-2026-09-19-02
- **Labels**: medium,water,bug,test-gap
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4486

**Severity**: MEDIUM · **Dimension**: WATAL · **Tier Violated**: single-boundary (the §4 table's guard cannot catch its violation) · **Game Affected**: all
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D5-2026-09-19-02)

**Location**: `byroredux/src/env_translate.rs:3075-3150` (guard), `:2511-2537` (`calm_watr` helper)

**Description**
`resolve_water_material_sentinels_are_game_invariant` claims the WATAL translate-up invariant, but (a) it builds both "game-shaped" records through the hand-rolled `calm_watr` literal — `normal_encoding: Default::default()`, no `GameKind`, parser never involved — so per-game sentinel sets decided at parse (FO3/FNV `OffsetNoise`, Oblivion NNAM-vs-Skyrim TNAM roles, damage-sentinel rows) are structurally untestable there; and (b) it pins only 5 sentinel scalars (`ior`, `shoreline_width`, `uv_scale_a/b`, `foam_strength`) plus `normal_map_index == u32::MAX`. Unpinned sentinels: `noise_map_indices [u32::MAX;3]`, `DEFAULT_WATER_WAVE_*`, underwater fog/depth/alpha/absorption zeros, `blend_normals == true`, specular zeros, deep-tint fallback.

**Impact**
A sentinel promoted to authored (or vice versa) for any unpinned field — the exact "silently changes underwater rendering for a whole game" case this guard exists to stop — compiles and passes. The implementation itself verified correct against watal.md §4 row by row; the guarantee is weak, not the code.

**Related**: watal.md §4 (the table being guarded); watal.md §8 per-game sentinel harness (still open — the fix vehicle)

**Suggested Fix**
Decode one real WATR per game behind the existing `BYROREDUX_*_DATA` fixture pattern and diff each game's resolved SENTINEL-field set against `WaterMaterial::default()`; assert the full sentinel list, not 5 fields.

## Completeness Checks
- [ ] **SIBLING**: Check the harness also pins `WaterNormalEncoding` per game (set at parse)
- [ ] **TESTS**: The per-game fixture harness is itself the regression test
