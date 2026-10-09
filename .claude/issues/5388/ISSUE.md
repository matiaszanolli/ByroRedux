# #5388: EXT-D1-2026-10-08-01: The Oblivion CS-naming climate rung tests only the child worldspace's own name — all 30 child worldspaces resolve the "richest" climate `AllWeather`; New Sheoth and the other SEWorld children lose SEWorldClimate

**Labels**: medium,terrain-exterior,bug,game:oblivion
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5388

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-08.md` — `EXT-D1-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: MEDIUM. The canonical climate is wrong for 30 worldspaces. It is visible on the 5 SEWorld children and invisible on the 25 Tamriel children only by accident.
- **Dimension**: EXAL boundary discipline
- **Location**:
  - `byroredux/src/cell_loader/exterior.rs:1869-1910` (rung order). Line 1894: the Oblivion gate.
  - `:2029-2044` (`named_or_richest_climate`: `format!("{}climate", worldspace_key)` uses only the child's key; the fallback is `max_by_key(weathers.len())`).
  - Default-weather pick: `byroredux/src/env_translate.rs:488-500` (`max_by_key(chance)` returns the last maximum).
- **Status**: NEW (introduced by `b4497ec1d`)
- **Tier Violated**: no-fabrication. The "richest" heuristic stands in for the authored parent climate.
- **Game Affected**: Oblivion
- **Description**:
  - Children have no own `CNAM`, and `resolve_worldspace_climate` does not walk them (parent_flags = 0, see D5-01). The region rung is inert: 0 of 211 REGNs author `CNAM` in Oblivion.esm.
  - "ICMarketDistrictClimate", "SETheFringeClimate" and similar names do not exist, so the rung falls to the climate with the most weathers: `AllWeather` (Overcast 25 / Cloudy 25 / Clear 25 / Rain 10 / Thunderstorm 5 / Snow 5 / Fog 5).
  - **Tamriel children** (25: the IC districts, Bruma/Bravil/Anvil/Leyawiin/Cheydinhal/Skingrad/Chorrol worlds, Kvatch…): the default weather comes out as Clear only because `max_by_key` returns the last of three 25 % ties. TNAM (6/10/16/20) and FNAM/GNAM (`Sky\Sun.dds` / `SunGlare.dds`) happen to equal TamrielClimate's. Any WLST consumer, or a reordering of AllWeather, would put snow and thunder over the Imperial City.
  - **SEWorld children** (5: SETheFringe = New Sheoth, SENSBliss, SENSCrucible, SENSPalace, SEVitharnWorld): their parent authors `CNAM` SEWorldClimate, whose default is SEFog (30). They get Tamriel's Clear palette instead.
- **Evidence**:
  - Oblivion.esm census: 30 children with `CNAM` absent.
  - The log line at `exterior.rs:1898-1906` reports "resolved through the region chain / Oblivion naming convention" for these worldspaces, which hides that the "richest" rung fired.
  - The unit test pins `region_climate_for_center` only; `named_or_richest_climate` has no test.
- **Impact**:
  - Wrong sky, fog and sun for the Shivering Isles city worldspaces.
  - A latent wrong climate for every Cyrodiil city.
- **Related**: EXT-D5-2026-10-08-01 (same root cause); ESM D2-02 (REGN `CNAM` decode, not re-reported here); #2450.
- **Suggested Fix**:
  - Walk the WNAM chain for Oblivion inheritance (D5-01's inherit-all rule makes `resolve_worldspace_climate` return SEWorldClimate for SE children).
  - Apply the naming convention to the chain's root key (`worldspace_name_chain`), so Tamriel children get TamrielClimate.
  - Demote "richest" to a logged last resort and test both rungs.

## Completeness Checks
- [ ] **SIBLING**: `region_climate_for_center` and the default-weather pick (`max_by_key(chance)` tie-break) reviewed alongside `named_or_richest_climate`
- [ ] **TESTS**: A regression test pins this specific fix
