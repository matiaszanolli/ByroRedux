**HEAD**: `fa6be4a6b` · **Baseline**: `AUDIT_OBLIVION_2026-10-21.md` (HEAD `4a3e89d56`, 395 commits ago) · **Audited**: Dims 2, 3, 4 and 5 delta-scoped, plus the Phase 1 corpus lane · **Unchanged since baseline (skimmed)**: Dim 1 (the NIF header, version and stream code has no commits; dispatch split #6144, the satellite-walker gate #6301 and the Havok/ragdoll work are covered by the green corpus lane), Dim 3's legacy-material import (`crates/nif/src/import/material`: renames and doc commits only)

# Oblivion (TES4) Compatibility Audit, 2026-10-27

Part of `/audit-suite --preset comprehensive`. Run solo and read-only: no engine launch, no sub-agents, repo `target/` only.

## Executive Summary

**Corpus lane: green.** All four Phase 1 gates pass on the 9,612-NIF corpus (the base archive plus the 8 DLC archives): `oblivion_block_count_parity`, `no_block_sizes_drift_detector_has_zero_false_positives_on_real_corpus`, `parse_rate_oblivion` and `per_block_baseline_oblivion`. `oblivion_truncations.tsv` still reads `truncating=0 parsed=9612`, and no baseline data file changed in the window. The #6144 dispatcher split did not lose any Oblivion or v10.x block arm.

**Compatibility level.** Live rows are in `ROADMAP.md`, Oblivion row 207 and M47.3.
- NIF parsing, including the v10.x tail, is complete.
- BSA v103 extracts all 17 archives with 0 errors.
- Every named ESM real-data pin is green.
- **The Tamriel exterior does not load at HEAD.** EXT-D6-2026-10-27-01 (CRITICAL, cross-game, already filed) panics at the first terrain-LOD block that borders resident ground. Every Oblivion LOD block goes through the synth path that panics (commit `398a6b525` says so itself). ROADMAP's "Exterior Tamriel (0,0) r1 passes" holds only once that is fixed.
- Interiors are unaffected. None of this was re-verified on a device, because the suite forbids launching the engine.

| Severity | NEW | Dup rows (cross-game, Oblivion leg measured) |
|---|---|---|
| CRITICAL | 0 | EXT-D6-2026-10-27-01 (terrain LOD skirt OOB panic) |
| HIGH | 0 | FNV-2026-10-27-02 (VFX colliders), FNV-2026-10-27-01 (catalog gaps), LC-2026-10-27-D2-01 (first sequence, not Idle), SCR-2026-10-27-01 (crime gold ignores the caller) |
| MEDIUM | 2 | — |
| LOW | 0 | — |

**The two new findings are Oblivion-only:**
- **OBL-D5-01.** Fallout-only ObScript opcodes are tagged `Games::Any`. Oblivion's command for the same opcode never runs:
  - **0x10ab `IsPlayerInJail`** (7 scripts): never reaches the condition evaluator, which already answers it from the new jail state. In object scripts it reads 0 silently. The prison cell doors and the Thieves Guild start (`TG00FindThievesGuildScript`) cannot see a jailed player.
  - **0x10c0 `CloseCurrentOblivionGate`** (16 sigil-stone and trigger-zone scripts): reported as done, but no gate closes.
- **OBL-D3-01.** The #6090 body-skin filter repaints Oblivion's underwear meshes with the race skin. They live under `textures\characters\` too: `underwear.dds` and `groinmale.dds` sit in the female upper and lower body and the male lower body.

**Method.**
1. `git log` per dimension since `4a3e89d56`.
2. Read the Oblivion-visible delta.
3. Re-ran the skill's pins as named tests.
4. Wrote throwaway Python readers over `Oblivion.esm` and `Oblivion - Meshes.bsa` (`/tmp/audit/oblivion/py/`).
5. Ran the `dump_alpha`, `material_dump` and `obscript_tools names` examples.
6. Checked names against the UESP 2022 dump: `Oblivion Mod/Function Indices.wiki` and `Oblivion Mod/Script Functions.wiki`. The second is an opcode table "extracted from Oblivion.exe".

Dedup used `/tmp/audit/open_issues_snapshot.txt`, closed-issue searches, every 2026-10-27 report, and SUITE_RULES § 6, re-read before writing.

## Pins and guards run (all green)

| Area | Command / test | Result |
|---|---|---|
| Corpus lane | `cargo test -p byroredux-nif --release --test parse_real_nifs --test per_block_baselines --test block_coverage_baselines --test oblivion_stream_drift_corpus -- --ignored oblivion no_block_sizes` | 4 / 4 |
| BSA v103 | `bsa_real -- --ignored oblivion` | 2 / 2 |
| ESM | `parse_real_esm -- --ignored clas_oblivion_knight race_oblivion parse_rate_oblivion` (incl. PKDT/PTDT), `oblivion_starts_dead` | 5 / 5 |
| Plugin lib | `--lib oblivion` (75), `--lib --ignored oblivion` (walker, XCLL, Tamriel WRLD, children inherit LAND, race head parts, ruleset falsifiability) | 81 / 81 |
| NIF real data | `real_archive_torch_meshes_surface_particle_emitters`, `dark_texture_role_population_matches_the_documented_census` | 2 / 2 |
| Scripting | `vanilla_oblivion_quest_scripts_execute_and_setstage`, `oblivion_quest_level_dialogue_gates_census` (1 quest / 9 INFOs, fn 313, unchanged) | 2 / 2 |
| MenuXml | `vanilla_corpus -- --ignored` (incl. `menu_texture_header_census_decodes`) | 4 / 4 |
| Bin crate | `parallax_alpha_gate oblivion placement_lod nif_light_spawn_gate race_body body_paths_kf_era creature_clip_candidates idle_paths tail beast synthesize falloff_exponent sunlight_dimmer` | 116 pass |
| Bin `--ignored` | `installed_oblivion_body_paths…`, `installed_oblivion_tails_and_beast_rig…`, `installed_oblivion_creature_assets…`, `installed_default_idle_clips…`, `an_installed_oblivion_voice_line…`, `oblivion_ltex_paths…`, `default_land_textures…` | all ok (23 / 23 incl. other games' `installed_*`) |

## Dimension Findings

### Dim 1: NIF version handling and corpus integrity

No findings. Header, version and stream code is untouched. The corpus histogram gate (base archive) and the DLC parity gate (9,612 files) both cover the #6144 split.

NIF-D4-2026-10-27-01 (`SkeletonBones::from_scene` composition) is already filed by `/audit-nif`.

### Dim 2: BSA v103 and the ESM data slice

No new findings.
- Every catalog `ROWS` index and all seven `OBLIVION_DIALECT` indices (116, 127, 171, 193, 249, 251, 329) match UESP *Oblivion Mod/Function Indices*, with 0 mismatches.
- The new Find package correctly refuses Oblivion's object-type searches. Oblivion numbers that enum differently, so the search is gated to `Fallout3NV` in `byroredux/src/npc_spawn/ai_package.rs` `find_behavior`.

**Dup row: FNV-2026-10-27-01, Oblivion leg** (`/tmp/audit/oblivion/py/ctda_vm2.py`). The census uses 20-byte TES4 record headers; the modeled set is catalog `ROWS` plus `OBLIVION_DIALECT`, minus foreign indices.
- 3,079 of 19,278 Oblivion INFOs (16%) are declined by their own CTDA.
- **743** of those are declined only by functions the ObScript VM already answers on Oblivion but the catalog has no row for:

| Fn | Name | INFOs | VM opcode |
|---|---|---|---|
| 50 | `GetTalkedToPC` | 505 | 0x1032, Both |
| 161 | `GetIsCurrentPackage` | 92 | |
| 48 | `GetGold` | 61 | (also SCR-01) |
| 130 | `GetPCIsRace` | 35 | |
| 18 | `GetCurrentTime` | 22 | |
| 300 | `IsInInterior` | 14 | |
| 32 | `GetInSameCell` | 12 | |
| 190 | `GetAmountSoldStolen` | 7 | |
| 327 | `IsRidingHorse` | 7 | |

- **548 of the 743 are `GREETING` INFOs** (plus 59 `HELLO`). Every Oblivion conversation opens through `GREETING`, so the first-meeting greetings (`GetTalkedToPC == 0`) never select.
- The quest-level gate is unchanged: only MQEndgame (fn 313) declines, 9 INFOs.

### Dim 3: Legacy-property rendering, NPC body assembly, spawn colliders

#### MEDIUM

### OBL-D3-2026-10-27-01: The race body skin is painted onto Oblivion's underwear meshes
- **Severity**: MEDIUM (visual; Oblivion only)
- **Dimension**: 3 (NPC body assembly). Owner of the mechanism: `/audit-nifal` and the NPC-spawn hooks.
- **Location**:
  - `byroredux/src/npc_spawn/loot_appearance.rs:42-75`: `set_body_base_color`, whose doc at `:46-53` claims "Oblivion's single-mesh bodies".
  - `byroredux/src/npc_spawn/loot_appearance.rs:84-97`: `skin_texture_symbols_from`.
  - Callers: `byroredux/src/npc_spawn/resumable/runtime.rs:719-737` (spawn) and `loot_appearance.rs:909-918` (restore after looting, #6128).
- **Status**: NEW. Pre-existing since #5487 (`1b45763fa`, which repainted every mesh). #6090 (`9d1a262ed`) added the `textures\characters\` path filter, but that only separates FO3/FNV underwear, which lives under `textures\armor\underwear\`.
- **Description**: The filter treats every mesh whose authored base colour is under `textures\characters\` as skin. Oblivion's body NIFs are not single-mesh, and their underwear is authored under that same folder.
- **Evidence**: `material_dump` over `Oblivion - Meshes.bsa`:
  - `femaleupperbody.nif`: `UpperBody:0` `…\imperial\female\upperbodyfemale.dds`, then `UpperBody:1` **`textures\characters\imperial\female\underwear.dds`**, then `Arms`.
  - `femalelowerbody.nif`: `LowerBody:0` `legfemale.dds`, then `LowerBody:1` **`underwear.dds`**.
  - `lowerbody.nif`: `LowerBody:0` `legmale.dds`, then `LowerBody:1` **`textures\characters\imperial\male\groinmale.dds`**.
  - `Oblivion - Textures - Compressed.bsa` ships one `underwear.dds` (Imperial female). `groinmale.dds` exists only for Imperial and Dremora.
  - The RACE body-section ICON runs (INDX 0..4) have no underwear entry. Vanilla therefore keeps the Imperial underwear for every race.
  - Every one of those meshes passes `skins.contains(&sym)`, so it gets the race's `UpperBodyFemale` or `LegMale` skin.
- **Impact**: Every unclothed Oblivion body part draws its underwear geometry in torso or leg skin, mapped through the underwear UVs. That covers spawn with no clothing in that slot and any part restored after the player loots a corpse's clothing. Imperials are affected too, because the override always applies.
- **Related**: #6090, #5487, #6128, NIFAL-D8-2026-10-27-01 (a different fragility in the same filter).
- **Suggested Fix**: Restrict the override to the meshes whose authored texture stem is the part's own skin stem (`upperbody*`, `leg*`, `hand*`, `foot*`, `tail*`). Alternatively, exclude `underwear` and `groin*` on Oblivion. Pin it with an `--ignored` test over the five vanilla body NIFs.

**Dup row: FNV-2026-10-27-02, Oblivion leg.** VFX sheets get synthesized trimesh colliders through `render_geometry_can_collide` (`byroredux/src/cell_loader/spawn.rs:83-90`) and `missing_collision_fallback`.
- Oblivion has no effect-shader or NoLighting dialect. Its VFX are plain `NiAlphaProperty` blend-only meshes (material kind 0), so all of them pass the predicate.
- Census method: Python over `Oblivion.esm` STAT and REFR records, joined to the block types in `Oblivion - Meshes.bsa`; then `dump_alpha` and `material_dump` on every candidate.
- **57 STAT bases** have no `bhk*`, blend-only alpha (no alpha test) and at least one imported mesh: **11,051 placements**.
- **18 of those bases are VFX or cobwebs: 8,869 placements.**
  - Cobwebs (`cobweb01`–`07`) and `HangCrud01`: 6,906 placements. These span dungeon corridors, so the result is invisible walls in nearly every Oblivion dungeon.
  - `FXLightBeamLong01` ×769, `FXCloudSmall01` ×531, `FXLightBeam01` ×343, `FXMistGroundEffect01` ×119, `FXWhiteTorchLarge` ×117, `LightBeam01` / `FXOblivionLightBeam01` ×22 each, `FXGasPlane01` ×14, `FXOblivionLightBeamLong01` ×13, `FXLightBeamPink01` ×13.
- The remaining blend-only bases are `DUNHay` ×563, `BloodDecal01`/`02` ×583 (imported with decal = 0), `ARWelkydClusterFX01` ×307, rugs and vines.
- **For the fix:** the `MATERIAL_KIND_NO_LIGHTING` key that FNV-02 suggests does not cover Oblivion. The canonical signal has to be blended && !alpha_test, or !z_write.

### Dim 4: Exterior and lighting data (Tamriel)

No Oblivion-only findings. All of the following are green: #6098 city-cell LOD, #6256, #6253 / #6254, the #6089 shared inherit gate (`oblivion_children_inherit_their_parents_land`; Oblivion count unchanged), placement-LOD / light-gate / falloff / sunlight-dimmer pins, and the LTEX archive pins.

**Dup rows**:
- **EXT-D6-2026-10-27-01 (CRITICAL)** at `byroredux/src/cell_loader/terrain_lod.rs:1105,1113`. Every Oblivion LOD block is synth, so every Tamriel exterior load reaches the panic.
- **LC-2026-10-27-D2-01 (HIGH)**: the cell spawn plays the first `NiControllerSequence`, not `Idle`. 314 of 423 animated Oblivion meshes are affected. Not re-measured; cited.

### Dim 5: Gameplay and UI data slice

#### MEDIUM

### OBL-D5-2026-10-27-01: Fallout-only opcodes tagged `Games::Any` shadow Oblivion's `IsPlayerInJail` (0x10ab) and `CloseCurrentOblivionGate` (0x10c0)
- **Severity**: MEDIUM
- **Dimension**: 5 (M47.3 ObScript). Owner of the mechanism: `/audit-scripting` (the command table).
- **Location**: `crates/scripting/src/obscript_effects.rs`:
  - `:1202` `CMD_IGNORE_CRIME` 0x10ab, `Games::Any`, `set_alert`
  - `:1831` `CMD_GET_IGNORE_CRIME` 0x10c0, `Games::Any`, `get_ignore_crime`
  - Latent, unused by vanilla Oblivion scripts: `:1183`/`:1184` `CMD_SET_ENEMY`/`CMD_SET_ALLY` 0x1078/0x1079 (Oblivion `PositionWorld`/`PositionCell`), and `:1203` `CMD_SEND_ASSAULT_ALARM` 0x105f (Oblivion `Yield`).
  - Constants at `:68-69` and `:1313-1317`. Routing in `crates/scripting/src/obscript_quests.rs:625-640`.
- **Status**: NEW. Pre-existing: the baseline `obscript_effects::handles` listed these ids with no game gate (`d1b298921`), and #6139 carried them over as `Any`. The `IsPlayerInJail` leg became observable when the jail landed (`e161c45f3`, 10-24).
- **Description**: The command table runs before the CTDA-getter route, and these rows match Oblivion.
  - **0x10ab.** On Oblivion this is `IsPlayerInJail`, and the evaluator already answers it: fn 171 is in `OBLIVION_DIALECT` and reads `jail::player_in_jail`. The call never reaches the evaluator. In object scripts, `set_alert` runs with `on = false`, which strips `IgnoreCrime` from the caller, and returns `Done { value: None }`. The VM pushes a `None` expression value as `0.0` without counting a decline (`obscript_vm.rs:879`, `:962`, `:1309`; `declined_calls` does not move), so `If IsPlayerInJail == 1` is silently false. In quest scripts there is no caller, so the call is declined.
  - **0x10c0.** On Oblivion this is `CloseCurrentOblivionGate`. The IgnoreCrime getter answers it with a number, so it reports done and no gate closes. That breaks the rule that unknown commands are counted, never faked.
- **Evidence**:
  - The UESP *Oblivion Mod/Script Functions* opcode column (from Oblivion.exe) lists 0x10AB IsPlayerInJail, 0x10C0 CloseCurrentOblivionGate, 0x1078 PositionWorld, 0x1079 PositionCell and 0x105F Yield. Oblivion has no IgnoreCrime, GetIgnoreCrime, SetEnemy, SetAlly or SendAssaultAlarm.
  - `obscript_tools names Oblivion.esm all 1` reports `+0x10ab scripts 7 lines 9 isplayerinjail 100%` and `+0x10c0 scripts 16 lines 16 closecurrentobliviongate 100%`.
  - `IsPlayerInJail` callers: `PrisonCellDoorScript`, `PCPrisonCellDoorScript`, `AmuseiScript`, `LeyawiinJailorScript` (object scripts, read 0), plus `TutorialScript`, `MS09Script`, `TG00FindThievesGuildScript` (quest scripts, declined).
  - `CloseCurrentOblivionGate` callers: `SigilStoneScript`, `OblivionGenericSigilStoneScript`, `MQ10SigilStoneScript`, `SigilRingBoom*`, and ten `TrigZoneCloseCurrentOblivion*` scripts.
- **Impact**: The jail sentence the new `jail.rs` tracks is invisible to every script that tests it:
  - the prison-door jailbreak bounty never applies, and its `== 0` arm always runs;
  - the Thieves Guild start, which needs the player in jail, and MS09's Bruma-dungeon branch never fire.
  - Sigil-stone gate closures look modelled in the census when they are not.
- **Related**: #6139, #6221 (the same "decline, don't answer a constant" rule), SCR-2026-10-27-01. `crates/scripting/examples/obscript_tools.rs:12` says "no opcode table exists locally", but the UESP dump has one.
- **Suggested Fix**:
  1. Mark the five rows `Games::Fallout`, so 0x10ab on Oblivion falls through to the fn-171 getter and 0x10c0 declines.
  2. Add a table test asserting that every `Any`/`Both` row's id names the same command in the Oblivion.exe opcode table.

Other Dim 5 checks, all with no finding:
- In the #6139 command table, 115 `Any`/`Both`/`Oblivion` rows name-match the Oblivion.exe opcodes. The other name mismatches are generic `cmd_XXXX` handlers, checked by hand.
- The 0x116c–0x1170 Shivering Isles-era ids are absent from that table (it predates SI); that is expected.
- Crime and jail semantics match the CS wiki text quoted in `jail.rs`. An indoor arrest declining is documented and deliberate.
- The MenuXml corpus is green.

**Dup row: SCR-2026-10-27-01 (HIGH).** Oblivion `Get`/`Set`/`ModCrimeGold` and `GetGold` ignore the calling reference. `jail.rs` reads that same global `ledger.crime_gold` as the player's bounty. Cited, not re-filed.

## Regression Guard List (all held)

- Stride-drift family #1506–#1509: the drift corpus is green over 9 archives and 9,612 NIFs.
- #170 BSStreamHeader dual band, `NiTexturingProperty` u32 shader-map count (#149), `MORPH_LEGACY_CUTOFF` (#1509), and the #3926 sizeless-recovery refusal: header, stream and lib code are unchanged and the corpus is green.
- BSA v103 sweep (#699 / #5073): `oblivion_all_bsas_v103_brute_force_extract_zero_errors` is green.
- `parallax_alpha_gate_tests` are green (in the 116-test bin filter).
- Disney gate: no BGSM or `.mat` path reaches Oblivion, and the material import is unchanged.
- Dark-map role (#4523) census is green. The torch emitter guard (#4261 / #4467 / #4560) is green.

## Open Work (not findings)

- **Disposition is not modelled.** `GetDisposition` (fn 76) is the sole blocker on **1,341** Oblivion INFOs. `ModDisposition` declines (`crates/scripting/src/obscript_more/state.rs:166`). This is the largest single remaining dialogue gate after the FNV-01 catalog rows.
- Known-open: #5351 (HNAM doc rot). The #5350 topic-reachability gap is latent; `topic_entry` still has no Oblivion arm.
- `CellData.pathgrids` (8,228 PGRD) still has no consumer, so Oblivion packages have no path graph. `EnableLinkedPathPoints`/`DisableLinkedPathPoints` stay in the open tail.
- The live gates (`m-exteriors.sh oblivion`, `p0-door-interaction.sh oblivion`, `/audit-runtime --game oblivion`) were not run under the suite rules. The Tamriel exterior is blocked by EXT-D6-01 until it is fixed.

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-10-27.md` (label every finding `game:oblivion` and `legacy-compat`, plus `scripting`/`quests` for D5-01 and `nifal`/`import-pipeline` for D3-01).
