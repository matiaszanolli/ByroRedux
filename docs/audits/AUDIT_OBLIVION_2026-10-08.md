# Oblivion (TES4) Compatibility Audit — 2026-10-08

**HEAD**: 00f580e09 · **Baseline**: `docs/audits/AUDIT_OBLIVION_2026-10-05.md` (HEAD `a2c24b16e`; 116 commits since) · **Audited**: Dims 1, 2, 3, 4, 5. Every dimension had commits on its Paths and was delta-reviewed. · **Unchanged since baseline (skimmed)**: none at dimension level. Dim 3's `seam_blend.rs`, `crates/nif/src/import/material/`, `render/static_meshes.rs` and `material_sampling.glsl` had no commits and were skimmed. Dim 5's ObScript VM edits are clippy-only.

This run is part of the 2026-10-08 `/audit-suite --preset comprehensive`.
- **How it was run**: solo, no sub-agents, against the vanilla Oblivion + DLC data at `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`.
- **Not run**: the engine binary and the device-bound smoke scripts (suite rule).
- **Scratch**: `/tmp/audit/oblivion/`. It holds `dim_1..5.md`, the corpus log, and the census scripts `greeting_census.py`, plus inline CLMT/REGN/WRLD walks.

## Executive Summary

**Corpus lane: GREEN.** The command was `BYROREDUX_REQUIRE_GAME_DATA=1 cargo test -p byroredux-nif --release --test parse_real_nifs --test per_block_baselines --test block_coverage_baselines --test oblivion_stream_drift_corpus -- --ignored oblivion no_block_sizes`, and it exited 0.
- **`parse_rate_oblivion`**: 9,612 / 9,612 clean (100.00%), with 0 truncated and 0 failed. By archive: Meshes 8,032, Shivering Isles 1,438, Knights 75, plus 6 small DLC.
- **`oblivion_block_count_parity`**: 9,612 whole, 0 truncating, every archive at or above baseline.
- **`per_block_baseline_oblivion`**: the histogram equals `oblivion.tsv` (`total=8032`).
- **`no_block_sizes_drift_detector_has_zero_false_positives_on_real_corpus`**: PASS.
- These results match the ROADMAP compatibility row (100%, 9,612 / 9,612).

**Compatibility level (measured live):**
- **NIF, including the v10.x tail**: 100%. Three meshes were traced through `import_probe`, and each mesh count equals its shape count, as at the baseline:
  - `chandelier01.nif` (bsver 11): 5 meshes.
  - `octavo01.nif`: 2 meshes.
  - `goblinhead.nif` (bsver 5): 1 mesh.
- **BSA v103**: `oblivion_all_bsas_v103_brute_force_extract_zero_errors` and the v103 magic test pass (2/2).
- **ESM**: 5/5 pass — CLAS Knight parity, RACE parity, `parse_rate_oblivion_esm`, the #5013 Starts Dead census and the spawn-time GLOB test.
- **Render path**:
  - `parallax_alpha_gate`: 5/5.
  - Dark-role census and dark-combine pin: pass.
  - Oblivion leg of the torch-emitter guard: 272 emitters, each with params, rate and budget.
- **Exterior and lighting**: 34/34 bin-crate guards pass — `_far` LOD, HNAM dimmer, the pre-Skyrim falloff sentinel, the #5189 / #5264 light-spawn gate, the WATR wind scope and the new region-climate rung.
- **Gameplay and UI**:
  - The M47.3 quest-script corpus gate passes.
  - The MenuXml Oblivion corpus passes 3/3 (89 menus, 1,868 tiles).

**Findings: 0 CRITICAL, 0 HIGH, 1 MEDIUM, 0 LOW (NEW).** There are no regressions.
- **Already tracked, with today's Oblivion data added**: #3301 (region weather is unconsumed, so Tamriel is permanently Clear) and #5350 (its "latent" premise has changed).
- **Still open from the baseline**: #5350 and #5351.

**What changed for Oblivion since the baseline:** #5367 Phase G (`14cff35ae`). Its generic-greeting fallback matches Oblivion's `GREETING` DIAL (0xC8) with no game gate. Every Oblivion NPC activation therefore now opens a real conversation, so the shared dialogue line-selection rules run on Oblivion data for the first time. OBL-2026-10-08-D2-01 is the defect this exposes.

## Dimension Findings

