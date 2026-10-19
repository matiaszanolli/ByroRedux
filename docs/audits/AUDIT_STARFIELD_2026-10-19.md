**HEAD**: 2139ae20e · **Baseline**: docs/audits/AUDIT_STARFIELD_2026-10-12.md (HEAD 5d31fd637) · **Audited**: Dim 4 (area part: load order and FormID slots, REFR/PKIN spawn gates, NPC spawn for Starfield), Dim 2 and Dim 3 (area contact points only); scope=streaming-deep AREA-SCOPED · **Unchanged since baseline (skimmed)**: Dims 1, 5, 6 (out of area; guards run where cheap)

**THIS REPORT IS AREA-SCOPED (streaming-deep: `byroredux/src/streaming/`, `npc_spawn/`, `cell_loader/` plus the callers they need), NOT A FULL-CODEBASE STARFIELD AUDIT.** The next full run must not treat it as a baseline for BA2 / NIF shader / CDB reader / material-flow work. The 2026-10-12 report remains the full baseline.

# Starfield Compatibility Audit — 2026-10-19 (streaming-deep)

One leg of `--area streaming-deep`. One auditor, no sub-agents. Scratch: `/tmp/audit/streaming-deep/starfield/` (probe scripts in `probe/`, run with `python3 -I`, read-only `mmap` over the installed Starfield, plus one Fallout 4 RACE read).

## Executive Summary

**0 CRITICAL, 2 HIGH, 2 MEDIUM, 1 LOW (all NEW; two extend sibling findings).**

The vanilla bring-up surface (Starfield.esm alone, regular master, walkable Cydonia) is unchanged and its guards are green. The new findings are on the other side of the area contract: what the loader does when Starfield's real data differs from the Skyrim / FO4 conventions the shared code was written against.

- **SF-SD-01 (HIGH)**: Starfield stores a small or medium master's own FormIDs, and every reference to one, in the `0xFE` / `0xFD` space with a file-local index. It indexes regular masters among regular masters only. `FormIdRemap::remap` treats the top byte as an index into the whole `MAST` list (the Skyrim / FO4 convention). Any load order with two or more small masters collides: 61,477 of 67,217 distinct own FormIDs across the 18 official small and medium masters are shared by two or more plugins.
- **SF-SD-02 (HIGH)**: Starfield `NPC_` carries a 16-byte `ACBS`. The parser has arms for Oblivion (16 B, gated to Oblivion), FO4 (gated to Fallout4, `>= 20`), Skyrim and FNV (`>= 24`). Nothing accepts Starfield's, so every Starfield NPC has `acbs_flags = 0` and `template_flags = 0`. 4,219 of 9,530 placed actors (44%) are female and spawn male. 6,326 are "Use Traits" templated, and 8,105 carry some template flag that never fires.
- **SF-SD-03 (MEDIUM)**: the RACE skeleton path (`ANAM`) is read for Skyrim only, and a unit test pins that. Starfield (and FO4 / FO76) races all carry it. Every non-human Starfield actor, and every female, spawns on the male human `skeleton_facebones.nif`.
- **SF-SD-04 (MEDIUM, extends FO4-SD-01 / SKY-SD-01)** and **SF-SD-05 (LOW, extends FO4-SD-03)**: the pre-baked FaceGen path uses the global FormID. Starfield's DLC FaceGen files are named by the plugin-local id. There are no `facetint` textures in any Starfield archive.

