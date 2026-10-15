**HEAD**: `adbd468e2` · **Baseline**: `AUDIT_OBLIVION_2026-10-12.md` (HEAD `5d31fd637`) · **Audited**: Dims 1–5 (delta `5d31fd637..adbd468e2`, 328 commits) plus the Phase 1 corpus lane · **Unchanged since baseline (skimmed)**: Dim 3's material, render and seam-blend paths (`crates/nif/src/import/material`, `render/static_meshes.rs`, `seam_blend`, `material_sampling.glsl`; only equivalent refactors and Fallout 4 / Skyrim `M47.2` work landed on the spawn path)

# Oblivion (TES4) Compatibility Audit — 2026-10-14

## Executive Summary

**Corpus lane: green.** All four Phase 1 gates pass on the 9 612-NIF Oblivion corpus (base + 8 DLC archives): `oblivion_block_count_parity`, `no_block_sizes_drift_detector_has_zero_false_positives_on_real_corpus`, `parse_rate_oblivion`, `per_block_baseline_oblivion`. Dim 1 re-measured 0 truncated, 0 recovered, 0 failed. There is no parser, archive or render-path defect in this window, and no regression of a closed issue.

The window is dominated by Fallout 4 / Skyrim `M47.2` scripting work, which leaves the TES4 branches intact (checked per commit; PROJ, ECZN, PERK, KYWD, LCTN, RELA, SNDR and CNDF have 0 records in `Oblivion.esm`). The Oblivion-visible changes are `#5696` (ARMO BMDT `u16` mask, correct on all 996 armours), `#5463` (CELL `XCCM` stays a CLMT on TES4, correct), `#5644` (DIAL override fold, identical to the old fold on 4 000 random rounds), `#5734` (quest-level dialogue gates), `f67d82b3c` (ObScript quest locals persist, fn 79 evaluated) and `5182f7817` (ObScript `Message` / `MessageBox` / `GetButtonPressed`).

| Severity | NEW | Findings |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 1 | OBL-D2-2026-10-14-01 |
| LOW | 12 | OBL-D1-01…03, OBL-D2-02, OBL-D3-01/02, OBL-D4-01/02, OBL-D5-01…04 |
| INFO | 1 | OBL-D1-04 |

The one MEDIUM is latent: nothing observes Oblivion dialogue today. It becomes user-visible the moment an Oblivion conversation route is exercised, because `#5734` makes 87 % of INFOs unselectable. Everything else is test gaps or doc rot.

**Compatibility level (cite `ROADMAP.md` / `docs/feature-matrix.md` for live rows).** NIF parse including the v10.x tail is complete on the vanilla corpus. BSA v103 extracts all 17 archives with 0 errors. ESM parse is clean on every named real-data pin. Tamriel exterior and interiors render (not re-verified on-device here).

**Method.** Five dimension agents, read-only; none launched the engine or any Vulkan process. I ran the corpus lane myself in release. Per dimension: `git log 5d31fd637..HEAD -- <Paths>`, the Oblivion-relevant delta read in full, the skill's named pins re-run as single named tests, then real-data checks against `Oblivion.esm` and the archives. Dedup against the 123 open issues and closed-issue searches.

## Pins and guards run

| Area | Result |
|---|---|
| Corpus lane (Phase 1) | 4 / 4 pass |
| Dim 1 header and recovery pins: #170 band, #149 `NiTexturingProperty` u32 count, #1509 morph cutoff (5), #3926 recovery refusal, #4257 inline names (3), `as_ni_node` wrapper coverage, #5438 / #5439 / #5440 unit tests | all pass |
| Dim 1 gates-go-red experiments (truncated base archive; garbage base archive) | parity, per-block and parse-rate go red on both; the drift corpus only on the garbage one (OBL-D1-02); a missing base archive skips green and fails under `BYROREDUX_REQUIRE_GAME_DATA=1` |
| Dim 2: BSA v103 all-17 sweep, zero-size placeholder tests, 6 named `--ignored` real-data tests (CLAS Knight, RACE, parse rate, PACK legacy PKDT/PTDT, starts-dead census, spawn-time global) | 6 / 6 pass |
| Dim 2: plugin lib 1148, scripting dialogue + condition 118 (Random-stack pins) | pass |
| Dim 3: `parallax_alpha_gate` 5, `body_paths_kf_era_*` 2, installed body paths (`--ignored`, real run), nif material 45, `legacy_is_pbr` + `pack_imported_material_flags` 10, dark-map census (8 meshes / 6 files), `dark_combine_is_the_pinned_bare_multiply_in_both_paths`, both-marchers bit-31, emitter unit set 51, torch emitter `--ignored` (272 emitters) | all pass |
| Dim 4: `placement_lod_supported_is_oblivion_only`, HNAM dimmer, LTEX and default-land-texture `--ignored` pins, `__MAX_Default_Light` gate, child water / distant water, inherit-all stamp, climate rungs, XCLR, LIGH falloff, #5463 XCCM; bin sweep 773 | all pass |
| Dim 5: vanilla Oblivion quest-script gate, `menuxml` `vanilla_corpus` 4 / 4, menuxml unit 58, scripting lib filter (`obscript`, `messages`, `condition`) 137 | all pass |

