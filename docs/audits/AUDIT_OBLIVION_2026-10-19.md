**HEAD**: `e1bd40bcf` (suite launch HEAD `2a6f8be2b`; first read at `2139ae20e`) · **Baseline**: `AUDIT_OBLIVION_2026-10-14.md` (HEAD `adbd468e2`) · **Audited**: Dim 3 (NPC body/head/skeleton/animation spawn path) and Dim 4 (Tamriel/child-worldspace exterior + legacy LOD) plus the `attach.rs` slice of Dim 5; scope=streaming-deep AREA-SCOPED · **Unchanged since baseline (skimmed)**: Dim 3 material/render/seam-blend paths; Dims 1, 2 and the rest of Dim 5 are out of area

> **This report is AREA-SCOPED (streaming-deep), not a full-codebase Oblivion audit.** It covers only `byroredux/src/streaming/`, `byroredux/src/npc_spawn/` (+ `npc_spawn.rs`) and `byroredux/src/cell_loader/`, read through the Oblivion (TES4) lens, with direct callers/callees followed where a data flow required it. The next full `/audit-oblivion` must not treat it as a baseline for Dims 1, 2, 5 or for the NIF corpus lane.

# Oblivion (TES4) Compatibility Audit, streaming-deep area, 2026-10-19

## Executive Summary

| Severity | NEW | Existing / extension |
|---|---|---|
| CRITICAL | 0 | |
| HIGH | 1 | OBL-SD-01 |
| MEDIUM | 3 | OBL-SD-02 (extends sibling FNV-SD-03), OBL-SD-03, OBL-SD-04 |
| LOW | 1 | OBL-SD-05 (extends sibling NIFAL-SD-01) |

