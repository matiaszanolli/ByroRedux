# Oblivion (TES4) Compatibility Audit — 2026-10-05

**HEAD**: `a2c24b16e` · **Baseline**: `docs/audits/AUDIT_OBLIVION_2026-09-29.md` (HEAD `9fcfdc3fc`) · **Audited**: Dimensions 1-5. Each had commits on its Paths since 2026-09-29 and was delta-reviewed. · **Unchanged since baseline (skimmed)**: none at dimension level. Some dimensions had commits, but none that change Oblivion behaviour:
- Dim 1: clippy, test-harness and doc commits only.
- Dim 5: clippy-only edits to test helpers in `obscript_vm.rs` / `obscript_quests.rs`, plus UI-owned commits.
- Dim 3: `byroredux/src/npc_spawn/seam_blend.rs` and `byroredux/src/asset_provider/texture.rs` had no commits and were skimmed.

This run is part of today's `/audit-suite --preset comprehensive`.
- **How it was run**: solo, with no sub-agents, one dimension at a time, against the real vanilla Oblivion + DLC data at `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`.
- **Not run**: the engine binary, Vulkan processes and smoke scripts.
- **Scratch notes and census scripts**: `/tmp/audit/oblivion/`.

## Executive Summary

**Corpus lane: GREEN.**
- **Command**: `BYROREDUX_REQUIRE_GAME_DATA=1 cargo test -p byroredux-nif --release --test parse_real_nifs --test per_block_baselines --test block_coverage_baselines --test oblivion_stream_drift_corpus -- --ignored oblivion no_block_sizes --nocapture`.
- **`parse_rate_oblivion`**: **9,612 / 9,612 clean (100.00%)**, with 0 truncated and 0 failed across 9 archives (Meshes 8,032; Shivering Isles 1,438; Knights 75; 6 small DLC).
- **`oblivion_block_count_parity`**: 9,612 whole, 0 truncating, every archive at or above baseline.
- **`per_block_baseline_oblivion`**: the histogram equals `oblivion.tsv`. The new #4628 corpus-size check (`8c16fec42`) now also compares the TSV header `total=8032` against the walked corpus, and they match.
- **`no_block_sizes_drift_detector_has_zero_false_positives_on_real_corpus`**: PASS.
- These results match the ROADMAP compatibility-matrix row (100%, 9,612 / 9,612).

**Compatibility level, measured live:**
- **NIF (including the v10.x tail)**: 100%, as above. Three meshes were traced through `import_nif_scene` with `import_probe`. In each, the imported mesh count equals the `NiTriShape` count:
  - `lights\chandelier01.nif`: 5 → 5.
  - `clutter\books\octavo01.nif`: 2 → 2.
  - `creatures\goblin\goblinhead.nif` (bsver 5): 1 → 1.
- **BSA v103**: 17 archives, 147,629 files, 6,770.4 MB, 0 errors.
- **ESM**:
  - Parity pins: `clas_oblivion_knight_against_vanilla` and `race_oblivion_data_and_subs_against_vanilla` pass.
  - `parse_rate_oblivion_esm` passes (56,282 records; CLOT 604; WTHR 37; LTEX 229).
  - The new #5013 census pin `oblivion_starts_dead_bases_and_their_placements_match_the_audit_census` passes.
- **Render path**: all guards green.
  - `parallax_alpha_gate`: 5/5.
  - Dark-role census: Oblivion 8 meshes in 6 files; every other game 0.
  - `dark_combine_is_the_pinned_bare_multiply_in_both_paths`: pass.
  - Oblivion leg of the torch-emitter guard: 272 emitters, each with params, rate and budget.
