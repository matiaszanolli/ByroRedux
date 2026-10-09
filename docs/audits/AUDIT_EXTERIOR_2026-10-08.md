# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD) — 2026-10-08

**HEAD**: 00f580e09 · **Baseline**: [AUDIT_EXTERIOR_2026-10-05.md](AUDIT_EXTERIOR_2026-10-05.md) (HEAD `a2c24b16e`, 116 commits since) · **Audited**: Dim 1 (EXAL boundary), Dim 4 (sky / weather / sun), Dim 5 (WATAL), Dim 6 (distant LOD) · **Unchanged since baseline (skimmed)**: Dim 2 (terrain, no commits), Dim 3 (ground cover, only the `SHADER_DEFINES` table refactor), Dim 7 (gates, harness-plumbing commits only)

**Method.** This run is part of `/audit-suite --preset comprehensive`.
- One auditor did every dimension synchronously. No sub-agents were used. No engine launch, smoke script or capture.
- Delta scope: `git log a2c24b16e..HEAD -- <Paths>` for each dimension. The exterior-relevant commits are:
  - `1162236fc`: FO3/FNV weather IMAD fold. Its message says "Add examples…", but it also changes `env_translate.rs`, `WeatherDataRes.image_space` (4 → 6 slots), the IMAD channel map and the presentation tint.
  - `b4497ec1d`: Oblivion climate rungs.
  - `424dfe0e1`: Starfield WTHS stand-in (#5363).
  - `c60405083`: sun-disc floor.
  - `56786698b`: sunset hold key.
  - `b7987d813`: #5222 legacy LOD index.
  - `54d713dee`: streaming split. It is a pure move; the lod-water code is unchanged.
- Read-only Python censuses (scripts in the suite scratch dir):
  - Oblivion.esm: WRLD `CNAM`/`NAM2`/`WNAM`; REGN `CNAM`; CLMT WLST/TNAM/FNAM; LAND VHGT minimum vs XCLW for each child worldspace.
  - FO3/FNV mesh and texture BSA name tables: legacy LOD quad overlap.
  - Starfield.esm: CELL `XCCM`.
  - Three washmontop LOD blocks were extracted and imported (`extract_nif` / `import_probe` examples) to measure their vertex bounds.

**Test state (green at HEAD, rustc 1.96.0):**

| Suite | Result |
|---|---|
| `cargo test -p byroredux --bin byroredux -- env_translate env_health terrain groundcover weather water lod resident_vwd sky spawner climate` | 570 passed, 0 failed, 20 ignored. All 20 need game data or a Vulkan device; none is a muted guard. |
| `cargo test -p byroredux-renderer --lib -- groundcover sky_ water terrain memory_budget shader_constants` | 252 passed, 0 failed, 1 ignored (the Vulkan prefilter test, not run because `sky_prefilter.comp` is unchanged) |
| `--ignored --exact` real-data: the 3 new #5222 FO3 tests, `distant_water_mesh_is_per_cell_on_real_fnv`, `water_sentinels_hold_on_real_watr_per_game`, `riverwater_flowne_real_record_layers_run_downstream`, `default_land_textures_exist_in_vanilla_archives`, `oblivion_ltex_paths_exist_in_vanilla_archives` | 8 passed (peak RSS 1.6 GB) |
| `scripts/check-shader-artifacts.sh` | 36 shaders + the opaque early variant match glslang 11:16.4.0 (byte parity with 16.2.0) |

## Executive Summary

**6 findings, all NEW: 0 CRITICAL · 1 HIGH · 2 MEDIUM · 3 LOW.** All baseline findings are tracked and open (#5334, #5335, #5337, #5339, #5341, #5343, #5344), and the code behind them is unchanged.

**Headlines.**
- **EXT-D5-01 (HIGH, pre-existing, first found now): Oblivion's 30 child worldspaces get no default water.**
  - `default_water_for_worldspace` inherits only through FO3+ `PNAM` bits. Oblivion has no PNAM, but its data shows implicit inheritance: **54/54** root WRLDs author `NAM2` and **0/30** children do.
  - Every child cell without its own `XCLW` therefore renders dry. That includes **33 of Bravil's 47** LAND cells below sea level, 22 in Leyawiin, 9–29 in each of the 9 Imperial City districts, and 11 in New Sheoth.
- **EXT-D6-01 (MEDIUM): #5222's authored-index selection assumes each level's quads tile the ground.** Vanilla FO3 ships overlapping, stale object-LOD quads, and the selection draws all of them.
  - Washington Monument top: 37 level-8 quads, 65 overlapping pairs.
  - Four DC worldspaces and DLC02BaileysCrossroads are also affected.
  - Three measured washmontop blocks contain the same geometry corner, so the overlap draws duplicate geometry.
- **EXT-D1-01 (MEDIUM): `b4497ec1d`'s CS-naming rung tests only the child worldspace's own name.** Every Oblivion child therefore falls to the "richest" climate, `AllWeather`.
  - The 25 Tamriel children happen to match only through a `max_by_key` tie-break.
  - The 5 Shivering Isles children render Tamriel's Clear weather instead of SEWorldClimate's.

**The fix wave holds.**
- The FO3/FNV IMAD fold is correct where it sits:
  - It lives at the boundary (`ImageSpaceSources::resolve`) and is parse-gated to `Fallout3NV`.
  - The `promote_weather_transition_target` destructure still carries `image_space`, and `env.health` scans all 6 slots.
  - There is one non-test `ImageSpaceSources {}` site (`world_setup.rs:379`).
- The sun-disc floor never fires at night (intensity 0, direction `[0,-1,0]`).
- Every `build_tod_keys` consumer is in `weather.rs` and was updated to the 8-key table. The clamps keep the keys strictly increasing.
- The WTHS stand-in is data-gated (`seasonal_weathers` non-empty), not game-gated.

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| **single-boundary** | **Holds for sky/weather/water producers. Dented for climate.** Resolution now has six rungs in two files: `env_translate::resolve_worldspace_climate` / `resolve_cell_climate`, plus `cell_loader/exterior.rs` `region_climate_for_center` / `named_or_richest_climate` and the `default_weather_by_edid` stand-in. The per-cell `XCCM` path skips the stand-in (D1-03). |
| **no-fabrication** | **Mostly holds.** The 1.8× disc floor and the sunset hold are documented engine choices in code comments, but skyal.md records neither (D4-01). The "richest climate" fallback is a heuristic; it is acceptable only as a last rung, and today it fires for every Oblivion child (D1-01). |
| **no-leak** | **Holds.** `image_space_modifiers` is all-`None` off FO3/FNV. The IMAD fold leaves IMSP games untouched. |
| **no-render-time-fallback** | **Holds in GLSL.** Sky, cloud and ground-cover shaders carry game names only in comments. **Three new Rust `game ==` logic branches** bypass the table-shaped scheme functions (D1-02). None is per-frame or render-side. |

## Per-Category Matrix

✓ = holds, ~ = dented (see the Findings column).

| Category (boundary fn) | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Findings |
|---|:---:|:---:|:---:|:---:|---|
| **Terrain / splatting** (`spawn_terrain_mesh` + `build_cell_splat_layers`) | ✓ | ✓ | ✓ | ✓ | none (unchanged) |
| **Sky** (`translate_sky`, `pack_sky_dome`, `sky_radiance` disc floor) | ✓ | ~ D4-01 (undocumented 1.8) | ✓ | ✓ | D4-01 |
| **Weather / sun / image space** (`translate_weather` + `ImageSpaceSources::resolve`, `build_tod_keys`) | ✓ | ✓ | ✓ | ✓ | D4-01 (doc) |
| **Climate** (`resolve_worldspace_climate`, `region_climate_for_center`, `named_or_richest_climate`, `resolve_cell_climate`) | ~ D1-02, D1-03 | ~ D1-01 | ✓ | ~ D1-02 | D1-01, D1-02, D1-03 |
| **Water** (`default_water_for_worldspace`, `resolve_water_material`, `build_distant_water_mesh`) | ✓ | ✓ | ✓ | ✓ | **D5-01**, plus open #5334/#5335/#5337/#5339/#5341/#5343 |
| **Ground cover** (`groundcover_translate.rs`, `resolve_authored_cover`) | ✓ | ~ #4906 (open) | ✓ | ✓ | none new |
| **Distant LOD** (`select_lod_quads`, `select_authored_lod_quads`, `terrain_lod_layout`, `object_lod_scheme`) | ✓ | ✓ | ✓ | ~ D1-02 (`game == Fallout3NV`) | D6-01 |

## Findings

### EXT-D5-2026-10-08-01: Oblivion child worldspaces inherit no default water — `default_water_for_worldspace` models inheritance only through FO3+ `PNAM`, so Bravil's canals, Leyawiin's river, the Imperial City lake shore and New Sheoth render as dry beds
- **Severity**: HIGH. A wrong canonical value comes out of an EXAL boundary producer, and the skill's EXAL row sets HIGH. The missing water is in flagship content: the Imperial City plus two major cities.
- **Dimension**: Water translation (WATAL)
- **Location**:
  - `byroredux/src/env_translate.rs:209-238` (`default_water_for_worldspace`). Line 217: `inherit_up_chain(.., pnam::INHERIT_WATER, ..)`. Lines 220-229: the Oblivion arm, whose comment says "Oblivion authors no PNAM either, so the chain walk is a no-op there".
  - `crates/plugin/src/esm/cell/wrld.rs:103-112` (`parent_flags` stays 0 when PNAM is absent).
  - Consumer: `byroredux/src/cell_loader/exterior.rs:1972-1976` and `:71-81` (`resolved_exterior_water_height`).
- **Status**: NEW. It is pre-existing: #2735 recorded "Oblivion is unaffected (it authors no PNAM)", and that is the premise this finding disproves. Searches for "BravilWorld", "Oblivion child worldspace", "Oblivion parent worldspace water", "city worldspace water" and "Oblivion PNAM" found no tracker.
- **Tier Violated**: n/a. The translation itself is wrong: no PNAM is read as "inherit nothing", but in Oblivion it means "inherit everything".
- **Game Affected**: Oblivion (including the Shivering Isles worldspaces in Oblivion.esm)
- **Description**:
  - A child worldspace with no own `NAM2` resolves `(None, None)`. Every cell without `XCLW` then gets no water plane.
  - Oblivion child worldspaces never author `NAM2`, `CNAM` or any PNAM.
  - The data has the same "absent ⟺ inherited" shape that #2735 used to establish PNAM semantics in the later games.
- **Evidence** (Oblivion.esm census):
  - **54/54** root WRLDs author `NAM2`; **0/30** child WRLDs (`WNAM` set) do. 27/54 roots author `CNAM`, 0/30 children do.
  - LAND cells with a VHGT minimum below Z=0 (Tamriel's sea level) and no `XCLW`:

    | Worldspace | Cells below Z=0, no XCLW / LAND cells |
    |---|---|
    | BravilWorld | 33/47 |
    | LeyawiinWorld | 22/38 |
    | ICTalosPlazaDistrict | 29 |
    | ICElvenGardensDistrict | 21 |
    | ICTempleDistrict | 15 |
    | ICImperialPrisonDistrict | 14 |
    | ICArboretumDistrict | 13 |
    | ICMarketDistrict, ICImperialPalace, ICTheArcaneUniversity | 12 each |
    | AnvilCastleCourtyardWorld | 10 |
    | ICArenaDistrict | 9 |
    | AnvilWorld | 7 |
    | SETheFringe (New Sheoth) | 11 |
    | SENSBliss | 4 |
    | SENSCrucible | 3 |
    | SEVitharnWorld | 2 |

  - Bruma, Cheydinhal, Skingrad, Chorrol and Kvatch have 0 such cells and are unaffected.
- **Impact**: Visible missing water in Bravil, Leyawiin, Anvil, all Imperial City districts and New Sheoth. The near field shows exposed canal and lake beds. Tamriel itself is correct (its own `NAM2` gives Z=0).
- **Related**: #2735 (closed; premise disproved); #1305; EXT-D1-2026-10-08-01 (same root cause, climate half); #5335 (Oblivion distant water).
- **Suggested Fix**:
  - At the parse/translate boundary, give Oblivion children (`parent_worldspace` set, no PNAM) an inherit-all parent flag word. One table-shaped rule (e.g. "pre-FO3: child ⟹ inherit LAND|WATER|CLIMATE") lets `inherit_up_chain` resolve `NAM2` from Tamriel / SEWorld.
  - Pin it with an Oblivion-shaped fixture (child without NAM2 → parent's form, Z=0) and correct the #2735 comment at `env_translate.rs:222-224`.

### EXT-D6-2026-10-08-01: #5222's authored-quad selection draws overlapping stale FO3 object-LOD quads — `select_authored_lod_quads` assumes a level tiles, but vanilla ships same-level quads whose footprints and geometry overlap
- **Severity**: MEDIUM. Duplicate distant geometry (z-fight, double cost) on shipped worldspaces.
- **Dimension**: Distant LOD and trees
- **Location**:
  - `byroredux/src/cell_loader/lod_bands.rs:441-496` (`select_authored_lod_quads`). Doc line 441: "Assumes a level's authored quads tile the ground they cover". There is no intra-level overlap check; the only suppression (`:482`) is against the immediately coarser level.
  - `byroredux/src/cell_loader/object_lod.rs:200-237` (the FalloutLegacyBlocks arm and the footprint-based `quad_intersects_full_detail` retain).
- **Status**: NEW. It was introduced by `b7987d813`; #5222 is closed and its fix is incomplete.
- **Tier Violated**: n/a
- **Game Affected**: Fallout 3. FNV census: 0 overlaps. Terrain diffuse quads: 0 overlaps in both games.
- **Description**:
  - The index enumerates every plain quad file in the archive name tables.
  - In six FO3 worldspaces, a single level holds quads on several residues whose 8- or 4-cell footprints overlap. They look like leftovers from different LOD generation runs.
  - With no finer level (washmontop is level-8 only), `any_finer` is false and every in-ring quad is selected. All overlapping blocks are drawn.
  - The geometry overlaps too, not just the names:
    - Washmontop blocks `x12.y-25` / `x12.y-22` / `x12.y-20` all reach the same far corner (world X ≈ 82,755, Y = −82,631 BU).
    - Their triangle counts nest (3,696 / 6,694 / 9,078).
    - `y-20`'s geometry also runs past its footprint (to cell y ≈ −11.6 vs the footprint edge −12), so the footprint-based full-detail retain can miss it.
- **Evidence** (FO3 BSA name tables; pairs of same-level quads with overlapping footprints):

  | Worldspace | Level | Quads | Overlapping pairs | Residues |
  |---|---|---|---|---|
  | washmontop (The Washington Monument) | 8 | 37 | 65 | (4,2), (4,4), (4,7) |
  | dcworld12 (Seward Square) | 8 | 8 | 6 | |
  | dcworld17 (Falls Church) | 8 | 6 | 5 | |
  | dcworld06 (Vernon Square) | 8 | 6 | 4 | |
  | dcworld01 (Chevy Chase) | 8 | 3 | 2 | |
  | dlc02baileyscrossroads | 4 | 14 | 11 | |
  | tlandscape (test world) | 4 | 48 | 168 | |

  - The unit tests use synthetic tilings. `washmontop_level8_spans_multiple_residues_and_all_select` asserts only that each quad is selected when standing in it, and never checks for overlap.
- **Impact**:
  - Duplicated, z-fighting distant buildings at the Washington Monument top, the vista worldspace where LOD matters most, and in four DC worldspaces.
  - The double draws also add GPU cost.
  - The live visual verification of #5222 is still owed, so no gate has seen this.
- **Related**: #5222 (closed, incomplete); #3502; #4468 (`.high.` variants, handled correctly).
- **Suggested Fix**:
  - Resolve same-level overlaps before selection. For example, per (worldspace, level), keep the largest mutually non-overlapping subset, or the majority residue class. Alternatively, prefer the block whose geometry bounds fit its own footprint.
  - Add a real-data test asserting that no two selected same-level quads overlap on washmontop.

### EXT-D1-2026-10-08-01: The Oblivion CS-naming climate rung tests only the child worldspace's own name — all 30 child worldspaces resolve the "richest" climate `AllWeather`; New Sheoth and the other SEWorld children lose SEWorldClimate
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

### EXT-D1-2026-10-08-02: Three new `game ==` logic branches bypass the table-shaped scheme functions, and climate resolution is now split across `env_translate.rs` and `cell_loader/exterior.rs`
- **Severity**: LOW (discipline; behaviour is correct today)
- **Dimension**: EXAL boundary discipline
- **Location**:
  - `byroredux/src/cell_loader/exterior.rs:1894`: `(record_index.game == GameKind::Oblivion).then(..)`.
  - `byroredux/src/cell_loader/terrain_lod.rs:462`: `let desired = if game == GameKind::Fallout3NV {`.
  - `byroredux/src/streaming_helpers.rs:119`: `&& wctx.record_index.game == GameKind::Fallout3NV`.
- **Status**: NEW (introduced by `b4497ec1d` and `b7987d813`)
- **Tier Violated**: no-render-time-fallback (table-shape rule) and single-boundary (climate)
- **Game Affected**: Oblivion, FO3/FNV (structural)
- **Description**:
  - The skill allows `GameKind` only as table-shaped `match` returning data.
  - `object_lod.rs` correctly gates on `ObjectLodScheme::FalloutLegacyBlocks`. The terrain ring and the index scan instead hard-code `Fallout3NV`, bypassing `terrain_lod_layout` and its FalloutLegacy layout. A future title that adopts the legacy layout would silently take the descent path in one ring and the authored path in the other.
  - The Oblivion naming / richest rungs live beside the cell loader rather than next to `resolve_worldspace_climate` in `env_translate.rs`. exal.md says that is where climate resolution is "settled at the boundary".
- **Suggested Fix**:
  - Gate both rings and the scan on the layout/scheme tables.
  - Move `region_climate_for_center` / `named_or_richest_climate` into `env_translate.rs` as one `resolve_exterior_climate`, with a table-shaped per-game rung list.

### EXT-D1-2026-10-08-03: The per-cell `XCCM` override path does not apply the #5363 WTHS `DefaultWeather` stand-in — two default-weather rules, and `resolve_default_weather`'s "same rule" doc is now false
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

### EXT-D4-2026-10-08-01: EXAL/SKYAL doc rot after `b4497ec1d` / `424dfe0e1` / `1162236fc` / `c60405083` / `56786698b`
- **Severity**: LOW (doc rot)
- **Dimension**: Sky, weather, sun (plus the Dim 1 docs)
- **Location**:
  - `docs/engine/exal.md:128-138` (climate resolution).
  - `docs/engine/exal.md:215-225` (image space).
  - `docs/engine/exal.md:790` (CLMT "carries only WLST").
  - `docs/engine/skyal.md` (no entry for either sky change).
- **Status**: NEW
- **Tier Violated**: no-fabrication (documentation)
- **Game Affected**: all (docs)
- **Description**:
  1. exal.md says climate "has two inputs, both settled at the boundary in `env_translate`". There are now also the REGN rung, the Oblivion naming / richest rungs (in `cell_loader/exterior.rs`) and the WTHS `DefaultWeather` stand-in.
  2. The image-space description covers IMSP → INAM → identity. It omits the FO3/FNV per-TOD weather IMAD fold, the 6-slot table, and the FO3/FNV IMAD channel remap (`0x12` = contrast pivot, `0x14` = brightness). The `1162236fc` message ("Add examples …") does not mention these either, so `git log` archaeology also misses them.
  3. CLMT now parses `WSLT` (`ClimateRecord.seasonal_weathers`).
  4. skyal.md does not record the sun-disc luminance floor (`sky.glsl:425-430`, the literal `1.8`, a documented engine choice only in the code comment) or the sunset hold key (`weather.rs:95`/`:103`). Both shape every golden-hour capture.
- **Suggested Fix**:
  - Update exal.md's climate paragraph and image-space step 3 to describe the rungs and the IMAD fold, and note `WSLT`.
  - Add a skyal.md entry for the disc floor (with its 1.8 rationale) and the sunset hold.

## Findings count

| Dimension | CRITICAL | HIGH | MEDIUM | LOW |
|---|---|---|---|---|
| D1 EXAL boundary | — | — | 1 | 2 |
| D2 Terrain (skimmed) | — | — | — | — |
| D3 Ground cover (skimmed) | — | — | — | — |
| D4 Sky / weather / sun | — | — | — | 1 |
| D5 WATAL | — | 1 | — | — |
| D6 LOD / trees | — | — | 1 | — |
| D7 Gates / harness (skimmed) | — | — | — | — |
| **Total NEW** | **0** | **1** | **2** | **3** |

**Dedup sources:**
- `/tmp/audit/issues.json` (open set).
- `gh issue list --state all` searches: "BravilWorld", "Oblivion child worldspace", "Oblivion parent worldspace water", "city worldspace water", "Oblivion PNAM", "distant water", "EXT-*-2026-10-05".
- #2450 / #2735 bodies were read for their Oblivion premise.
- Not re-reported, per the suite brief:
  - REN-D5-2026-10-08-01 (DDPF_RGB channel masks on the Skyrim terrain LOD atlases).
  - ESM D2-02 (REGN `CNAM` / `WNAM` decode).
  - #5301 (Starfield fog lift).

## Known-Open Register (dated; what this pass changed)

| Item | Status this pass |
|---|---|
| #5334, #5335, #5337, #5339, #5341, #5343, #5344 (2026-10-05 findings) | OPEN; code unchanged (the streaming split moved `spawn_lod_water` / `recenter_lod_water` to `streaming/mod.rs` verbatim). |
| #5222 (FO3/FNV legacy LOD origin) | **CLOSED** by `b7987d813`. The fix is correct for enumeration and band selection. It is incomplete on intra-level overlap (D6-01). Live visual verification is still owed. |
| #5363 / #5364 (Starfield WTHS) | #5363 closed (`424dfe0e1`, stand-in); #5364 (WTHS decode) open. The stand-in skips the XCCM path (D1-03). |
| #4913 (Skyrim `.btt` tree LOD) | OPEN, unchanged. |
| #4906 (LTEX→GRAS keying) | OPEN, unchanged. |
| #5170 / #5171 (Starfield WATR tile unit / LGTM lift pin) | OPEN, unchanged. |
| #3307 (VWD full-model culling) | OPEN, unchanged. |
| skyal §2.3–§3 documented-open items | Still documented; not re-filed. |
| `renderer-eval-groundcover.sh` washout / backlit re-mint | Not re-measured (no engine launch). Precheck unchanged (0.039). |

## Skill drift (for the next `/audit-exterior` edit)

- **Dim 1**:
  - Document the FO3/FNV weather-IMAD fold in `ImageSpaceSources::resolve` (6-slot `image_space`) and its guard `weather_imad_folds_over_the_worldspace_grade_per_slot`.
  - Record that "Oblivion only" rungs fire for every Oblivion child worldspace (D1-01).
- **Dim 4**: the sunset-hold and disc-floor text is already present. Add `tod_keys_hold_sunset_through_the_suns_descent` as a guard (it is live).
- **Dim 5**:
  - Add `byroredux/src/streaming/mod.rs` (`spawn_lod_water` / `recenter_lod_water`) to `Paths:`. This is the second report asking; the file moved from `streaming.rs`.
  - Record that Oblivion children inherit implicitly (D5-01).
- **Dim 6**: record the FO3 intra-level overlap census (D6-01) next to the #5222 text.

## Cross-audit routing

- **`/audit-oblivion`**: D5-01 and D1-01 (Oblivion child-worldspace inheritance).
- **`/audit-fo3`**: D6-01 (washmontop / DC worldspaces' overlapping LOD blocks).
- **`/audit-esm`**: if D5-01 is fixed at the parser, a synthesized `parent_flags` for pre-FO3 WRLD belongs in `wrld.rs`.
- **`/audit-renderer`**: `1162236fc`'s `presentation.frag` tint now blends toward luminance × tint (renderer-owned; not re-audited here).

Suggested next step: `/audit-publish docs/audits/AUDIT_EXTERIOR_2026-10-08.md`.

**Labels:**
- All findings: `terrain-exterior`.
- D5-01: `high`, `bug`, `water`, `game:oblivion`.
- D6-01: `medium`, `bug`, `game:fo3`.
- D1-01: `medium`, `bug`, `game:oblivion`.
- D1-02: `low`, `tech-debt`.
- D1-03: `low`, `bug`, `game:starfield`.
- D4-01: `low`, `documentation`, `doc-rot`.