**Headline (OBL-SD-01, HIGH)**: the Oblivion humanoid default idle clip path is the FNV one (`meshes\characters\_male\locomotion\mtidle.kf`). Oblivion ships no `locomotion\` directory; its idle is `meshes\characters\_male\idle.kf`. The idle pool is therefore empty for every Oblivion humanoid, and no clip drives a standing NPC. The unit test that "covers" the path only asserts `is_some()`.

**What the window changed for Oblivion** (`adbd468e2..HEAD`): `75b921cf4` (#5505, child-worldspace LAND inheritance) is the one Oblivion-visible fix. It is **correct and covers every exterior entry** (verified below). `cb5368348` (#2586, Skyrim `.lod` lattice) and `2a8663f39` (#5408, Skyrim/FO4 head parts) are gated off the kf-era path. The M47.3 ObScript commits touch `references/attach.rs` and `load.rs` (`ensure_obscript_tables`); their Oblivion-visible effect is covered by the scripting audit.

**Compatibility level for the area.** Interior and exterior cell load, asset resolution through BSA v103, NPC/creature spawn and distant LOD all run. The measured gaps are all in animation and enable-state fidelity, not parse or load: standing humanoids unanimated (OBL-SD-01), roughly half of all XESP children in the wrong initial state (OBL-SD-02), city worldspaces missing parent placement LOD (OBL-SD-03), Shivering Isles creatures without a walk clip (OBL-SD-04).

**Method.** Read-only, no sub-agents, no engine, no Vulkan. `cargo` ran in the repo `target/` (no private target dir); the `byroredux` bin crate compiled cleanly at the time of the run, so the in-progress `crates/nif` ragdoll edit named in the brief was not blocking by then. Real-data checks were done with throw-away Python readers over `Oblivion.esm` and the BSA v103 archives (scratch only, under `/tmp/audit/streaming-deep/oblivion/`). Dedup: `/tmp/audit/issues.json` (121 open), closed-issue searches, and `SIBLINGS.md` plus the sibling reports in `docs/audits/*_2026-10-19.md`.

## Pins and guards run (all green)

| Pin | Result |
|---|---|
| `cargo test -p byroredux-plugin --lib -- inherit_parent oblivion_child` | 3 passed, 1 ignored (real-data variant needs the full master) |
| `cargo test -p byroredux --bin byroredux -- --ignored installed_oblivion_body_paths` | ok |
| `... -- --ignored oblivion_ltex default_land_textures` | ok (2) |
| `... -- parallax_alpha_gate placement_lod oblivion_apply_hilight2 nif_light_spawn_gate race_body` | ok (all matched) |
| `idle_kf_path_only_for_kf_era_games`, `body_paths_kf_era_*` | ok (and see OBL-SD-01: the first is vacuous) |

## Verified correct (premise tested, no finding)

- **#5505 landscape inheritance.** `EsmCellIndex::inherit_parent_landscapes` (`crates/plugin/src/esm/cell/index.rs:378`) runs once in `build_exterior_world_context` (`byroredux/src/cell_loader/exterior.rs:1795-1805`). That function is the single funnel for boot `--grid`, the door-transition exterior arm and the save reload; interior loads (`load.rs:384`, `:550`) legitimately skip it. The co-location premise holds on all 30 children, measured over `Oblivion.esm` with `/tmp/audit/streaming-deep/oblivion/wrld.py`: every child cell that authors its own LAND also has LAND in its parent at the same grid coordinate (0 not co-located), and 1 047 of 1 050 LAND-less child cells in the master (Tamriel and Shivering Isles children together) have LAND in the parent at the same coordinate (the 3 misses are `(-345,-3)` in the Kvatch worlds; the commit's own 1 043 / 1 040 figures count the Tamriel children only). `TGTempleOfTheEmperorZero` (300 cells, 3 with own LAND) fills 297 cells with Tamriel terrain; that is a Tamriel-shaped quest world and the CS rule gives the same answer.
- **Oblivion body table (#5487).** Eight vanilla body paths plus `skeleton.nif` exist; head-part INDX map (0 head, 1/2 ears, 3 mouth, 4/5 teeth, 6 tongue, 7/8 eyes) matches `Argonian` RACE dumped from the master.
- **Asset resolution through the loader.** For 17 821 `MODL`/`MOD2-4` paths across 21 base record types, 61 (52 STAT, 4 ACTI, 2 DOOR, 1 each CONT/LIGH/MISC; 0.35 %) miss the mesh archives, all dev or test content such as `effects\testfiresorting.nif`; the 142 TREE paths resolve through `SPT_CANDIDATE_DIRS`, not `meshes\`.
- **Creature base tree.** 914 `CREA` records: 0 missing skeletons, every distinct creature directory ships `idle.kf`.
- **Retain-refactor (#5820).** `retain_resident` preserves the placement ring's "empty sentinel is dropped but not counted" rule (test `retain_resident_evicts_counts_and_honours_sentinels`).

## Dimension Findings

### Dim 3: NPC spawn, animation, body and head

### OBL-SD-01: Oblivion humanoid idle clip path points at a file Oblivion does not ship; the idle pool is empty
- **Severity**: HIGH
- **Dimension**: 3 (NPC spawn / animation)
- **Location**: `byroredux/src/npc_spawn.rs:1028-1034` (`humanoid_default_idle_kf_path`), consumers `:719-729` (`load_idle_clip`), `:863` (`load_idle_pool`), `byroredux/src/cell_loader/references/mod.rs:420-421`, `byroredux/src/player_body.rs:389`, `byroredux/src/systems/cinematic.rs:144`; vacuous pin `byroredux/src/npc_spawn/tests.rs:899-908`
- **Status**: NEW. Searched open and closed issues for `mtidle`, `idle.kf`, "Oblivion idle clip": only #794 / #772 (FNV-era, closed) and #2567 (creatures). No Oblivion humanoid idle finding exists in any `AUDIT_OBLIVION_*.md`.
- **Description**: the match arm `GameKind::Oblivion | GameKind::Fallout3NV => meshes\characters\_male\locomotion\mtidle.kf` shares FNV's path. The sibling function `humanoid_walk_kf_paths` (`npc_spawn.rs:~890`) already documents "Oblivion has no `locomotion\` directory at all (BSA scan 2026-09-18: zero hits)", but the idle function was not split the same way. Oblivion's standing idle is `meshes\characters\_male\idle.kf`.
- **Evidence**: parsing `Oblivion - Meshes.bsa` (20 182 names) and every DLC/Knights archive: `meshes\characters\_male\locomotion\mtidle.kf` absent, no path containing `\locomotion\` or `mtidle` in any of the 17 archives, `meshes\characters\_male\idle.kf` present (25 817 B, a `NiControllerSequence` with B-spline `Bip01` channels and an `IdleK…` text key), `handtohandforward.kf` present. `load_idle_clip` -> `load_kf_clip_by_path` returns `None`, so `load_idle_pool` returns an empty `Vec`; `Finalize` (`npc_spawn/resumable/runtime.rs:~860`) then calls `pick_idle_handle(&[], form_id)` and inserts no `AnimationPlayer`. The walk clip works (`handtohandforward.kf`), but `npc_walk_animation_system` removes the inserted player after a sustained stop (`RemovePlayer`), so a stopped NPC holds its last sampled pose and a never-moving NPC holds the bind pose. `installed_oblivion_body_paths_exist_in_the_meshes_archive` covers body paths only; `idle_kf_path_only_for_kf_era_games` asserts `is_some()` for Oblivion, which passes for a path that does not exist.
- **Impact**: every standing Oblivion humanoid (NPC_, and the player body's idle, and the cinematic generic idle) is unanimated. This is the most visible Oblivion-only defect in the area, in every populated interior and city.
- **Related**: #794, #772 (the FNV fixes this path was copied from); #5495 (creature seating, separate); see also the sibling FNV report's creature-clip finding for the FNV spelling of the same resolver.
- **Suggested Fix**: split the arm (`Oblivion => meshes\characters\_male\idle.kf`, FO3/FNV keep `mtidle.kf`) and add `humanoid_default_idle_kf_path(Oblivion)` to the installed-archive test next to the body-path assertions, so a non-archived idle fails the lane instead of passing on `is_some()`.

### OBL-SD-04: Creature walk clip is hard-wired to `forward.kf`; Shivering Isles creatures ship `forwardwalk.kf`
- **Severity**: MEDIUM
- **Dimension**: 3
- **Location**: `byroredux/src/npc_spawn.rs:944` (`creature_walk_kf_path`), consumer `byroredux/src/npc_spawn/resumable/runtime.rs:473`
- **Status**: NEW (a sibling FNV finding covers FNV's `mtforward` creature clips; Oblivion's file-name convention differs)
- **Description**: `creature_walk_kf_path(dir)` is `{dir}forward.kf`. Of 92 creature animation directories in the Oblivion archives (the 42 referenced by `CREA` records matter here), 36 ship `forward.kf`; the Shivering Isles creature sets `grummite`, `gnarl` and `elytra` ship `forwardwalk.kf` instead (plus `runforward.kf` / `fastforward.kf`), and `hunger`, `slaughterfish`, `mehrunesdagon` ship neither. 196 `CREA` bases (grummite 113, gnarl 48, elytra 35) have no walk clip under the resolver; 223 including the other three.
- **Evidence**: `/tmp/audit/streaming-deep/oblivion/crea.py` over the 914 `CREA` records: `no_fwd = 223`, `no_idle = 0`. `load_kf_clip_by_path` misses, `walk_kf_path` yields no `WalkAnimation`, so a moving creature plays its idle while the root slides.
- **Impact**: Shivering Isles creatures glide when an AI package moves them. Cosmetic, SI-only, bounded.
- **Related**: #2567 (the directory-namespace convention this extends).
- **Suggested Fix**: try `forward.kf` then `forwardwalk.kf` in the creature resolver, or read the authored `KFFZ` list (currently unparsed into the spawn path) rather than guessing a name; extend the installed-archive pin to creatures.
- **Note (unverified, not a finding)**: 21 SI `CREA` records (e.g. `SEFetidPatrol07TEMPLATE`) author `NIFZ` names (`GoblinHead.NIF`, `GobLegs01.NIF`) that live in `creatures\goblin\`, not in their `MODL` directory `creatures\grummite\`, and `Dreugh.NIF` / `Daedroth.NIF` name files that exist only as `landdreugh.nif` / `daedroth.nif` in other directories. The "directory is the namespace" rule of #2567 therefore leaves these invisible. Whether the original engine resolves NIFZ against another root is not established here; per the no-guessing policy this is recorded for a data-backed follow-up, not filed.

### OBL-SD-05: Race body skin override is dropped when a stripped body part is restored (Oblivion consequence of NIFAL-SD-01)
- **Severity**: LOW
- **Dimension**: 3
- **Location**: `byroredux/src/npc_spawn/loot_appearance.rs:~780` (the `load_nif_bytes_with_skeleton(..., part.tint.as_deref(), None)` restore), vs `byroredux/src/npc_spawn/resumable/runtime.rs:574-640` (spawn-time hook)
- **Status**: Existing sibling NIFAL-SD-01 (restored stripped NPC body part skips pre_spawn hooks); this entry records the Oblivion-specific effect only.
- **Description**: spawn-time, each Oblivion body part gets the RACE body-section `ICON` swapped in through the `pre_spawn` hook (#5487) and hands get the wrist-seam blend. The loot-appearance restore path loads the same NIF with `pre_spawn = None`, so a looted or stripped Argonian, Khajiit, Dremora or Elf that gets its naked body back reverts to the Imperial skin every body NIF authors (the exact defect #5487 fixed at spawn), and the hand loses its seam tone.
- **Impact**: appearance reverts to the pre-#5487 state only after equipment removal or a restore; low frequency.
- **Related**: NIFAL-SD-01, CHAR-SD-01, #5487, #5753 (still open, see below).
- **Suggested Fix**: carry the per-part `body_textures` entry in `RestorePart::body` and run the same hook on restore (fixes the sibling at the same time).

### Dim 4: Exterior, child worldspaces, legacy LOD

### OBL-SD-03: Oblivion placement LOD (`DistantLOD\<ws>_x_y.lod`) is keyed on the child worldspace only, so city worldspaces get 0 to 16 of the parent's ~900 ring files
- **Severity**: MEDIUM (needs one on-device confirmation of the inherit intent, see Evidence)
- **Dimension**: 4
- **Location**: `byroredux/src/cell_loader/placement_lod.rs:450` (`placement_lod_archive_path(&wctx.worldspace_key, cx, cy)`); contrast `object_lod.rs` `object_lod_quad_paths` / `ExteriorWorldContext::lod_worldspace_key` (`exterior.rs:179`)
- **Status**: NEW. Not covered by #5496 (closed, terrain and `.bto` only) or #5786 (closed; it states "`placement_lod.rs` still uses the child's own key, which is right (children ship their own `distantlod\<child>_*.lod`)"; the measurement below shows that is only half the picture).
- **Description**: #5374 stamps the "use LOD data" inherit bit on all 30 Oblivion children and #5496 routes every other distant-ring key through `lod_worldspace_key()`. Placement LOD is the one Oblivion-only LOD scheme and still reads only `distantlod\<child>_x_y.lod`. The children ship a handful of files for their own landmarks; the surrounding countryside (the cells #5505 now fills with the parent's terrain) is covered only by the parent's files.
- **Evidence** (`allmeshes.pkl` + `wrld.pkl` scratch, radius `PLACEMENT_LOD_RADIUS_CELLS = 16` around the child's LAND centroid): BrumaWorld own 7 vs Tamriel 800; BravilWorld 10 vs 939; ChorrolWorld 16 vs 1 034; SkingradWorld 8 vs 1 065; Imperial City districts 1 to 6 vs 926 to 962; The Fringe / Bliss / Crucible / Palace / Vitharn 0 vs 365 to 630 (SEWorld); TGTempleOfTheEmperorZero 0 vs 912. Whether the original game draws the parent's placement list inside a city worldspace is inferred from the inherit bit and #5496's policy, not observed.
- **Impact**: inside a city worldspace the distant terrain (inherited) is drawn but the distant statics (farmhouses, towers, rocks, ruins) are not, ~99 % fewer placements than the same view from Tamriel.
- **Related**: #5496, #5505, #5786, EXT-D6 siblings.
- **Suggested Fix**: per cell, read the child's own `.lod` when present and fall back to `lod_worldspace_key()`'s file; a child cell that has its own file must not also load the parent's (duplicate landmarks). Pin it with an archive-path test like the `.bto` one.

### OBL-SD-02: Oblivion XESP children spawn in the wrong initial state for ~28 % (single-level) to ~56 % (full chain) of the 9 661 in `Oblivion.esm`
- **Severity**: MEDIUM
- **Dimension**: 4 (cell reference spawn)
- **Location**: `byroredux/src/cell_loader/references/mod.rs:500-517` (skip only `EnableParent::default_disabled()`), `:640-649` (`reference_is_disabled(…, initially_disabled && enable_parent.is_none())`); `crates/plugin/src/esm/cell/placed_ref.rs:487` (`default_disabled = form_id != 0 && inverted`)
- **Status**: Extends sibling **FNV-SD-03** (`AUDIT_FNV_2026-10-19.md`, the same mechanism quantified on FNV: 56 %). No open issue (closed #471 / #349 chose the "parent assumed enabled" interim and #4813 explicitly leaves XESP children to it). Oblivion numbers are new.
- **Description**: the loader assumes every XESP parent is enabled, so every non-inverted child spawns and every inverted child is skipped. The parent REFR's own `Initially Disabled` header bit (0x800) is decoded for the parent's own spawn but never read for its children.
- **Evidence** (`/tmp/audit/streaming-deep/oblivion/xesp.py` over every REFR/ACHR/ACRE in `Oblivion.esm`, 1 029 280 placements, 9 661 with XESP, all parents are placed references): children whose parent is initially disabled and that we get wrong, single level: non-inverted REFR 2 091, ACHR 197, ACRE 92 (spawn although hidden); inverted REFR 340, ACHR 19, ACRE 4 (skipped although they should show). 2 743 total. Following the parent chain (3 968 parents are themselves XESP children): REFR 3 635 + 1 460, ACHR 208 + 19, ACRE 102 + 4 = 5 428. In Oblivion the wrongly spawned set includes ~310 actors that the data says start disabled.
- **Impact**: quest-gated alternate states (before/after set dressing, scripted-appear NPCs and creatures) render all at once; disabled hostile actors spawn live. Nothing cascades a parent's runtime `Enable`/`Disable` to children either.
- **Related**: FNV-SD-03, #471, #349, #4813, GAME-SD-01.
- **Suggested Fix**: as in FNV-SD-03: a per-cell pass that resolves each XESP chain against the parents' real 0x800 bits (cycle-capped) before the spawn loop, and a `ReferenceEnableState` cascade for runtime enable/disable.

## Regression Guard List

Held in this window: `placement_lod_supported_is_oblivion_only`, `oblivion_ltex_paths_exist_in_vanilla_archives`, `default_land_textures_exist_in_vanilla_archives`, `oblivion_apply_hilight2_binds_the_derived_normal_as_its_height_map`, the `nif_light_spawn_gate` set (the `__MAX_Default_Light` suppression, #5189), `race_body_skin_selects_gendered_and_shared_icons` / `installed_oblivion_body_paths_exist_in_the_meshes_archive` (#5487), `inherit_parent_landscapes_*` (#5505). Not run here (out of area): the NIF corpus lane, the #170 dual band, `NiTexturingProperty` u32 count, the BSA v103 sweep, Disney-gate-stays-0.

## Open Work / still open (verified at HEAD)

- **#5753** (Argonian/Khajiit tails and the beast rig) is still open and unchanged: `skeletonbeast.nif` is referenced nowhere in `byroredux/src`; `npc_skeleton_path` (`npc_spawn.rs:518`) only reads `skeleton_models`, which `parse_race` fills for Skyrim only (`race.rs:611`). The doc comment at `npc_spawn.rs:491` still says "its beast rig is handled separately (OBL-D3-01)", which is not true (doc rot, covered by that issue).
- **CHAR-SD-01** (runtime spawn reads gender from the shell record, `runtime.rs:222`) applies to Oblivion only for templated records; Oblivion's 16-byte ACBS has no template flags, so the practical Oblivion impact is nil.
- ObScript attach (`attach.rs:348-356`) runs the static `LegacyObscriptProgram` and the VM `ObScriptInstance` on the same script and shares one `ScriptVariables`; the legacy handlers are assignment/conditional-only so I could not construct a non-idempotent double run. Left to `/audit-scripting` (SCR-SD-03 area).

## Out of area / skipped, and why

| Item | Reason |
|---|---|
| Dim 1 (NIF version handling, corpus lane `parse_real_nifs` / `per_block_baselines` / `block_coverage_baselines` / `oblivion_stream_drift_corpus`) | Paths are `crates/nif/`, outside streaming-deep; no commit in the area changes them. Not run. |
| Dim 2 (BSA v103 sweep, TES4 ESM record decode) | `crates/bsa`, `crates/plugin/src/esm/records`: out of area. Only the area's consumption of them was checked (asset resolution, RACE/CREA fields). |
| Dim 5 (M47.3 VM, MenuXml HUD) | `crates/scripting`, `crates/menuxml`, `hud.rs` out of area; only `references/attach.rs` was read. |
| `docs/smoke-tests/m-exteriors.sh oblivion ...`, `p0-door-interaction.sh oblivion`, `/audit-runtime --game oblivion`, `m48-4-oblivion-hud.sh` | Need the engine, a Vulkan device and a display; forbidden by the suite rules. Not run. |
| On-device confirmation of OBL-SD-01 (standing NPC pose), OBL-SD-02 (wrong-state children) and OBL-SD-03 (distant statics in a city worldspace) | Requires the engine. Each finding is backed by archive/ESM measurement and code reading, not a rendered frame. |
| `cargo test --ignored` on the plugin crate | Not run (OOM rule); only named single tests were run. |

Private cargo target dir: none (repo `target/` used).

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-10-19.md` (labels per finding: `game:oblivion` + `legacy-compat`, plus `animation` for OBL-SD-01 / -04, `terrain-exterior` for OBL-SD-03, `gameplay` for OBL-SD-02, `character` for OBL-SD-05).