- **Exterior and lighting**: all Oblivion guards green — LTEX paths (229, 3 known unshipped), default land textures, the HNAM dimmer, the pre-Skyrim falloff sentinel, `placement_lod_supported_is_oblivion_only`, and the #5189 `__MAX_Default_Light` gate.
- **Gameplay and UI**:
  - The M47.3 quest-script corpus gate passes, and it now fails rather than silently passing when data is missing under `REQUIRE` (#5088).
  - The MenuXml Oblivion corpus passes 3/3.

**Every baseline finding is closed and verified in code:**

| Baseline finding | Fixed by | What the code does now |
|---|---|---|
| OBL-2026-09-29-D2-01 HIGH (placed corpses spawn alive) | #5013, `dca2bb401` | The base `0x80000` flag is ORed into the actor-job corpse stamp (`byroredux/src/cell_loader/references/mod.rs:761-766`). The census pin is green. |
| D2-02 (`EFID` codes looked up by FormID) | #5084, `ce73f658b` | `EsmIndex::resolve_magic_effect` (`index.rs:813`) is the only route used by `magic.rs:92` and by `consumables.rs:115/141/178`. |
| D2-03 (CLOT doc rot) | #5086 | Doc corrected. |
| D5-01 (silently passing quest gate) | #5088, `1377d3463` | The test resolves its path through `test_paths::oblivion_esm()`, panics under `REQUIRE`, and is wired into `real-data-gates.yml:217-236` with a check that it actually ran. |

**Findings: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 2 LOW.**
- Both are new. There are no regressions.
- Both are latent or doc-level: one is a dialogue-reachability claim that is wrong for Oblivion, the other is WTHR HNAM doc rot.
- The main Oblivion-visible gap found today is owned elsewhere: EXT-D5-2026-10-05-02 (MEDIUM, Oblivion never draws distant water). It is cross-referenced here, not re-filed.

## Dimension Findings

### Dimension 1: NIF Version Handling & Corpus Integrity — clean
- **Corpus lane**: green (see the Executive Summary).
- **Commits reviewed on these Paths**:
  - `8c16fec42` (#4628): the baseline gate now detects corpus-size drift. Oblivion's `total=8032` header agrees with the walked corpus.
  - `530c9e7aa` / `4ad847a81` / `3f0852e08`: clippy. The `chunks_exact` → `as_chunks` change in `NiLegacyParticlesData` and `BsSkinBoneData` gives identical behaviour.
  - `72769b95a` (#5119): `version_literal_tests`.
  - `f270ef5e3` (#5050): the dhat harness.
  - `097f51b48` (#5227): helper docs.
- **Related import commits**:
  - `6e3ca2716` (#4633): `compose_transforms` overflow neutralisation. It has no effect on finite Oblivion data, and parity stays green.
  - `e44152d45` (#5189).
- **Pins still present**:
  - #170 dual band (`header.rs:743`).
  - `uses_inline_block_type_names` (`lib.rs:172`).
  - `parse_ni_texturing_property_with_zero_shader_maps` (`properties_tests.rs:465`).
  - `MORPH_LEGACY_CUTOFF` = 10 (`version.rs:414`), gated at `morph.rs:116` / `:226`.
  - The #3926 contradiction guard (`lib.rs` ~651).
- **Cross-referenced, not re-filed**:
  - NIF-D2-2026-10-05-01: the #5119 version-literal guard skips about 1,000 lines, including `blocks/controller/mod.rs` and `NiTextureEffect`, which are Oblivion no-`block_sizes` paths.
  - NIF-D4-2026-10-05-01: the `affected_node_names` doc claims Oblivion is pre-10.1.

### Dimension 2: BSA v103 & ESM Data Slice

#### OBL-2026-10-05-D2-01: The P4 topic-reachability model says every Oblivion topic is an opening list entry, but Oblivion's menu is the player's *known-topics* list: 1,761 of 3,184 Topic DIALs are reached only as INFO `TCLT` "Choices"
- **Severity**: LOW. It is latent: Oblivion has no quest aliases, so `running_quests_binding_entity` (`crates/scripting/src/scene/quest_alias.rs:923`) returns nothing and no Oblivion NPC owns a topic today.
- **Dimension**: BSA v103 & ESM Data Slice. The routing owner is `/audit-gameplay` (the P4 dialogue mechanism). The Oblivion data half is owned here.
- **Location**:
  - `byroredux/src/systems/npc_dialogue.rs:27` (module doc: "Oblivion authors no branches, so its topics are all list entries").
  - `byroredux/src/systems/npc_dialogue.rs:135-138` (`TopicEntry::TopLevel` doc).
  - `byroredux/src/systems/npc_dialogue.rs:144-156` (`topic_entry`: a branch-less topic with `top_level() == None` → `TopLevel`).
- **Status**: NEW.
  - It is the Oblivion twin of #5224 (closed, FO3/FNV: "choice-only but would list as top-level"). #5224 fixed the Fallout branch only, through the FO3/FNV DIAL `DATA` flags byte, which Oblivion does not author.
  - `gh` searches for "AddTopic", "known topics", "Oblivion topic list" and "choice-only" find only #5224 and #3600 (PNAM ordering), both closed.
  - Today's reports do not cover the Oblivion half. GAME-D2-2026-10-05-01 is about FNV shared Top-level topics. The SCR note says the route is "unreachable by design" for Oblivion/FO3/FNV.
- **Description**:
  - #5037 and #5224 model the opening menu as "Top-Level entries; everything else via `TCLT` links". For Oblivion, branch-less and with no flags byte, every owned Topic is classified as `TopLevel`.
  - The Oblivion runtime does not work that way. Per the CS wiki:
    - `AddTopic.wiki`: "Only topics in this list [the player's known topics] can appear in an NPC's topic list". Topics enter that list through `AddTopic` or the INFO "Add Topics" box (`NAME`).
    - `Dialogue Tutorial.wiki`, "Decisions, Decisions": "The choices box will give the player a list of those topics only, after the line that lists them has been said".
  - So a Topic DIAL that appears only in some INFO's `TCLT` and is never added by `NAME` or a script is a choice-only follow-up, never an opening entry.
- **Evidence**: raw walk of `Oblivion.esm` this run (`/tmp/audit/oblivion/dial_census.py`):
  - DIAL `DATA` is 1 byte on all 3,817 DIALs. The categories are {Topic 3,184, Conversation 555, Persuasion 39, Combat 16, Service 14, Misc 5, Detection 4}.
  - Of the 3,184 Topic DIALs:
    - 1,777 are `TCLT` (Choices) targets.
    - 582 are `NAME` (Add Topics) targets.
    - **1,761 are `TCLT` targets and never `NAME` targets.**
    - 841 are neither; they are added by script `AddTopic`, or are GREETING-style.
- **Impact**:
  - Today: none, because no Oblivion NPC owns a topic.
  - Once Oblivion dialogue ownership is wired (an ObScript or condition-based owner in place of aliases), the opening menu would offer choice-only follow-ups. Examples are quest "Yes"/"No" answer topics and mid-conversation responses, which would be offered before the line that asks the question. That is the #5037 / #5224 failure mode, on Oblivion.
  - The doc states the wrong rule as fact, so the implementer of that wiring has no warning.
- **Related**: #5224 (closed, FO3/FNV twin), #5037 (closed), #3600 (closed), GAME-D2-2026-10-05-01, the SCR-2026-10-05 note on P4 reachability, and ESM-2026-10-05-D2-01 (INFO `DATA` decode; Oblivion row 3 B × 19,276).
- **Suggested Fix**:
  - Correct the two doc sites now. Oblivion's opening list is the player's known-topic set (`NAME` Add Topics + `AddTopic`), and `TCLT` targets are link-only.
  - When Oblivion ownership lands, give `topic_entry` an Oblivion arm that returns `LinkOnly` for topics never named by an INFO `NAME` (or by a known-topic set seeded from `NAME` / `AddTopic`).
  - Pin it with the 1,761 / 3,184 census.

Other Dim 2 deltas were reviewed with no finding:
- `9b099b31f` (#5075/#5076): `EFID` is kept out of the remap. The Oblivion `EFID` test is green.
- `cc9353043` (#4469): the INFO `DATA` arm. The mis-documented field split is filed as ESM-2026-10-05-D2-01 and covers Oblivion's 3-byte row.
- `7ab87c0fb` (#5045): `DialogueCategory::from_data` reads byte 0 on Oblivion, with 0-6 = Topic…Misc. This matches the census.
- `260afc33f`: `XRGD` decode.
- The rest are FO3/FNV, FO4, FO76 and Starfield.

Note, not a finding:
- Oblivion's object-script kill idioms are conditional: `SE43SkeletalHoundScript` runs "hound starts game dead" only if `GetStage SE43 < 200`, and `SE32GhostStagingSCRIPT` is on 14 bases. These are outside #5013 and outside the FO3/FNV name lists in `script_killed_corpse_forms`.
- They belong to M47.3 phase 2 (object-script blocks). SCR-D6-2026-10-05-01 covers the name-keyed recognizer.

### Dimension 3: Legacy-Property Rendering Path — clean
- **Guards**: all green (see the Executive Summary).
- **`257e973d2` (#4938)**: NIF-authored lamp `NiLight`s now consume the canonical LIGH falloff lane. An Oblivion LIGH base therefore renders the pre-Skyrim 2.0 exponent whether or not its lamp NIF carries a spawnable light. Before this fix that case got 1.0.
- **`3c197ed8c` (#5057)**: early-fragment admission now extends to lighting-shader kinds 1-16. It still requires `!alpha_blend && alpha_threshold == 0.0 && !is_decal` (`crates/renderer/src/vulkan/context/types.rs:383-393`), so every Oblivion alpha-tested and alpha-blended draw stays on the late-test module.
- **No effect on Oblivion**: `5282f1906` (#5199, path-2 tangent guard) and `e80f7e854` (#5012, BGSM).
- **Not re-measured**: the APPLY_HILIGHT2 BC1 `_n.dds` census (1,274 / 100). There is no in-tree census tool for it, as at the baseline.

### Dimension 4: Exterior & Lighting Data (Tamriel)

#### OBL-2026-10-05-D4-01: `OblivionHdrLighting` docs say no renderer reads HNAM (the sunlight dimmer has been consumed since `df59c6362`), and say the 56-byte HNAM is "Oblivion / FO3 / FNV" (FO3/FNV author none)
- **Severity**: LOW (doc-rot).
- **Dimension**: Exterior & Lighting Data. The parse-side doc is in `/audit-esm`'s path, but the data fact is Oblivion-only.
- **Location**:
  - `crates/plugin/src/esm/records/weather.rs:113-122`: "**Parse-but-don't-consume gate (TD5-010):** no renderer system reads `OblivionHdrLighting` fields yet".
  - `crates/plugin/src/esm/records/weather.rs:160`: "Wire size of the full 14-field HNAM payload (Oblivion / FO3 / FNV)". This contradicts the same file's `:111` "FNV and Fallout 3 do not ship HNAM at all".
- **Status**: NEW.
  - No match in today's reports.
  - `gh` search "OblivionHdrLighting" finds only the closed #537, #1045, #1057 and #1062.
- **Evidence**:
  - `df59c6362` (2026-09-18, "consume the Oblivion HNAM sunlight dimmer on exterior sun"). `env_translate.rs:1628-1630` translates `hdr.sunlight_dimmer` onto `WeatherDataRes`, and `systems/weather.rs:852-854` / `:929-931` multiply the sampled sun by it.
  - Raw census this run (`/tmp/audit/oblivion/wthr_hnam.py`): WTHR with HNAM is Oblivion 37 / 37, FalloutNV 0 / 63, Fallout3 0 / 27.
- **Impact**:
  - A reader trusting the gate doc would conclude that HNAM is inert and might move or retype it without checking the live consumer.
  - The `WIRE_SIZE` doc invites an FO3/FNV HNAM arm that no data supports.
  - The 13 other fields (eye-adapt, bloom, grass/tree dimmer) are still unconsumed, so the gate statement is only partly true.
- **Related**: #537 (closed), `df59c6362`, `sunlight_dimmer_translates_from_the_hnam_block`.
- **Suggested Fix**:
  - Restate the gate as: `sunlight_dimmer` consumed via `translate_weather` (EXAL); the other 13 fields are still parse-only.
  - Change `:160` to "Oblivion only (FO3/FNV author no HNAM: 0 / 27, 0 / 63)".

Other Dim 4 notes:
- **Closed since baseline and verified**:
  - #4910 (`0d113b35a`): Oblivion's WATR wind is now read un-rotated, and `wind_angle_conversion_is_scoped_per_game` pins it.
  - #4909 (`387bb9d9c`): the froxel and interior sun now take the dimmer.
  - #5189: `__MAX_Default_Light`.
  - #5184: the sentinel sweep now resolves under every `GameKind`, including Oblivion.
- **Cross-referenced, not re-filed**:
  - **EXT-D5-2026-10-05-02 (MEDIUM)**: Oblivion never gets distant water. `spawn_lod_water` needs WRLD `NAM3`/`NAM4`; Tamriel authors only `NAM2`.
  - **EXT-D5-2026-10-05-05**: watal.md §2's Oblivion WATR frame row re-settles a frame the same doc marks OPEN.
  - **EXT-D5-03**: the inline tri-state rule.
- **Not run**: `m-exteriors.sh oblivion static`, because suite rules forbid launching the engine.

### Dimension 5: Gameplay & UI Data Slice (M47.3, MenuXml HUD) — clean
- **Gate status**: the M47.3 gate is green and now strict, and CI runs it (baseline D5-01 closed by #5088). The MenuXml corpus passes 3/3.
- **UI-owned commits, reviewed**:
  - `161ab5ae8` (#4724): HUD command unit tests. The `m48-*` smokes now exit 77 (SKIP) when data is absent.
  - `a7d3222a8` (#4723): `--menu` owns the overlay against both HUD routes.
  - `6a83c4629` (#5024).
  - `2465740cc` (#5007).
  - `cfaccff57` (#4892/#4893).
- **ObScript VM and quest diffs**: clippy only, confined to test helpers.
- **Existing, not re-filed**: **#5033** (UI-D7-2026-09-29-02). `hud.rs:43` still describes the Skyrim `0x3E8` bar source.
- **Known-open**: the Oblivion HUD bars draw full, because there is no AVIF and the legacy actor-value resolver is unbuilt.

## Regression Guard List (all hold, live 2026-10-05)
- **Stride-drift family #1506-#1509**: corpus lane green; 0 truncating, 0 unknown, histogram parity, and corpus-total parity (new with #4628).
- **#170 dual BSStreamHeader band**: pin present.
- **`NiTexturingProperty`**: unconditional `u32` shader-map count; pin present.
- **`NiGeomMorpherController` `MORPH_LEGACY_CUTOFF` (10)**: gate present.
- **BSA v103 sweep**: 147,629 files, 0 errors.
- **`parallax_alpha_gate_tests`**: 5/5.
- **Disney-BSDF gate** (`MAT_FLAG_PBR_BSDF` == 0 for Oblivion): no change on Oblivion paths.
- **16-byte ACBS (#1650) and Oblivion LVLO/LVLD (#4638)**: pins present.
- **Dark-role census**: 8 meshes in 6 files, Oblivion only.
- **#5013 Starts Dead**: 87 NPC_ + 48 CREA flagged bases and 787 placements (census pin).
- **#5084 EFID**: `resolve_magic_effect` is the only effect route.
- **#5189 `__MAX_Default_Light`**: never spawns.

## Open Work
- **Distant water**: Oblivion draws none (EXT-D5-2026-10-05-02, MEDIUM, owned by `/audit-exterior`). This is the most visible Oblivion exterior gap today.
- **From ROADMAP Status**:
  - Oblivion's CHARAL ruleset is built but unwired. `Oblivion.esm` has no AVIF, so a legacy actor-value resolver comes first. That blocks the HUD bars, regen and the effect runtime's actor-value application.
  - M47.3 phase 2: object-script blocks (including the conditional OnLoad kill idioms noted in Dim 2), Message UI and actor-state functions.
  - Oblivion dialogue ownership has no alias mechanism. When it lands, OBL-2026-10-05-D2-01 becomes live.
  - PGRD-only navigation has no path-graph consumer.
- **Device-bound gates not run under this audit's constraints**:
  - `m-exteriors.sh oblivion static`.
  - `p0-door-interaction.sh oblivion`.
  - `m48-4-oblivion-hud.sh`.
- Interiors and the Tamriel exterior already render, so neither is a blocker.

## Statistics
- **Dimensions audited**: 5/5. 88 unique commits on Oblivion Paths; 313 in the repo since the baseline.
- **Real-data gates run live**:
  - 4 corpus-lane tests.
  - 2 BSA tests.
  - 5 ESM tests: CLAS, RACE, parse rate, the Starts Dead census and the spawn-time GLOB test.
  - Bin-crate guards, 31 tests: parallax 5, light-spawn gate, HNAM dimmer, falloff sentinel, `_far` LOD.
  - 2 `--ignored` terrain tests.
  - The dark-role census, dark-combine and torch emitters.
  - The M47.3 quest corpus and MenuXml 3.
- **Raw censuses** (Python walks of `Oblivion.esm`, FO3 and FNV): DIAL/INFO topic reachability, INFO `DATA` sizes, WTHR HNAM presence, and OnLoad kill scripts on actor bases.
- **Findings**: 0 CRITICAL / 0 HIGH / 0 MEDIUM / 2 LOW. Both are NEW; there are no regressions.

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-10-05.md`. Label every finding `game:oblivion` + `legacy-compat`, plus its domain:
- D2-01: `low documentation dialogue gameplay`.
- D4-01: `low documentation doc-rot esm-plugin`.
