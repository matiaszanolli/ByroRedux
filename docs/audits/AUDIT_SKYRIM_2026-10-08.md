# Skyrim (SE + LE) Compatibility Audit — 2026-10-08

**HEAD**: 00f580e09 · **Baseline**: `docs/audits/AUDIT_SKYRIM_2026-10-05.md` (HEAD `a2c24b16e`, 116 commits ago) · **Audited**: Dim 2 (shader-type dispatch + material slice), Dim 3 (NPC equip + FaceGen), Dim 4 (multi-master load order + TES5 cell load), Dim 5 (archives + corpus gates) · **Unchanged since baseline (skimmed)**: Dim 1 (BSTriShape packed geometry + SSE reconstruction). Its only commits are `0abc86c8b` (#4268, which touches the Starfield `BSGeometry` skin path only) and `2be7c7c0b` (doc comments). Its guards were spot-checked and are green.

Run context: one leg of `/audit-suite --preset comprehensive`, run solo with no sub-agents. Scratch notes are in `/tmp/audit/skyrim/dim_{1..5}.md`. The census scripts are `/tmp/audit/skyrim/{esm,bsa105,hdpt_census,player_ovr,race_child,xccm,fuzcensus}.py`.

Constraints:
- **No engine launch.** The Whiterun control bench, the DLC repro and the live render trace were not re-driven.
- **No `--ignored` plugin tests (suite rule 5).** `parse_real_skyrim_esm` and `ms01_eltrys_authored_placement` were not run. Their claims were re-checked by reading the code and by byte-walking the installed SE masters.

---

## Executive Summary

**Parse and archive layers are still fully green:**
- **NIF corpus:** SE 33,468 / 33,468 clean across 8 archives; LE 22,466 / 22,466 clean.
- **Per-block baselines:** SE and LE both OK. `unknown_ceiling_skyrim_se` is OK.
- **BSA:** `bsa --lib` passed 110. The three `bsa_real` Skyrim tests pass.
- **SSE packed bone indices:** 19,606 weighted lanes, 0 changed by the importer.
- **Filtered unit runs:** nif 245, plugin 405, bin (rustc 1.96) 293. All green.

**The one Skyrim-specific code change with real risk this window is #5095 (`8c925ec54`).** It adds a pre-baked FaceGen miss fallback: when no per-NPC facegeom mesh exists, the head is built from the NPC's PNAM head parts.
- **Gap (SKY-D3-2026-10-08-01, MEDIUM):** the fallback reads only the NPC's own `PNAM` list.
- **Effect on the player:** vanilla Skyrim's player record authors exactly one PNAM entry, `HairMaleNord01`. The "assembled head" is therefore a hair mesh with no face, mouth or eyes. The fix commit's own live check says `head_parts=1`.
- **Where the face lives:** the face, mouth and eyes are the race's **base head parts**: RACE Head Data `INDX`/`HEAD` (xEdit `wbHeadParts`). The parser never decodes them, and `RaceRecord.head_parts` documents them as "empty on Skyrim+".
- **Extra parts are dropped too:** HDPT `HNAM` extra parts, such as the hairline, are also never followed.
- **Why the gate misses it:** the P3 smoke gate is `head_parts > 0`, which passes on hair alone.

**Three LOW findings.** Each one is a Skyrim data-semantics error that sits on a false premise in code or docs:
- **SKY-D4-2026-10-08-01:** TES5/FO4 CELL `XCCM` is "Sky/Weather from Region" (a REGN reference), not a CLMT climate. All 214 `Skyrim.esm` occurrences are interior → REGN.
- **SKY-D5-2026-10-08-01:** Skyrim `.fuz` is FUZE + raw `.lip` + **xWMA**, not "RIFF lip + XMA2". 3,000/3,000 files match this layout on SE and on LE. The XMA2 claim is now steering the V2 decoder scope (the 2026-10-08 audio report repeats it).
- **SKY-D3-2026-10-08-02:** a #5079 test pins TES5 RACE flag `0x4` as unused on Skyrim. In fact it is xEdit's `Child` bit, and exactly the 5 `*RaceChild` races set it.

**The baseline's two MEDIUM findings were filed as #5358 (BODT) and #5359 (NPC_ WNAM).** Both are still open and unfixed: there is no `BODT` arm and no NPC_ `WNAM` arm. #5095 and #5098 are closed. #5098's fix was verified (the ROADMAP rows read 33,468 across 8 archives, plus the LE row).

**Skyrim-relevant findings from sibling audits, cited and not re-filed:** ESM-D4-01 / ESM-D2-01, SCR-D5-01..04, ECS D7-03, GAME-D7-01/02 and SAVE-D2-01 (Story Manager); SAVE-D5-01 / ECS D7-01 (MQ101 cart CellRoot); FNV-D2-01 (DIAL override replaces the INFO list); REN-D5-01 (SE LOD atlas R/B swap); PEX-D4-01; CHAR-D4-02 / SCR-D1-01; FNV D6-01. CHARACTER 2026-10-08 noted, without filing, that the #5095 fallback reads the shell's PNAM rather than the TPLT terminal. That is a different axis from SKY-D3-2026-10-08-01.

**Totals: 4 NEW** (0 CRITICAL, 0 HIGH, 1 MEDIUM, 3 LOW) · 0 regressions · 5 matched-existing (open): #4256, #5358, #5359, #5015, #4913.

---

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction

**Skimmed. 0 findings.**

- **`0abc86c8b` (#4268):** adds a `bone_count` bound to `convert_bs_geometry_skin_weights`. That function is the Starfield `BSGeometry` producer; the Skyrim `BSTriShape` / SSE-reconstruction producers are untouched.
- **`2be7c7c0b`:** doc comments only, in `tangent.rs` and `effect.rs`.
- **Invariants that still hold:**
  - `widen_packed_bone_indices` (`crates/nif/src/import/mesh/skin.rs:576`) is the only packed-index path; `remap_bs_tri_shape_bone_indices` has 0 hits.
  - `BSLODTriShape` → `NiLodTriShape::parse` (`blocks/mod.rs:484`).
  - `BSMeshLODTriShape` → `BsTriShape::parse_lod` (`:489`).
  - The specialty arms are present: BSTreeNode `:355`, BSPackedCombined[Shared] `:760`, BSLagBoneController `:910`, BSProceduralLightningController `:911`.
- **Real data:** `packed_sse_indices_match_partition_palette_expansion_on_real_data` passes (19,606 lanes, 0 changed).

### Dimension 2 — Shader-Type Dispatch + Skyrim Material Slice

**0 new; 1 matched-existing.**

Since the baseline, `crates/nif/src/blocks/shader/` and `crates/nif/src/import/material/` have no non-comment diff. `import/types.rs` gains only a `Mat` material-source variant (#4277, Starfield loose `.mat`). Window commits on this dimension's paths:
- **`9f0a8a7cc` (#5230):** a mirror-pane provenance exemption. It is gated behind `bgsm_pbr_scalars_authored`, which is BGSM only, so vanilla Skyrim (no BGSM) is unaffected.
- **`c2f28e06c` (#4277):** Starfield.
- **`46f59e1d2`, `c717c177b`:** docs.
- **`34c3adf14` (#5249):** transmission shadow traces. Renderer-owned.

#### SKY-D7-2026-09-11-02 — `ImportedMaterial.shader_type` never crosses the NIFAL boundary
- **Severity**: MEDIUM
- **Status**: Existing: #4256 (OPEN). `byroredux/src/cell_loader/spawn/mesh_instance.rs:317` still reads `material.shader_type` from the raw tier.

### Dimension 3 — NPC Equip + FaceGen (M41)

**2 NEW (1 MEDIUM, 1 LOW); 2 matched-existing (#5358, #5359).**

Window commits:
- `8c925ec54` (#5095).
- `6e0594e68` (#5266): the reconcile no longer counts the weapon slot.
- `fec2d14b2` (#5268).
- `7c7711cff` (#5079).
- `9813af435`: racial spells. Covered by CHAR-D4-02 / SCR-D1-01.
- `00f580e09`: Eat/Sleep, which is gameplay-owned.

Re-verified: #5358 still has no `BODT` arm in `items.rs` or `misc/equipment.rs`, and #5359 still has no NPC_ `WNAM` arm. `reconcile_worn_gear` (`loot_appearance.rs:537`) still derives "equipped" from biped-slot occupancy, so #5358's Draugr hair/beard consequence still stands.

#### SKY-D3-2026-10-08-01: #5095's FaceGen-miss head fallback loads only the NPC's own `PNAM` parts — the vanilla Skyrim player gets a hair mesh with no face, mouth or eyes, because RACE base head parts and HDPT extra parts are never decoded
- **Severity**: MEDIUM
- **Dimension**: 3 — NPC equip + FaceGen
- **Location**:
  - `byroredux/src/npc_spawn/resumable/prebaked.rs:174-185`: `prebaked_head_fallback_paths` reads PNAM only.
  - `:296-357`: the `HeadParts` phase.
  - `crates/plugin/src/esm/records/actor/race.rs:55-66`: the doc says "empty on Skyrim+, which moved head parts out to standalone `HDPT` records". The head section's `HEAD` sub-record has no arm; only `MODL` is collected.
  - `crates/plugin/src/esm/records/misc/character.rs:12-21`: `HdptRecord` has no `PNAM` type and no `HNAM` extras. Its flags doc is wrong.
  - `byroredux/src/player_body.rs:636`: the `head_parts` count.
  - `docs/smoke-tests/p3-player-body.sh`: the `head_parts > 0` gate.
- **Status**: NEW. #5095 (CLOSED 2026-10-06) is an incomplete fix, not a regression: its fallback never covered race base parts. No sibling report or issue matches. `gh` "head part race default" across all states finds only #5095, #591 and #967.
- **Description**:
  - **The fix's premise.** The commit and the code doc rest on two claims: "Skyrim RACEs carry no head-part table at all", and the player's authored PNAM list is "ManHead / eyes / hair / brows".
  - **Both claims are false on the shipped data.**
    - A TES5 RACE carries per-gender **Head Data**: a `NAM0` marker, then `MNAM` / `FNAM`, then `wbHeadParts` = `INDX` + `HEAD` → HDPT (xEdit `wbDefinitionsTES5.pas:9282`, `:9470-9497`). The CK Race page lists these as "Base Head Parts", and the CK Character Gen Parts tab lists the base head parts as Face, Eyes, Hair, Facial Hair and Eyebrows.
    - The vanilla player record authors a single hair part.
    - The parser decodes no RACE `HEAD`, no HDPT `PNAM` type and no HDPT `HNAM` "Extra Parts" (xEdit `:5296-5309`).
  - **Result.** The fallback loads only what PNAM names. For the player that is one hair NIF, so the third-person player still has no face.
- **Evidence**: byte walk of the installed SE masters.
  - **Player record.** `Skyrim.esm` NPC_ `0x00000007` (`Player`, ACBS `0x30`, male, race `NordRace` `0x13746`) has PNAM = `[0x051507 HairMaleNord01]` (type 3 Hair, `Hair\Male\Hair01.nif`, HNAM extra `HairLineMaleNord01`). No DLC overrides the player record.
  - **Race defaults.** `Update.esm` overrides NordRace.
    - Male base head parts: `HairMaleNord10` (Hair), `MaleHeadNord` (Face), `MaleMouthHumanoidDefault` (Misc), `MaleEyesHumanLightBlue` (Eyes).
    - Female base head parts: 5, including `FemaleBrowsHuman01`.
    - In `Skyrim.esm`, 32 of 99 RACEs author head parts.
  - **What the fix itself reported.** Its live check reports `head_parts=1 (meshes 8→9)`, and that single part is the hair.
  - **Extra parts.** 230 of 766 HDPTs author `HNAM` extras (271 refs). Of the 2,912 NPC_ records that author PNAM, 2,443 reference an extra part that is absent from their own PNAM list. Every facegeom-less NPC (modded NPCs, any record without baked facegeom) loses those parts too.
  - **Why the gate misses it.** `p3-player-body.sh` fails only on `head_parts == 0`. The count is a world-wide `PrebakedHeadPart` query, not one scoped to the player root, so a hair-only player passes.
  - **The flags doc is wrong.** `HdptRecord.flags` says Skyrim "bits 0-2 encode the head-part type slot". xEdit has DATA = Playable / Male / Female / Is Extra Part / Use Solid Tint, and the type is the separate `PNAM` u32.
- **Impact**:
  - **The player.** On every Skyrim session, the third-person player shows a floating hair shell over an empty neck: no face, eyes, mouth or hairline. This is the case #5095 was filed to fix, and the issue is now closed.
  - **Other NPCs.** Any facegeom-less humanoid NPC gets the same partial head.
  - **Detection.** The fallback is silent and the gate stays green.
- **Related**: #5095 (closed, incomplete). The CHARACTER 2026-10-08 note (the fallback reads the shell's PNAM, not the TPLT terminal) is a separate axis. Also related: #3409 (hidden-partition handling, reused correctly) and #5359 (NPC_ `WNAM`, same "Skyrim body source" family).
- **Suggested Fix**:
  1. Decode RACE Head Data `HEAD` per gender section into `RaceRecord`, and decode HDPT `PNAM` (type) and `HNAM` (extras).
  2. Build the fallback list from the race's base parts for the NPC's gender. For each type the NPC's PNAM authors, use the NPC's part instead. Then expand `HNAM` extras recursively, with a cycle guard.
  3. Source the exact per-type override rule before coding it (for example SKSE's `TESNPC::GetCurrentHeadPartByType` race fallback). Do not guess it.
  4. Tighten the P3 gate to require a Face-type part on the player root, and fix the `race.rs` / `character.rs` docs.

#### SKY-D3-2026-10-08-02: #5079's `is_child_race` test pins "Skyrim+ child races do not use the flag", but TES5 RACE flag `0x4` is `Child` and is set on exactly the 5 child races
- **Severity**: LOW
- **Dimension**: 3 — NPC equip + FaceGen (Skyrim race data)
- **Location**: `byroredux/src/npc_spawn/tests.rs:2884-2904` (`is_child_race_keeps_oblivion_beast_race_distinct_from_fo3_fnv_child`, which asserts `!is_child_race(GameKind::Skyrim, Some(0x04))` with the comment "Skyrim+ child races do not use the flag at all (#2455 unverified)"); `byroredux/src/npc_spawn.rs:597-608`.
- **Status**: NEW.
- **Description**: xEdit `wbDefinitionsTES5.pas:9136` defines the TES5 RACE DATA flag `0x00000004` as `'Child'`. A byte walk of `Skyrim.esm` RACE DATA (flags at offset 32) finds the bit set on exactly `NordRaceChild`, `ImperialRaceChild`, `RedguardRaceChild`, `BretonRaceChild` and `BretonRaceChildVampire`, with no false positives. The helper's game gate itself is harmless today: the only consumers are the KF-era body-path and walk-clip ladders, and Skyrim never takes them (`humanoid_body_paths` returns `&[]`; the walk comes from HKX). The problem is that the test asserts a false data claim as a contract.
- **Impact**: None at runtime today. The next Skyrim consumer of "is child" (for example child-specific scale, dialogue conditions or combat exclusion) would find a pinned test telling it the bit is meaningless on TES5.
- **Suggested Fix**: Correct the comment to cite xEdit TES5 `Child = 0x4`, and either keep Skyrim out of this KF-era helper explicitly ("no Skyrim consumer") or admit `GameKind::Skyrim` once a consumer exists.

### Dimension 4 — Multi-Master Load Order + TES5 Cell Load

**1 NEW (LOW); 1 matched-existing (#5015).**

Window commits:
- **`78c2b2d61` (#5309):** dedups the CELL sub-record walkers through `CellSubrecordFields::absorb` (`crates/plugin/src/esm/cell/helpers.rs:35-226`). Verified equivalent:
  - The removed arm sets of both `walkers.rs` and `wrld.rs` (EDID, FULL, XCLW, XCIM, XCWT, XCAS, XCMO, LTMP, XCRI, XPRI, XCMT, XCCM, XLCN, XEZN, XCLR, XOWN / XRNK / XGLB, RCLR) are exactly `absorb`'s arm set.
  - The interior walker keeps DATA + XCLL, and the exterior walker keeps XCLC.
- **`bc260fda1` (#5303):** extracts `strings_archive_directory`. Behaviour is unchanged.
- **`80e00008e`, `32c8e6e6c`:** docs.
- **`b24cb46b6`:** clippy.

DIAL QNAM → `quest_refs` is intact (`misc/dialogue.rs:490`). The Skyrim INFO `DATA` retype (`25b678106`, #5295) is ESM-owned.

#### SKY-D4-2026-10-08-01: TES5/FO4 CELL `XCCM` is "Sky/Weather from Region" (a REGN reference) but is decoded and consumed as a CLMT climate override
- **Severity**: LOW
- **Dimension**: 4 — TES5 cell data through the shared CELL walker
- **Location**:
  - `crates/plugin/src/esm/cell/helpers.rs:44-46` and `:187-191`. The doc says "XCCM Skyrim climate override (per-cell CLMT FormID, exterior cells in vanilla — boss arenas…)".
  - Consumer: `byroredux/src/env_translate.rs:442-479` (`resolve_cell_climate`), reached from `scene/world_setup.rs:472` (`apply_cell_climate_override`, exterior cells only).
- **Status**: NEW.
  - #693 (CLOSED) added the decode on the "Skyrim climate" premise, and #2451 (CLOSED) wired the exterior consumer.
  - EXT-D1-2026-10-08-03 covers the WTHS stand-in on the same path, a different defect.
  - ESM D2-02 (2026-10-08) covers Oblivion REGN `CNAM`, also different.
- **Description**:
  - **The definitions differ by game.** xEdit defines `XCCM` as `wbFormIDCk(XCCM, 'Sky/Weather from Region', [REGN])` on TES5 (`wbDefinitionsTES5.pas:4293`) and FO4 (`wbDefinitionsFO4.pas:6078`). Only TES4, FO3 and FNV define it as `'Climate', [CLMT]`.
  - **The code ignores the difference.** It decodes the field game-blind into `climate_override`. Its doc asserts the CLMT/exterior semantics for Skyrim specifically.
- **Evidence**: `xccm.py` byte walk.
  - `Skyrim.esm`: 214 CELLs author `XCCM`. All 214 are interior, and all 214 target a REGN (for example `NightingaleHall01` → `0xC5857`, `ThalmorEmbassy05` → `0xC5853`).
  - `Dawnguard.esm` / `Dragonborn.esm`: 35 / 19, all interior.
- **Impact**:
  - **Vanilla: none.** The only consumer is exterior-keyed, and every vanilla occurrence is interior.
  - **The interior feature is lost.** Interiors that show the sky take their sky and weather from a region. That is what `XCCM` means on TES5, and it is unconsumed.
  - **Modded exteriors.** A modded Skyrim/FO4 exterior that authors `XCCM` would have a REGN id looked up in the CLMT map, log a "not among parsed CLMT" warning, and fall back.
- **Suggested Fix**: Split the decode by game. Keep `climate_override` (CLMT) for TES4 / FO3 / FNV, and add a `sky_region` (REGN) field for TES5 / FO4. Correct both docs. Leave the interior sky-from-region consumer as forward scope, owned by `/audit-exterior`.

#### SKY-D4-2026-09-29-01 — Skyrim authored corpse poses (XRGD) not applied
- **Severity**: MEDIUM
- **Status**: Existing: #5015 (OPEN). No window commit touches it.

### Dimension 5 — Archives + Corpus Gates

**1 NEW (LOW).**

Window commits:
- **`3970c8d19` (#5232):** the real-data harness now panics on a present-but-unopenable archive, and fails a corpus where every archive is absent.
- **`75755796d` (#5260):** pins the coverage baselines' corpus total.
- **`44986c6ef` (#5259):** routes every corpus filter through `is_nif_entry`.
- **`f8950e7cc`:** FNV voice. It adds `skyrim_fuz_container_shape`.

There are no codec, layout, naming or sibling changes in `crates/bsa/src` or `asset_provider/archive.rs`.

Runs:
- `parse_rate_skyrim_se`: 33,468 / 33,468 (Meshes0 18,862, Meshes1 13,847, _ResourcePack 149, CC 231 / 266 / 65 / 4, Animations 44).
- `parse_rate_skyrim_le`: 22,466 / 22,466.
- `per_block_baseline_skyrim_{se,le}`: OK.
- `unknown_ceiling_skyrim_se`: OK.
- `byroredux-bsa --lib`: 110 passed.
- `bsa_real` Skyrim (v105 magic, v105 brute-force extract, fuz shape): 3 passed.

#### SKY-D5-2026-10-08-01: Skyrim `.fuz` is documented as "FUZE + RIFF lip + XMA2", but every SE and LE file is FUZE + raw `.lip` + RIFF **xWMA** — the claim is now steering the V2 decoder scope
- **Severity**: LOW
- **Dimension**: 5 — Archives + corpus gates (Skyrim voice archive content)
- **Location**:
  - `crates/bsa/tests/bsa_real.rs:497-540`: doc lines 501-502 say "XMA2 on LE … Decoding XMA2 is its own project"; lines 535 and 537 label the first RIFF "lip chunk".
  - `docs/engine/dialogue-trees.md:186-187`.
  - The `ROADMAP.md:321` M43 row ("Skyrim+ `.fuz` (FUZE + RIFF lip + XMA2) stays V2 scope").
- **Status**: NEW. AUDIT_AUDIO_2026-10-08 (line 224) repeats the premise ("needs an XMA2 decoder") and has not filed it as a defect.
- **Description**: The FUZE container is magic `FUZE`, u32 version, u32 lip size, then the raw `.lip` bytes, then the audio. The test's `windows(4).position(RIFF)` lands on the audio chunk at `12 + lip_size`, not on lip data, and that audio chunk is `RIFF…XWMA`. XMA2 is the Xbox 360 codec, and no PC file carries it.
- **Evidence**: `fuzcensus.py` (an independent BSA v104 / v105 reader).
  - SE `Skyrim - Voices_en0.bsa`: 3,000 / 3,000 sampled `.fuz` are `FUZE` v1 + RIFF/XWMA, with the first RIFF at `12 + lip_size`.
  - LE `Skyrim - Voices.bsa`: identical, 3,000 / 3,000.
  - The test's own sample, `dialoguege_dialoguegeneric_0006ce5e_1.fuz`, has lip_size 1474 and `RIFF … XWMAfmt` at 1486.
- **Impact**:
  - **Decoder choice.** The V2 voice plan targets the wrong decoder: xWMA (WMA v2 / WMA Pro in RIFF) is needed, not XMA2.
  - **Test shape.** The shape test passes while recording a false layout, so it cannot catch a reader that mistakes the audio for the lip track.
- **Suggested Fix**:
  - Parse the `lip_size` header field in the test.
  - Assert that `RIFF` sits at `12 + lip_size` with a `XWMA` form type.
  - Correct the three doc sites and tell `/audit-audio` the codec is xWMA.

---

## Shader-Type Coverage Matrix

The parse and import code is unchanged since 2026-09-22, and the render column has not changed since 2026-10-05.

| Variant | Numeric type(s) | Parse | Import (→ `ImportedMesh.material`) | Render |
|---|---|---|---|---|
| `None` (no trailing data) | 0, 2, 3, 4, 8, 9, 10, 12, 13, 15, 17, 18, 19, 20 | Complete (0 bytes; #4252 pin) | N/A (kind carried as `material_kind`) | Default lit; kinds 2..=20 protected from the glass classifier; kinds 0..=16 early-Z eligible (#5057), 17–20 late-test |
| `EnvironmentMap` | 1 | Complete | Complete (`env_map_scale`) | Consumed; alchemy glass → glass (#4392) |
| `SkinTint` | 5 | Complete | Complete | Consumed; #4423 tint alpha weight |
| `HairTint` | 6 | Complete | Complete | Consumed |
| `ParallaxOcc` | 7 | Complete | Complete | Consumed |
| `MultiLayerParallax` | 11 | Complete | Complete | Consumed |
| `SparkleSnow` | 14 | Complete | Complete | Consumed |
| `EyeEnvmap` | 16 | Complete | Complete | Consumed |
| FaceTint (no payload) | 4 | Complete | `material_kind = 4` keys the FaceGen tint override (#4421) | Consumed |
| `Fo76SkinTint` etc. | FO76 table | N/A for Skyrim (`parse_shader_type_data_fo76`) | — | — |

Residual: #4256 (the `shader_type` discriminator is still not canonical).

---

## Cell-Load Regression Status

- **TES5 walk**: the walker refactor (#5309) is arm-for-arm equivalent (see Dim 4). The real-master tests (`parse_real_skyrim_esm`: 590 cells / 18,318 statics / 37 worldspaces on 2026-10-05) were **not re-run** because they are `--ignored` plugin tests, which suite rule 5 forbids. No window commit touches STAT / REFR / LIGH resolution or the LAND scale.
- **Multi-master**: `load_order.rs` changed only through the #5303 helper extraction. The #3813 ordered fold is unchanged. The DLC repro was not re-driven (no engine launch).
- **Body/head data on the spawn path**: #5358 (BODT), #5359 (NPC_ WNAM) and the new SKY-D3-2026-10-08-01 (race base head parts) are all open on the Skyrim actor-appearance path.
- **Whiterun BanneredMare control bench**: INCOMPLETE, not FAILED, because there was no engine launch. The reference is the ROADMAP Bench-of-record: 5,777 entities, 83.8 FPS / 11.93 ms (`ROADMAP.md:189`, HEAD `a37fcba3c`). `/audit-runtime` owns the live baseline.

---

## Totals

| Severity | New | Regression | Matched-existing (open) |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 1 (SKY-D3-2026-10-08-01) | 0 | 4 (#4256, #5358, #5359, #5015) |
| LOW | 3 (SKY-D3-2026-10-08-02, SKY-D4-2026-10-08-01, SKY-D5-2026-10-08-01) | 0 | 0 |

`#4913` (Skyrim `.btt` tree LOD, exterior-owned) also stays open and is out of this skill's scope.

| Dimension | New | Matched-existing |
|---|---|---|
| 1 — BSTriShape / SSE recon | 0 (skimmed) | 0 |
| 2 — Shader-type / material slice | 0 | 1 (#4256) |
| 3 — NPC equip + FaceGen | 1 MEDIUM, 1 LOW | 2 (#5358, #5359) |
| 4 — Load order + cell load | 1 LOW | 1 (#5015) |
| 5 — Archives + corpus | 1 LOW | 0 |

Suggested: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-10-08.md`. Label every finding `game:skyrim` + `legacy-compat`, then add:
- **SKY-D3-2026-10-08-01:** `medium`, `bug`, `esm-plugin`, `character`.
- **SKY-D3-2026-10-08-02:** `low`, `test-gap`.
- **SKY-D4-2026-10-08-01:** `low`, `bug`, `esm-plugin`, `terrain-exterior`, plus `game:fo4`.
- **SKY-D5-2026-10-08-01:** `low`, `documentation`, `audio`, `doc-rot`.
