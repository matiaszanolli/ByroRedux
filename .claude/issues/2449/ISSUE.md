# EXAL-01: WRLD NAM3/NAM4 LOD-water is parsed but no LOD-ring water plane is ever spawned

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2449
**Finding ID**: EXAL-01 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 5 — EXAL
**Location**: `crates/plugin/src/esm/cell/wrld.rs:151-169`; `crates/plugin/src/esm/cell/mod.rs:844-861`; `byroredux/src/cell_loader/water.rs:77`; `terrain_lod.rs`, `object_lod.rs`
**Status**: NEW (concrete sub-finding under open epic #2373)

## Description
`WorldspaceRecord::lod_water_form`/`lod_water_height` (landed #1849) are read by nothing. `spawn_water_plane` has exactly two production call sites (full-detail exterior cell, interior cell) — none of the four LOD providers emits a water surface. The parser's own doc comment records NAM3≠NAM2 on 18/28 Fallout3.esm worldspaces and NAM4≠DNAM on 22/30 Skyrim.esm worldspaces, so full-detail values cannot substitute.

## Evidence
Confirmed directly: `lod_water_form`/`lod_water_height` are parsed and stored (`mod.rs:854,860`) but grep for their names outside the parser tree returns zero hits. `spawn_water_plane` call sites: `cell_loader/load.rs:439`, `cell_loader/exterior.rs:1022` — both full/interior only.

## Impact
Open-world oceans/lakes terminate at the streaming ring boundary on every game — the classic "dry ocean" artifact beyond `radius_unload`. A naive fix reusing NAM2/DNAM would place the LOD sheet at the wrong Z on the majority of worldspaces.

## Related
#2373 (OPEN, EX-12/13 — exterior sky/weather/fog/water/parent-world continuity epic this is a concrete sub-finding under).

## Suggested Fix
Add a `translate_lod_water(wrld)` arm to `env_translate.rs` reading NAM3/NAM4 (with the documented Oblivion `None` sentinel), spawn one large `IsLodTerrain`-marked water quad per ring clipped to exclude the full-detail radius.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: New water-LOD translation lives in `env_translate.rs`, matching the EXAL boundary convention
- [ ] **TESTS**: A regression test confirms LOD water spawns on a worldspace where NAM3≠NAM2 and lands at the NAM3-derived height, not NAM2's
