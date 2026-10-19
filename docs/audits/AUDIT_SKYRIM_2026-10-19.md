**HEAD**: 2139ae20e · **Baseline**: `docs/audits/AUDIT_SKYRIM_2026-10-17.md` (HEAD `eec7e00cf`, 109 commits earlier; 16 of them touch the streaming-deep area) · **Audited**: Dim 3 (NPC equip + FaceGen), Dim 4 (multi-master load order + TES5 cell-load / XRGD / LOD lattice); scope=streaming-deep AREA-SCOPED · **Unchanged since baseline (skimmed)**: Dim 4's `load_order.rs` (4 lines, a Starfield-only call), the LSCR loading cover (`systems/loading_model.rs`, no commits). Dims 1, 2, 5 are out of area (NIF / shader / archive mechanisms).

**This report is AREA-SCOPED to the streaming-deep area, not a full-codebase Skyrim audit.** The next run must not treat it as a full baseline for Dims 1, 2 or 5; the 2026-10-17 report remains the full baseline for those.

Scope read: `byroredux/src/{streaming/,npc_spawn/,npc_spawn.rs,cell_loader/,streaming_helpers.rs,ragdoll_pose.rs}` through the Skyrim lens. Newest-first delta (`eec7e00cf..HEAD`): `2a8663f39` (#5408 head parts), `2139ae20e` (#5015 XRGD marker family), `cb5368348` (#2586 `.lod` lattice origin), `b9e60846e` (#5914), `b14a1a7ec` (#5920), `a5343c806` (#6051 script packages, ObScript, not a Skyrim path). Measurement was done statically against the installed SE data (`Skyrim.esm`, `Dawnguard.esm`, `Dragonborn.esm`, `HearthFires.esm`, `Update.esm`, `Skyrim - Meshes0/1.bsa`, `Skyrim - Textures0..8.bsa`, the CC BSAs) and LE (`Skyrim - Meshes.bsa`, `Textures.bsa`) with throwaway readers under `/tmp/audit/streaming-deep/skyrim/`.

---

## Executive Summary

The head-part work that landed today (#5408) is correct for what it was written for (the vanilla player) but exposes two upstream defects that now decide what ~40% of Skyrim's NPC base records look like. Both are in the pre-baked FaceGen lookup, not in the new fallback code:

- **SKY-SD-01 (MEDIUM, extends sibling FO4-SD-01)**: the facegeom/facetint path is built from the load-order-global FormID, but every shipped DLC and Creation Club file is named by the plugin-local id. 608 vanilla DLC facegeom NIFs (Dawnguard 225, Dragonborn 366, HearthFires 17) plus 81 CC ones can never resolve. Serana (`0x02002B6C`) asks for `dawnguard.esm\02002b6c.nif`; the archive holds `00002b6c.nif`.
- **SKY-SD-02 (MEDIUM, NEW)**: the facegeom path, the facetint path and the PNAM head-part list all key off the *placed shell* record, while race, gender, skeleton and WNAM already come from the "Use Traits" terminal. 2,053 of 5,118 `Skyrim.esm` `NPC_` records inherit Traits from a template, and only 2 of them ship a facegeom of their own (the template ships it). Every templated humanoid (bandits, soldiers, vampires, Forsworn, `TreasCorpse*`, and the guards that template through an `LVLN`) therefore misses and lands on the generic race head.

Everything else in the delta read clean: the XRGD marker-family change is conservative by construction, the #2586 lattice fix tiles all 12 SE worldspaces and the 7 LE `.lod` worldspaces, and the Markarth quad mismatch the commit mentions is exactly 238 stale `.btr` files that also have no texture sibling.

Not run (rule 4 / environment): control bench, DLC repro (`Forelhost01`), `m41-equip.sh`, `p6-loading-model.sh`, `sm1-story-manager.sh`, the NIF parse-rate corpus (Dim 5, out of area), `#[ignore]`d real-data tests (the plugin crate is forbidden; the one bin-crate ignored test `lod_descriptor_tiles_the_shipped_quad_names` could not be built, see "Checks I could not run").

| Severity | NEW | Existing / extends |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 1 | 1 (extends sibling FO4-SD-01) |
| LOW | 1 | 0 |

| ID | Severity | Title | Status |
|---|---|---|---|
| SKY-SD-01 | MEDIUM | Skyrim DLC / CC facegeom + facetint paths use the global FormID; 608 DLC (+81 CC) files never resolve | Extends FO4-SD-01 (Skyrim half; that report asks for it here) |
| SKY-SD-02 | MEDIUM | Facegeom, facetint and PNAM fallback key off the placed shell, not the Use-Traits terminal; templated humanoids get the generic race head | NEW |
| SKY-SD-03 | LOW | Doc rot after #5408 (PNAM-only wording in code, log line, docs) and in `tree_lod.rs` (#2586 described as open) | NEW |

---

## Dimension Findings

### MEDIUM

### SKY-SD-01: Skyrim DLC / CC facegeom and facetint paths are built from the load-order-global FormID; the shipped files are named by the plugin-local id
- **Severity**: MEDIUM
- **Dimension**: 3 (NPC equip + FaceGen), multi-master load order
- **Location**: `byroredux/src/npc_spawn.rs:1640-1649` (`prebaked_facegen_nif_path`), `:1655-1664` (`prebaked_facegen_tint_path`); callers `byroredux/src/npc_spawn/resumable/prebaked.rs:146-147` and `byroredux/src/cell_loader/references/mod.rs:726-729` (`plugin_for_form_id(child_form_id, load_order)` supplies the plugin, `npc.form_id` supplies the number); the unit test `byroredux/src/npc_spawn/tests.rs:455-475` pins `Dawnguard.esm` with `0x0001684C`, a value no global Dawnguard FormID has.
- **Status**: Extends sibling **FO4-SD-01** (`AUDIT_FO4_2026-10-19.md`: "File under `/audit-skyrim` as well for the Skyrim DLC half"). Same code, same root cause; this entry adds the Skyrim measurement. Not a separate fix. Open and closed issue search for "facegeom", "FaceGen DLC": only #5095/#5408 (head fallback) and #2095 (tint DDS), neither about the number.
- **Description**: `npc.form_id` is already remapped to the global space by the ESM reader (`crates/plugin/src/esm/reader.rs:857`, `remap_form_id`), so its top byte is the plugin's load-order slot. The CK names facegeom and facetint by the plugin-local id with the top byte stripped to `00`. For any NPC whose owner is not slot 0 the probe asks for a name that cannot exist, so `extract_mesh` always misses and the NPC goes down the head-part fallback.
- **Evidence** (static, SE install; scripts `/tmp/audit/streaming-deep/skyrim/{lib,npc_census}.py`, scratch):
  - `Skyrim - Meshes0.bsa` holds 3,158 facegeom NIFs: `skyrim.esm` 2,549, `dragonborn.esm` 366, `dawnguard.esm` 225, `hearthfires.esm` 17, `update.esm` 1. **All 3,158 have top byte `00`.** `Skyrim - Textures0.bsa` facetint has the identical distribution.
  - By local id (`fid & 0x00FFFFFF`) 225 of Dawnguard's 509 own NPCs, 366 of Dragonborn's 703 and 17 of HearthFires' 30 match a shipped file. For the standard order (Skyrim, Update, Dawnguard, HearthFires, Dragonborn) their global ids start `02`, `04`, `03`.
  - Concrete case: Serana is `DLC1Serana`, file-local `0x02002B6C`, shipped as `facegeom\dawnguard.esm\00002b6c.nif`; the engine asks for `dawnguard.esm\02002b6c.nif`.
  - Creation Club: `ccBGSSSE001-Fish.bsa` 34 and `ccBGSSSE025-AdvDSGS.bsa` 47 facegeom files, all `000008f6`-shaped. These plugins are light masters (global `FExxxyyy`), so they miss for the same reason.
  - The Dawnguard-style DLC repro in the skill (`--master Skyrim.esm --master Update.esm --esm Dawnguard.esm`) is exactly this shape. Not run (engine launch is forbidden here).
- **Impact**: Every DLC- or CC-owned authored NPC (Serana, Miraak, Neloth, Hearthfire adoptable children, the Fishing and Saints and Seducers NPCs) misses its authored face. Since `2a8663f39` it renders the generic race head (before that, headless). 608 + 81 faces and their tints lost; vanilla `Skyrim.esm` NPCs are unaffected, which is why the control bench never shows it.
- **Related**: FO4-SD-01 (same fix), SKY-SD-02 (same builder takes the wrong FormID for templated NPCs), #5095, #5408.
- **Suggested Fix**: Format the file name from the plugin-local part of the id (`global_slot_of(form_id)` already classifies Regular / Medium / Light; `scol_derived_cm_path` does the equivalent masking for `cm<local>`). Do it once in a shared helper used by both builders, and change the unit test to a real DLC global id (`0x0200_xxxx`) plus an ignored real-data gate that every shipped `facegeom\<plugin>\*.nif` name round-trips from a synthetic global id.

### SKY-SD-02: Pre-baked FaceGen, facetint and the PNAM fallback key off the placed shell record, not the "Use Traits" terminal
- **Severity**: MEDIUM
- **Dimension**: 3 (NPC equip + FaceGen), template chain (#4086 / #4457)
- **Location**: `byroredux/src/npc_spawn/resumable/prebaked.rs:146-147` (`facegen_path`, `tint_path` from `npc.form_id`), `:154` and `:182-243` (`prebaked_head_fallback_paths(npc, ...)` reads `npc.face_morphs` PNAM list of the shell); caller `byroredux/src/cell_loader/references/mod.rs:726-729` passes `plugin_for_form_id(child_form_id)`.
- **Status**: NEW. Searched open and closed: "facegeom template", "FaceGen templated", "Use Traits facegen" (#5359 is the WNAM skin, #4133/#4457 the TPLT hoist, #1658 inventory inheritance; none cover the face). `#4457`'s `ResolvedNpc` hoist and the pinning test `resolve_inherited_call_sites_are_enumerated_and_pinned` count only calls of `resolve_inherited_*`, so a field read straight off the shell (`npc.form_id`, `npc.face_morphs`) is invisible to the guard, the recurrence that issue was meant to end.
- **Description**: `prepare_prebaked_state` resolves the Use-Traits terminal (`resolved.r#traits`) and uses it for race, gender, skeleton path and `WNAM` skin. The three face inputs are read from the shell instead: the facegeom NIF path, the facetint DDS path, and the NPC's PNAM head parts. For an NPC whose `ACBS` template flags set Use Traits (0x01), the face belongs to the template ("Traits tab, including race, gender, height, weight, voice type... Character Gen tabs", UESP *Skyrim Mod:Mod File Format/NPC*, template flag 0x01), and the CK does not bake facegeom for the inheriting record.
- **Evidence** (static; scripts `npc_census.py`, `achr_census.py`, `lvln.py`, scratch):
  - `Skyrim.esm`: 5,118 `NPC_`; 2,053 have Use Traits set with a `TPLT`. **Only 2 of those 2,053 have their own facegeom.** The other 2,051 would need the terminal's.
  - Walking each chain to its terminal: 460 terminals are `NPC_` records with a shipped facegeom (humanoids), 475 are facegeom-less terminals (creatures), 1,118 chains end in an `LVLN` that `walk_inherited_records` resolves at spawn.
  - Placed references: of ~10,500 non-deleted `ACHR`, 7,757 point at a Use-Traits base. 351 of them resolve directly to a humanoid terminal with facegeom (`TreasCorpseBandit*`, `CWSiegeSons/ImperialArcher*`, `TG02Mercenary01`), 634 to a creature terminal, and 6,773 through an `LVLN`. Of the 2,738 NPC entries in the 527 `LVLN` lists, 651 are Use-Traits shells and 370 of those terminate on a record with authored facegeom.
  - After `#5408`, the miss is no longer headless: the fallback adds the race base head (`HEAD` of the traits race, e.g. `NordRace` male: `MaleHeadNord`, `MaleMouthHumanoidDefault`, `MaleEyesHumanLightBlue`, `HairMaleNord10`). So every bandit, guard and soldier of a race gets the same face and hair, while the template's authored face is never loaded. The shell's own PNAM list (432 Use-Traits records carry one) is ignored by the game for these actors, yet is what the fallback reads.
- **Impact**: Roughly 40% of NPC base records, including the `LVLN`-driven encounter population (by editor id: 285 of 528 `*Bandit*` records and 363 of 413 `*Guard*` records are Use-Traits; 135 bandit, 25 vampire and 18 Forsworn records end on an `NPC_` terminal with authored facegeom, and 353 guard records on an `LVLN` template), render a generic race head with no authored face, no facetint and the wrong hair. It is a visual, not a crash. It also means the #5408 fallback, designed for the one vanilla record without facegeom (the player), is the steady path for a large share of the world.
- **Related**: SKY-SD-01 (plugin segment must also come from the terminal's owner), CHAR-SD-01 (same family: a field read from the shell on one arm), #4457, #5359, #5408.
- **Suggested Fix**: Derive the facegeom id, the facetint id, the owning plugin segment and the PNAM list from `resolved.r#traits` (the terminal) instead of `npc`, and pass the load order (or a pre-resolved plugin name for the terminal) into `prepare_prebaked_state`, since `plugin_name` currently comes from the shell. Extend the call-site pin test to catch `npc.form_id` / `npc.face_morphs` reads in the prebaked arm. Add a unit test: a Use-Traits shell whose terminal has a facegeom resolves that path.

### LOW

### SKY-SD-03: Doc rot after #5408 and #2586
- **Severity**: LOW
- **Dimension**: 3 / 4 (documentation)
- **Location**: `byroredux/src/npc_spawn/resumable/prebaked.rs:23-32` (`head_fallback` doc: "the NPC's own PNAM head-part meshes... authored order"), `:36-40`, `:48-52` (log line "assembling the head from N authored PNAM head part(s)"), `:75`, `:336-339`; `byroredux/src/npc_spawn/resumable/mod.rs:297` (phase label "PNAM head-part fallback") and `:1456`; `byroredux/src/npc_spawn.rs:1169`; `byroredux/src/player_body.rs:36`; `docs/engine/playable-vertical-slice.md:1305`; skill text `.claude/commands/audit-skyrim/SKILL.md` Dim 3 ("loading the NPC's authored `PNAM` HDPT meshes in authored order"). Separately `byroredux/src/cell_loader/tree_lod.rs:52-55` ("the baked-LOD lattice origin is an open problem for the other rings (#2586 ...)").
- **Status**: NEW
- **Description**: `#5408` changed the fallback to race base parts for the NPC's gender, minus the types the NPC authors, plus the NPC's own parts, plus depth-first `HNAM` extras. Every comment, the phase label, the `info!` log line and two docs still describe the PNAM-only list, and `skip_missing_facegen`'s "only an NPC with neither source continues headless" is now "neither race head parts nor PNAM". `tree_lod.rs` still calls #2586 an open problem; `cb5368348` closed it today (all `.btr`/`.bto` tile from the `lodsettings\<ws>.lod` cell), and the tree ring's by-name selection no longer needs the justification.
- **Impact**: Misleads the next reader debugging a head (the log line says "authored PNAM" for a race-derived set).
- **Related**: SKY-SD-02 (the real behavior of this path), EXT-D2-05 (a different doc-rot entry).
- **Suggested Fix**: Reword the six code sites and two docs to "head-part fallback (race base parts + NPC parts + extras)"; refresh the `tree_lod.rs` paragraph and the audit-skyrim SKILL Dim 3 sentence.

---

## Verified Holding (no finding)

- **#5408 decode is right on real data.** An independent parse of `Skyrim.esm` finds 32 of 99 `RACE` records carrying a Head Data `HEAD` list, in the shape the commit tests (`NAM0`, `MNAM`/`FNAM`, `INDX`+`HEAD`). `NordRace` male: hair `HairMaleNord10`, face, mouth, eyes; female adds brows. `DraugrRace` and the creature races carry none, so Draugr and animals are unaffected (no double head). `HDPT` `PNAM` types (0 Misc, 1 Face, 2 Eyes, 3 Hair, 6 Brows) match the part names. The cycle guard (`seen` set) and the per-type replacement are covered by `prebaked_head_fallback_merges_race_base_parts_with_the_npcs_own`. Only soft spot: a Misc-type part the NPC authors replaces the race's Misc part (the mouth); the commit documents this as an interpretation, and the one vanilla record that exercises the fallback (the player) authors only a hair.
- **#2586 / `cb5368348` correct on SE and LE.** SE: all 12 worldspaces with `lodsettings\<ws>.lod` tile their `.btr`/`.bto` names from the descriptor's SW cell with **0 off-lattice** (Tamriel (-96,-96) 3,777 quads; Solstheim (-64,-64) 3,136; Apocrypha (-50,-50) 1,063; Soul Cairn (-52,-51) 944; ...), except Markarth (238 of 286 off-lattice). Those 238 are exactly the 238 `.btr` with no diffuse or `_n` sibling in `Textures*.bsa`, i.e. stale leftovers the engine never loads, so the commit's reading is right. LE: 7 `.lod` worldspaces, same result, and `labyrinthianmazeworld` (no `.lod`) tiles from the `NAM0` fallback (-1,-3). `lod_grid_origin` is resolved once per `WorldStreamingState` via `extract_mesh_exact("lodsettings\\<key>.lod")`, which does not prefix `meshes\`; that matches where the file lives (`Skyrim - Meshes0.bsa`).
- **LOD-inheriting children (#5496) are sound for Skyrim.** `WhiterunWorld`, `WindhelmWorld`, `SolitudeWorld`, `RiftenWorld` set the PNAM Use-LOD bit (flags `0x7F`/`0x77`) and their cell ranges sit on Tamriel's grid (WhiterunWorld x[-3..16] y[-12..15]); `MarkarthWorld` (`0x44`) owns its LOD and its `.lod` cell (-48,-3) equals its cell minimum.
- **XRGD marker family (`2139ae20e`) is conservative.** `is_trailing_marker` needs `bone_id == 0`, `x ~ 0`, a nonzero `(y, z)` and a zero rotation; a real body that happened to match would drop the entry count to `bodies - 1` and be refused, never mis-posed. Non-humanoid rigs still fail `is_humanoid_ragdoll`. `ragdoll_pose` unit tests pass. #5015 stays open for creature rigs, `SkeletonS` and FO4 (as the commit says).
- **Load order.** `load_order.rs` changed by one Starfield-only call since the baseline. Slot allocation (Regular / Light / Medium), the parallel walk with ordered fold and the dependency edges are unchanged; `plugin_for_form_id` uses `global_slot_of`. The single-`--master` 404 (#2583) regression guard is untouched.
- **Tree LOD (`tree_lod.rs`).** `.btt`/`.lst` parsers bound every count against the bytes present; quads selected by name; the ring respects the full-detail footprint. LE ships 4 `.lst`, 353 `.btt`. The only issue is a capacity-refused upload cached as a permanent miss, already filed as EXT-D6-01.
- **Delta commits outside the Skyrim lens.** `b9e60846e` (#5914 class/level pool term) and `b14a1a7ec` (#5920) belong to `/audit-character`; `a5343c806` (#6051) and the ObScript commits are the FNV/Oblivion script-package path; `seam_blend` is Oblivion/FO3/FNV only (its module doc says Skyrim hides the seam with baked heads).

## Shader-Type Coverage Matrix
Out of area (Dim 2 is the BSLightingShaderProperty parser/import/render mechanism, not streaming-deep code). The 2026-10-17 matrix stands; the one delta (type 16 `EyeEnvmap` shaded, #5961) is a renderer change that did not touch this area.

## Cell-Load Regression Status (static only)
- TES5 cell load through the unified walker: no commit to `cell/` walkers in the area beyond doc pointers (`61b9c5066`), `stamp_exterior_cell`/`note_player_entered_cell` additions (M47.3, game-clock and script state, no spawn-path change for Skyrim) and the #2586 lattice read. Whiterun BanneredMare entity count and FPS: **not measured** (no engine launch). The head fallback (SKY-SD-02) does not change BanneredMare's named NPCs, which all author their own facegeom.
- Unit runs in the repo `target/` (no private target dir): `cargo test -p byroredux --bin byroredux -- ragdoll_pose prebaked_head lod_settings tamriel_origin missing_lod_descriptor inheriting_child pre_skyrim_lod facegen npc_spawn` 177 passed, 13 ignored; `cargo test -p byroredux-plugin --lib -- skyrim_race_base_head_parts parse_hdpt` 3 passed.

## Checks I could not run
- Engine legs (control bench, `Forelhost01` DLC repro, `m41-equip.sh skyrim`, `p6-loading-model.sh`, `sm1-story-manager.sh`): forbidden by rule 4.
- `lod_descriptor_tiles_the_shipped_quad_names` (bin-crate ignored test): the build failed in the working tree's in-progress `crates/nif` change (`ImportedRagdollBody` missing `pose_bone_id`, `crates/nif/src/import/collision/ragdoll.rs:116`), not in anything this audit touched. The Python reimplementation above reproduces the test's assertion on SE and LE.
- Plugin `--ignored` real-data tests: forbidden (OOM).
- Dims 1, 2, 5 (NIF parse-rate corpus, shader dispatch, archive sweep): out of area.
