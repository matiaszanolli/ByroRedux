**HEAD**: `4a3e89d56` · **Baseline**: `AUDIT_OBLIVION_2026-10-14.md` (HEAD `adbd468e2`, last full run) for Dims 1, 2 and 5; `AUDIT_OBLIVION_2026-10-19.md` (HEAD `e1bd40bcf`, area-scoped streaming-deep) for Dims 3 and 4 · **Audited**: Dims 1–5 delta-scoped, plus the Phase 1 corpus lane · **Unchanged since baseline (skimmed)**: Dim 3's legacy-material and render paths (`crates/nif/src/import/material`, `render/static_meshes.rs`, `seam_blend`, `material_sampling.glsl`). Only Starfield and FO4 material-swap work landed there; the guards were re-run.

# Oblivion (TES4) Compatibility Audit, 2026-10-21

Part of `/audit-suite --preset comprehensive`. Solo run, read-only, no engine launch, repo `target/` only.

## Executive Summary

**Corpus lane: green.** All four Phase 1 gates pass on the 9 612-NIF corpus (base + 8 DLC archives):
- `oblivion_block_count_parity`
- `no_block_sizes_drift_detector_has_zero_false_positives_on_real_corpus`
- `parse_rate_oblivion`
- `per_block_baseline_oblivion`

`oblivion_truncations.tsv` still reads `truncating=0 parsed=9612`. There is no parser, archive or render-path defect in this window, and no closed issue has regressed.

**Prior findings.** All five 10-19 findings are fixed except OBL-SD-03:
- OBL-SD-01 (idle clip): fixed as #6100, `abd3d5001`.
- OBL-SD-02 (XESP children): fixed as #6077, `22ff0e12e`.
- OBL-SD-04 (creature walk clip): fixed as #6078, `578d8432a`.
- OBL-SD-05 (restored body part loses race skin): fixed as #6128, `1a8c05139`.
- OBL-SD-03 (placement LOD in city worldspaces) is still open as **#6098**, unchanged at `placement_lod.rs:450`.