Two earlier worries are disproved and are recorded under "Checked and not filed": the ESL / medium slot *allocation* is correct (#4639), and the SCOL combined-mesh path already uses the plugin-local id.

**NPC spawn is not stubbed on Starfield.** `references/mod.rs:724` routes Starfield through `NpcSpawnJob::prebaked`, with the `skeleton_facebones.nif` rig. Cydonia's `citycydoniamainlevel` has 75 placed actors: 66 `HumanRace`, 7 `ModelARace` and 2 `MiniBotARace`. 29 are female (SF-SD-02) and 9 are non-human robots (SF-SD-03).

## Guard runs

| Suite | Result |
|---|---|
| bsa `--lib ba2` | 44/44 |
| sfmaterial `--lib` | 51/51 |
| nif `--lib bs_geometry` | 72/72 |
| plugin `--lib starfield` | 22 passed, 1 ignored |
| byroredux bin: `load_order`, `starfield_mat`, `asset_provider::material`, `pkin`, `normalize_mesh_path`, `light_anim`, `scol`, `humanoid_skeleton`, `prebaked_facegen`, `npc_spawn::tests` | 97 + 118 passed (two runs), 0 failed |

The default toolchain built the bin crate despite the uncommitted `crates/nif` edit. No private cargo target dir was used.

## Dimension Summaries

| Dim | Verdict |
|---|---|
| 1 BA2 + corpus | out of area; `ba2` guards green; corpus gate not re-run |
| 2 BSGeometry | area contact only: the streaming worker resolves external `.mesh` through the immutable `TextureProvider` (`pre_parse.rs:284-344`). That path is idle on Starfield, which has no exterior grid (`terrain.rs:808`, `lod_support.rs:555`). The `ParseInputBudget` still excludes Starfield external meshes (`pre_parse.rs:425`; CONC-SD-02 sibling). Only the first usable LOD slot is imported (`bs_geometry.rs:80`), so the pack-in fan-out does not multiply colliders by LOD count |
| 3 CDB | area contact only: provider rebuild per door keeps the CDB byte cache and index module-scope (`app_step.rs:1140-1175`); guards green |
| 4 ESM + bring-up | SF-SD-01 .. SF-SD-05; spawn gates intact |
| 5 shader blocks | out of area |
| 6 material flow | out of area |

---

## Findings

### HIGH

### SF-SD-01: Starfield small / medium master FormIDs use a file-local `0xFE` / `0xFD` encoding the remap treats as a master-list index; multi-plugin loads collide and mis-slot
- **Severity**: HIGH (silent wrong-record merging under a realistic load order; workaround is to load only regular plugins)
- **Dimension**: ESM + cell bring-up, load order
- **Location**:
  - `crates/plugin/src/esm/reader.rs:541-585` (`FormIdRemap::remap`; owner `/audit-esm`);
  - `byroredux/src/cell_loader/load_order.rs:370-410` (`build_remap_for_plugin` builds `master_slots` positionally from the `MAST` order; in area);
  - downstream in area: `load_order.rs:351` (`plugin_for_form_id`) and the `GlobalFormIdResolver` identity built from the same slots.
- **Status**: NEW. #4639 (closed) fixed the TES4 flag bits and slot *allocation*. It did not look at how Starfield addresses those slots inside the files. The 2026-10-19 ESM report reviewed the `remap` arms and passed them (Skyrim ESL semantics); it did not check Starfield data.
- **Description**: `remap` decodes `top byte == master count` as "self" and `top byte < master count` as `master_slots[top byte]`, the Skyrim SE / FO4 rule that `form_id_remap_esl_plugin_own_forms_decode_into_light_space` pins. Starfield's real files follow a different rule:
  1. A small master's own records are `0xFE` + a 12-bit field + a 12-bit object id. The 12-bit field is the number of small masters in the plugin's own `MAST` list.
  2. A medium master's own records are `0xFD` + an 8-bit field (the count of medium masters in its `MAST` list) + a 16-bit id.
  3. A reference to a small or medium master uses the same `0xFE` / `0xFD` form, with the master's ordinal among the small (or medium) masters of this file.
  4. A regular plugin's top byte indexes regular masters only. Its own records use the count of regular masters, and overrides of a regular master use that master's regular ordinal, skipping small and medium entries.
  5. As a result, `remap` mis-routes every `0xFE` / `0xFD` FormID. The top byte exceeds the master count, so it takes the "out-of-range" arm and passes through unchanged with a `log::warn!`, whatever the plugin's real slot is.
  6. For a regular plugin with a small master in its `MAST` list, `remap` mis-slots even the regular references: the top byte is a regular ordinal but is used as a `MAST` position.
- **Evidence** (the install under `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data`; probes `probe/fidstat.py`, `own.py`, `refs.py`, `refs2.py`, `enc.py`, `ovr.py`, `collide.py`):
  - **Own records**:
    - `Constellation.esm` (small, masters `[Starfield]`): `0xFE000002`, `0xFE00001A` .. (sub field 0).
    - `SFBGS019.esm` (small, masters `[Starfield, SFBGS004(small)]`): all 29 records `0xFE001xxx` (sub 1).
    - `SFBGS047.esm` (small, masters `[Starfield, SFBGS004(small), SFBGS00D(regular)]`): all 40 own records `0xFE001xxx` (sub 1).
    - `SFBGS003.esm` and `SFBGS006.esm` (medium): 64,384 and 60,890 records, all `0xFD00xxxx`. The NPC `0xFD000E90` in `SFBGS003.esm` has its FaceGen file named `00000e90.nif`.
  - **References**: `SFBGS047.esm` `KWDA` entries `0xFE0002A5` and `0xFE000017` resolve to `KYWD` records in `SFBGS004.esm` (its first small master, ordinal 0), not in `Constellation.esm`. `outpostmissionboards.esm` (masters `[Starfield, BlueprintShips, SFBGS003(medium)]`) references `0xFD000050`.
  - **Regular-ordinal rule**: `SFBGS047.esm`'s two override records of `SFBGS00D.esm` (`WRLD 0x010079E3`, `CELL 0x0100873F`) have top byte `0x01`. `MAST` position 2 holds `SFBGS00D`, and both low-24-bit ids exist in `SFBGS00D.esm`. `better_living.esm` (regular, masters `[Starfield, BlueprintShips, SFBGS004(small)]`) writes its own 12,217 records with top byte `0x02`, while `master_slots.len()` is 3: `remap` sends them to `master_slots[2]`, which is `SFBGS004`'s light slot, and `compose` truncates the id to 12 bits.
  - **Collision measurement** (`collide.py`, 22 official plugins, 18 of them small or medium): pass-through leaves 67,217 distinct `0xFE` / `0xFD` FormIDs, and **61,477 of them are claimed by two or more plugins**. For example `0xFE000002` is a record in 13 plugins. `EsmIndex::merge_from` keeps the later record, so one plugin's record silently replaces another's.
  - **Code path**: `reader.rs:541` (the `mod_index == len` / `< len` / `is_empty()` / out-of-range chain, ending in the `log::warn!` and `return raw`); the record header's `form_id` goes through `remap_form_id` (`reader.rs:857`). Each record header therefore logs one warning line on a small / medium plugin load.
- **Disproof attempted**:
  1. Does a Starfield-specific pre-pass normalize the encoding? None. `FormIdRemap` has only `plugin_slot` and `master_slots`.
  2. Is the `0xFE` ids' 12-bit field a global slot that already equals the assigned `Light(n)`? No. Several plugins share sub 0 and sub 1 (the collision table above), so it is file-local.
  3. Does this hit the supported vanilla path? No, and it is not claimed to. `Starfield.esm` alone, and the regular DLCs (`ShatteredSpace`, `SFBGS00D`, `SFBGS050`, `BlueprintShips-*`), have only regular masters and are untouched. The first small master loaded alone also works, by luck (`Light(0)` composes to the same bits).
  4. What loads small masters: `--master Constellation.esm --master SFBGS004.esm ...` (any two) or a Creations-style order. The install also holds many third-party plugins flagged small (`0x101`) or medium.
- **Impact**: any Starfield load order with two or more small (or two or more medium) masters merges distinct forms onto one global FormID (record, NPC, REFR, KYWD, GLOB ...). Cross-plugin references and quest / script identity resolve to the wrong record. `plugin_for_form_id` then names the wrong plugin, which feeds FaceGen paths (SF-SD-04). A regular plugin with a small master in its list loses its own records into that master's slot. The log fills with a `FormID ... has mod_index 254 but plugin has N masters` warning per record.
- **Related**: #4639, ESM-SD-03 (a separate top-byte consumer; not reachable on Starfield, see "Checked and not filed"), FO4-SD-01 / SKY-SD-01 (path from the global id), SF-SD-04.
- **Suggested Fix**: give `FormIdRemap` a Starfield decode. Partition the `MAST` list into regular / small / medium ordinals. Route a `0xFE` top byte by `(raw >> 12) & 0xFFF` (ordinal among the file's small masters, equal to the small-master count means self when the plugin is small) and `0xFD` by `(raw >> 16) & 0xFF`, with the object id masked to 12 / 16 bits. Index a regular top byte among regular masters only, with self at the regular-master count. Pin it with fixtures built from the `SFBGS019` / `SFBGS047` / `better_living` shapes above, and add a default-lane test that loads two synthetic small masters and asserts no shared global id.

### SF-SD-02: Starfield `NPC_` `ACBS` (16 bytes) matches no parse arm; every Starfield NPC has `acbs_flags = 0` and `template_flags = 0`
- **Severity**: HIGH (wrong gender, no template inheritance, no essential / unique flags on 100% of Starfield NPCs; the data is there and is dropped)
- **Dimension**: ESM + NPC spawn (the consumer in area)
- **Location**:
  - `crates/plugin/src/esm/records/actor/npc.rs:772` (Oblivion, `GameKind::Oblivion`, `>= 16`), `:792` (FO4, `GameKind::Fallout4`, `>= 20`), `:821` (Skyrim, `>= 24`), `:836` (FNV / FO3, `>= 24`);
  - area consumers: `npc_spawn/resumable/prebaked.rs:102` (`Gender::from_acbs_flags(traits.acbs_flags)`), `cell_loader/references/mod.rs:704-722` (race from `resolve_inherited_traits`, which keys on `template_flags`), `npc_spawn.rs:517` (`npc_skeleton_path`).
- **Status**: NEW. Searched open and closed issues for "Starfield ACBS", "Starfield NPC ACBS template flags", "Starfield NPC gender female", "FO76 ACBS": nothing. Related but not the same: CHAR-SD-01 (gender read from the shell record instead of the Use-Traits terminal; that finding assumes `acbs_flags` is decoded).
- **Description**: Starfield's `NPC_.ACBS` is the FO4 layout minus the trailing `bleedout_override` and `unknown` words: `flags(u32), xp_offset(i16), level(i16), calc_min(u16), calc_max(u16), disposition(i16), template_flags(u16)`, 16 bytes. The FO4 arm needs `GameKind::Fallout4` and 20 bytes. No arm matches Starfield, so `acbs_flags`, `level`, `calc_min`, `disposition_base` and `template_flags` keep their defaults.
- **Evidence** (`probe/acbs.py`, `tpta.py`, `cyd.py` over `Starfield.esm`):
  - All 7,131 `NPC_` `ACBS` are exactly 16 bytes.
  - Template-flag offset: of 45,738 non-zero `TPTA` entries on 5,168 NPCs, the bit `i` of the `u16` at `ACBS+14` is set for every one of them, at no other offset.
  - Flag bit 0 is the FO4 female bit: 2,021 of 7,131 `NPC_` records.
  - **Placed actors** (9,530 `ACHR`, all resolve to an `NPC_`): 4,219 female (44%), 6,326 Use Traits, 7,321 Use Stats, 5,516 Use Inventory, 8,105 any template flag.
  - **Cydonia** (`citycydoniamainlevel`, 75 `ACHR`): 29 female.
  - The unit test at `actor/tests.rs` (`parse_race` / `parse_npc`) has no Starfield `ACBS` case.
- **Impact**: every female Starfield NPC spawns male (skeleton, race default skin, equip gender filter). Templated actors (the majority) do not inherit race / traits, stats, factions, AI or inventory, so the shell record's own, often incomplete, values are used. The essential / unique / protected `acbs_flags` bits are 0, so essential-actor bleedout (M47.2) never sees Starfield's essential NPCs.
- **Secondary (unverified, not claimed)**: Starfield's `TPTA` is 60 bytes (15 template slots; entries 13 and 14 are non-zero on 2,528 and 3,759 NPCs), while the parser stores only the first 13 under the FO4 `wbTemplateFlags` order (`npc.rs:757-760`). Whether the first 13 keep FO4's meaning is not established here (no xEdit SF1 definitions available locally); do not guess it. Settle it against the definitions when the ACBS arm is fixed.
- **Related**: CHAR-SD-01, #5498 (TPTA), #4639.
- **Suggested Fix**: add Starfield to the FO4 `ACBS` arm with a `>= 16` length gate (template flags at `+14`, level at `+6`). Add a Starfield `ACBS` fixture test (female bit, Use Traits). Settle the 15-slot `TPTA` layout separately.

### MEDIUM

### SF-SD-03: RACE skeleton path (`ANAM`) is read for Skyrim only; every non-human Starfield actor and every female spawns on the male human rig
- **Severity**: MEDIUM
- **Dimension**: NPC spawn for Starfield
- **Location**: `crates/plugin/src/esm/records/actor/race.rs:611` (`b"ANAM" if game == GameKind::Skyrim && in_skeleton_section`); the pin `actor/tests.rs:36-41` (asserts `skeleton_models == ["", ""]` for Fallout4, Fallout76 and Starfield); consumer `byroredux/src/npc_spawn.rs:517-531` (`npc_skeleton_path` falls back to `humanoid_skeleton_path`, `:498`).
- **Status**: NEW (Starfield; FO4 and FO76 share the gate, see below). Searched "skeleton", "creature", "race": nothing.
- **Description**: with `skeleton_models` empty, `npc_skeleton_path` always returns `meshes\actors\human\characterassets\skeleton_facebones.nif` for Starfield. Starfield `RACE` carries the real path in `ANAM`.
- **Evidence** (`probe/race.py`):
  - 31 of 31 Starfield `RACE` records carry `ANAM`: `HumanRace` has `Actors\Human\CharacterAssets\skeleton.nif` (male) and `...\Female\skeleton.nif`; `ModelARace` has `Actors\ModelA\CharacterAssets\Skeleton.nif`; also `LaserTurretRace`, `MiniBotARace`, `CritterRace`, `QuadrupedARace`, `SecurityCameraRace` and others.
  - Those skeletons ship: `meshes/actors/modela/...`, `laserturret/...`, and `human/characterassets/female/skeleton_facebones.nif` all exist in `Starfield - Meshes01.ba2` / `Meshes02.ba2`.
  - Placed-actor races (9,530 `ACHR`): `HumanRace` 7,096, `CCT_DummyRace` 795, `ModelARace` 402, `HumanCorpseRace` 346, `LaserTurretRace` 272, `ParasiteARace` 133, `SecurityCameraRace` 120, `MannequinRace` 90, `MiniBotARace` 73, `CritterRace` 50, `ChildRace` 39, `ModelSRace` 39, `BallisticTurretRace` 32, `TerrormorphRace` 25. So roughly 1,300 non-human placements (turrets, robots, creatures) are bound to a human rig. Cydonia: 9 of 75.
  - **FO4 shares it**: all 45 `Fallout4.esm` `RACE` records carry `ANAM` (Dogmeat `actors\Dogmeat\...`, ghouls, turrets, the alien); the same gate empties them.
- **Impact**: non-human actors skin to the wrong skeleton (bone-name misses), so their bodies are mis-posed or collapsed. Females use the male rig even once SF-SD-02 is fixed.
- **Related**: SF-SD-02, #5753 (Argonian / Khajiit body-section tail, another per-race RACE section gap).
- **Suggested Fix**: widen the `ANAM` arm to the games whose `RACE` carries it (FO4 / FO76 / Starfield, verified here for FO4 and Starfield), and resolve a Starfield human `skeleton.nif` to its `_facebones` sibling when that exists (the comment at `npc_spawn.rs:505-509` requires the face joints). Update the test pin.

### SF-SD-04: Starfield DLC FaceGen NIFs are named by the plugin-local id; the path is built from the load-order-global FormID (extends FO4-SD-01 / SKY-SD-01)
- **Severity**: MEDIUM (275 vanilla DLC FaceGen NIFs never resolve; matches the sibling's rating basis)
- **Dimension**: NPC spawn for Starfield
- **Location**: `byroredux/src/npc_spawn.rs:1641` (`prebaked_facegen_nif_path`) called from `npc_spawn/resumable/prebaked.rs:147` with `npc.form_id`; plugin name from `cell_loader/references/mod.rs:726` (`plugin_for_form_id`).
- **Status**: Existing, extends FO4-SD-01 and SKY-SD-01 (this audit's counts for Starfield; the fix is shared). The same helper already exists on the SCOL side and is the model: `cell_loader/refr.rs:777-784` (`scol_derived_cm_path` strips to the plugin-local id).
- **Evidence** (`probe/ba2names.py` listing the archives' names):
  - Starfield FaceGen files: `meshes/actors/character/facegendata/facegeom/<plugin>.esm/<local 8-hex>.nif`.
  - `Starfield - FaceMeshes.ba2`: 1,282 (the base plugin, whose slot byte is 0, so global equals local: resolves).
  - DLC files: `shatteredspace.esm` 135, `sfbgs00d.esm` 62, `sfbgs003.esm` 41 (medium; local `00000e90`, global `0xFD000E90`), `sfbgs050.esm` 32, `sfbgs00c.esm` 5 (small; local `00000027`): **275 vanilla DLC files**, plus `kgcdoom.esm` 6.
  - A regular DLC NPC is probed as `<slot byte><local 6 hex>.nif` (for example `0b00421f.nif` for a `ShatteredSpace` NPC in slot `0x0B`), a medium one as `fd000e90.nif`, a small one as `fe00....nif`: none of these names exist.
- **Impact**: the DLC NPCs fall back to the race / NPC head-part fallback (`prebaked_head_fallback_paths`). Starfield races carry `HEAD` base parts in only 4 of 31 records, so the fallback is thin.
- **Related**: SF-SD-01 (a wrong global id also names the wrong plugin), SKY-SD-02 (shell-versus-template keying; on Starfield 7,108 of the 7,904 placements without their own FaceGen carry a `TPLT`, but only 176 of those templates have one).
- **Suggested Fix**: as FO4-SD-01 states: derive the file name from the plugin-local id, reusing the `scol_derived_cm_path` decode.

### LOW

### SF-SD-05: `prebaked_facegen_tint_path` is a guaranteed miss on Starfield (extends FO4-SD-03)
- **Severity**: LOW
- **Dimension**: NPC spawn for Starfield
- **Location**: `byroredux/src/npc_spawn.rs:1656` (`prebaked_facegen_tint_path`), `npc_spawn/resumable/prebaked.rs:148`.
- **Status**: Existing, extends FO4-SD-03.
- **Evidence**: listing the names of every `.ba2` archive in the Starfield `Data` folder: none contains `facetint` (`probe/ba2names.py`, grep `-i facetint`). Starfield tints faces another way.
- **Impact**: one failed archive lookup per prebaked NPC; nothing tinted.
- **Suggested Fix**: gate the tint lookup per game, as FO4-SD-03 proposes.

---

## Extension of a sibling (not a new finding)

**PERF-SD-01 on Starfield**: `docs/engine/memory-budget.md` gives the Starfield index at about 4.1 GB steady and a 5.3 GB peak for the 14-plugin parse (7.72 s). A door transition that re-parses while the previous `LoadedCellIndex` is resident would hold the two together. This is derived from those figures, not measured. If the previous index is dropped before the new parse, the transient is just the 5.3 GB peak. Either way, the 7.7 s main-thread stall is per door on Starfield; the fix PERF-SD-01 proposes (reuse the resident `Arc<EsmIndex>`) is worth more here than on any other game.

## Checked and not filed

- **ESL / medium slot allocation** (`allocate_global_slot`, `GlobalSlot::compose`, `read_file_header` flag decode): correct for Starfield's `0x100` / `0x400` bits, and the `0xFC` regular ceiling once a medium master exists. Only the on-disk *decode* is wrong (SF-SD-01).
- **ESM-SD-03** (`resolve_precombine_owner` keys on the FormID top byte): precombined `_oc.nif` bakes are an FO4 mechanism; Starfield's own cell ingestion goes through PKIN template cells (#5489). Not re-measured through zlib-compressed `CELL` records, so "not reachable" rests on the FO4-only design in `precombined.rs`, not a census.
- **Static-trimesh fallback gate** (`mesh_instance.rs:1495-1545`): still keyed on `base_layer` (#1294); the synthesized ghost carries no `MeshHandle`.
- **PKIN expansion** (`refr.rs:520-700`): the template CELL instancing is intact, bounded by `MAX_PKIN_DEPTH = 4`, and each child is a budgeted unit (`references/mod.rs`, `budget.should_yield()` per synth child).
- **Starfield skeleton asset**: `skeleton_facebones.nif` ships in `Starfield - Meshes01.ba2`; the `humanoid_skeleton_path` pin holds.
- **Exterior streaming for Starfield**: unsupported by design (no `LAND`, no baked LOD: `terrain.rs:808`, `lod_support.rs:548-570`, `object_lod.rs:1310`), so the streaming worker, pre-parse and LOD rings are idle on Starfield.

## Remaining-Work Chain

Unchanged from the 2026-10-12 report. Add two items ahead of NPC work on Starfield: the multi-plugin FormID decode (SF-SD-01) and the `ACBS` arm (SF-SD-02). SF-SD-03 and SF-SD-04 follow.

## Not checked (and why)

- No engine, `--sf-smoke`, smoke script, GPU or bench leg (suite rule 4), so a walkable-Cydonia re-verification and any live door-transition timing were skipped.
- The `parse_rate_starfield_all_meshes` corpus gate and the `real_cdb.rs --ignored` gates were not re-run (not area; the 2026-10-12 run stands).
- Plugin-crate `--ignored` tests were not run (OOM rule).
- Starfield `TPTA` slot order beyond the first 13 entries and the Starfield `HEAD` / HDPT appearance model are not settled (no xEdit definitions locally).
- A multi-plugin Starfield load was not executed, so SF-SD-01's collisions are established from the file bytes plus a static read of `remap`, not from a live run.

Suggest: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-10-19.md`
(label every finding `game:starfield` + `legacy-compat`, plus its own domain label: SF-SD-01 `esm-plugin`, SF-SD-02 `esm-plugin` + `character`, SF-SD-03 `esm-plugin` + `gameplay`, SF-SD-04 `import-pipeline`, SF-SD-05 `import-pipeline`.)