## Dimension Findings

### Dim 2 — BSA v103 & ESM data slice

### OBL-D2-2026-10-14-01: #5734 quest-level dialogue gates fail closed on 339 of 390 Oblivion quests, declining 87 % of all INFOs and 87 % of GREETING
- **Severity**: MEDIUM (HIGH once any Oblivion dialogue gate / live path exists; today nothing observes Oblivion dialogue)
- **Dimension**: 2 (DIAL/INFO selection; owner `/audit-scripting` for the condition catalog)
- **Location**: `select_info` / `quest_gate_passes` (`crates/scripting/src/dialogue.rs`), `is_modeled` + `ConditionFunction::from_index` (`crates/scripting/src/condition.rs`), install in `populate_scene_runtime` (`byroredux/src/asset_provider/script.rs`, unconditional, no game gate), `parse_qust_contextual` (`crates/plugin/src/esm/records/misc/quest.rs`, pre-`INDX` `CTDA` → `QustRecord.dialogue_conditions`)
- **Status**: NEW (commit `6709eb65e`, after the 2026-10-12 baseline). Sibling of open #5750 (Skyrim leg) and #5752 (INFO-level uncatalogued functions); #5734 itself is closed and its commit message names "Oblivion 254/365" as accepted fallout. Dedup checked: no open issue covers the quest-level Oblivion leg.
- **Description**: #5734 evaluates each quest's Quest-Conditions list before any of its INFOs and applies the #5380 doctrine to it: one uncatalogued function in the list and the whole quest's INFOs decline. Oblivion authors a quest-level list on nearly every dialogue quest, and the lists carry fn 254 and fn 365, neither of which is in the 48-function catalog (`from_index` has no arm for either).
- **Evidence**: raw-byte census of `Oblivion.esm`. 339 of 390 QUST have `CTDA` records before the first `INDX` (e.g. `0x42b9c`: EDID SCRI FULL ICON DATA **CTDA(fn 254)** INDX …; `0x3be84`: … DATA CTDA(254) CTDA(71)). Function counts: fn 254 ×308, fn 365 ×297, fn 71 ×75, 69 ×12, 67 ×11, 72 ×10, 70 ×2, 310 / 313 ×1. All 339 lists contain an uncatalogued function. INFOs owned (by `QSTI`) by those quests: **16 799 of 19 278**; GREETING: **3 247 of 3 743**. The remaining 496 GREETING INFOs belong to quests with no list. The pre-`INDX` placement is quest-level (stage and target `CTDA` come after `INDX` / `QSTA`; the 3 065 stage-level `CTDA` are parsed into the stage, not the gate).
- **Impact**: once an Oblivion dialogue route is exercised, 87 % of lines can never be selected. Every activation opens GREETING (#5367 Phase G), so NPCs fall back to the ~13 % of lines in ungated quests. The fail-closed choice is correct for the priority-hijack case #5734 targeted, but on Oblivion these are ordinary "playable race / not in Shivering Isles" gates. Static analysis plus code reading; not executed in a live conversation (no Oblivion dialogue gate exists).
- **Related**: #5750, #5752, #5734, #5380. 254 and 365 are very probably `GetIsPlayableRace` / `GetPlayerInSEWorld`, but the opcode-to-name binding is unsourced (the CS wiki list carries no opcode numbers, the xEdit TES4 table is not on disk). Do not guess.
- **Suggested Fix**: source the TES4 opcodes (OBSE command table / xEdit `wbDefinitionsTES4`), catalogue 254 and 365, and re-run the census. Until then consider scoping the fail-closed rule to quest lists that actually outrank another quest (the hijack case) rather than every unreadable list.

### OBL-D2-2026-10-14-02: CELL `XCCM` has no Oblivion-keyed pin; the Oblivion arm is correct only by `matches!(Skyrim | Fallout4)` omission
- **Severity**: LOW (test gap, behaviour correct)
- **Dimension**: 2 (`walkers.rs` / `cell/helpers.rs`)
- **Location**: `xccm_targets_region`, `CellSubrecordFields::absorb` (`crates/plugin/src/esm/cell/helpers.rs`); tests in `crates/plugin/src/esm/cell/tests/cell.rs`
- **Status**: NEW
- **Description**: #5463 splits XCCM by game. Oblivion falls into the CLMT arm (`climate_override`), which is right: `Oblivion.esm` authors 55 CELL XCCM and all 55 resolve to CLMT records (8 distinct climates, e.g. `0x32E16` ×26, `0xC099A` ×12). The new tests cover FO3/FNV (CLMT) and Skyrim/FO4 (REGN) only; the FO3 test stands in for TES4.
- **Evidence**: `xccm_targets_region(game) = matches!(game, Skyrim | Fallout4)`; the XCCM tests name only `Fallout3NV`, `Skyrim`, `Fallout4`.
- **Impact**: none today. A future edit that adds `Oblivion` to the region arm, or a `GameKind` refactor, passes every current pin.
- **Related**: #5463 (closed), #693.
- **Suggested Fix**: add `GameKind::Oblivion` to the CLMT-side test loop, and optionally an `--ignored` `Oblivion.esm` census that every XCCM target is a CLMT.

### Dim 1 — NIF version handling & corpus integrity

### OBL-D1-2026-10-14-01: an unopenable Oblivion DLC mesh archive is still "absent" for two corpus gates
- **Severity**: LOW (test gap)
- **Dimension**: 1 (NIF corpus integrity)
- **Location**: `crates/nif/tests/common/mod.rs` `open_optional_mesh_archives` (called from `block_coverage_baselines.rs::oblivion_block_count_parity` and `parse_real_nifs.rs::parse_rate_oblivion`)
- **Status**: NEW (adjacent to closed #5439 / #5232, which deliberately left this opener alone)
- **Description**: #5439 routed `open_mesh_archive`, `open_all_mesh_archives`, `open_ba2_by_name` and the drift corpus through `open_present_archive` (panic). `open_optional_mesh_archives` keeps its own `match` and only `eprintln!`s "present but failed to open", then drops the archive. For Skyrim SE the tier is account-varying (Creation Club), so tolerance is defensible; for Oblivion it is the eight static vanilla DLC archives, and `oblivion_block_count_parity` already keys `archive_parsed` per archive precisely so a present archive is always compared.
- **Evidence**: experiment with a real base BSA and a garbage `DLCFrostcrag.bsa`: `oblivion_block_count_parity` reported "1 archive(s) checked, all parsed >= baseline" and `parse_rate_oblivion` passed, while the drift corpus on the same directory panicked. The gates disagree on identical input.
- **Impact**: a BSA-reader regression specific to a DLC archive shape (1 580 NIFs, 16.4 %, the population #3712 added the gate for) passes both gates. A regression that also breaks the base archive is still caught, so the blast radius is small.
- **Related**: #5439, #5232, #3712, #3369.
- **Suggested Fix**: in `open_optional_mesh_archives` panic via `open_present_archive` (an absent file is still skipped; only a present-but-unopenable file fails). If the Skyrim SE tier must stay tolerant, make it per-game.

### OBL-D1-2026-10-14-02: the drift corpus swallows per-entry extract failures
- **Severity**: LOW (test gap, defense in depth only)
- **Dimension**: 1
- **Location**: `crates/nif/tests/oblivion_stream_drift_corpus.rs` `no_block_sizes_drift_detector_has_zero_false_positives_on_real_corpus` (`let Ok(bytes) = archive.extract(f) else { continue; }`, `let _ = parse_nif(&bytes)`)
- **Status**: NEW
- **Description**: #5439 closed the open-failure facet. An archive that opens but whose entries fail to extract (or NIFs that hard-fail to parse) is still counted in `total_files` and contributes nothing. The test asserts only `count == 0` and never that anything was actually walked.
- **Evidence**: with the base BSA truncated to 300 MB the test printed "1 archive(s), 8032 NIF(s), 0 drift warning(s)" and passed while 6 643 of 8 032 extracts failed.
- **Impact**: none today, because `parse_rate_oblivion`, `oblivion_block_count_parity` and `per_block_baseline_oblivion` all go red on the same input. The detector alone would silently measure ~17 % of the corpus.
- **Related**: #5439, #3711.
- **Suggested Fix**: count extract and parse failures and assert zero (or assert `parsed == total_files`).

### OBL-D1-2026-10-14-03: nothing checks that a sizeless parse lands on the footer
- **Severity**: LOW (test gap)
- **Dimension**: 1
- **Location**: `crates/nif/src/lib.rs` `dispatch_blocks` / `finalize_scene` (no footer read; `root_index` is the first NiNode-family block); `crates/nif/tests/` (no EOF-exactness gate)
- **Status**: NEW
- **Description**: with no `block_sizes` table (every Oblivion file) a wrong *last* block is invisible: a successor would fail, but the last has none, and `parse_nif` never reads the footer or compares the cursor with the file length. The corpus gates (truncation, per-block histogram, drift detector over fixed-size types) cannot see an under-read of a file's final block, such as a missing trailing field on `NiTriStripsData` or `NiSkinPartition`.
- **Evidence**: throwaway scan outside the repo over all 9 612 Oblivion NIFs (6 pre-5.0.0.1 inline-name files skipped): 9 598 of 9 606 end exactly at `4 + 4*num_roots` bytes before EOF. The other 8 are Tamriel terrain-LOD files (`meshes\landscape\lod\60.*`) whose footer is correct right after the last block and then followed by 1.4–235 KB of stale bytes (a longer file overwritten by a shorter one, not a parser under-read). `root_index` is block 0 in all 9 612.
- **Impact**: none measured; a blind spot, not a defect. A future last-block under-read would be silent.
- **Related**: #3926, #3711.
- **Suggested Fix**: add an `--ignored` corpus assertion that, for sizeless files, the cursor after the last block satisfies `remaining == 4 + 4*num_roots`, with the 8 known stale-tail LOD files as an explicit allowlist.

### OBL-D1-2026-10-14-04: the #170 BSStreamHeader dual band is pinned one-sided by a unit test
- **Severity**: INFO
- **Dimension**: 1
- **Location**: `crates/nif/src/header.rs` `has_bs_stream_header`, test `bs_stream_header_not_read_for_off_spec_version`
- **Status**: NEW (records pin strength; no defect)
- **Description**: the only unit pin is the negative case (v20.1.0.0, user_version 4). The positive bands Oblivion lives in (v10.0.1.2 unconditional; 10.1.0.0..=20.0.0.4 with `3 <= user_version <= 11`; 20.0.0.5) and both edges have no header-level test; the corpus guards them. The expression matches nif.xml `#BSSTREAMHEADER#`.
- **Evidence**: header census of all 9 612 files gives 16 header tuples, none off-band (e.g. 20.0.0.4/10/11: 1 497, 20.0.0.4/11/11: 5 785, 20.0.0.5/11/11: 1 680, 10.2.0.0/10/9: 228).
- **Impact**: the corpus is sufficient for vanilla; only mod content is unprotected.
- **Related**: #170, #171.
- **Suggested Fix**: optionally add three small header tests (10.0.1.2 reads the stream header at user_version 0; 20.0.0.4 with user_version 12 does not; 10.1.0.0 reads it).

### Dim 3 — Legacy-property rendering path

### OBL-D3-2026-10-14-01: the RACE body-section ICON decode and the spawn-time race-skin selection (second half of #5487) have no test
- **Severity**: LOW
- **Dimension**: 3 (NPC body assembly)
- **Location**: `crates/plugin/src/esm/records/actor/race.rs` (`parse_race`, the `b"ICON" if !in_head_section && !in_skeleton_section` arm and the `NAM0` / `NAM1` / `MNAM` / `FNAM` section state); `byroredux/src/npc_spawn/resumable/runtime.rs` (`prepare_runtime_state`, the `body_skin` closure)
- **Status**: NEW
- **Description**: #5487 shipped as two halves: the body-path table (pinned by `body_paths_kf_era_*` and the archive-presence test) and the race body skins (RACE `ICON` run → `RaceRecord::body_part_textures` → per-part base-texture swap + seam context). The second half decides every Oblivion NPC's skin tone and nothing exercises it. `body_part_textures` appears outside `race.rs` / `runtime.rs` only as `Vec::new()` in six fixtures. `body_skin` is an inline closure, so the `(index, gender-section)` pick cannot be unit-tested.
- **Evidence**: `git show 1b45763fa --stat` touches `race.rs` (+23) with no test file. Re-measured on `Oblivion.esm`: 124 entries, 13 races × 8 (idx 0–3 × M/F) plus Khajiit and Argonian × 10 (idx 4 Tail), all resolving. The code works today; only the guard is missing.
- **Impact**: a reorder of the head/body `INDX`/`ICON` state machine (it shares `icon_indx` with the head-section arm and relies on `NAM1` resetting it) would silently drop every race skin back to the Imperial default, returning the symptom #5487 fixed with every test green.
- **Related**: #5487 (closed), #5753 (tail MODL, open; would extend the same RACE arm).
- **Suggested Fix**: add a synthetic-RACE unit test asserting `body_part_textures` and untouched `head_part_textures`; factor `body_skin` into a free fn over `(&RaceRecord, icon_idx, want_gender_tag)` and pin the Male / Female / `None`-section picks; optionally a named `Oblivion.esm` assertion (15 races, 124 entries).

### OBL-D3-2026-10-14-02: the #5487 archive-presence pin skips green under `BYROREDUX_REQUIRE_GAME_DATA=1`, and none of Dim 3's real-data pins run in the strict CI lane
- **Severity**: LOW
- **Dimension**: 3
- **Location**: `byroredux/src/npc_spawn/tests.rs` `installed_oblivion_body_paths_exist_in_the_meshes_archive` (the `let Ok(archive) = … else { eprintln!("skipping …"); return; }`); `.github/workflows/real-data-gates.yml`
- **Status**: NEW (same family as closed #3850 / #4660 / #5326 / #5011)
- **Description**: the same file already owns `require_game_data(env_var, tried)` (#3850) and other tests call it; this one does not. With `BYROREDUX_REQUIRE_GAME_DATA=1 BYROREDUX_OBLIVION_DATA=/nonexistent` it prints "skipping …" and reports `ok`. Separately, no CI lane executes it: the `corpus` matrix filters `-p byroredux-nif` on the substring `oblivion`, so `installed_oblivion_body_paths_exist_*` (bin crate), `dark_texture_role_population_matches_the_documented_census` and `real_archive_torch_meshes_surface_particle_emitters` (names carry no `oblivion`) are manual-only.
- **Evidence**: runs recorded above; the `body_paths_kf_era_*` unit test compares the function to the very literals it returns, so only the `--ignored` test can catch archive-name drift.
- **Impact**: a future edit that makes `humanoid_body_paths(Oblivion, …)` request a name the archive does not ship (the original #5487 defect) stays green in CI.
- **Related**: #3850, #5326, #5487, #5088.
- **Suggested Fix**: call `require_game_data("BYROREDUX_OBLIVION_DATA", &bsa_path)` in the skip branch; add the three Dim 3 real-data tests by exact name to a strict-lane step, as was done for the ObScript gate (#5088).

### Dim 4 — Exterior & lighting data

### OBL-D4-2026-10-14-01: Oblivion's 55 `XCCM` realm interiors never apply their authored climate
- **Severity**: LOW
- **Dimension**: 4 (mechanism owned by `/audit-exterior`, interior sky)
- **Location**: `CellData::climate_override` (written by `CellSubrecordFields::absorb`, `crates/plugin/src/esm/cell/helpers.rs`); sole consumer `apply_cell_climate_override` (`byroredux/src/scene/world_setup.rs`), which reads it from `exterior_cells` only; the interior Show-Sky path (`byroredux/src/render/sky.rs`, `outdoor_sky_params` over the installed `SkyParamsRes`) never looks at it
- **Status**: NEW (Oblivion-visible; #5463 left the Oblivion decode correct and unchanged)
- **Description**: Oblivion's 55 `XCCM` cells (Oblivion-plane towers `OblivionRD00x*`, `OblivionMqKvatch*`, sigil / MQ14 / MQ16 realms) are interior cells with Show Sky set. They point at CLMTs such as `Obliviondefaultclimate` (26), `ClimateSigil` (12), `MQ14OblivionClimate` (6) and `DefaultClimate` (4). `3b28dca71` correctly keeps TES4 `XCCM` as a CLMT, so `climate_override` is populated, but nothing applies it to an interior.
- **Evidence**: census of `Oblivion.esm`; `rg climate_override` finds consumers only in `world_setup.rs` (exterior streaming) and `climate.rs`.
- **Impact**: the sky seen from the Oblivion-realm Show-Sky interiors is whatever outdoor environment happens to be installed, not the authored realm climate. Not verified on-device. Same forward-scope gap #5463 records for Skyrim's `sky_region`. No open issue covers the TES4 interior-CLMT leg.
- **Related**: #5463 (closed), #2451 / #5529 (exterior per-cell hook), #4908 / #4839 (interior Show-Sky sky lane).
- **Suggested Fix**: let the interior load resolve `climate_override` through `resolve_cell_climate` and install the CLMT's default weather for Show-Sky interiors, sharing the exterior hook's re-apply path.

### OBL-D4-2026-10-14-02: #5505's LOD half is already consumed since #5496; no test composes the Oblivion parse stamp with `lod_source_worldspace`
- **Severity**: LOW (doc and coverage gap, not a behaviour bug)
- **Dimension**: 4
- **Location**: `lod_source_worldspace` / `ExteriorWorldContext::lod_worldspace_key` (`byroredux/src/env_translate/lod.rs`, `byroredux/src/cell_loader/exterior.rs`); skill Dim 4 bullet "nothing consumes the land / LOD bits yet (#5505)"
- **Status**: NEW (the staleness and the missing pin); the underlying gap is Existing: #5505, state re-verified (still OPEN, no comments, nothing in the delta touches it)
- **Description**: `2ca57c4bc` (#5496, 2026-10-10) keys the terrain-LOD quad index, `.btr` / generated-LOD paths, worldspace form id and band bounds on the nearest `WNAM` ancestor that does not set `INHERIT_LOD`. The #5374 parse stamp sets that bit on all 30 Oblivion children, so for BravilWorld, AnvilWorld and the others the terrain-LOD ring should already draw Tamriel's `landscapelod\generated` quads, holed against the child's own cells. The `INHERIT_LAND` bit is still read only for `DNAM` water height. The residual of #5505 is the full-detail parent LAND for LAND-less ring cells (1 043 cells, 1 040 of which have parent LAND).
- **Evidence**: `rg INHERIT_LOD` finds `translate_lod_water` and `lod_source_worldspace`; `terrain_lod.rs` takes `worldspace_key = wctx.lod_worldspace_key()`; `placement_lod.rs` still uses the child's own key, which is right (children ship their own `distantlod\<child>_*.lod`). Existing tests use synthetic Wasteland / Whiterun chains and an installed-FO3 pin.
- **Impact**: #5505's description overstates the missing LOD. Without a pin, an Oblivion-specific break (the `OblivionLegacy` layout branch, `legacy_lod_index`) would be invisible.
- **Related**: #5505, #5496, #5374.
- **Suggested Fix**: add a plugin + `env_translate` pin that parses an Oblivion-shaped child (`WNAM`, no `PNAM`) and asserts `lod_source_worldspace == root`; re-scope #5505 to the parent-LAND ring after an on-device check of BravilWorld.

### Dim 5 — Gameplay & UI data slice

### OBL-D5-2026-10-14-01: the Oblivion branch of the `f67d82b3c` flip (GetQuestVariable evaluated, quest locals persisted, `is_modeled` game split) has no test
- **Severity**: LOW (test gap; code behaves correctly on every probe)
- **Dimension**: 5 (routes the mechanism to `/audit-scripting`)
- **Location**: `byroredux_scripting::condition::{uses_obscript_quest_variables, is_modeled}`, the `ConditionFunction::GetQuestVariable` arm of `evaluate_function`, `obscript_quests::{ObScriptQuestLocals, run_quest_game_mode}`
- **Status**: NEW
- **Description**: `f67d82b3c` turns index 79 from "uncatalogued → fail-closed everywhere" into "evaluated against `ObScriptQuestLocals` when `ScriptGame` is Oblivion / Fallout3NV", and makes quest locals survive between GameMode runs. Neither is pinned. The only tests that mention fn 79 (`npc_spawn/ai_package.rs`) run with no `ScriptGame` and pin the pass-through branch (the opposite one). `run_quest_game_mode` has no test caller outside the system registration.
- **Evidence**: `grep -rn "ObScriptQuestLocals|uses_obscript_quest_variables|is_modeled" crates byroredux` finds definitions and uses, no `#[test]`; `grep -rn "function_index: 79"` gives two hits, both the no-game fixture. On real data 3 774 Oblivion INFO + 403 QUST conditions are fn 79 (all 4 177 `param_2` values match an SLSD index field); 1 011 of them (898 INFO + 113 QUST) are satisfied by an all-zero local, so wherever the variable is only written by a path the engine does not run (remote `set Quest.var` is declined per #5630, result scripts, object scripts) the gate now opens where it used to stay closed. That is the documented phase-2 limit, not a regression.
- **Impact**: a later refactor of `ScriptGame` install, `is_modeled` or the `of` / `store` seeding could silently return Oblivion / FNV dialogue and package gates to fail-closed (or Skyrim / FO4 to misread index 79) with every suite green.
- **Related**: `f67d82b3c`; `m42-eat-sleep.sh` is the only (device-bound, FNV) guard; save-side shape is covered by `saved_type_shape_changes_require_format_major_bump` (FORMAT 97).
- **Suggested Fix**: three small unit tests — (a) `evaluate` of a fn-79 condition under `ScriptGame(Oblivion)` with a seeded `ObScriptQuestLocals`, under `ScriptGame(Skyrim|Fallout4)` (passes), and with no resource (0.0); (b) `is_modeled` false for 79 under Skyrim / FO4 and true under Oblivion / FNV; (c) run a synthetic quest script twice through `run_quest_game_mode` asserting a `short` set on tick 1 is read on tick 2 (the `doonce` idiom).

### OBL-D5-2026-10-14-02: the `obscript_quests.rs` module doc still documents Message / MessageBox / GetButtonPressed as phase-1 stubs
- **Severity**: LOW (doc rot)
- **Dimension**: 5
- **Location**: module header of `crates/scripting/src/obscript_quests.rs` (command table rows `0x1059 / 0x1000` and `0x101f`)
- **Status**: NEW
- **Description**: the table says "Message / MessageBox → effect-log entry (HUD is later work)" and "GetButtonPressed → -1 (no message menu this phase)". `5182f7817` made both real (`QuestCommandHost::message` → `show_obscript_message`; `0x101f` → `take_button`); the `0x1059 | 0x1000` arm comment was updated but the header was not. The locals persistence from `f67d82b3c` is not mentioned either.
- **Evidence**: `sed -n 1,25p crates/scripting/src/obscript_quests.rs` against `fn message` and the `0x101f =>` arm.
- **Impact**: misleads the next reader about what a vanilla quest script's `MessageBox` does today (it now raises a native, blocking page).
- **Suggested Fix**: update the two table rows and add a `GetQuestVariable` / `ObScriptQuestLocals` row.

### OBL-D5-2026-10-14-03: `substitute_numbers` mis-renders a flag-prefixed `%0.f` (precision falls back to 6)
- **Severity**: LOW (forward-looking; no quest script is affected today)
- **Dimension**: 5 (shared formatter in the M47.2 message path; routes to `/audit-scripting`)
- **Location**: `byroredux_scripting::messages::substitute_numbers`
- **Status**: NEW
- **Description**: the spec scan collects `[.0-9]*` into `spec`, then derives precision with `spec.strip_prefix('.')`. For `%0.f` the spec is `"0."`, `strip_prefix('.')` is `None`, so precision defaults to 6: value 3 renders `3.000000` instead of printf's `3`. Any width or flag digit before the dot (`%2.0f`) has the same shape.
- **Evidence**: `Oblivion.esm` script text census of Message / MessageBox specifiers: `%.0f` ×39, `%0.f` ×3, `%.2f` ×3, `%.1f` ×1 (+ one source typo `%Of`). The three `%0.f` sites are `SEHillofSuicidesSCRIPT` (commented out) and `XPKnotboneFactionFixerSCRIPT` ×2, all object scripts, none a quest script. The unit test covers only `%.0f`.
- **Impact**: cosmetic wrong text ("1.000000 taken out") once object scripts run on Oblivion; any MESG / Papyrus text using a width.
- **Suggested Fix**: parse `[flags][width][.precision]` (take the part after `.` as precision, ignore leading digits) and add a `%0.f` row to the message tests.

### OBL-D5-2026-10-14-04: `GetButtonPressed` answer is keyed by the raising quest; the CS wiki contract is global
- **Severity**: LOW (latent; unreachable today)
- **Dimension**: 5
- **Location**: `messages::{ButtonPresses, record_button, take_button, show_obscript_message}` (`source: message.message_box.then_some(quest)`)
- **Status**: NEW
- **Description**: `take_button(world, quest)` only sees boxes that same quest raised. The CS wiki (`GetButtonPressed.wiki`) says the first call after a button press returns the button number and otherwise -1, with no per-script scope. Vanilla quest scripts `MQ01Script` (4 reads) and `Dark02WateryScript` read it without raising any box themselves; their boxes come from stage result scripts or object scripts, which the engine does not run yet. Vanilla quest scripts that raise boxes (MS43, Vampire ×13, FGConversation…, Dark00General, Dark01Knife, DarkExileControl, SE04, SE14, MQ14, TG11Heist, FGD08Infiltration, Dark16Kiss, Charactergen) never read the button, so persisted locals cannot strand them.
- **Evidence**: census over `Oblivion.esm` quest scripts (SCRI of 390 QUST) for `getbuttonpressed` / `0x1000` / `0x1059`.
- **Impact**: none now. When stage-result or object scripts start raising boxes, a reader in a different script or quest will see -1 forever.
- **Suggested Fix**: when result scripts land, make the press a single global slot (last answered box) consumed by the first `GetButtonPressed` after it, keeping `source` only for routing.

## Known-open items, re-verified

- #5505 — still OPEN; the LOD half is stale (OBL-D4-2026-10-14-02), the parent-LAND ring remains.
- #5526 — still OPEN (no `VTCK` / `VTYP`; voices ship as race\sex `.mp3`).
- #5350 — still holds (latent: `topic_entry` has no Oblivion arm; nothing observes it).
- #5752 — still OPEN, partly overtaken: the catalog is now 48 functions and fn 79 / 237 are modeled on Oblivion.
- #5750 (Skyrim twin of OBL-D2-2026-10-14-01) — OPEN.
- #5753 (Argonian / Khajiit tailless) and #5754 (stale `humanoid_skeleton_path` doc) — both still hold.
- #5439 — **closed and fixed** (`5f414d681`); verified live. The skill lists it as known-open: stale (see below). Residual: OBL-D1-2026-10-14-01.
- #5011 (EGM sweep) and #5351 (HNAM doc rot) — not re-examined this run.
- Fixed items re-checked and holding: #5504, #5073, #5397, #4638, #5013, #5084, #5400, #5377, #5033, #5696 (ARMO BMDT `u16`), #5463.

## Regression Guard List

Stride-drift family (#1506–#1509, the four-gate corpus lane above); #170 dual band (corpus + one-sided unit pin); `NiTexturingProperty` u32 shader-map count (#149); morph cutoff (#1509); BSA v103 all-17 sweep and the zero-size placeholder tests (#5073); `parallax_alpha_gate_tests`; the Disney gate stays 0 for Oblivion (re-measured over 41 886 imported meshes: `is_pbr`, `material_path`, `from_bgsm` and translucency all 0, so `MAT_FLAG_PBR_BSDF` stays 0); census figures that reproduce exactly (1 274 `APPLY_HILIGHT2` properties, 100 with a BC1 `_n.dds`; apply modes 1 and 3 on 681 of 30 121; 14 `bump_texture` uses; dark-map role on 8 meshes / 6 files).

## Open Work

From `ROADMAP.md` Known Issues. Interiors and the Tamriel exterior already render, so neither is a blocker. Items that bear on Oblivion: the full-detail parent LAND for child worldspaces (#5505), Oblivion voice resolution (#5526), the opcode-to-name catalogue for TES4 conditions (OBL-D2-2026-10-14-01, #5752), object-script blocks and actor-state functions (M47.3 phase 2), tail MODL on Argonian / Khajiit (#5753), and an unverified-on-device ground-cover change (below).

## Needs on-device verification (not run)

No Vulkan process was launched. Not run: `m-exteriors.sh oblivion …`, `/audit-runtime --game oblivion`, the Gilded Carafe runtime baseline, `p0-door-interaction.sh oblivion`, the HUD smoke `m48-4-oblivion-hud.sh`, `cell_climate_probe`. Visible-behaviour questions:
- **Ground cover is live on Oblivion** (`592f31b87`, `c14dd2b39`): 75 of 229 LTEX author `GNAM` (166 refs, all resolving) and 108 GRAS are all placeable. Cover is now localised, so the 154 LTEX without `GNAM` grow nothing. That matches the authored data but is a visible change; not rendered.
- `_far.nif` placement LOD and child-world terrain LOD (BravilWorld).
- Interior Show-Sky skies in the realm cells (OBL-D4-2026-10-14-01).
- POM zeroing, seam-blend tone and the dark-map multiply (verified by code and tests only).

## Stale skill premises (for the next `/audit-oblivion` sync)

- **Preamble:** #5439 is closed and fixed; move it to "Fixed since" with the residual note (OBL-D1-2026-10-14-01). The condition catalog is 48 functions, not 24.
- **Dim 1:**
  - The #4257 pins are `pre_gamebryo_version_with_blocks_uses_inline_names`, `gamebryo_version_never_uses_inline_names_even_with_an_empty_table` and `zero_blocks_never_uses_inline_names_regardless_of_version` (`crates/nif/src/tests.rs`), not `uses_inline_block_type_names`.
  - The #3926 pins are in `crates/nif/src/tests.rs` (`a_median_smaller_than_the_failed_block_s_own_consumption_is_declined`, `a_median_larger_than_the_failed_consumption_still_recovers`); `oblivion_skip_sizes` has no production caller.
  - The #149 pin builds a v20.2.0.7 / bsver 83 header, not an Oblivion one.
  - Add the corpus census as the "off-band version" baseline: 16 header tuples; 8 stale-tail `landscape\lod\60.*` files.
- **Dim 2:**
  - "0 `PNAM` / 0 `ANAM`" holds for `Oblivion.esm` only; the DLC `.esp` files carry hundreds of PNAMs (Knights 868, BattlehornCastle 120, ThievesDen 41).
  - The section should mention #5734 quest-level gates (339 of 390 quests carry one) next to the QSTI / Random-stack bullet.
  - The ARMO bullet should say the BMDT mask is the `u16` and that the General Flags byte (Hide Rings / Amulets, Non-Playable bit 6, Heavy bit 7) is decoded nowhere for Oblivion ARMO / CLOT, so `armor_type` is `None` on all 996 armours.
- **Dim 3:**
  - "15 / 15 races author 8" is wrong: 13 races author 8; Khajiit and Argonian author 10.
  - The swap runs in the per-spawn `pre_spawn_hook` on a copy-on-write clone (`apply_spawn_hook` → `Arc::make_mut`), not "at import".
  - Add `byroredux/src/npc_spawn/resumable/runtime.rs` and `crates/plugin/src/esm/records/actor/race.rs` to the Paths.
  - "140 of 208 emitter-bearing NIFs carry several `NiPSysEmitter`s": measured 87 of 140 in `Oblivion - Meshes.bsa` and 144 of 214 across base + DLC; 140 is the emitter-bearing count.
- **Dim 4:** the land / LOD bit claim is stale (LOD consumed since #5496). Add the live, localised ground-cover fact (75 of 229 LTEX) with `/audit-exterior` as owner, and the WTHR `HNAM` size (56 bytes, 37 / 37).
- **Dim 5:**
  - `Message` / `MessageBox` / `GetButtonPressed` are no longer phase 2; only object-script blocks and actor-state functions remain.
  - Quest locals persist (saved `ObScriptQuestLocals`, FORMAT 97), and fn 79 `GetQuestVariable` is evaluated on Oblivion / FO3 / FNV; `is_modeled` replaces the four copies of the unknown-function fail-closed test.
  - The `HudBar` `0x3E8` doc rot is fixed (`468c66142`, #5033).
  - `menu_texture_header_census_decodes` now also pins per-set counts 1 975 / 1 712 / 1 789 (#5669), and `vanilla_corpus_parses` pins 1 868 / 2 773 tiles (#5329).

## Process notes

**Dedup.** Open-issue titles searched for dialogue / condition / quest-gate, XCCM, climate, `GetQuestVariable`, `GetButtonPressed`, LOD. Related existing: #5752 (INFO-level, distinct from the quest-level OBL-D2-2026-10-14-01), #5750, #5505, #5526, #5350, #5753, #5754. Closed and not regressed: #5439, #5463, #5734 (accepted fallout), #5696, #5487.

**Dropped candidates (by dimension agents).** Barter (`61ad95cf3`) is off the Oblivion path (no Oblivion FACT carries VEND / VENC / VENV, and the `attach.rs` change is a visibility change). The `attach.rs` hunks from `5f1671e59` and `74a05df6e` are VMAD / Papyrus-only. The menuxml fixes (#5329, #5668, #5670, #5693, #5669) show no defects. The #149 pin running on a Skyrim header still discriminates the bug it guards. A runaway float overflow to inf / NaN in `ObScriptQuestLocals` could break the JSON save column on load: routed to `/audit-save` as a hardening note, not filed here. Gameplay effect of the new Essential / Protected stamping on Oblivion NPCs: routed to `/audit-gameplay`. `triangle.frag` ReSTIR / GI changes and the fog-colour decode are shared renderer code, covered by `AUDIT_RENDERER_2026-10-14.md`.