All fourteen 10-14 findings are closed (#5778–#5790). The XCCM fix (#5785, `34a820ceb`) and the #5778 quest-gate census were re-verified.

| Severity | NEW | Findings |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 2 | OBL-D2-2026-10-21-01, OBL-D4-2026-10-21-01 |
| LOW | 0 | — |

**Both findings are Oblivion-only translation gaps at the parse or rule boundary:**
- **OBL-D4-01.** Oblivion authors its cloud colours as two of the ten `NAM0` colour groups. The parser treats those groups as FO3/FNV "Unused" slots. Every Oblivion sky therefore draws white-chroma clouds, at night and in Oblivion-realm storms alike.
- **OBL-D2-01.** The PSDT weekday gate added for FNV in #6079 was left off for Oblivion. The reason given was that Oblivion's enum is not established. It is: UESP documents it, the master's EditorIDs confirm it, and the in-tree calendar already knows Oblivion's start weekday. The day-of-month byte is not decoded at all. As a result, 306 weekday packages and 75 date packages run every day.

**Compatibility level** (live rows: `ROADMAP.md` Oblivion row, `docs/feature-matrix.md`):
- NIF parse, including the v10.x tail, is complete on the vanilla corpus.
- BSA v103 extracts all 17 archives with 0 errors.
- ESM parse is clean on every named real-data pin.
- Interiors and the Tamriel exterior render. This was not re-verified on a device here: the suite rules forbid launching the engine.

**Method.** Per dimension:
1. `git log` on the dimension's Paths since the baseline (456 commits since `adbd468e2`, 152 since `e1bd40bcf`).
2. The Oblivion-visible delta read in full.
3. The skill's named pins re-run as single named tests.
4. Throwaway Python readers over `Oblivion.esm` (scratch `/tmp/audit/oblivion/py/`), plus xEdit `wbDefinitionsTES4.pas` / `wbDefinitionsCommon.pas` and the local UESP and CS wiki dumps.

Dedup used `/tmp/audit/issues.json` (119 open), closed-issue searches and `/tmp/audit/SUITE_FINDINGS.md`.

## Pins and guards run (all green)

| Area | Command / test | Result |
|---|---|---|
| Corpus lane | `cargo test -p byroredux-nif --release --test parse_real_nifs --test per_block_baselines --test block_coverage_baselines --test oblivion_stream_drift_corpus -- --ignored oblivion no_block_sizes` | 4 / 4 |
| BSA v103 | `bsa_real -- --ignored oblivion` (`oblivion_all_bsas_v103_brute_force_extract_zero_errors`, `oblivion_meshes_bsa_v103_extracts_nif_with_gamebryo_magic`) | 2 / 2 |
| ESM | `parse_real_esm -- --ignored clas_oblivion_knight race_oblivion parse_rate_oblivion` (incl. `parse_rate_oblivion_pack_legacy_pkdt_and_ptdt`) | 4 / 4 |
| Scripting | `vanilla_oblivion_quest_scripts_execute_and_setstage`, `oblivion_quest_level_dialogue_gates_census` (`--ignored`) | 2 / 2 |
| MenuXml | `crates/menuxml/tests/vanilla_corpus.rs -- --ignored` (incl. `menu_texture_header_census_decodes`) | 4 / 4 |
| Bin crate | `parallax_alpha_gate oblivion placement_lod nif_light_spawn_gate race_body body_paths_kf_era creature_clip_candidates idle_paths` | 70 pass |
| Bin crate, real data (`--ignored`) | `installed_oblivion_body_paths_exist_in_the_meshes_archive`, `installed_default_idle_clips_exist_in_the_meshes_archive`, `oblivion_ltex_paths_exist_in_vanilla_archives`, `default_land_textures_exist_in_vanilla_archives`, `an_installed_oblivion_voice_line_resolves_and_decodes` | 5 / 5 |

## Dimension Findings

### Dim 2 — BSA v103 and ESM data slice

#### OBL-D2-2026-10-21-01: Oblivion's PSDT calendar fields are never evaluated: 306 weekday-scheduled and 75 date-scheduled packages run every day (158 actors)
- **Severity**: MEDIUM
- **Dimension**: 2 (TES4 PACK decode → ambient package selection)
- **Location**:
  - `crates/scripting/src/globals.rs:141-150` (`package_weekday` returns `None` for every game but `Fallout3NV`)
  - `crates/scripting/src/globals.rs:124-139` (`day_of_week`, which already carries Oblivion's Morndas origin)
  - `crates/plugin/src/esm/records/misc/pack.rs:970-1003` (the PSDT arm drops byte 2, the date)
  - `crates/plugin/src/esm/records/misc/pack.rs:422-431` (`PackSchedule`, which has no date field)
  - consumer `byroredux/src/npc_spawn/ai_package.rs:782-791`
- **Status**: NEW. #6079 (closed) fixed FO3/FNV only and states "the weekday is only supplied for Fallout 3/NV, where the enum is established". The skill records that same premise. The measurements below show the premise does not hold for Oblivion. No open or closed issue covers the Oblivion arm or the PSDT date byte.
- **Description**: The decode is shared. `PackDays::from_raw(sub.data[1] as i8)` runs for every game, and Oblivion's 8-byte PSDT has the FO3/FNV byte layout. But the selector only receives a weekday where `package_weekday` supplies one, and it supplies `None` on Oblivion, so `active_at` treats every Oblivion day constraint as satisfied. Both inputs the gate needs already exist for Oblivion:
  - **The enum.** UESP *Oblivion Mod:Mod File Format/PACK* (local dump) documents PSDT byte 1 as 0–6 Sundas–Loredas, 7 MTWTF, 8 Loredas+Sundas, 9 MWF, 10 TTh, 255 any. That is exactly the `PackDays` mapping #6079 implemented.
  - **The calendar.** `day_of_week(world, false, …)` already uses Oblivion's documented Morndas start ("always starts on Morndas", UESP *Calendar*). `GetDayOfWeek` (`obscript_more/extras.rs:185`) answers from it today.

  Fallout's start weekday is, by the code's own comment, *not* documented. So the gate is enabled for the game whose calendar is less established and disabled for the one whose calendar is established.

  Separately, PSDT byte 2 (day of month, 0 = any) is read for no game. FNV authors 0 on all 4 163 packages, so it never mattered there. Oblivion authors it on 75 packages.
- **Evidence** (`/tmp/audit/oblivion/py/psdt*.py` over `Oblivion.esm`, 7 209 PACK):
  - **Weekday byte.** 6 903 are -1. 153 name a single day (0:53, 1:14, 2:11, 3:20, 4:11, 5:15, 6:29). 153 name a group (7:52, 8:36, 9:35, 10:30).
  - **EditorIDs confirm the enum.**
    - Of the 83 single-day packages whose EditorID names a day (`…Mon…`, `…Sat…`, `…Sundas…`), 82 carry exactly that index.
    - 15 `…SS…` packages are 8. 24 `…MWF…` packages are 9. 22 `…TT…` packages are 10.
  - **Affected actors.** 158 `NPC_`/`CREA` bases list a weekday package. On all 158 it sits above at least one other package, so on a non-matching day it shadows the package below. Examples:
    - `SECaldanaFightTue20x2` / `Thu` / `Sat`: the Crucible fight club runs nightly.
    - `SEFellmoorRanarrjoStalkCindanweSS9x12` (weekend) sits above five `SEFellmooreFarming*9x12` weekday packages (7), so the stalking package wins every day and the farming never runs.
    - `SEMazaddaFightMon/Wed/Sat20x2` likewise.
  - **Date byte.** 75 packages author a date, such as the `ImpExRequest02`…`ImpExRequest27` family (12:00 for 5 h on the Nth of the month).
- **Impact**: Shivering Isles and Tamriel NPCs with weekly or monthly routines run the first-listed day variant every day, in its hour window. The intended activity for that day (farming, worship, home) is never reached in that window. This is behavioural, Oblivion-wide (about 2 % of actor bases) and persistent, but there is no crash and no data loss.
- **Related**: #6079 (FNV fix), ESM-2026-10-21-01 (Skyrim+ PSDT duration units, a different field), #3352 (zero-duration windows).
- **Suggested Fix**:
  1. Add `GameKind::Oblivion => Some(day_of_week(world, false, days_elapsed))` to `package_weekday`.
  2. Decode PSDT byte 2 into `PackSchedule` and test it against the calendar's day of month. `GameClockGlobals::start_date` already holds Oblivion's authored `GameDay`/`GameMonth`.
  3. Pin both with a test like `day_of_week_variants_select_by_the_game_calendar` that uses `GameKind::Oblivion`.

### Dim 4 — Exterior and lighting data

#### OBL-D4-2026-10-21-01: Oblivion's authored cloud colours (`NAM0` groups 2 and 9) are never read; every Oblivion cloud layer gets a constant white tint
- **Severity**: MEDIUM
- **Dimension**: 4 (TES4 WTHR decode → SKYAL cloud tint)
- **Location**:
  - `crates/plugin/src/esm/records/weather.rs:1-25` and `:36-55` (module doc and `SKY_UNUSED_2` / `SKY_UNUSED_9`: "Unused" for every game)
  - `:522-545` (the 160-byte `NAM0` arm fills `sky_colors` only)
  - `:696-703` (`PNAM`, the only writer of `cloud_layer_colors` outside the Creation-era path)
  - `:408-413` (white default)
  - consumers `byroredux/src/env_translate/sky_weather.rs:263-270`, `:407-411`, `byroredux/src/systems/weather.rs:505-514`, `crates/renderer/shaders/include/clouds.glsl:160-183`, `:575-597`
- **Status**: NEW. #6037 (closed) fixed the `PNAM` stride, and #6041 (closed) fixed the Oblivion `CNAM`/`DNAM` layer order and the `DATA` speeds. Neither touched `NAM0`'s cloud groups. A closed-issue search for WTHR cloud colours finds no Oblivion entry. This is not on `SUITE_FINDINGS.md`.
- **Description**: xEdit `wbWeatherColors` (`wbDefinitionsCommon.pas:9707-9725`) names `NAM0`'s groups by game:
  - **TES4**: group 2 is `Clouds-Lower` and group 9 is `Clouds-Upper` (`IsTES4(...)`).
  - **FO3**: group 2 is `Unused` and group 9 is `Clouds (Unused)`.
  - **FNV**: group 2 is `Unused` and group 9 is `Effect Lighting`.

  The parser's slot constants and module doc follow the FO3/FNV reading for every game, and nothing ever reads groups 2 or 9. Fallout and Creation-era games author cloud colours in `PNAM`. Oblivion authors no `PNAM`, so its `cloud_layer_colors` keep the `[255, 255, 255, 255]` default at every time of day. #6041 put Oblivion's lower deck (`CNAM`) on layer 0 and the upper deck (`DNAM`) on layer 1, so these two groups map directly onto those layers.
- **Evidence** (`/tmp/audit/oblivion/py/wthr.py` over `Oblivion.esm`): 37 / 37 WTHR author a 160-byte `NAM0`, and 0 / 37 author `PNAM`. Authored day / night cloud colours that are currently replaced by white:

  | Weather | Clouds-Lower (day / night) | Clouds-Upper (day / night) |
  |---|---|---|
  | `Clear` | (255,255,255) / (4,12,23) | (255,255,255) / (6,15,26) |
  | `Cloudy` | (239,237,237) / (14,27,44) | (255,255,255) / (14,22,35) |
  | `OblivionStormTamriel` | (185,162,151) | (241,156,99) |
  | `OblivionMountainFog` | (235,141,101) | (241,156,99) |
  | `SEClear` (lower deck at night) | — / (219,78,222) | |
  | `CamoranWeather` | (244,192,170) | (251,177,134) |

  In the shader, the tint sets the clouds' chroma (`cloud_authored_colour` → `mood_chroma`, `clouds.glsl:575-587`) and their alpha.
- **Impact**: Every Oblivion exterior draws neutral-white-chroma clouds. Lost as a result:
  - the blue-black night decks of `Clear` and `Cloudy`;
  - the orange decks of the Oblivion-gate storms (`OblivionStorm*`, the main-quest Kvatch and Great Gate skies);
  - the Shivering Isles mania and dementia tints.

  This is a visual fidelity loss on the title's signature skies. Nothing crashes or gets corrupted.
- **Related**: #6041, #6037, #5351 (the same file's open `OblivionHdrLighting` doc rot), SKYAL.
- **Suggested Fix**: In `parse_wthr`, under `GameKind::Oblivion`, copy `sky_colors[2][0..4]` into `cloud_layer_colors[0]` and `sky_colors[9][0..4]` into `cloud_layer_colors[1]`. Keep alpha at 255 unless the authored A byte is shown to be opacity. Rename the slot constants and doc per game (TES4 `Clouds-Lower`/`Clouds-Upper`; FNV slot 9 `Effect Lighting`). Pin it with a 160-byte fixture alongside `parse_wthr_oblivion_cloud_layers_and_data_speeds`.

### Dims 1, 3, 5 — no new findings

The verification behind each "no finding" result:

- **Dim 1.** 11 commits touched the Paths.
  - `a071ed217` (#5960) consumes the trailing `bhkConstraintMotorCInfo` only after the Oblivion early-return arm of `BhkConstraint::parse` (`constraints.rs:~770-810`, with the Tau/Damping trailer), so Oblivion constraints are untouched. The corpus confirms it.
  - `c2d3914bf` / `7869a93fd` / `6ad0c0214` (#5780–#5782) hardened the drift corpus. It ran green with them in place.
  - `ce08ec0cf` (#5798) and `f3380f6c8` (PlayGroup on embedded sequences) move animation, not parse; LC-2026-10-21-D2-01/-02 own their channel defects.
- **Dim 2.**
  - **#5778.** The census declines 1 quest and 9 INFOs. That is the fn-313 `IsActorEvil` gate, left uncatalogued on purpose. Fn 365 is Oblivion-only through `FOREIGN_INDICES`.
  - **#6077.** The REFR 0x800 decode is ungated (`walkers.rs:987`). `install_authored_disabled_references` walks interior, exterior and worldspace-persistent cells. It runs in `populate_scene_runtime` on every load, for every game.
  - **#5921.** ACBS Essential is bit `0x2` on TES4 `NPC_` and `CREA` alike (`wbDefinitionsTES4.pas:1955`, `:2792`).
  - **`c53d87679` LTEX `HNAM`.** The TES4 Havok material enum is identical to FO3's.
  - **#6043.** All 33 560 Oblivion `XCLC` are 8 bytes, so `land_flags` is 0.
- **Dim 3.**
  - #6100, #6078 and #6128 are fixed and pinned (see Pins).
  - #6130 restricts the Fallout hair mask. Oblivion keeps the diffuse-multiply path (`runtime.rs:788-800`).
- **Dim 5.**
  - `vanilla_oblivion_quest_scripts_execute_and_setstage` is green.
  - The `0x1000` MessageBox is now Oblivion-only (#6015).
  - Voice resolution survives the Skyrim `.fuz` work (#5912).
  - The menuxml BC-payload validation (`0b7501e70`) keeps the corpus census at zero undecodable headers.

## Cross-references (Oblivion evidence for findings already filed in this suite; not counted)

- **FNV-2026-10-21-02 (XRGD corpse poses dropped): Oblivion has the same two-layer drop.** Measured over `Oblivion.esm`: 153 `ACHR` (138 with 18 entries, 13 with 22, 2 with 17) and 618 `ACRE` carry `XRGD`, all on bases with the 0x80000 Starts Dead header bit. The other 4 `ACHR` sit on live bases. xEdit's TES4 definition does not list `XRGD`, but the master authors it.
  - The pose is parked only when `placed_ref.starts_dead` is set (`byroredux/src/cell_loader/references/mod.rs:775`). That flag is `Tes5Plus`-only (`walkers.rs:990-992`). Oblivion corpses are marked through `npc.starts_dead` (#5013), so the pose is never parked.
  - `pose_skeleton` (`byroredux/src/ragdoll_pose.rs:601-620`) would also refuse the Oblivion `Bip01` rig.
  - Add the Oblivion leg when that issue is filed.
- **FNV-2026-10-21-04 (restored hands skip the seam blend)**: applies to Oblivion too. `blend_part_seams` runs only at spawn (`runtime.rs:713`, `:1135`), and `seam_blend.rs` covers Oblivion, FO3 and FNV.
- **SCR-2026-10-21-06 / -07** (ObScript GameMode on stubs whose home cell is not loaded; `IsGuard` / `GetIsPlayerBirthsign` constants), **LC-2026-10-21-D2-01 / -02** (spawn-clip cycle type; `PlayGroup` channel sinks) and **AUD-2026-10-21-D1-01** (`PlayMusic` NaN attenuation): all reach Oblivion object and quest scripts. Their owners are the scripting, legacy-compat and audio audits.

## Still open (verified at HEAD, not re-filed)

- **#6098** (OBL-SD-03): placement LOD is keyed on the child worldspace only. `placement_lod.rs:450` still passes `wctx.worldspace_key`.
- **#5753**: Argonian and Khajiit spawn tailless. Nothing references `skeletonbeast.nif`, and the doc at `npc_spawn.rs:506` still says the beast rig "is handled separately (OBL-D3-01)".
- **#5351**: the `OblivionHdrLighting` doc rot in `weather.rs:114-174`, unchanged.
- **#5350** (closed as a doc fix, latent): `topic_entry` still has no Oblivion arm.
- **Navigation is PGRD-only**: `CellData.pathgrids` still has no consumer beyond the walker.

## Regression Guard List

Held at HEAD:
- the v10.x stride-drift family (#1506–#1509), through the corpus lane;
- the #170 BSStreamHeader dual band (corpus green; the unit pin is still one-sided, see OBL-D1-2026-10-14-04, INFO);
- the `NiTexturingProperty` u32 shader-map count (#149);
- the BSA v103 17-archive sweep, including the #5073 empty compressed placeholder;
- `parallax_alpha_gate_tests` and `oblivion_apply_hilight2_binds_the_derived_normal_as_its_height_map`;
- `placement_lod_supported_is_oblivion_only`;
- the `__MAX_Default_Light` gate (#5189);
- the #5487 body table and race skins;
- the #6100 idle and #6078 creature clip pins;
- the ACBS 16-byte arm (`race_oblivion` / `clas_oblivion_knight` real-data pins);
- the legacy PKDT/PTDT parse rate;
- the menuxml texture census (1 975 / 1 712 / 1 789).

Not checked here: the Disney-gate-stays-0 renderer pin (renderer audit), and every engine-side gate (`m-exteriors.sh oblivion …`, `p0-door-interaction.sh oblivion`, `m48-4-oblivion-hud.sh`, `/audit-runtime --game oblivion`), which the suite rules forbid.

## Open Work (from `ROADMAP.md` Known Issues; not blockers, since interiors and the Tamriel exterior render)

- Beast tails (#5753).
- Placement LOD in child worldspaces (#6098).
- A PGRD path graph for packages.
- The Oblivion known-topics dialogue model (#5350).
- No Oblivion leg in the dialogue or ObScript live gates: `dt1` and `m47-3` are FNV only.

## Out of scope / not run

| Item | Reason |
|---|---|
| Engine smokes (`m-exteriors.sh oblivion`, `p0-door-interaction.sh oblivion`, `m48-4-oblivion-hud.sh`), `/audit-runtime --game oblivion` | The suite rules forbid engine launches. Both findings rest on ESM measurements and code reading, not on a rendered frame. |
| `cargo test -p byroredux-plugin -- --ignored` (whole crate) | OOM rule; only named tests were run. |
| Re-measuring the `APPLY_HILIGHT2` / BC1 `_n.dds` census | No commit touched the Oblivion material path since 2026-10-14. |

Private cargo target dir: none (repo `target/`). Scratch: `/tmp/audit/oblivion/`.

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-10-21.md`. Labels:
- OBL-D2-2026-10-21-01: `game:oblivion`, `legacy-compat`, `ai`, `esm-plugin`, `medium`, `bug`.
- OBL-D4-2026-10-21-01: `game:oblivion`, `legacy-compat`, `terrain-exterior`, `esm-plugin`, `medium`, `bug`.