### Dimension 1: NIF Version Handling & Corpus Integrity — clean
- **Commits reviewed**:
  - `3970c8d19` (#5232): the harness now handles an archive that is present but cannot be opened.
  - `75755796d` (#5260): pins the corpus total for the sized-game coverage ceilings. Oblivion uses the truncation TSV instead, whose header reads `parsed=9612`.
  - `44986c6ef` (#5259): routes the `is_nif_entry` filter. The Oblivion count is unchanged at 9,612.
  - `2be7c7c0b` (#5081/#5082): doc comments only.
- **Pins still present**:
  - #170 dual band: `header.rs:743`.
  - `uses_inline_block_type_names`: `lib.rs:172`.
  - `parse_ni_texturing_property_with_zero_shader_maps`: `properties_tests.rs:465`.
  - `MORPH_LEGACY_CUTOFF` = 10: `version.rs:414`, gated at `morph.rs:116` / `:226`.
  - The #3926 contradiction guard: `lib.rs:643-672`.

### Dimension 2: BSA v103 & ESM Data Slice

#### OBL-2026-10-08-D2-01: #5367 Phase L's Random-INFO rule inverts the authored semantics. A passing non-Random INFO beats earlier passing Random INFOs, Random sets pool across boundaries, and Random End is never read. Phase G makes this live on every Oblivion NPC activation.
- **Severity**: MEDIUM. The wrong line is chosen, and it can invert quest priority. There is no crash or data loss.
- **Dimension**: BSA v103 & ESM Data Slice. The data half is Oblivion's. The mechanism is owned by `/audit-scripting` (`crates/scripting/src/dialogue.rs`) and `/audit-gameplay` Dim 2.
- **Location**:
  - `crates/scripting/src/dialogue.rs:348-364`: the `select_info` pick.
  - `crates/scripting/src/dialogue.rs:645-662`: the test `non_random_candidate_keeps_priority_over_the_random_pool`, which pins the deviating behaviour.
  - `crates/plugin/src/esm/records/misc/dialogue.rs:388-411`: `InfoDataHeader` exposes only Goodbye, Random and Say Once. It has no Random End (`0x20`) accessor.
  - `docs/engine/dialogue-trees.md:98-100` and `:137`: the doc calls the rule "vanilla semantics".
  - `byroredux/src/systems/npc_dialogue.rs:275-280` (`generic_greeting_record`, with no game gate) and `:310-311`.
- **Status**: NEW.
  - `gh` searches for "Random End" and "random INFO dialogue" find only #5367 (the open epic, whose Phase L shipped this rule).
  - No 2026-10-08 report covers the selection rule. FNV-D2 mentions "Say-Once/Random pools" only in the context of DLC INFO loss.
- **Description**:
  - `select_info` collects every passing candidate in quest-priority / file order.
  - If any passing candidate is not Random-flagged, the first such one wins. Only when every passing candidate is Random does it roll uniformly over all of them.
  - Both authoring references describe a different, positional rule.
  - The CS wiki, `Category/Editing Dialogue.wiki` and `Dialogue Tutorial.wiki` (Oblivion):
    - "the game starts at the top of the list of info lines and proceeds down until it finds one that satisfies current conditions".
    - "All random infos that appear sequentially … are put in a list. One is chosen at random".
    - "An Info marked Random End will terminate the random set even if the next info is also marked Random."
  - The GECK wiki, `Category/Dialogue.wiki` (FO3/FNV): "If an actor qualifies for a Random info … the info is put on a stack … The stack keeps building until an info that is not marked Random is found, or an info marked Random End is found. At that point one of the infos in the stack is selected randomly."
  - The engine diverges in three ways:
    1. A later passing non-Random INFO beats an earlier passing Random set. Vanilla speaks from the Random set. With priority ordering, a lower-priority quest's plain line therefore outranks a higher-priority quest's Random greetings.
    2. Random INFOs pool across non-adjacent runs.
    3. `Random End` (Flags `0x20`, decoded in xEdit TES4 `wbDefinitionsTES4.pas` INFO `DATA`) is ignored, so adjacent sets merge.
- **Evidence**:
  - The in-tree test is the exact counter-example: `[0x401 Random, 0x402 plain]`, both passing, asserts `0x402`. Vanilla's stack is `{0x401}`, terminated by the non-Random `0x402`, so vanilla speaks `0x401`.
  - Raw `Oblivion.esm` census (`/tmp/audit/oblivion/greeting_census.py`): the `GREETING` DIAL 0xC8 has 3,743 INFOs.
    - 461 are Random, in 102 runs (the longest is 80), and 57 carry Random End.
    - 3,281 non-Random INFOs follow the first Random one.
    - Master-wide: 6,201 Random and 322 Random End INFOs.
  - Random End is load-bearing in vanilla. For example, the SECrime/Crime guard arrest sets "Halt, lawbreaker…" and "Halt, scofflaw…" are adjacent Random sets that are separated only by Random End.
  - Why it is live on Oblivion:
    - `generic_greeting_record` matches EDID `GREETING`.
    - `SceneAliasCandidate` is stamped regardless of game.
    - Oblivion NPCs own no topics (there are no aliases), so `open_conversation` falls through to the greeting at `npc_dialogue.rs:310` on every activation.
- **Impact**:
  - Every Oblivion activation greeting takes the first plain passing line, or a merged pool, instead of the authored positional Random set.
  - Generic and specific greeting variety collapses toward fixed lines.
  - Quest priority can be inverted.
  - The same mechanism runs on FO3/FNV, where the GECK states the same stack rule. FNV `Flags1` Random appears on about 5,500 INFOs (dialogue-trees.md census).
  - The doc presents the deviation as vanilla behaviour, so the next implementer has no warning.
- **Related**: #5367 (Phase L), #5271 (QSTI per INFO plus the priority sort it orders by), #5350 (Oblivion topic-list model), FNV-2026-10-08 D2 (DLC INFO merge).
- **Suggested Fix**:
  - Walk the candidates in priority / file order to the first passing INFO. If it is Random, gather the immediately following passing Random INFOs until a non-Random INFO or a Random End INFO, and roll within that stack.
  - Add `random_end()` (`flags1 & 0x20`).
  - Rewrite the pinned test to the vanilla expectation, and add an Oblivion `GREETING` real-data floor.
  - Correct the dialogue-trees.md wording.

**Other Dim 2 deltas, reviewed with no finding:**
- `bd7aa2d13` (#5093 actor split): the 16-byte ACBS arm is now at `actor/npc.rs:678`, still ahead of the FO4, Skyrim and FNV arms. Its pins are at `actor/tests.rs:93` / `:123`.
- `25b678106` (#5295): INFO `DATA` typing matches xEdit TES4. The layout is `Type u8`, `wbNextSpeaker`, then `Flags u8` (Goodbye / Random / Say Once / Run Immediately / Info Refusal / Random End / Run for Rumors), with `SetOptionalFrom(2)`.
- `dbc07e8f0` (#5271): the per-INFO QSTI gate applies to Oblivion INFOs too. The 3,743 GREETING INFOs carry 271 distinct QSTI values, and 2,383 of them belong to Start-Game-Enabled quests.
- `78c2b2d61` (#5309): the CELL walker dedup is guarded by `walker_equivalence.rs`.
- `d4e8c31be` (#5248): the Oblivion base Starts Dead OR is still in place at `cell_loader/references/mod.rs:782-786`.

**Update to #5350 (not re-filed):**
- #5350's "latent" premise has changed in part. Phase G now opens `GREETING` on every Oblivion activation, and the list it shows is that INFO's `TCLT` links, or the greeting topic alone.
- Owned-topic classification, which is #5350's own subject, is still unreachable, because Oblivion has no aliases.

### Dimension 3: Legacy-Property Rendering Path — clean
- **Guards**: all green (see the Executive Summary).
- **`9f0a8a7cc` (#5230)**: the mirror-pane exemption is gated on `bgsm_pbr_scalars_authored`. Oblivion authors no BGSM, so it has no Oblivion effect.
- **`b7987d813` (#5222)**: its `asset_provider/texture.rs` and LOD changes are reached only under `GameKind::Fallout3NV` / `ObjectLodScheme::FalloutLegacyBlocks`.
- **Not re-measured**: the APPLY_HILIGHT2 BC1 `_n.dds` census. There is still no in-tree tool for it, as at the baseline.

### Dimension 4: Exterior & Lighting Data (Tamriel)

**`b4497ec1d` (Oblivion climate rungs)**:
- Its defects are already filed today, so they are cross-referenced here and not re-filed:
  - EXT-D1-2026-10-08-01: children resolve to `AllWeather`.
  - EXT-D5-2026-10-08-01: children get no default water.
  - ESM D2-02: REGN has no `CNAM`.
- Distinct check made this run — a root-worldspace census:
  - 27 of the 54 root WRLDs author no `CNAM`.
  - Apart from Tamriel, which the naming rung resolves to `TamrielClimate`, all 26 are unshipped test worlds (`Toddland`, `Test*`, `SETest*`).
  - So the "richest climate" rung reaches no shipping root worldspace. No finding.

#### Existing #3301 — Oblivion data angle: region weather is unconsumed, so every Tamriel exterior is permanently Clear
- **Severity**: as tracked (#3301 is open).
- **Location**:
  - `crates/plugin/src/esm/records/misc/world.rs:1073` / `:1130`: `RDWT` is decoded.
  - `byroredux/src/env_translate.rs:487-499`: `resolve_default_weather` picks the climate's highest-chance `WLST` row.
  - The only runtime reference to `RegionDataKind::Weather` is a test in `byroredux/src/components.rs:822`.
- **Status**: Existing: #3301. That issue scopes "Weather selection would overlap with the existing worldspace-level climate/weather system and needs its own design pass".
- **Evidence** (raw `Oblivion.esm` census this run):
  - `TamrielClimate` `WLST` = [Clear 100].
  - 57 of 211 REGN records author `RDWT`, covering 16 weathers: Clear 39, Overcast 39, Cloudy 38, Fog 38, Thunderstorm 33, Rain 31, Snow 8, SE* weathers, and ThunderstormKvatch 3.
- **Impact**:
  - All of Cyrodiil's authored weather variety comes from region tables.
  - Through the only climate the engine can reach for Tamriel, Rain, Fog, Snow on the Jerall range and the Kvatch storm never appear.
- **Suggested Fix**: attach this census to #3301 so that Oblivion's dependence on regional weather informs its design pass.

**Other Dim 4 commits:** no Oblivion behaviour.
- `126b8ca06` (#5264): guards green.
- `d63131c57` (PKIN), `63bf3347f` (carts), `2d47bf7f1` (FO3/FNV recognizer).
- Save and despawn commits: `a02a9e70e`, `27562f8ae`, `ec0e0c8b4`.
- **Not run**: `m-exteriors.sh oblivion` (no engine launches under suite rules).

### Dimension 5: Gameplay & UI Data Slice (M47.3, MenuXml HUD) — clean
- **Gate status**: the M47.3 gate is green, and the MenuXml corpus passes 3/3.
- **`23fd95055` (#5314)**: the include budgets are 256 KiB of total fragment bytes, 64 KiB per fragment and 16,384 tiles per document. They were calibrated on the three legacy corpora (largest document: 205 tiles), and all 89 Oblivion menus parse.
- **#5367 on Oblivion**:
  - Phase F (force-greet) is inert. Oblivion's PKDT procedures stop at 11 (CastMagic), while `PROCEDURE_DIALOGUE` = 15 (`pack.rs:334` / `:419`).
  - Phase G is live (see D2-01).
  - INFO result scripts and `NAME` "Add Topics" have no Oblivion executor on the dialogue path. This is M47.3 phase-2 scope per ROADMAP and was not filed.
- **Existing #5033**: the doc at `hud.rs:43` still describes the Skyrim `0x3E8` bar keys.

## Regression Guard List (all hold, live 2026-10-08)
- **Stride-drift family #1506-#1509**: 0 truncating, 0 unknown, histogram parity and corpus-total parity.
- **#170 dual BSStreamHeader band**: pinned.
- **`NiTexturingProperty`**: the `u32` shader-map count is read unconditionally; pinned.
- **`MORPH_LEGACY_CUTOFF`** (10): the gate is present.
- **BSA v103 brute-force sweep**: 0 errors.
- **`parallax_alpha_gate_tests`**: 5/5.
- **Disney-BSDF gate**: no Oblivion-path change. The only material commit is BGSM-gated.
- **16-byte ACBS (#1650)** and **LVLO/LVLD (#4638)**: pins present.
- **Dark-role census**: green.
- **#5013 Starts Dead census**: green.
- **#5084 EFID**: unchanged.
- **#5189 `__MAX_Default_Light`**: never spawns, and #5264 now shares the predicate.

## Open Work
- **Dialogue**:
  - The Random / Random End selection rule (D2-01).
  - Oblivion's known-topic list model (#5350).
  - INFO result scripts and Add Topics (M47.3 phase 2).
- **Exterior**:
  - Region weather (#3301).
  - Child-worldspace water and climate inheritance (EXT-D5-01, EXT-D1-01).
  - Distant water (#5335).
- **From ROADMAP**:
  - The CHARAL Oblivion ruleset is unwired; there is no AVIF, so the legacy actor-value resolver comes first. This also leaves the HUD bars full.
  - Navigation is PGRD-only, with no path-graph consumer.
- **Device-bound gates not run under suite constraints**: `m-exteriors.sh oblivion`, `p0-door-interaction.sh oblivion` and `m48-4-oblivion-hud.sh`.
- Interiors and the Tamriel exterior already render, so neither is a blocker.

## Summary

| Severity | NEW | Already tracked (Oblivion data added) |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 1 (D2-01) | — |
| LOW | 0 | — |
| Tracked notes | — | #3301 (Dim 4), #5350 premise update (Dim 2), #5033 (Dim 5) |

**Real-data gates run live**:
- The corpus lane (4 tests).
- BSA (2 tests) and ESM (5 tests).
- 34 bin-crate guards.
- The dark-role census, dark-combine and torch emitters.
- The M47.3 quest corpus and MenuXml (3 tests).

**Raw censuses** of `Oblivion.esm`: GREETING INFO flags, quests and conditions; CLMT `WLST`; REGN `RDWT`; and WRLD roots without `CNAM`.

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-10-08.md`. Label D2-01 `medium bug dialogue scripting legacy-compat game:oblivion` (and `game:fnv` / `game:fo3`, since the GECK states the same rule).
