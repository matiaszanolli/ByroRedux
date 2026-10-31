**HEAD**: `670f81917` · **Baseline**: `AUDIT_OBLIVION_2026-10-27.md` (HEAD `fa6be4a6b`, 136 commits ago) · **Audited**: Dims 2, 3, 5 delta-scoped (plus both 10-27 findings and dup rows re-verified), Phase 1 corpus lane · **Unchanged since baseline (skimmed)**: Dim 1 (NIF header/version/stream/blocks: only `477124477` adds `ImportedMaterial::character_skin` and `974d00657` is doc), Dim 4 (only #6452/#6453/#6454/#6472/#6114/#6133 touched `cell_loader`/`env_translate`; none is Oblivion-specific and every Oblivion pin is green)

# Oblivion (TES4) Compatibility Audit, 2026-10-31

Part of `/audit-suite --preset comprehensive`. Run read-only: no engine launch, no sub-agents, repo `target/` only, `CARGO_INCREMENTAL=0`. One probe crate lives at `/mnt/data/tmp/oblprobe` (own `target/`; path-depends on `byroredux-nif`, `-bsa`, `-core`).

## Executive Summary

**Corpus lane: green.** All four Phase 1 gates pass on the 9,612-NIF corpus: `oblivion_block_count_parity`, `no_block_sizes_drift_detector_has_zero_false_positives_on_real_corpus`, `parse_rate_oblivion`, `per_block_baseline_oblivion`. No baseline TSV changed.

**Compatibility level.** Unchanged in kind from 10-27 (live rows: `ROADMAP.md` Oblivion row, M47.3).
- NIF parsing including the v10.x tail: complete. BSA v103: 17 archives, 0 errors. All named ESM pins green.
- The 10-27 CRITICAL (terrain-LOD `holes[i]` OOB that stopped every Tamriel exterior) is **fixed**: `terrain_lod.rs:1125` now reads `holes.get(i).copied().unwrap_or(false)` (EXT-D6-2026-10-31-01 notes the missing regression guard). Not re-verified on a device, because the suite forbids launching the engine.
- Both 10-27 Oblivion-only findings are **fixed**: OBL-D3-01 by #6477 (`477124477`; `oblivion_body_skin_flag` green) and OBL-D5-01 by #6478 (`598067e2a`; `fallout_only_opcodes_do_not_shadow_oblivions` green). Of the dup rows, #6459 (VFX colliders), #6460 (catalog getters) and #6471 (spawn idle) landed as **partial for Oblivion**; the first two leave residuals, reported below as new Oblivion findings. LC-D2-01 (#6471) is covered in Dim 4 notes.
- The 136 commits are overwhelmingly M43 (Story Manager, crafting, mines, FO4/Skyrim). The Story Manager systems are inert on Oblivion (`random_conversation_system` returns when the tree has no `ADIA`/`AHEL` node), and `mine_system` has no Oblivion bases.

| Severity | NEW | Existing (cited, not re-filed) |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 1 | #6510 (SCR-2026-10-27-01, caller-scoped crime gold/`GetGold`) |
| MEDIUM | 1 | — |
| LOW | 0 | #5351 (HNAM doc rot), #6431 (Oblivion vitals rows scope) |

## Pins and guards run (all green)

| Area | Command | Result |
|---|---|---|
| Corpus lane | `parse_real_nifs`, `per_block_baselines`, `block_coverage_baselines`, `oblivion_stream_drift_corpus` (`--ignored oblivion no_block_sizes`, release) | 4 / 4 |
| BSA v103 | `bsa_real -- --ignored oblivion` | 2 / 2 |
| ESM | `parse_real_esm -- --ignored clas_oblivion_knight race_oblivion parse_rate_oblivion` | 4 / 4 (incl. PKDT/PTDT) |
| Plugin lib | `--lib oblivion` / `--lib -- --ignored oblivion` | 75 / 6 |
| NIF real data | `dark_texture_role_population_matches_the_documented_census`, `the_underwear_sub_meshes_of_the_vanilla_body_nifs_are_not_race_skin` | 2 / 2 |
| Scripting | `vanilla_oblivion_quest_scripts_execute_and_setstage`, `oblivion_quest_level_dialogue_gates_census` (1 quest / 9 INFOs, unchanged); `oblivion`/`is_player_in_jail` filters | 17 pass |
| MenuXml | `vanilla_corpus -- --ignored` | 4 / 4 |
| Bin crate | `parallax_alpha_gate oblivion placement_lod nif_light_spawn_gate race_body body_paths_kf_era creature_clip_candidates idle_paths tail beast synthesize falloff_exponent sunlight_dimmer spawn_starts_the_idle` | 119 pass |
| Bin `--ignored` | `installed_oblivion_body_paths`, `installed_default_idle_clips`, `an_installed_oblivion_voice_line`, `oblivion_ltex_paths`, `default_land_textures`, `installed_oblivion_creature_assets`, `installed_oblivion_tails_and_beast_rig` | 7 / 7 |

## Dimension Findings

### Dim 1: NIF version handling and corpus integrity
No findings. The corpus lane is green and the only commits under `crates/nif/src/{header,version,stream,lib,blocks}` or `crates/nif/tests` add the `character_skin` import flag (#6476/#6477) and a doc fix.

### Dim 2: BSA v103 and the ESM data slice

#### MEDIUM

### OBL-D2-2026-10-31-01: #6460 bridged four CTDA getters to the ObScript VM but only for FO3/FNV; 535 Oblivion INFOs (493 `GREETING`) are still declined only by them
- **Severity**: MEDIUM
- **Dimension**: 2 (DIAL/INFO selection). Mechanism owner: `/audit-scripting` (condition catalog); the Oblivion leg of closed #6460 and of FNV-2026-10-27-01. Related, not the same as FO3-2026-10-31-01 / FNV-2026-10-31-03 (those are FO3/FNV getters the catalog still lacks).
- **Location**:
  - `crates/scripting/src/obscript_cmd.rs:156-158`: `getter_answers_conditions` is `Game::of(world) == Game::Fallout && find(cmd, Game::Fallout).is_some()`, and `run_getter_for_condition` (`:164-178`) always runs `find(cmd, Game::Fallout)`.
  - `crates/scripting/src/condition/eval.rs:95-113`: `is_modeled` routes every `vm_getter_command()` function through that gate.
  - `crates/scripting/src/condition/catalog.rs:548-558,764-768`, `crates/scripting/src/condition.rs:98-107`: rows and table say "(FO3/FNV)".
- **Status**: NEW for Oblivion. The `#6460` commit says the ten getters "stay declined (as before) on Oblivion, Skyrim and FO4" because the ids "were aligned with the indices of" FO3/FNV. That does not hold for four of them.
- **Description**: Four of the ten bridged getters are the same function at the same index on Oblivion, and the ObScript command table already answers them there:
  - UESP *Oblivion Mod/Function Indices*: 50 `GetTalkedToPC`, 18 `GetCurrentTime`, 32 `GetInSameCell`, 5 `GetLocked`.
  - Their command rows are `Games::Both`: `0x1032` (`obscript_more.rs:553`), `0x1012` (`CMD_GET_CURRENT_TIME`), `0x1020` (`CMD_GET_IN_SAME_CELL`), `0x1005` (`CMD_GET_LOCKED`), all in `obscript_effects/getters.rs`.
  - The fail-closed dialogue gate declines an INFO whose CTDA function `is_modeled` rejects, so on Oblivion these INFOs can never be selected, although the VM answers the same getter for scripts.
- **Evidence** (`/tmp/audit/oblivion/py/ctda.py`: 20-byte TES4 headers, zlib records inflated, CTDA function at +8; modeled set = catalog `ROWS` indices minus the ten bridged getters, plus `OBLIVION_DIALECT`):
  - 3,079 of 19,278 Oblivion INFOs are declined (same as 10-27).
  - **535** are declined only by the four getters: `GetTalkedToPC` 502, `GetCurrentTime` 22, `GetInSameCell` 12, `GetLocked` 1.
  - By topic: **`GREETING` 493**, `HELLO` 26, and 16 others (`JauffreTopic` 4, `CountessValga` 2, `CountessCarvain` 2, `GOODBYE` 1, ...).
  - Every Oblivion activation opens through `GREETING` (#5367), so the first-meeting greetings (`GetTalkedToPC == 0`) are the ones that never select.
- **Impact**: 493 `GREETING` lines (the first-meeting greetings) can never be chosen, so the affected NPCs fall through to another line or none. Interiors and the Tamriel exterior are unaffected.
- **Suggested Fix**: Make `getter_answers_conditions` / `run_getter_for_condition` resolve the row for the loaded game (`Game::of(world)`), not `Game::Fallout`, and pass `fallout = game == Game::Fallout` to `run_row`. A getter with no row for the loaded game (`find` returns `None`) still declines, which keeps the other six bridged functions as they are on Oblivion (confirm each has no `Games::Both` row before relying on it). Keep 365 on the dialect (`GetPlayerInSEWorld`). Pin: the Oblivion declined-INFO count falls from 3,079 to 2,544 (`condition_catalog_real_data.rs` has the FO3/FNV pins to extend).
- **Note on the rest of the Oblivion gap** (not findings): sole-blocker INFOs after this fix are `GetDisposition` (76) 1,341, `GetScriptVariable` (53) 545, `GetIsCurrentPackage` (161) 82, `GetGold` (48) 61, `GetPCIsRace` (130) 35, `GetTrespassWarningLevel` (144) 31. The FO3 catalog finding (FO3-2026-10-31-01) covers the shared rows.

Other Dim 2 checks, no finding:
- `16b0235be` (`GetWeaponAnimType` on Oblivion): the grouping `0|2 -> 1, 1|3 -> 2, 5 -> 3, else 0` (`obscript_state.rs:457`) matches the CS page (0 hand-to-hand or staff, 1 one-handed, 2 two-handed, 3 bow) and the WEAP `DATA` type enum.
- `fa2870a56` (`IsPlayerInJail`, CTDA 171, UESP-aligned) is in `OBLIVION_DIALECT`; indices 116/127/193/249/251/329/365 re-checked against UESP, 0 mismatches.

### Dim 3: Legacy-property rendering, NPC body assembly, spawn colliders

#### HIGH

### OBL-D3-2026-10-31-01: #6459 stopped unlit VFX sheets from synthesizing solid colliders on FO3/FNV, but Oblivion's cobwebs and FX sheets still pass `render_geometry_can_collide`
- **Severity**: HIGH. Same impact class as FNV-2026-10-27-02 (filed HIGH, fixed by #6459); not confirmed on a device (suite rule).
- **Dimension**: 3 (spawn colliders). Mechanism owner: `/audit-nifal` and the spawn collision fallback; sibling of FNV-2026-10-31-05, which is the same predicate on FNV content.
- **Location**: `byroredux/src/cell_loader/spawn.rs:94-103` (`render_geometry_can_collide`), `byroredux/src/cell_loader/spawn/mesh_instance.rs:1526-1530` (the `ArchitectureTriMesh` site), `crates/nif/src/import/material/legacy_properties.rs:835-860` (the only writer of `MATERIAL_KIND_NO_LIGHTING`).
- **Status**: NEW for Oblivion. The 10-27 report recorded this as an Oblivion leg of FNV-02 and said "the `MATERIAL_KIND_NO_LIGHTING` key does not cover Oblivion". #6459 chose exactly that key.
- **Description**: The new predicate excludes `material_kind == MATERIAL_KIND_NO_LIGHTING && has_alpha`. That kind (102) is assigned only from `BSShaderNoLightingProperty`, which Oblivion does not have. Oblivion VFX sheets and cobwebs are plain `NiAlphaProperty` blend-only meshes with `material_kind == 0`, no alpha test, no decal, no effect shader, so every term of the predicate passes and a STAT base with no `bhk*` goes through the missing-collision fallback and gets a `Fixed` trimesh.
- **Evidence**:
  - Probe over `Oblivion - Meshes.bsa` (`/mnt/data/tmp/oblprobe/src/bin/vfx.rs`, `import_nif`, predicate re-evaluated with the #6459 terms): `cobweb01`..`07`, `hangcrud01`, `fxgasplane01`, `fxmistgroundeffect01` (and the other FX bases probed) all print `bhk=0 kind=0 has_alpha=true alpha_test=false decal=false effect=false => can_collide=true`.
  - `Oblivion.esm` census (Python over STAT/REFR, 2026-10-31 re-run): 19 STAT bases (`cobweb0*`, `HangCrud01`, `FXLightBeam*`, `FXCloudSmall01`, `FXMistGroundEffect01`, `FXWhiteTorchLarge`, `LightBeam01`, `FXGasPlane01`, `FXOblivionLightBeam*`) with **8,949 placements**, **6,906** of them cobwebs and `HangCrud01`. Unchanged from the 10-27 count.
- **Impact**: Zero-thickness solid sheets hang across dungeon corridors and over open ground: invisible walls for the player capsule, NPC walks and dropped bodies. `DUNHay`, `BloodDecal01/02` and `ARWelkydClusterFX01` share the same signature but are different cases and need their own look.
- **Suggested Fix**: key on the dialect-independent canonical signal #6459 intended, not on NoLighting: a blended (`has_alpha`), non-alpha-tested mesh with `material_kind == 0` and no authored collision should not be a body. Check whether the importer carries a `z_write` or blend flag usable here; the cobweb and FX set above is the pin (`--ignored`, over `Oblivion - Meshes.bsa`, like the FNV test #6459 added). Register it in `.github/real-data-pins.txt`.

Other Dim 3 checks, no finding:
- #6476/#6477: `character_skin` is computed at import; the vanilla body NIFs' `underwear.dds`/`groinmale.dds` meshes are not flagged; `the_underwear_sub_meshes_of_the_vanilla_body_nifs_are_not_race_skin` and the 119 bin pins are green.
- `body_paths_kf_era`, creature clips, tails, voice and LTEX archive pins are all green.

### Dim 4: Exterior and lighting data (Tamriel)
No findings. Green: #6098, #6256, the #5505/#6089 inherit path, placement LOD, sunlight dimmer, light-spawn gate and falloff pins; the `holes[i]` fix is present.
- **#6471 (spawn plays `Idle`, else `SpecialIdle`, else nothing)**. Probe over the 8,032 base-archive NIFs (`/mnt/data/tmp/oblprobe/src/main.rs`): 423 NIFs carry `NiControllerSequence`s (matches the 10-27 figure); 113 have `Idle`/`SpecialIdle`; the other 310 are clamped activation clips (`open`/`close`, `forward`/`backward`, `unequip`, magic-effect `specialidle_*` names, persuasion menu), which is the intended outcome of the fix and resolves LC-2026-10-27-D2-01 for Oblivion. Five files loop without an idle (`lorgenhand01`, `flashglow01`, `obgatemini01`, `siegecrawlerdeath`, `kvatchcastleportcullis02`); the placed ones are script driven (`flashglow01` is a DOOR x6, `obgatemini01` an ACTI x1, `lorgenhand01` a MISC); no finding without a live check.
- Dup rows: EXT-D6-2026-10-31-01 (the `holes[i]` fix has no regression guard) and EXT-D1-2026-10-31-02 (displaced FO4 gate test) are the exterior audit's.

### Dim 5: Gameplay and UI data slice
No new findings.
- `598067e2a` verified: 0x10ab on Oblivion answers `IsPlayerInJail` (`obscript_more`, `jail::player_in_jail`), the other four Fallout-only ids are `Unhandled` and counted.
- `0x106c` `GetWeaponAnimType` is `Games::Both`; the grouping is checked above.
- MenuXml corpus 4 / 4. The M43 Story Manager, crafting, mine and attraction systems do not run on Oblivion (no `SmTree` `ADIA`/`AHEL` nodes, no mine bases).
- **Existing**: #6510 (SCR-2026-10-27-01, HIGH) is still open: Oblivion `GetCrimeGold`/`SetCrimeGold`/`ModCrimeGold`/`GetGold` ignore the calling reference. #6431 (Oblivion vitals rows scope) and #5351 (HNAM doc rot) are also still open; not re-filed.

## Regression Guard List (all held)

- Stride-drift family #1506-#1509: drift corpus green over 9 archives / 9,612 NIFs.
- #170 BSStreamHeader dual band, `NiTexturingProperty` u32 count (#149), `MORPH_LEGACY_CUTOFF` (#1509), #3926 sizeless-recovery refusal: header/stream/lib untouched, corpus green.
- BSA v103 sweep (#699 / #5073): `oblivion_all_bsas_v103_brute_force_extract_zero_errors` green.
- `parallax_alpha_gate_tests` (in the 119 bin tests) green; Disney gate untouched (no BGSM/`.mat` path reaches Oblivion).
- Dark-map role (#4523) census, torch emitter guard, `fallout_only_opcodes_do_not_shadow_oblivions` (#6478), `race_skin_overrides_only_the_skin_meshes` (#6477): green.

## Open Work (not findings)

- **Dialogue reachability**: after OBL-D2-01, `GetDisposition` (76, 1,341 sole-blocker INFOs) and `GetScriptVariable` (53, 545) are the largest remaining gates; `ModDisposition` declines (`obscript_more/state.rs`).
- #5350 (`topic_entry` has no Oblivion arm) stays latent. `CellData.pathgrids` (8,228 PGRD) still has no consumer; `EnableLinkedPathPoints` / `DisableLinkedPathPoints` stay in the open tail.
- The live gates (`m-exteriors.sh oblivion`, `p0-door-interaction.sh oblivion`, `/audit-runtime --game oblivion`) were not run under suite rules. EXT-D7-2026-10-31-01 (`m-exteriors.sh --self-test` runs in no lane) is the exterior audit's.

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-10-31.md` (label `game:oblivion` + `legacy-compat`, plus `scripting`/`quests` for OBL-D2-01 and `nifal`/`import-pipeline` for OBL-D3-01).
