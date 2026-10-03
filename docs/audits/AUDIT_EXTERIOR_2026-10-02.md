# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD) — 2026-10-02

**HEAD**: `c9f95283a` · **Baseline**: [AUDIT_EXTERIOR_2026-09-29.md](AUDIT_EXTERIOR_2026-09-29.md) (HEAD `9fcfdc3fc`, 189 commits since) · **Audited**: Dim 1–7 (all; every dimension's `Paths:` changed) · **Unchanged since baseline (skimmed)**: none

**Method.** There were about 30 exterior fix commits since the baseline, so this pass is mostly adversarial verification of those fixes.
- Dims 1, 2+3, 4 and 5+6 each went to a static-review agent (at most 3 at a time). Dim 7 was reviewed directly.
- The orchestrator re-checked every HIGH and MEDIUM claim against source before accepting it.
- Agents read source at `554259315`/`c9f95283a`. The commits after `7f2cee3e8` (#5165 debug-load path confinement, #5162 launcher settings) touch no exterior path.

**Game-data censuses** (Python, read-only):
- SeventySix.esm + NW.esm WATR (47 records);
- Starfield WTHR/WATR/LGTM;
- FNV + Skyrim SE LAND (BTXT/ATXT);
- FNV GRAS;
- FO4 + Skyrim XWCU REFR → ACTI → WATR;
- Oblivion WATR;
- Skyrim `Meshes1.bsa` `.btt` (386 files).

**Scope limits.**
- No engine launch, smoke script or capture.
- The GPU prefilter test was not run.
- `probe_lod_corpus` was not run.

**Test state (green at HEAD, rustc 1.96.0):**

| Suite | Result |
|---|---|
| `cargo test -p byroredux --bin byroredux -- env_translate env_health terrain groundcover weather water lod resident_vwd sky` | 552 passed, 0 failed, 16 ignored (all 16 are real-game-data tests; none is a muted guard) |
| `cargo test -p byroredux-renderer --lib -- groundcover sky_ water` | 164 passed, 0 failed, 1 ignored |
| `--ignored` exterior real-data tests: `water_sentinels_hold_on_real_watr_per_game`, `riverwater_flowne_real_record_layers_run_downstream`, `default_land_textures_exist_in_vanilla_archives`, `oblivion_ltex_paths_exist_in_vanilla_archives` | 4 passed against installed game data (peak RSS 1.6 GB) |
| `scripts/check-byro-dbg-harness-contracts.sh` | OK (50 scripts scanned) |

## Executive Summary

**22 findings: 0 CRITICAL · 1 HIGH · 2 MEDIUM · 19 LOW.**
- 2 duplicate open issues and are recorded as Existing: #5135 and #5136 (the latter partly resolved).
- 1 asks for #4913 to be reopened.
- 1 should be attached to open #4906.
- The other 18 are NEW.

**The fix wave holds.**
- 27 baseline-era issues were closed since `9fcfdc3fc`. These were re-read:
  - Dim 1: #4837, #4902, #4909, #4914–#4917, #5001, #5002, #5134, #5151
  - Dims 2–3: #4903, #4905, #4907, #4918–#4924, #4956, #4957, #5089, #5113
  - Dim 4: #4901, #4908, #4925, #4926, #4928, #5087
  - Dims 5–6: #4910–#4913
  - Dim 7: #5142
- **Correct and complete:** most of them, including every sky/weather fix and every single-boundary fix (#4902/#4914 closed the last render-loop rebuild and the caller-side IMGS patch).
- **Incomplete, with a sibling the fix did not sweep:**
  - #4907: no coupling on tile-less cells (D3-02).
  - #4926: water's night factor (D4-02).
  - #4928: `weather_sky_state` (D4-01).
  - #5002: no test on the lift (D1-03).
  - #5113: the affinity chain is still 8-wide (D2-02).
  - #5151 / #4837: correct for Starfield, wrong for FO76 (D1-01, HIGH).
- **Introduced a defect:**
  - #4906's partial fix (D3-01, MEDIUM).
  - #4903 regressed #4918's tile-size comment (D2-01).
- **Over-closed:** #4913. The `.btt` consumer never shipped (D6-01, MEDIUM).

**Headline.** EXT-D1-01: FO76 runs Starfield's WATR DNAM decoder and the same translate. #5151 judged FO76 "BU" from its distance lanes only. Its absorption triplet is per-metre: five records author exactly Starfield's `WaterSulfuric` 0.3 / 0.075 / 0.01. So FO76 absorption reaches WATAL 70× too strong. This is latent while FO76 is parse-only.

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| **single-boundary** | **Holds, and is stronger than at baseline.** The translate callers are now only `scene/world_setup.rs::apply_environment` (+ `procedural_fallback_*` in world_setup/weather/cornell). The `render/sky.rs` render-loop fallback (#4902) is gone. The interior outdoor sky has one install site, `install_interior_outdoor_defaults`. `translate_weather` owns IMGS (#4914). Water has exactly two production `WaterMaterial` composition sites (`material_translate.rs:223`, `env_translate.rs:1131`). `compose_flow_scrolls` has two callers. There is one `pack_sky_dome`. |
| **no-fabrication** | **Dented.** FO76's absorption unit was assumed without a census (D1-01). Oblivion's wind frame is marked settled without evidence (D5-02). The #4906 share normalisation discards authored density (D3-01). The `.btt` recon comment misreads the format (D6-02). The #4905 "vanilla parity" claim is asserted, not sourced; it is accepted as a documented engine deferral. |
| **no-leak** | **Holds.** One semantic dent: an XWCU current on a Calm WATR yields a `WaterFlow` against the component's own contract (D5-03). |
| **no-render-time-fallback** | **Holds.** `watr_angle_to_engine_xz`'s new `GameKind` parameter is a table-shaped match reached only from translate helpers, and all 6 call sites pass the merged `EsmIndex.game`. No `GameKind` exists in any Dim 4 Rust file or in sky/cloud/groundcover GLSL. `grep GameKind crates/physics/src` is empty. The LSCR loading cover (`e60911864`) overrides only the per-frame view, with no game branch. |

## Per-Category Matrix

✓ = holds; findings listed in the last column.

| Category (boundary fn) | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Findings |
|---|:---:|:---:|:---:|:---:|---|
| **Terrain / splatting** (`spawn_terrain_mesh` + `build_cell_splat_layers`) | ✓ | ✓ (#4905 parity asserted, documented) | ✓ | ✓ | D2-01, D2-02 (doc/const drift) |
| **Sky** (`translate_sky`, `pack_sky_dome`, `install_interior_outdoor_defaults`) | ✓ (#4902 closed) | ✓ | ✓ | ✓ | D4-03, D4-04 |
| **Weather / sun / fog** (`translate_weather`, `translate_exterior_cell_lighting`, `fit_legacy_fog_extinction`, `spatial_units::normalize`) | ✓ (#4914 closed) | ✓ Starfield lifts verified | ✓ | ✓ | D1-03, D1-04 (#5135), D4-01, D4-02 |
| **Water** (`resolve_water_material`, `water_material_from_mesh`, `compose_flow_scrolls`) | ✓ (2 sites) | ✗ D1-01 (FO76 units), D5-02 (Oblivion frame) | ~ D5-03 | ✓ | D1-01, D1-02, D5-01…05 |
| **Ground cover** (`groundcover_translate.rs`, `resolve_authored_cover`) | ✓ | ✗ D3-01 (density discarded) | ✓ | ✓ | D3-01…05 |
| **Distant LOD / trees** (`select_lod_quads`, `terrain_lod_layout`, `object_lod_scheme`) | ✓ | ~ D6-02 (recon misread) | ✓ | ✓ (#4912 clamp from the boundary) | D6-01, D6-02 |
| **Acceptance gates** | — | — | — | — | none (#5142 fixed baseline EXT-D7-01) |

## Fix verification (since baseline)

| Fix | Verdict |
|---|---|
| #5134 Starfield WTHR fog lift (`69ae5bffa`) | Correct. One `normalize` call (`parse.rs:548`), so no double lift. Pinned. |
| #5001 FNAM tail + height lift (`3ab5d2122`) | Correct. 8 heights lifted, density scales untouched, consumer divides by 70 (`fog.rs:147`). |
| #5002 Starfield LGTM 108-B decode (`8347fde67`) | Decode correct; the lift is unpinned (D1-03). |
| #5151 WATR DNAM lift (`58e877137`) / #4837 oceanness (`76e47b281`) | Correct for Starfield; FO76 wrong (D1-01); tile UV deferral untracked (D1-02). |
| #4914 IMGS owned by `translate_weather` (`2eab265fe`) / #4901 promotion (`d093f194b`) | Correct. Blended by `transition_t`, promoted, steady state reads it. |
| #4916 env.health (`a55a2b906`) / #4917 spawner-guard stripper (`5fe978b1d`) | Correct. Source scan is not vacuous. |
| #4902 #4909 #4915 (`387bb9d9c`) | Correct. Froxel sun now follows cloud cover + dimmer (`post_passes.rs:62-82`). The #2226 guard is live. Clear-sky exterior haze drops ~4× by design; worth an exterior capture. |
| #4908 #4925 (`c213af6ed`) | Correct. Nit: the interior `sky_mode` branch at `frame_params.rs:1136` is dead. |
| #4926 sun arc in cross-fade (`db0ec5467`) | Correct. Lerps the TNAM hours, not angles. The water sibling is D4-02. |
| #4928 shared TOD fold (`7b37eb49e`) | Correct in `weather.rs`; the `weather_sky_state` sibling is D4-01. |
| #5087 `draw.rs` → `frame_params.rs` (`c57e5cc4a`) | Correct for Dim 4 scans. Negative scans at `shader_constants.rs:1325` and `resources.rs:1154` don't cover `frame_params.rs` (→ /audit-renderer). |
| #4903 BTXT base affinity (`177774890`) | Correct. CPU/GPU lane order agree. Docs not updated (D3-05); tile-size comments regressed (D2-01). |
| #4907 BTXT ground colour (`14e6befba`) | Incomplete: tile-less cells (D3-02). |
| #4905 feather cell edge (`ab8de0972`) / #4918 docs (`2816f8e17`) | Doc-only. #4918 regressed at one site by #4903 (D2-01). |
| #5113 `TERRAIN_SPLAT_LAYERS` (`67969adc2`) | Incomplete (D2-02). |
| #4906 partial (`3e6d94ff0`) | Introduces D3-01. Zeroing unplaceable records is correct. Still open: the LTEX→GRAS keying (`authored_grass` dead), cross-DLC bleed, §12.12 sync. |
| #4919–#4924 model tier (`4dfe97f3e`, `cba78990e`) | Correct. #4920's spatial side effect is D3-04. |
| #4956/#4957 (`de7f46578`), #5089 split (`0f9982177`) | Correct. No guard went vacuous; moved guards scan `production_text` of their new file. |
| #4910 wind frame per game (`0d113b35a`) | Code correct. Docs half-migrated (D5-01 / #5136); Oblivion row unsupported (D5-02); test gap (D5-04). |
| #4911 XWCU scroll (`faf924c2b`) | Correct on the flowing arm; calm arm D5-03. |
| #4912 terrain-LOD clamp (`235a90ba2`) | Correct. `.btr` + legacy quads clamp, tiled base-LTEX fallback stays WRAP, registry keyed per (path, clamp). |
| #4913 `.btt` (`cf39f0ccc`, `8e512b02e`) | Over-closed (D6-01); recon misread (D6-02). |
| #5142 harness opt-in + bare screenshots (`858dee21f`) | Correct. Fixes baseline EXT-D7-2026-09-29-01. `m34` `sun: intensity=4.000` still matches `SUN_INTENSITY_PEAK` = 4.0. |

**Guard liveness:** every guard named in the skill is a live plain `#[test]`. One exception: `gpu_terrain_tile_is_160_bytes` is now `gpu_terrain_tile_is_176_bytes` (`scene_buffer/gpu_instance_layout_tests.rs:418`). `composite_does_not_carry_its_own_copy_of_the_sky` is live, but two of its needles are stale (D4-03). `resolve_water_material_sentinels_are_game_invariant` is live but blind to game keying (D5-04).

## Findings

### EXT-D1-2026-10-02-01: FO76 shares Starfield's WATR decoder and WATAL translate, but #5151/#4837 settled units on Starfield data only — FO76's per-metre absorption triplet reaches WATAL as per-BU (~70× too opaque), and its lane 3 saturates the oceanness term
- **Severity**: HIGH (divergent canonical value out of the WATAL translate: the same authored numbers in FO76 and Starfield produce values 70× apart). Live impact is latent, because FO76 is "Parse only" in the compat matrix.
- **Dimension**: EXAL boundary discipline (unit lift completeness). Overlaps Dim 5 (WATAL).
- **Location**:
  - `crates/plugin/src/esm/records/spatial_units.rs:67` (Starfield-only gate) and `:151-181` (waters arm);
  - `crates/plugin/src/esm/records/misc/water.rs:1263-1265` (`decode_dnam_fo76` → `decode_dnam_starfield`) and `:1283-1299` (absorption at 4/8/12, concentration at 16/20/24/28);
  - `byroredux/src/env_translate.rs:889-918`;
  - consumers `crates/renderer/shaders/water.frag:576-584` (`exp(-hitDist * coeff)`, hitDist in BU), `:571`, `:1246` (oceanness), and `byroredux/src/systems/water.rs:593-613` (`underwater_color_at_depth`).
- **Status**: NEW. It is the unverified half of #5151's SIBLING check ("FO76 … stays in BU"). That check was settled on FO76's distance lanes (noise falloff 4096, underwater fog −9000/850) and never on the inverse-length lane.
- **Tier Violated**: no-fabrication (a unit was assumed for a lane without a census).
- **Game Affected**: Fallout 76.
- **Description**:
  - #5151 divides Starfield's absorption triplet by 70 because the values are per-metre extinction. Its own evidence: red > green > blue, with magnitudes that match liquid water.
  - FO76 runs the byte-identical decoder and the same game-agnostic translate, but is excluded from the lift. Its triplet therefore reaches `exp(-hitDist_BU * coeff)` unconverted.
  - #4837 then passes lane 3 through `clamp(0,1)` on the strength of a Starfield-only census.
- **Evidence** (census, SeventySix.esm + NW.esm, 47 WATRs, every one 148-B DNAM):
  - All 47 author a non-zero absorption triplet; the median red is 0.207.
  - 5 records author **0.3 / 0.075 / 0.01**: `ExtTroughWater`, `IntTroughWater`, `ExtTestWater`, `DEBUG_ExtRiverCharlesUpper`, `ExtCranBogWaterFlow`.
    - That is exactly Starfield's `WaterSulfuric` / `SFBGS001_WaterVaruunWok_*` triplet, which #5151 now divides by 70.
    - It is also the pure-water absorption spectrum per metre (Pope & Fry: ~0.34 at 650 nm, 0.06 at 550 nm, 0.009 at 450 nm).
  - `Burn_ExtAbraxoWaterBasin` authors Starfield's 0.07627 blue value.
  - Read per BU, FO76's "clear" `ExtClearWaterPuddle` (0.24/0.18/0.2) reaches 1/e at ~4–5 BU (≈7 cm), and a trough's red channel at 3.3 BU (≈5 cm). Read per metre, both are plausible (1/e at 3–5 m).
  - Oceanness: FO76 lane 28 spans 0.16–75.72 (median 4.0; 39/47 > 1.0). After #4837, 39/47 FO76 waters saturate to 1.0, which gives +0.25 density and +50 % forward scatter.
  - FO76's pigment lanes (16/20/24) are 9e-5–0.52, not Starfield's 0–20, so `/STARFIELD_WATER_CONCENTRATION_REFERENCE` (`env_translate.rs:911-915`) reduces them to ~0.
- **Impact**:
  - Any FO76 water surface or underwater view would go to the deep tint within centimetres, the same symptom #5151 fixed for Starfield.
  - The oceanness/pigment terms are driven by a scale nobody measured for FO76.
  - No gate renders FO76 water.
- **Suggested Fix**:
  - Settle FO76's inverse-length unit from the shared-triplet census above. If it is confirmed, extend the absorption ÷70 to FO76: either in `spatial_units` with a FO76 arm limited to the triplet, or by having `decode_dnam_fo76` stop sharing the inverse-length convention silently.
  - Census FO76 lanes 16–28 before applying the Starfield concentration semantics. Until then, zero them for FO76 at the decoder (the documented "absent" sentinel).
  - Pin both with a FO76 DNAM fixture next to `fo76_index_watr_keeps_authored_units`. Today that test asserts the unconverted triplet `[0.16558, 0.096239, 0.076271]` (`spatial_units_tests.rs:269`), which locks in the questionable behaviour.

### EXT-D3-2026-10-02-01: The #4906 partial fix (`density / share`, capped at 1) removes the authored GRAS density for most records and flattens the species mix
- **Severity**: MEDIUM (visual)
- **Dimension**: Ground-cover pipeline
- **Location**: `byroredux/src/render/groundcover.rs:940-953` (bake), `:965-972` (`density_normalized_by_share`); the shader accept test is `crates/renderer/shaders/groundcover_models.comp:295,321`
- **Status**: NEW. Introduced by `3e6d94ff0`, the partial fix for #4906. Better attached to open #4906 than filed alone.
- **Tier Violated**: n/a
- **Game Affected**: Oblivion, FO3, FNV, Skyrim, FO4 (every game that runs the authored-model tier)
- **Description**:
  - PLACE picks a record with probability `s_i` (its share of the selection table). It then accepts with probability `d'_i × field × fade`.
  - The fix uploads `d'_i = min(1, d_i / s_i)`, so a record's coverage becomes `min(s_i, d_i) × field × fade`.
  - Before the fix, coverage was `s_i × d_i`. That diluted the absolute rate, but the mix stayed correct: `w_i·d_i : w_j·d_j`.
  - After the fix:
    - **When `d_i ≥ s_i`** (most records in a real load order): coverage equals the share. The authored density no longer has any effect.
    - **When `d_i < s_i`**: coverage equals the density. The climate weight no longer has any effect.
  - So a record's authored rate comes back only when its density is below its share. The commit message says every record's rate is restored, which is wrong.
- **Evidence** (FNV, WastelandNV → Arid; climate weights 2.0 / 0.4 / 0.1 from `groundcover_translate.rs:302-321`; table apportionment from `species_selection_table`):
  - **FNV GRAS records (24)**:
    - 15 arid records: `GrassWasteland*`, `…STRIP`, `…Green*`.
    - 4 temperate-default records: Lawn, Golf ×3.
    - 5 wetland records: Oasis ×2, SeaGrass ×3.
  - **Table shares**: arid ≈ 15/256 = 0.059, temperate 4/256 = 0.016, wetland 2/256 = 0.008.
  - **Before the fix**, total lattice coverage was ≈ 0.28.
  - **After the fix**, total coverage is ≈ 0.93–0.95, about 3.3× more plants.
  - 22 of 24 records now saturate at their share, so their authored density (0.20–1.00) has no effect:
    - `GrassWasteland02` (40%) and `GrassWasteland02b` (25%) render identically.
    - `GrassLawn01` (100%) and `NVGolfGrassTall01` (20%) render identically.
  - The Strip's deliberately sparse grasses (`GrassWasteland04STRIP` 3%, `05STRIP` 4%) now sit at 0.03–0.04 against 0.059 for normal wasteland grass. That ratio is 0.6; the authored ratio is 0.1, so they are about 6× over-represented.
- **Impact**:
  - Visible density no longer follows GRAS `DATA` density. Sparse or rare records (Strip grass, flowers, rocks) become as common as the dominant grass.
  - Instance demand roughly triples, which makes the 32,768-instance cap (and EXT-D3-04) more likely to bind on Skyrim/FO4's 20-unit grid. Not measured.
  - §12.12's register interpretation ("GRAS density = percent of grid points that grow the record") no longer describes the code.
- **Suggested Fix**:
  - Keep the mix proportional and correct only the absolute rate.
  - Build the table from `w_i × d_i` (placeable records only).
  - Upload one common accept rate `T` for every record, for example `T = min(1, Σ d_i)` over the climate-weighted set, or `1 − Π(1 − d_i)`.
  - Pin the composition ratio in a test as well as the total.

### EXT-D6-2026-10-02-01: #4913 was closed by a `Fix #4913` keyword while the `.btt` consumer never shipped. Skyrim distant trees are still not drawn and no open issue tracks it
- **Severity**: MEDIUM
- **Dimension**: Distant LOD and trees
- **Location**:
  - `byroredux/src/cell_loader/object_lod.rs:774-792`: `tree_lod_supported` / `tree_lod_archive_path` / `tree_lod_atlas_path`, each `#[cfg_attr(not(test), allow(dead_code))] // #4913: the consumer is open work`.
  - `docs/engine/exal.md:175-176, 345-353`.
  - `docs/engine/exal-trees.md:84-95, 323`.
  - `ROADMAP.md:281`.
  - `.claude/commands/audit-exterior/SKILL.md:84`.
- **Status**: #4913 needs to be reopened. It was closed prematurely: the subject of the commit cf39f0ccc is "Fix #4913: …", which auto-closed it. This is neither NEW nor a regression.
- **Tier Violated**: n/a (tracking integrity of a known functional gap)
- **Game Affected**: Skyrim (LE + SE)
- **Description**:
  - #4913, "Skyrim distant trees are never drawn" (labels `bug`, `medium`, `game:skyrim`), was closed on 2026-10-01T18:43Z.
  - Both of its own comments say the opposite: "Keeping this issue OPEN for that consumer" (on cf39f0ccc), and "**Staying open for the consumer**" (on 8e512b02e).
  - What landed: doc corrections, three naming fns with no production caller, and `probe_lod_corpus` counters. No `.btt` parser, billboard draw path or residency wiring exists.
  - Every doc that names the gap points at #4913 as its tracker. No other open issue covers tree LOD or billboards (searched the open set for tree / billboard / btt / LOD).
- **Evidence**:
  - `gh issue view 4913`: `CLOSED`, with comments as quoted above.
  - Production grep for `tree_lod_supported|tree_lod_archive_path|tree_lod_atlas_path` outside `object_lod.rs`: no hits.
  - `Skyrim - Meshes1.bsa` ships 386 `.btt` (43,333 tree instances) and 9 `.lst`. Textures5/6/7 ship 7 `<ws>treelod.dds`.
- **Impact**: Distant Skyrim forests render empty past the full-detail radius (the visible gap filed at MEDIUM), and the gap now has no live tracker. Known-open registers that check issue state would read it as fixed.
- **Suggested Fix**: Reopen #4913, or file a successor for "`.btt` + `.lst` instanced-billboard consumer", and repoint the dead-code `allow` comments and the docs at it. When a commit fixes only part of an issue, use `Partial #N` / `Refs #N`, not `Fix #N` (*feedback_multi_issue_commit_close*).

### EXT-D1-2026-10-02-02: Starfield WATR noise-UV tile sizes stay unlifted beside the lifted lanes with no tracking issue (#5151 closed with the deferral in a code comment only)
- **Severity**: LOW
- **Dimension**: EXAL boundary discipline (unit lift completeness)
- **Location**:
  - `crates/plugin/src/esm/records/spatial_units.rs:156-158` (deferral comment);
  - `crates/plugin/src/esm/records/misc/water.rs:1348-1357` (`noise_uv_scale_{a,b,c}` at 120/124/128);
  - `byroredux/src/env_translate.rs:806-814` (clamp `[1/4096, 1/8]`).
- **Status**: NEW. #5151's issue body says "settle it with a capture before lifting", but #5151 is closed, and no open issue tracks this (searched "noise UV Starfield" and "tile sizes water Starfield capture").
- **Tier Violated**: no-fabrication (a unit is chosen implicitly: the tile is read as BU).
- **Game Affected**: Starfield.
- **Description**:
  - Every other Starfield DNAM length is lifted. The three tile sizes (vanilla 72.11 / 39 / 13 m-or-BU) are inverted as BU, so the primary noise tile repeats every ~1 m.
  - Read as metres, the tile is 5 048 BU, which the translate clamp would cap at 4 096.
  - FO76, the same layout in BU, authors 279 / 168 / 56.
  - The Starfield-only `displacement` (72/76/80) and `normal_falloff` (52/56/60) lanes have the same unsettled status and are not listed anywhere.
- **Impact**: If the lanes are metric, Starfield water normals tile ~70× too finely. Today nothing records that an open decision exists.
- **Suggested Fix**: File a tracking issue (capture-gated) covering the tile sizes and the other Starfield DNAM lanes with an unclassified unit. List them in watal.md's per-game unit table.

### EXT-D1-2026-10-02-03: No test pins the Starfield LGTM unit lift — #5002 added four height-field lifts to `normalize` with only a wire-decode test, and LGTM fog/fade lifting has never been pinned
- **Severity**: LOW
- **Dimension**: EXAL boundary discipline (test coverage of the parse-boundary lift)
- **Location**:
  - `crates/plugin/src/esm/records/spatial_units.rs:106-124` (LGTM arm, including the #5002 lines 117-123);
  - `crates/plugin/src/esm/records/misc/world.rs` test `starfield_lgtm_data_decodes_the_sf_xcll_layout` (calls `parse_lgtm` directly: wire values, no normalize);
  - `crates/plugin/src/esm/records/spatial_units_tests.rs` (no LGTM or XCLL case).
- **Status**: NEW
- **Tier Violated**: n/a (test gap)
- **Game Affected**: Starfield
- **Description**:
  - `spatial_units_tests.rs` pins REFR/LIGH/SCOL (`:55`), WTHR (`:158`) and WATR (`:245`). It has no case for `index.lighting_templates`, and none for CELL XCLL `lighting()`: fog near/far, fog_clip, light fades, the SF height mid/ranges.
  - Deleting the whole LGTM arm, or just #5002's four height lines, leaves every test green.
  - The baseline's suggested-fix text ("a spatial_units test like the XCLL/LGTM ones") assumed tests that do not exist.
- **Impact**: A regression to metric LGTM/XCLL fog (the ~70× class of #5134) would go undetected. 6 vanilla LGTMs and every Starfield interior XCLL depend on this arm.
- **Suggested Fix**:
  - Add a `parse_esm` fixture with a 108-B Starfield LGTM, using `ShipInteriorLT` values plus distinct heights, and a cell with a 108-B XCLL.
  - Assert fog/clip/fade/heights ×70 and the scales/gravity untouched.
  - Add a non-Starfield companion that asserts the authored values.

### EXT-D1-2026-10-02-04: `BASE_FOG_STRENGTH` still undocumented in the exterior specs; env.health still reports the unscaled extinction
- **Severity**: LOW
- **Dimension**: EXAL boundary discipline
- **Location**: `byroredux/src/fog.rs:29-38`; `byroredux/src/app_frame.rs:656-662`; `byroredux/src/commands/env_health.rs:486-489`
- **Status**: Existing #5135. Unchanged since the baseline: no commit since 9fcfdc3fc touches these lines.
- **Tier Violated**: no-fabrication (documentation), no-leak (telemetry)
- **Game Affected**: all
- **Description / Evidence**: As in the baseline EXT-D1-2026-09-29-02. `e60911864` adds one more frame producer (`ModelStage::apply_to_frame` sets `FogMedium::default()` = `DISABLED`). That producer is scaled at the same site and is harmless at zero extinction.
- **Suggested Fix**: Per #5135.

### EXT-D2-2026-10-02-01: `GpuTerrainTile` grew from 160 to 176 B (#4903), but six docs and comments still say 160 B and one cites a test that no longer exists
- **Severity**: LOW (doc rot)
- **Dimension**: Terrain, splatting
- **Location**:
  - `crates/renderer/shaders/include/terrain_sample.glsl:20`. #4918 rewrote this exact comment to "160 B" on 09-30, and #4903 made it stale the next day.
  - `docs/engine/memory-budget.md:161`: row says "160 B … pinned by `gpu_terrain_tile_is_160_bytes`", "~160 KB". The test is now `gpu_terrain_tile_is_176_bytes`; the real size is 176 B and ~176 KB, and the row omits `base_cover_affinity` and `base_diffuse_index`.
  - `crates/renderer/src/vulkan/scene_buffer/constants.rs:260` ("1024 × 160 B = 160 KB").
  - `crates/renderer/src/vulkan/scene_buffer/buffers.rs:552`.
  - `crates/renderer/src/vulkan/context/shrink_frame_scratch.rs:112` (rationale string).
  - Skills: `.claude/commands/audit-exterior/SKILL.md:42`, `.claude/commands/audit-safety/SKILL.md:188`.
- **Status**: Regression of #4918 at the `terrain_sample.glsl` site; the other sites are NEW.
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Description**: The layout pin `gpu_terrain_tile_is_176_bytes` is correct. No guard scans the memory-budget terrain row, which is how it drifted.
- **Suggested Fix**:
  - Update each site to 176 B and name the live test.
  - Optionally extend the `memory_budget_ledgers…` guard to cover the terrain-tile row (`size_of::<GpuTerrainTile>()`).

### EXT-D2-2026-10-02-02: #5113 says "end to end", but the cover-affinity lane chain still hard-codes 8 lanes
- **Severity**: LOW (latent maintainability)
- **Dimension**: Terrain, splatting
- **Location**: The chain is created by #4903, before #5113 landed the same day.
  - `byroredux/src/cell_loader/terrain.rs:1089` (`[DEFAULT_AFFINITY; 8]`), `:1117-1119` (`[0u32; 8]` ×3).
  - `byroredux/src/components.rs:474,508`.
  - `byroredux/src/render/groundcover.rs:545`, `:401-412`.
  - `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:39-40` and `vulkan/groundcover.rs:94-95` (`[f32; 4]` ×2).
  - `crates/renderer/shaders/include/groundcover_density.glsl:122-138`: eight unrolled `mix` calls.
  - `shader_contract_tests.rs:7087-7089`: asserts `count() == 8`.
- **Status**: Incomplete fix of #5113 (CLOSED)
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Description**:
  - Most of these sites fail to compile when `TERRAIN_SPLAT_LAYERS` changes (the index arrays and `authored_grass`).
  - `layer_affinity` does not: `[f32; 8]` is filled by `zip` over `layers`, which would silently drop lanes 8 and up.
  - The GLSL affinity mix and its guard both hard-code 8. A `Vertex` lane bump would leave density running on 8 lanes while the colour chain runs on `TERRAIN_SPLAT_LAYERS`.
- **Suggested Fix**:
  - Size `layer_affinity` / `EntityCell` from `TERRAIN_SPLAT_LAYERS`.
  - Write `byroGcAffinity` as the same `for (i < TERRAIN_SPLAT_LAYERS)` lane loop that `byroTerrainSplatAlbedo` uses (affinities indexed the same way as the weights).
  - Make the guard assert the loop shape rather than a count of 8.

### EXT-D3-2026-10-02-02: The #4907 ground-colour coupling depends on the terrain tile; cells with a BTXT base but no ATXT have no tile, so a new blade root-colour step appears at their borders
- **Severity**: LOW (visual)
- **Dimension**: Ground-cover pipeline
- **Location**:
  - `byroredux/src/cell_loader/terrain.rs:1116` (a tile is allocated only when `!splat_layers.layers.is_empty()`) and `:1243-1245` (`terrain_tile_slot` sentinel).
  - `crates/renderer/shaders/groundcover_blade.vert:411-413`.
  - `crates/renderer/shaders/groundcover_blade.frag:151-160`.
- **Status**: NEW (gap in #4907's fix)
- **Tier Violated**: n/a
- **Game Affected**: all LAND games
- **Description**:
  - The original audit suggested carrying the BTXT diffuse index into the ground-cover cell record. #4907 put it in the `GpuTerrainTile` slot instead (`base_diffuse_index`).
  - A cell whose four quadrants share one base and that has no ATXT gets no tile. Its blades skip the coupling block entirely (`vTerrainTileSlot == GROUNDCOVER_NO_TERRAIN_TILE`).
  - Before #4907, blades on zero-paint ground got no coupling in either kind of cell (`weightSum == 0`). Now they couple to the base colour in painted cells but not in no-ATXT cells.
  - The result is a straight root-colour step along every border between the two kinds of cell, even when both show the same base texture.
  - #4903's affinity is not affected: `base_affinity` rides `GpuGroundCoverCell`, not the tile.
- **Evidence** (Python LAND census):
  - **Skyrim.esm**: 866 of 15,564 LAND records have an authored BTXT and no ATXT. Another 5,309 have no BTXT at all (default land).
  - **FalloutNV.esm**: 164 records have a BTXT and no ATXT, plus 23,840 default-land records (mostly outside the playable area).
  - No cell without ATXT has differing quadrant bases (0 in both games), so every such cell gets no tile.
- **Impact**: A root-tint seam along cell borders in exterior regions near no-ATXT cells, at about 5.6% of Skyrim's LAND.
- **Suggested Fix**:
  - Carry `base_diffuse_index` on `GpuGroundCoverCell` (`pad2` is free), or allocate a tile for every LAND cell.
  - Then let the blade run `byroTerrainSplatAlbedo` with zero weights wherever a base exists.

### EXT-D3-2026-10-02-03: `placeable_fold` hashes records k and k+64 into the same bit, and the frame path allocates a Vec again
- **Severity**: LOW
- **Dimension**: Ground-cover pipeline
- **Location**: `byroredux/src/render/groundcover.rs:871-888`
- **Status**: NEW. The allocation half partly undoes #4922.
- **Tier Violated**: n/a
- **Game Affected**: any load order with more than 64 placeable GRAS records (vanilla Oblivion alone has 108)
- **Description**:
  - **(a) Aliasing.** The key fold is `h = rotl(h, 1) ^ p` over up to 128 bools in a `u64`.
    - Record k contributes bit `(n−1−k) mod 64`, so records k and k+64 alias.
    - If a frame's change in placeability flips both records of an aliased pair (both load, or one loads as the other fails), the XORs cancel. The key stays equal and the table is not re-derived.
    - The newly placeable records then keep zero weight and place nothing until some later, unrelated change.
    - With exactly 128 records, the all-false and all-true folds are equal.
  - **(b) Allocation.** `vec![false; count_ahead]` is allocated every frame before the key check. #4922 had just removed per-frame Vecs from this path.
- **Evidence**: Reasoning from the code above. The fold arithmetic is in the description.
- **Impact**:
  - (a): A record can silently draw nothing in Oblivion-scale load orders. It depends on streaming timing, so it is hard to reproduce.
  - (b): One small heap allocation per frame.
- **Suggested Fix**:
  - Key on the exact bitset: `u128`, since `GROUNDCOVER_MODEL_MAX_RECORDS` is 128. A const-assert ties the two together.
  - Build it without allocating, or into persistent scratch.

### EXT-D3-2026-10-02-04: Over budget (#4920), plants are kept by rank in residency-slot order, so whole late-slot chunks go bare instead of thinning evenly
- **Severity**: LOW (visual; only past the cap)
- **Dimension**: Ground-cover pipeline
- **Location**:
  - `crates/renderer/shaders/groundcover_models.comp:366-397` (LAYOUT grants) and `:438-439,465-467` (EMIT `rank < count`).
  - `byroredux/src/render/groundcover.rs:360-375` (chunks arrive in slot order).
  - `docs/engine/exal-groundcover.md` §12.12 "Over budget".
- **Status**: NEW (side effect of #4920's fix, not covered by it)
- **Tier Violated**: n/a
- **Game Affected**: all, once demand exceeds the 32,768-instance tail
- **Description**:
  - A plant's rank is its record's running count across chunks in `gcChunks` order, which is residency-ring slot order (`reconcile`), not distance.
  - `floor(t_r × cap / demand)` keeps each record's first N plants by rank. The plants dropped are therefore whole spatial runs of late-slot chunks, which can be next to the camera.
  - When the camera moves, slots are reassigned and the bare regions jump.
  - The doc says "every record keeps the same fraction of its plants". That is true per record, but it does not mention the spatial effect.
- **Evidence**: The code path cited above. EXT-D3-01 raises demand about 3×, so the cap is more likely to bind than when #4920 was verified (FNV `demanded=50`).
- **Impact**: Bare patches near the player once Skyrim/FO4 density crosses the cap. Not measured.
- **Suggested Fix**:
  - Admit by a frame-invariant per-plant hash threshold. Keep a plant if `hash(point) < cap / demand`, so thinning is uniform and deterministic.
  - Or rank chunks nearest-first before granting.
  - Record the chosen policy in §12.12.

### EXT-D3-2026-10-02-05: Doc rot from #4903 / #4907 / #4906 (bundle)
- **Severity**: LOW (doc rot / test gap)
- **Dimension**: Ground-cover pipeline (plus one Dim 2 test comment)
- **Location**: see each item
- **Status**: NEW
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Description**:
  1. **`docs/engine/exal-groundcover.md`, three stale sections.** None of the three fix commits touched the spec doc.
     - §3 (`:151-153`) still describes `affinity(splat)` as the 8 weights "dotted with" `cover_affinity`. It is now an ordered mix from the BTXT base.
     - §12.3 (`:1195`) still says the blade blends toward "the splat-weighted average of the cell's painted layer diffuse textures". It now uses BTXT base plus `byroTerrainSplatAlbedo`.
     - §12.12 (`:1624-1626`) states accept probability `density × …` and pure climate selection. It omits the `density/share` bake and the placeable-only weighting.
  2. **`groundcover_blade.frag:136-147`** still says the blend is "a weighted average … not `mix` against a base texture: the blade's base has no BTXT base layer of its own". That is the premise #4907 removed, and the code directly below contradicts it.
  3. **Three doc comments repeat the false premise #4903 removed**: `DEFAULT_COVER_AFFINITY` (`crates/core/src/ecs/components/groundcover.rs:107-113`), `groundcover_translate.rs:71-73`, and `shader_constants_data.rs:232-236`.
     - They say the scatter shader uses the default for unpainted ground because the base "has no LTEX record".
     - `GROUNDCOVER_DEFAULT_AFFINITY` is still emitted to GLSL but no shader reads it.
  4. **Moot rationale.** `terrain.rs:1084-1088` and `components.rs:471-473` say unused slots must hold the default "or an unused layer reads as a vegetation hole". Under the ordered mix a zero-weight lane cannot affect the result.
  5. **Weak #4903 guard.** The "host-side half" of `groundcover_affinity_composes_the_base_in_diffuse_loop_order` exercises a closure defined inside the test, not the shader. The source half counts 8 `mix` calls but not lane pairing: `affinity0.y` paired with `splat0.x` would pass.
  6. **Wrong direction in a #4905 test comment.** `terrain_splat_tests.rs:190` labels SW row 0 "top edge (faces the cell above)". Row 0 is the south border: row 16 is north, as the same test's `:193` says. The assertion itself is correct.
- **Suggested Fix**:
  - Rewrite the cited text.
  - Drop the dead GLSL constant export, or document it as Rust-only.
  - Pin lane pairing in the #4903 guard.

### EXT-D4-2026-10-02-01: #4928's sibling sweep missed `weather_sky_state`: a third TOD-slot reduction (`.min(3)`) and a cloud-alpha rule that disagrees with the steady-state sampler
- **Severity**: LOW. The code is dormant today.
- **Dimension**: Sky, weather, sun
- **Location**: `byroredux/src/env_translate.rs:1388-1402` (`weather_sky_state`). Callers: `:1307` (`TOD_DAY`) and `:1586` (literal `1`). Compare with `byroredux/src/systems/weather.rs:476-506` (`sample_weather_sky`) and `env_translate.rs:1531-1539`.
- **Status**: NEW. Related: #4928 (closed; its SIBLING checkbox) and #4494 (closed; constants in the same function).
- **Tier Violated**: single-boundary (two derivations of one quantity)
- **Game Affected**: all
- **Description**:
  - `weather_sky_state` seeds `WeatherSkyState.cloud_tints` using `let slot = tod_slot.min(3)`. That is a third 6→4 TOD reduction beside the shared `fold_to_four_tod_slots` that #4928 consolidated onto. It is also wrong for `TOD_HIGH_NOON`: 4 becomes 3, which is NIGHT, where the shared fold gives DAY.
  - The seed's alpha is `color.a / 255 × JNAM alpha` (`:1400`). The steady-state sampler drops PNAM's fourth byte: `cloud_layer_colors` goes through `to_rgb_f32` at `:1531-1539`, and alpha is the JNAM value alone (`weather.rs:496-503`).
  - UESP's Skyrim WTHR page lists the PNAM entries as `rgb` per TOD, so the fourth byte is padding, not alpha.
- **Evidence**:
  - `env_translate.rs:1392`: `let slot = tod_slot.min(3);`
  - `env_translate.rs:1400`: `(color.a as f32 / 255.0 * wthr.cloud_layer_alphas[layer][slot]).clamp(0.0, 1.0)`
  - `weather.rs:496-503`: `lerp1(alpha_a, alpha_b, t)` with no PNAM byte.
  - `/mnt/data/src/reference/uesp-wiki/Skyrim Mod/Mod File Format/WTHR.wiki:27-31`: PNAM = `:rgb Sunrise / Day / Sunset / Night`.
  - The test fixture at `env_translate.rs:4711-4716` authors `a: 200`, which encodes the "byte 3 is alpha" reading.
- **Impact**:
  - Both callers pass DAY, so the `.min(3)` misfold is unreachable today.
  - `weather_system` rebuilds `cloud_tints` from `wd.weather` on every tick (`weather.rs:483-505`), so the seed alpha is visible only before the first tick after `apply_environment`.
  - The seed remains a near-copy that will drift. A future caller seeding at another TOD (for example `seeded_at_wrong_tod_resample`'s scenario) would read HIGH_NOON as NIGHT, and the first frame would carry padding-scaled cloud alpha.
- **Suggested Fix**: Seed through the same rule as the sampler. Call `fold_to_four_tod_slots` (or make the seed `sample_weather_sky` on the just-translated `WeatherDataRes`), drop the PNAM fourth byte from the alpha, and replace the literal `1` at `:1586` with `TOD_DAY`.

### EXT-D4-2026-10-02-02: #4926's sibling: water's day/night factor still reads the source climate's TOD breakpoints through a cross-climate fade and steps at promotion
- **Severity**: LOW
- **Dimension**: Sky, weather, sun (water's TOD consumer; `render/water.rs` is in no Dim 5 path)
- **Location**: `byroredux/src/render/water.rs:121-133`; `byroredux/src/systems/weather.rs:1009-1014`, `:1045`. Same pattern, harmless: `byroredux/src/commands/time.rs:62-65` (phase label).
- **Status**: NEW. Related: #4926 (closed) and #1018 (same class).
- **Tier Violated**: single-boundary (the effective TOD breakpoints exist only as a `weather_system` local)
- **Game Affected**: all, on a WTHR cross-fade between worldspaces whose CLMT TNAM differ (e.g. base game ↔ DLC worldspace)
- **Description**:
  - db0ec5467 blends the source and target `tod_hours` for `compute_sun_arc`, but only into a local (`weather.rs:1009-1014`); nothing publishes the effective breakpoints.
  - `render/water.rs` computes the GNAM day/night surface blend as `night_factor_for_hour(hour, WeatherDataRes.tod_hours)`. During the 8 s fade `WeatherDataRes` still holds the source weather, because the target lives in `WeatherTransitionRes.target` until promotion (`weather.rs:1343`).
  - Result: water follows the source climate for the whole fade, then jumps to the target climate's night factor on the promotion frame. Meanwhile the sky palette, fog and (since #4926) the sun all ease.
- **Evidence**: `render/water.rs:129`: `let tod_hours = weather.as_ref().map(|w| w.tod_hours);` then `:132` `night_factor_for_hour(hour, hours)`. `weather.rs:1343`: `wd.tod_hours = new_tod;` runs only in the promotion.
- **Impact**:
  - The water surface's day/night variant pops once at the end of a cross-climate fade.
  - The pop is visible only in hours where the two climates' night factors differ, i.e. the dawn/dusk bands.
  - Same-climate fades (the common case) are unaffected.
- **Suggested Fix**: Have `weather_system` publish the effective breakpoints or the night factor it already derives. For example, store the blended `tod_hours` on `SkyParamsRes` or a small resource, and have `render/water.rs` (and `time` command's phase label) read that instead of `WeatherDataRes.tod_hours`. Add a mid-fade water night-factor test beside `sun_arc_crossfade_tests`.

### EXT-D4-2026-10-02-03: `composite_does_not_carry_its_own_copy_of_the_sky` names two retired functions and never names `sky_radiance` / `cloud_march`
- **Severity**: LOW (test hygiene; the guard is partially vacuous)
- **Dimension**: Sky, weather, sun
- **Location**: `crates/renderer/src/vulkan/sky_dome.rs:117-135`
- **Status**: NEW. No prior report names it.
- **Tier Violated**: n/a (guard integrity)
- **Game Affected**: n/a
- **Description**:
  - The audit skill cites this guard as the "one `cloud_march` for background and bake" pin.
  - Two of its four forbidden needles no longer exist anywhere: `vec3 compute_sky(` and `vec4 weather_procedural_cloud(`. Both were retired when 564d0d2fe (2026-09-13) made the entry point `sky_radiance` (`include/sky.glsl:195`) and the cloud body `cloud_march` (`include/clouds.glsl:274`).
  - Neither live name is checked, and `sky_cube.comp` (the other consumer) is not scanned at all.
  - Mitigation: a same-name re-declaration alongside the `#include` is a GLSL redefinition error, so the guard's effective teeth are only the `#include "include/sky.glsl"` assertion. A renamed fork in `composite.frag` or `sky_cube.comp` passes both the compiler and the guard.
- **Evidence**:
  - `sky_dome.rs:124-129` lists `compute_sky(`, `weather_procedural_cloud(`, `weather_sky_details(`, `weather_star_field(`.
  - `grep -rn "compute_sky\|weather_procedural_cloud" crates/renderer/shaders` returns nothing.
  - `git log -S "weather_procedural_cloud("` → 564d0d2fe.
- **Impact**:
  - The single-implementation claim rests on the include line alone.
  - A copy of `cloud_march`/`sky_radiance` under a new name in either consumer would ship green.
- **Suggested Fix**:
  - Derive the forbidden set from the function declarations in `sky.glsl` + `clouds.glsl`: parse `^(vec[234]|float|bool|void) name(`.
  - Assert that `composite.frag` and `sky_cube.comp` each include `sky.glsl` and declare none of those names.
  - Also assert that `cloud_march(` is called only from `sky.glsl`.

### EXT-D4-2026-10-02-04: Stale pointers in sky/weather docs after #5087 and #4925, plus a misplaced rustdoc block in `weather.rs`
- **Severity**: LOW (doc rot)
- **Dimension**: Sky, weather, sun
- **Location**:
  - `docs/engine/skyal.md:555`: names `context/draw.rs` for `interior_portal_sky_preserves_room_weather_gate`. It is now `crates/renderer/src/vulkan/context/frame_params.rs:1201-1202`; the doc landed in e4df3abb9 at 09:14 on 2026-10-01, and c57e5cc4a moved the test the same day.
  - `crates/renderer/shaders/include/clouds.glsl:317`: "The host packs `[dir.x, speed, dir.z, 0]` (`build_composite_params`)". The packer is `pack_sky_dome`, at `frame_params.rs:1065-1070` since #4925.
  - `crates/renderer/shaders/composite.frag:575`: `draw.rs::hdr_clear`. It lives in `context/begin_frame_recording.rs:125`. This is older than this pass.
  - `byroredux/src/systems/weather.rs:83-119`: the rustdoc blocks for `pick_tod_pair` (`:83-91`) and `compute_sun_arc` (`:92-110`) sit above `SUN_SOUTH_TILT` (`:111-120`). Rustdoc attaches all three to the const, and `compute_sun_arc` (`:122`) and `pick_tod_pair` (`:161`) render undocumented. This has been the case since 2026-06-02.
- **Status**: NEW
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Description / Impact**:
  - Readers following skyal.md's gate table, or the GLSL wind-packing comment, land in a file that no longer holds the code.
  - The wind-packing comment matters most: `wind_consumers_match_the_host_packing` exists because this exact swizzle was misread once already.
  - The two TOD functions behind the sun arc and palette have no rendered docs.
- **Suggested Fix**:
  - Repoint the three references: `frame_params.rs`; `pack_sky_dome`; `begin_frame_recording.rs`.
  - In `weather.rs`, move the "Walk a `build_tod_keys` table…" block to directly above `pick_tod_pair`, and the "Derive sun direction…" block to directly above `compute_sun_arc`.

---

### EXT-D5-2026-10-02-01: #4910 added a correct per-game frame table but left the old §2 table and the parser doc claiming "+90° for every game"
- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Location**:
  - `docs/engine/watal.md:392-403`: the §2 "Wind-angle frame per game (#4932)" table.
  - `docs/engine/watal.md:451-457`: the opening sentence of "Directional-scroll frame".
  - `crates/plugin/src/esm/records/misc/water.rs:312-322`: the `noise_wind_directions` doc.
- **Status**: Existing #5136, partly resolved by 0d113b35a. #5136 should stay open.
- **Tier Violated**: no-fabrication (documentation)
- **Game Affected**: Oblivion, FO3, FNV, FO76, Starfield
- **Description**: #4910's doc change added a new, correct table at `watal.md:471-484`: Skyrim/FO4 convert, FO76 converts tentatively, Oblivion is un-rotated, FO3/FNV and Starfield are un-rotated and OPEN. It did not touch the original table that #5136 is about. Three texts in the same docs now contradict the code:
  - `watal.md:394`: "`watr_angle_to_engine_xz` applies the single +90° rotation at the translate boundary".
  - `watal.md:401`: FO76 / Starfield are the "same wind-FROM bearing".
  - `watal.md:402`: FO3/FNV layers are the "same wind-FROM bearing".
  - `watal.md:452-455` still opens with "WATR's per-layer wind angles are wind-FROM compass bearings … and the engine XZ plane reads them rotated +90°", without scoping.
  - The parser doc at `water.rs:313-321` says the boundary "applies the one +90° rotation to either" convention. It also lists FO3/FNV `DNAM[100]` and Starfield as wind-FROM bearings, which the code now records as undetermined.
- **Evidence**: `env_translate.rs:603-610` keeps the +90° only for `Skyrim | Fallout4 | Fallout76` and returns β unchanged for `Oblivion | Fallout3NV | Starfield`. `watal.md:394-402` is byte-identical to the text #5136 cites.
- **Impact**: A reader of §2 or of the parser field doc gets the pre-#4910 frame for four games, and the spec now contradicts itself. There is no runtime effect.
- **Suggested Fix**: Collapse the §2 table into the #4910 table (or make the §2 one point to it). Scope the first sentence of the "Directional-scroll frame" paragraph. Rewrite `water.rs:312-322` so it says the rotation is per game and FO3/FNV and Starfield are OPEN. Then close #5136.

### EXT-D5-2026-10-02-02: The Oblivion row of the #4910 table settles a frame without evidence; by the doc's own convention the un-rotated read mirrors north and south
- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Location**:
  - `docs/engine/watal.md:481` (the Oblivion row: "**un-rotated**", not OPEN).
  - `byroredux/src/env_translate.rs:592-595` and `:608`.
  - The test comment at `env_translate.rs:3230-3232`.
  - The parser derives the angle at `crates/plugin/src/esm/records/misc/water.rs:600-606`.
- **Status**: NEW. Related to #5136 and #4910.
- **Tier Violated**: no-fabrication
- **Game Affected**: Oblivion
- **Description**:
  - The parser stores Oblivion's layer 0 as `atan2(y, x)` of the `DATA[28]/[32]` scroll pair, and the docs call it a counter-clockwise direction-of-travel angle in the record frame. The un-rotated read then places `(cos θ, sin θ)` straight into engine XZ.
  - The same doc's Z-up→Y-up map (`watal.md:405-406`: "(x, y, z) maps to Y-up (x, z, −y)", "+Z is game south") sends a record-frame pair (x, y) to engine (x, −y), so φ = −θ.
  - So if the pair is a world-frame Z-up vector, the un-rotated read is a north/south mirror. If it is a UV-space vector, its relation to world depends on Oblivion's water UV mapping, which nobody has established.
  - Either way, "the angle is not a bearing" only shows that the +90° bearing formula does not apply. It does not show that φ = θ.
  - The commit message itself says the un-rotated read is "the pre-2026-09-24 status quo, not a frame claim". The table nevertheless lists FO3/FNV and Starfield as OPEN and Oblivion as settled.
  - #4910 also silently changed the frame of Oblivion's `wind_direction` (`DATA[4]`, authored per #4931), which feeds the physics-current fallback at `env_translate.rs:1001-1003`. It went from +90° to un-rotated, and neither table mentions it.
- **Evidence**: Census of `Oblivion.esm`, 23 WATR:
  - The scroll pairs are `(0.0011, 0.0011)` (DefaultWater family, θ = 45°), `(0.001, 0.002)` (dungeon/sewer, θ = 63°), `(0.0008, 0.0008)` (SwampWater), and zero on the rest.
  - Read as world-frame vectors, they would drift NE/NNE in game terms. The un-rotated read drifts them SE/SSE.
  - No Oblivion WATR has a directional kind: no river/stream/creek/rapid name, no NAM5, no NAM0. So the `wind_direction` physics-fallback change is latent in vanilla.
- **Impact**: The pattern drift direction on every Oblivion water may be mirrored, though the visual effect on near-isotropic slow scrolls is small. The spec presents an unverified frame as resolved, which is the same defect class as #5136.
- **Suggested Fix**: Mark the Oblivion row "un-rotated, OPEN", as FO3/FNV are. State that a world-frame reading would need φ = −θ, and add a row for Oblivion's `wind_direction`. Settle the question with a capture, or from Oblivion's water shader UV convention, before choosing a frame.

### EXT-D5-2026-10-02-03: An XWCU current on a Calm WATR reaches physics as a `WaterFlow` with no pattern term (8 FO4 REFRs). #4911's "ripples and current agree" does not hold on the calm arm
- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Location**:
  - `byroredux/src/cell_loader/water.rs:712-737`: `reference_flow` is built unconditionally, the compose is gated at `:722`, and `:737` returns `reference_flow.or(watr_flow)`.
  - `crates/core/src/ecs/components/water.rs:493`: the doc says "`Calm` waters do not carry this component".
  - Contrast `byroredux/src/env_translate.rs:950-962`: an authored NAM0 promotes Calm → River.
- **Status**: NEW. The physics override predates #4911; #4911's calm gate is what makes the disagreement explicit.
- **Tier Violated**: single-boundary (two arms apply different rules to an authored current)
- **Game Affected**: FO4
- **Description**:
  - In the WATR arm, an authored current never coexists with `Calm`. `has_authored_linear_flow` promotes the kind to River (or Rapids), so both the pattern term and the physics flow follow. watal.md §2 states the rule: authored flow "cannot silently fall back to calm-water physics".
  - The merge arm does not mirror this. It keeps `watr_kind` (Calm) and skips the compose because of `has_directional_flow()`, but it still returns the XWCU current as the entity's `WaterFlow`.
  - The result is a Calm plane that carries a `WaterFlow`, against the component's own invariant. Floating bodies drift along the XWCU vector while the surface pattern is the WATR's unrelated authored layers.
- **Evidence**: Census, each XWCU REFR traced → base ACTI `WNAM` → WATR, classified with `env_translate`'s kind rules:
  - Skyrim.esm: 128/128 River, unaffected.
  - Fallout4.esm: 163 River and **8 Calm**:
    - 7 interior pond REFRs (`IntPondDarkWaterCalm`, `IntPondDarkWaterCalm_NoFalloff`, `IntPondWaterCalmInteriorLitRed`), at 0.08–0.25 BU/s.
    - `ExtOldGulletWater` (REFR 0x1B273E), XWCU (−3.41, −3.08), 4.6 BU/s.
  - The test `xwcu_current_recomposes_the_pattern_scroll` covers only a River WATR. The calm-gate test pins "no synthesized term", not "no physics flow".
- **Impact**: On those 8 placements the physics current and the visible water disagree, which #4911 set out to remove. Any consumer that trusts "Calm ⇒ no `WaterFlow`" is wrong for them.
- **Suggested Fix**: Make the merge follow the WATR arm's rule. Either let an XWCU current promote Calm → River (and Rapids at `SPEED_RAPIDS`), then compose, or drop the XWCU flow when the kind stays Calm. Pin the chosen rule with a Calm-WATR + XWCU test.

### EXT-D5-2026-10-02-04: The sentinel guard resolves both "per-game" records under `GameKind::Skyrim`, so it cannot catch a game-keyed sentinel now that the translate takes a game
- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Location**: `byroredux/src/env_translate.rs:3937-3938` (`resolve_water_material_sentinels_are_game_invariant`)
- **Status**: NEW (a test gap opened by #4910's signature change)
- **Tier Violated**: no-render-time-fallback (guard coverage)
- **Game Affected**: all
- **Description**:
  - Before #4910, `resolve_water_material` had no game input, so a game-dependent sentinel was structurally impossible and the guard only had to vary the records.
  - #4910 added `game: GameKind`, and the guard now passes `GameKind::Skyrim` for both the "Oblivion-shaped" and the "Skyrim-shaped" record.
  - A future `match game { … }` that sets a sentinel field such as `ior`, `uv_scale_*` or `shoreline_width` inside the translate would pass this plain guard.
  - The only test that varies the game is the real-data `water_sentinels_hold_on_real_watr_per_game`, which is `#[ignore]` (`:4123`). The new `wind_angle_conversion_is_scoped_per_game` sweeps games, but only for layer motion.
- **Evidence**:
  ```rust
  let (ob, ob_kind, ob_flow, _, _) = resolve_water_material(&waters, Some(0x0001_0000), GameKind::Skyrim);
  let (sk, _, _, _, _) = resolve_water_material(&waters, Some(0x0002_0000), GameKind::Skyrim);
  ```
- **Impact**: The guard named "game_invariant" no longer exercises the game axis it is named for.
- **Suggested Fix**: Resolve the sentinel records under every `GameKind`, at least resolving the Oblivion record under `GameKind::Oblivion`, and assert that the §4 sentinel list is identical across them.

### EXT-D5-2026-10-02-05: watal.md §2 still says Starfield pigment concentrations are normalized in the shader, and records neither #5151's metric lift nor the open unit of the noise-UV tile
- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Location**: `docs/engine/watal.md:340-343`; the code is at `byroredux/src/env_translate.rs:905-917` and `crates/plugin/src/esm/records/spatial_units.rs:151-158`.
- **Status**: NEW. Stale since #4285 (closed); the missing record dates from #5151 (closed).
- **Tier Violated**: no-fabrication (documentation)
- **Game Affected**: Starfield
- **Description**:
  - The §2 text reads: "Its authored pigment concentrations remain in their vanilla 0..20 range and are normalized in the shader against the shared `STARFIELD_WATER_CONCENTRATION_REFERENCE`".
  - Since #4285 the ÷20 runs at the WATAL translate (`env_translate.rs:905-913`, comment "normalized … HERE, at the WATAL translate boundary, not in `water.frag`"). The shader only clamps (`water.frag:563`).
  - §2 has no mention of #5151 (Starfield DNAM metres → BU: depth, underwater fog near/far and noise falloff ×70, absorption ÷70).
  - The open item #5151 left behind is recorded only in a code comment (`spatial_units.rs:157-158`): the noise UV tile sizes at DNAM 120/124/128 "stay unlifted until a capture settles their unit". The skill's promotion rule wants open items in watal.md §2.
- **Impact**: The spec places a per-game unit conversion at render time, which is the defect #4285 fixed. A reader cannot find out from WATAL that the Starfield noise tile unit is unresolved.
- **Suggested Fix**:
  - Rewrite `watal.md:340-343` to say the normalization happens at the translate.
  - Add a #5151 sentence on the metric lift and its boundary (`spatial_units::normalize`, Starfield-gated, FO76 untouched).
  - List "Starfield noise-UV tile unit (DNAM 120/124/128)" as open in §2.

### EXT-D6-2026-10-02-02: The `.btt` recon in object_lod.rs misreads the header, and its claim that `.lst` is "not needed" is false: the billboard size and atlas UV rect live only in the `.lst`
- **Severity**: LOW
- **Dimension**: Distant LOD and trees
- **Location**: `byroredux/src/cell_loader/object_lod.rs:765-773` (the doc on `tree_lod_supported`); the same claim is in the 8e512b02e commit message and the #4913 comment.
- **Status**: NEW
- **Tier Violated**: no-fabrication
- **Game Affected**: Skyrim
- **Description**:
  - The comment reads the header as "three u32s (`15, 9, 21` — version, counts?)". It concludes that "the `.lst` lists are generation-time species data and not needed to consume the baked `.btt`".
  - The census shows something different. A `.btt` is a u32 **group count**, then per group a u32 **tree-type index**, a u32 count, and that many 32-byte records. Each record is: f32 x, y, z (world, Z-up); f32 rotation (radians); f32 scale; u32 REFR FormID; two zero u32s.
  - For `tamriel.4.4.-12.btt`, 15/9/21 means 15 groups, then type 9 with 21 trees.
  - The type index keys the `.lst`: u32 count, then 32-byte entries of u32 index, f32 width, f32 height, f32 u_min, v_min, u_max, v_max, u32.
  - The `.lst` is therefore the only source of each billboard's world size and its rect in `<ws>treelod.dds`. A consumer cannot render a `.btt` without it.
  - `exal.md:347-352` already says this ("the 9 `.lst` tree species lists that key them", "a `.btt`+`.lst` … consumer"). The code comment contradicts the spec.
- **Evidence**: A Python parse of `Skyrim - Meshes1.bsa` (LZ4 v105):
  - 380 of 386 `.btt` parse to exactly their length under this layout. Every type index is below its worldspace's `.lst` entry count (tamriel 34, dlc2solstheimworld 36, sovngarde 15, …).
  - The 6 exceptions are `dlc2solstheimworld.4.{12,16}.{4,8,12}.btt`, which carry trailing bytes past the declared groups (e.g. 2,796 of 3,244 bytes). That tail is unexplained.
  - `tamriel.lst` entry 0: index 0, 566.4 × 1521.5 BU, UV (0.809, 0.002)–(0.895, 0.250).
- **Impact**: A future consumer that follows the in-code recon would skip the `.lst` and have no billboard dimensions or atlas coordinates.
  - Side note: each `.btt` record carries a REFR FormID, so the tree tier *does* have per-object ids. #3307's premise that "baked quads carry no per-object ids" holds for `.bto` but not for `.btt`. That matters when VWD culling is designed.
- **Suggested Fix**: Replace the recon paragraph with the verified layout of both files, record the 6 Solstheim files that carry trailing data as open, and drop the "not needed" sentence. Note the per-tree REFR FormID for #3307.

---

## Findings count

| Dimension | HIGH | MEDIUM | LOW |
|---|---|---|---|
| D1 EXAL boundary | 1 | — | 3 (1 Existing #5135) |
| D2 Terrain | — | — | 2 |
| D3 Ground cover | — | 1 | 4 |
| D4 Sky / weather / sun | — | — | 4 |
| D5 WATAL | — | — | 5 (1 Existing #5136) |
| D6 LOD / trees | — | 1 | 1 |
| D7 Gates / harness | — | — | — |
| **Total** | **1** | **2** | **19** |

**Dedup sources:**
- `/tmp/audit/issues.json` (143 open).
- `gh issue list --state all` searches: "FO76 water absorption", "placeable_fold", "density share groundcover", "btt tree LOD", "GpuTerrainTile 176", "weather_sky_state", "XWCU calm", "LGTM lift test", "noise UV tile Starfield". Each hit is the closed parent a finding extends; none is a duplicate.
- Sibling reports: AUDIT_EXTERIOR_2026-09-27/-29.

## Known-Open Register (dated; what this pass changed)

| Item | Status this pass |
|---|---|
| #4837, #4866, #4901–#4905, #4907–#4928, #4956, #4957, #5001, #5002, #5089, #5113, #5134, #5142, #5151 | **CLOSED since baseline, fixes re-read.** Incomplete or regressed ones are cross-referenced in the Fix-verification table. |
| #4913 (Skyrim `.btt` tree LOD) | **CLOSED but not delivered** → reopen (D6-01). |
| #4906 (LTEX→GRAS keying) | OPEN. The partial `3e6d94ff0` introduced D3-01; attach it there. |
| #4929 (Rapids third normal layer BU/s as UV/s) | OPEN, still present (`water.frag:839-840`); untouched by #4911. |
| #5135 (`BASE_FOG_STRENGTH`) | OPEN, unchanged (D1-04). |
| #5136 (watal.md frame table) | OPEN, partly resolved by #4910 (D5-01). |
| #3307 (VWD full-model culling) | OPEN. New input: every `.btt` record carries a REFR FormID, so per-tree VWD culling is possible (D6-02). |
| #4760 (Skyrim env var) | OPEN; out of exterior scope (re-flagged twice before). |
| skyal §2.3–§3 documented-open items | Still documented; not re-filed. |
| `renderer-eval-groundcover.sh` washout / backlit reference re-mint | Not re-measured (no engine launch). |

## Skill drift (for the next `/audit-exterior` edit)

- **Dim 1:**
  - Remove `exterior_image_spaces` (deleted by #4914) and the `render/sky.rs` fallback caller (#4902).
  - Its known-open list (#4901, #4902, #4914–#4917) is all closed.
  - Add `install_interior_outdoor_defaults` / `ProvisionalOutdoorEnvironment`.
  - Add `crates/plugin/src/esm/records/spatial_units.rs` to `Paths:` (second report asking), plus the FO76 shared-decoder unit question.
- **Dim 2:**
  - Guard rename `gpu_terrain_tile_is_160_bytes` → `gpu_terrain_tile_is_176_bytes`.
  - #4903 and #4905 are closed.
- **Dim 3:**
  - Add `crates/renderer/src/vulkan/groundcover/` (construct/frame halves from #5089) to `Paths:`.
  - #4866, #4907, #4919–#4921 and #4924 are closed; #4906 is partial.
- **Dim 4:**
  - Add `crates/renderer/src/vulkan/context/frame_params.rs` to `Paths:`.
  - #4908, #4909, #4925, #4926 and #4927 are closed.
  - Add the new pins.
- **Dim 5:** #4910 and #4911 are closed; the #4910 text should now describe the per-game table.
- **Dim 6:** #4912 is closed; #4913 is pending reopen.
- **Dim 7:** add `crates/debug-server/src/system.rs` and `scripts/check-byro-dbg-harness-contracts.sh` to `Paths:`.

## Cross-audit routing

- **`/audit-esm`:**
  - D1-01's fix lands in `spatial_units.rs` / `decode_dnam_fo76`.
  - The Starfield WTHR arm lifts parser defaults on an FNAM-less record. No vanilla record reaches this; not filed.
- **`/audit-renderer`:**
  - The `frame_params.rs` gap in the negative source scans (`shader_constants.rs:1325`, `resources.rs:1154`).
  - #4929.
  - An exterior before/after capture of #4909's ~4× clear-sky haze drop.
- **`/audit-physics`:** D5-03 (a Calm-WATR `WaterFlow` from XWCU) feeds the current sampler.
- **`/audit-performance`:** D3-03's per-frame `Vec` in `render/groundcover.rs` (#4922 class).

Suggested next step: `/audit-publish docs/audits/AUDIT_EXTERIOR_2026-10-02.md`.

**Labels:**
- All findings: `terrain-exterior`.
- D1-01: `high`, `bug`, `water`, `esm-plugin`, `game:fo76`.
- D3-01: `medium`, `bug` (comment on #4906 rather than a new issue).
- D6-01: reopen #4913 (`medium`, `game:skyrim`).
- D5-0x: `water`.
- D4-03, D5-04, D1-03: `test-gap`.
- D2-01, D3-05, D4-04, D5-01, D5-05, D6-02: `doc-rot`.
- D1-02: `game:starfield`.
- D5-03: `game:fo4`.
- D5-02: `game:oblivion`.
