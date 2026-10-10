# Oblivion (TES4) Compatibility Audit — 2026-10-09

**HEAD**: 3bcf6c8e8 · **Baseline**: `docs/audits/AUDIT_OBLIVION_2026-10-08.md` (HEAD `00f580e09`; 81 commits since) · **Audited**: Dims 2, 4, 5 (commits on their Paths), plus an area sweep of `npc_spawn/` + `seam_blend` on Oblivion data (Dim 3's NPC half) · **Unchanged since baseline (skimmed)**: Dim 1 (0 commits on header/version/stream/lib/blocks/tests; corpus lane re-run green), Dim 3 Paths (0 commits on `material/`, `seam_blend.rs` + `seam_blend/`, `mesh_instance.rs`, `nif_loader.rs`, `static_meshes.rs`, `asset_provider/texture.rs`, `material_sampling.glsl`; guards re-run green)

This run is part of the 2026-10-09 `/audit-suite --preset streaming-deep`. Its area emphasis is the code under `byroredux/src/streaming/`, `npc_spawn/` and `cell_loader/`, as that code applies to Oblivion: Tamriel / Shivering Isles streaming, child-worldspace inheritance, NPC spawn + `seam_blend`, transitions, and LOD.
- **How it was run**: solo, read-only, against the vanilla Oblivion + DLC data at `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`.
- **Not run**: the engine binary and the device-bound smoke scripts (suite rule).
- **Scratch**: `/tmp/audit/oblivion/`, which holds `dim_1..5.md`, `corpus_lane.log`, and the census tools under `tools/`:
  - `esmwalk.py`: a TES4 GRUP walker.
  - `pack_census.py`, `pkdt4.py`, `pack_impact.py`.
  - `wrld_land.py`, `child_land.py`, `gridmap.py`, `noland_refs.py`.
  - `body_slots.py`, `bsalist.py`.
  - Archive listings, plus `extract/` with the body NIFs.
- **Cargo target**: `/mnt/data/tmp/oblivion-audit-target`, built with the 1.96 toolchain.

## Executive Summary

**Corpus lane: GREEN.** The command was `BYROREDUX_REQUIRE_GAME_DATA=1 cargo test -p byroredux-nif --release --test parse_real_nifs --test per_block_baselines --test block_coverage_baselines --test oblivion_stream_drift_corpus -- --ignored oblivion no_block_sizes`, and it exited 0.
- **`parse_rate_oblivion`**: 9,612 / 9,612 clean (100.00%), with 0 truncated and 0 failed. By archive: Meshes 8,032, Shivering Isles 1,438, Knights 75, plus 6 small DLC.
- **`oblivion_block_count_parity`**: 9,612 whole, 0 truncating, every archive at or above baseline.
- **`per_block_baseline_oblivion`**: PASS.
- **`no_block_sizes_drift_detector_has_zero_false_positives_on_real_corpus`**: PASS.
- These results match the ROADMAP compatibility row (100%, 9,612 / 9,612).

**Other gates run live:**
- **Bin crate**: 87/87 Oblivion-relevant guards pass. They cover `parallax_alpha_gate` (5), placement LOD (13), the nif-light spawn gate (24), the HNAM dimmer, the falloff sentinel, the WATR wind scope, the 3 `oblivion_climate_rung_tests`, the 2 `default_weather_rule_tests`, Oblivion water inheritance, `seam_blend` (6) and the Oblivion eye roles.
- **Bin crate real-data `--ignored`**: 3/3 pass — `oblivion_ltex_paths_exist_in_vanilla_archives`, `default_land_textures_exist_in_vanilla_archives`, `installed_oblivion_creature_assets_resolve_from_their_records`.
- **M47.3**: `vanilla_oblivion_quest_scripts_execute_and_setstage` passes 1/1.
- **MenuXml Oblivion corpus**: 4/4 pass. That is 89 menus and 1,868 tiles, and the new header census decodes 1,975 menu DDS headers.

**Findings: 0 CRITICAL, 1 HIGH, 2 MEDIUM, 1 LOW (all NEW). There are no regressions.**
- **The baseline finding is fixed.** OBL-2026-10-08-D2-01 was filed as #5397, which is now closed. Its fix was verified against the TES4 INFO layout (see Dim 2).
- **The headline is NPC body assembly (D3-01, HIGH).** Since #793 the Oblivion arm has requested `lefthand.nif` / `righthand.nif`, which Oblivion does not ship. It never requests the shipped `hand.nif` / `lowerbody.nif` / `foot.nif`. As a result:
  - Every Oblivion humanoid without gauntlets spawns handless.
  - Uncovered legs and feet never appear.
  - The per-race body skins are dropped, so `seam_blend` tones necks toward Imperial skin.
- **D2-01 (MEDIUM)**: the PACK parser silently drops two TES4 sub-record layouts. 479 packages misread as Find, which affects 650 placements. All 1,776 package targets are lost, so every Follow package idles.
- **D4-01 (MEDIUM)**: the 30 child worldspaces (cities, Imperial City districts, New Sheoth) draw no ground outside their own LAND footprint. The CS documents that a child uses its parent's landscape, and #5374 stamps the inherit bits, but nothing reads those bits for LAND or terrain LOD.
- **D5-01 (LOW)**: the voice path never resolves on Oblivion, so every greeting is silent.

**What changed for Oblivion since the baseline:**
- **#5374 / #5388**: child worldspaces inherit the parent's water and climate. Verified on data:
  - The Tamriel children reach TamrielClimate through chain naming.
  - The SEWorld children resolve SEWorldClimate.
- **#5397**: the positional Random / Random End stack is now live on every Oblivion GREETING.
- **#5423**: Oblivion's LOD ring routing is unchanged.

## Dimension Findings

### Dimension 1: NIF Version Handling & Corpus Integrity — unchanged, clean
- **Commits**: none on this dimension's Paths. The only `crates/nif` commit is `cf9dec258` (#5389), which adds `decline_unbounded_packed_indices` to the classic `NiSkinData` path in `import/mesh/skin.rs`.
- **Why #5389 cannot fire on Oblivion**:
  - `extract_skin_ni_tri_shape` returns `None` unless `data.bones.len() == bone_refs.len()`.
  - `densify_sparse_weights` takes the bone index from `enumerate()` over `data.bones`.
  - So every index is already below the bone count. The decline fires only on an empty bone list, and Oblivion's skinning path has no behaviour change.
  - The mislabelled warn text on the classic arm is already NIF-D4-2026-10-09-02.
- **Pins**: the #170 / `uses_inline_block_type_names` / `NiTexturingProperty` / `MORPH_LEGACY_CUTOFF` / #3926 pins are untouched since 10-08 (no commits). The corpus lane above is the live guard.

### Dimension 2: BSA v103 & ESM Data Slice

**Delta reviewed with no finding:**
- **`0436bea5c` (#5397, which closes baseline D2-01)**:
  - `select_info` (`crates/scripting/src/dialogue.rs:348-378`): the first passing INFO wins. A Random first INFO stacks the following passing Random INFOs, up to the next non-Random INFO (excluded) or a Random End INFO (included).
  - `random_end()` is `flags1 & 0x20` (`misc/dialogue.rs:411` / `:435`). TES4 INFO `DATA` is 3 bytes (Type, Next Speaker, Flags), so `flags1 = data[2]` (`:688`) is correct for Oblivion.
  - The pinned test was rewritten to the vanilla expectation, and it includes the adjacent Random End sets.
- **`479414ffe` (#5358 BODT)**: the arm is gated on `is_skyrim_or_later`. The `equip.rs` / `npc_spawn.rs` changes are comments only on the Oblivion path.
- **`edb5fbdfe` (#5359 NPC_.WNAM)**: it decodes with no game gate, on the premise that "pre-Skyrim NPC_ ships none".
  - Census of `Oblivion.esm` (2,482 NPC_): 0 `WNAM`, so the premise holds.
  - The full NPC_ sub-record set is EDID, FULL, MODL, MODB, ACBS, SNAM, INAM, RNAM, SPLO, CNTO, AIDT, PKID, CNAM, DATA, HNAM, LNAM, ENAM, HCLR, ZNAM, FGGS, FGGA, FGTS, FNAM, SCRI and KFFZ.
- **BSA archive**: no commits.

#### OBL-2026-10-09-D2-01: `parse_pack` drops Oblivion's legacy 4-byte `PKDT` and its 12-byte `PTDT`. 479 packages read as Find (650 placements), and every Oblivion package loses its target, so all 208 Follow packages idle.
- **Severity**: MEDIUM. Ambient AI is wrong for hundreds of placements, with no crash. It is a silent parse-boundary loss.
- **Dimension**: BSA v103 & ESM Data Slice (a TES4-only decode branch). The runtime consumer is in `npc_spawn/ai_package.rs`, which is in the suite's area.
- **Location**:
  - `crates/plugin/src/esm/records/misc/pack.rs:799-810`: the `PKDT` arm, `if sub.data.len() >= 8`.
  - `crates/plugin/src/esm/records/misc/pack.rs:905-921`: the `PTDT` arm, `if sub.data.len() >= 16`.
  - Consumers in `byroredux/src/npc_spawn/ai_package.rs`:
    - `:171-262`: `AmbientBehavior::from_package`.
    - `:743-754`: `select_active_package`.
    - `:763-820`: `apply_ai_package_behavior`, which is reached from the Oblivion runtime spawn at `npc_spawn/resumable/runtime.rs:990`.
- **Status**: NEW.
  - Searched `gh` all-states for "PTDT" and "PKDT". The only hits are #5376, #3332, #3042, #5367, #5390, #3350, #2012 (`PSDT`, Skyrim+) and #446.
  - `AUDIT_ESM_2026-10-09`'s pack.rs finding is D2-02 (`PKDD`), which is a different field.
- **Description**: The length gates are shaped for FO3/FNV.
  - **`PKDT`**:
    - `Oblivion.esm` authors 6,648 `PKDT` at 8 bytes and **561 at 4 bytes**.
    - The 4-byte layout is a legacy form: `u16` flags at 0, a `u8` procedure at 2 and a junk byte at 3. It is validated against EDIDs: 49 of 50 "…Sleep…" EDIDs have byte 2 = 4, 24 of 24 "…Wander…" have 5, and 14 of 19 "…Follow…" have 1.
    - The `>= 8` gate skips the whole sub-record, so `procedure_type` stays 0 (Find) and `package_flags` stays 0.
    - The misread procedures are Wander 179, Travel 158, Eat 73, Sleep 49, Follow 15 and Escort 5 (479 packages), plus 82 real Find.
    - OpenMW's ESM4 reader has the same gap (`loadpack.cpp:50`: `mData.type = 0; // FIXME`). UESP's Oblivion `PACK.wiki` documents only the 8-byte form.
  - **`PTDT`**:
    - Oblivion's `PTDT` is **12 bytes in all 1,776 records**: type `i32`, target `u32`, count `i32`. The FO3/FNV trailing `f32` does not exist.
    - The `>= 16` gate drops every one, so `PackRecord::target` is always `None` on Oblivion.
    - FO3 also authors 26 twelve-byte `PTDT` among 1,163 (`/audit-fo3` owns that). FNV is all 16 bytes.
- **Evidence**:
  - The raw-ESM censuses are `tools/pack_census.py`, `tools/pkdt4.py` and `tools/pack_impact.py`.
  - Runtime trace:
    1. `active_package` picks the first PKID entry that is scheduled and passes its conditions.
    2. A misread entry still passes, because its `PSDT` is 8 bytes and decodes.
    3. `from_package` then matches no `is_*` and no `PROCEDURE_EAT` / `PROCEDURE_SLEEP`, and returns `None`.
    4. `AmbientPackageRuntime.active_package_form_id` still records it as the winner. The actor stands idle, and the next PKID entry gets no fallback.
  - A Follow package whose `target_form_id` is `None` is terminal (`systems/follow.rs:79-84`).
  - An Escort package with no target skips its collect phase (`escort.rs:25`).
- **Impact**:
  - 551 actor bases (423 NPC_, 128 CREA; 650 placements) list a misread package. For 295 of them (338 placements) it is the first PKID entry, so it wins whenever it is scheduled. Examples: the `AnvilBreakfastFlowingBowl8x2`, `LaytheWavrickChorrolSleep1x6` and `CarandialAnutwyllSleep` diners and sleepers, and the SI obelisk priests' `aaaDefaultStayAtCurrentLocationSkipFallout`.
  - 196 actors (248 placements) list a Follow package. 188 of the 208 Follow packages target a specific reference, mostly the player (`ICPrisonFollowPC`, `MS13FollowPlayer`, `FGD05ModrynFollow`). None of them can follow.
  - Escort affects 52 actors (79 placements).
- **Related**:
  - ESM-2026-10-09-D2-02 (`PKDD` byte, same parser).
  - GAME-D5-2026-10-09-02 (#5391 anchoring).
  - FNV-2026-10-09-D5-01: once Eat and Sleep decode, Oblivion creatures with those packages (128 CREA list affected packages) also reach the humanoid seating path.
  - #3332, #2012.
- **Suggested Fix**:
  - In `parse_pack`:
    - Accept a 4-byte `PKDT` as `flags = u16 @0`, `procedure = u8 @2` (game-gated to Oblivion, or by size).
    - Accept a 12-byte `PTDT` (type, target, count).
  - Add an Oblivion real-data pin: 561 legacy `PKDT` decode to procedures 0..=6, with the per-procedure counts above, and every `PTDT` yields a target.

### Dimension 3: Legacy-Property Rendering Path, plus the NPC body / `seam_blend` area sweep
- **Paths**: unchanged since 10-08. The guards above are green, including the `seam_blend` tests `skin_is_recognised_by_its_characters_texture_root` and `own_file_clothing_and_distant_meshes_are_never_neighbours`.
- **Not re-measured**: the APPLY_HILIGHT2 BC1 `_n.dds` census. There is still no in-tree tool for it.

#### OBL-2026-10-09-D3-01: Oblivion NPC bodies are assembled from FO3/FNV paths. The hands are requested as `lefthand.nif` / `righthand.nif`, which Oblivion does not ship; `lowerbody.nif` / `foot.nif` are never requested; and the RACE body-section skins are dropped. Every non-gauntleted Oblivion humanoid spawns handless, and the seam blend tones necks toward the Imperial skin.
- **Severity**: HIGH. Visible actor geometry is missing on essentially every Oblivion humanoid spawn, under ordinary conditions, with no workaround.
- **Dimension**: Legacy-Property Rendering Path / NPC spawn + `seam_blend`. The mechanism belongs to `/audit-gameplay`; the Oblivion data is owned here.
- **Location**:
  - `byroredux/src/npc_spawn.rs:640-649`: `humanoid_body_paths`, the Oblivion arm shared with FO3/FNV.
  - `byroredux/src/npc_spawn.rs:661-673`: `humanoid_body_path_biped_mask`. It has only Oblivion `lefthand`/`righthand` → `1 << 4`, with no lower-body or foot arm.
  - `byroredux/src/npc_spawn/resumable/runtime.rs:633-638`: an archive miss logs at `debug` and is skipped silently.
  - `crates/plugin/src/esm/records/actor/race.rs:507`: `b"ICON" if in_head_section`, so body-section ICONs are dropped.
  - `byroredux/src/npc_spawn/resumable/runtime.rs:1241-1290`: `build_seam_context` reads the neighbour texture from the cached body import.
  - Tests that pin the wrong paths, in `byroredux/src/npc_spawn/tests.rs`:
    - `:612` `body_paths_kf_era_include_separate_hand_meshes` (it loops over `GameKind::Oblivion`).
    - `:665` (Oblivion female).
    - `:675` `kf_body_piece_masks_follow_each_games_hand_layout`.
- **Status**: NEW.
  - `gh` all-states searches for "Oblivion hand.nif", "handless Oblivion", "lowerbody.nif", "Oblivion body part meshes" and "race body texture" find no match.
  - #793 (closed) fixed FNV only. Its commit `da8d7e216` says for Oblivion: "needs verification … If Oblivion ships hands at different paths the load will silently miss (debug-logged)".
  - #3419 (closed) is the head/body section collision, which is a different defect.
- **Description**:
  - **Meshes**:
    - `Oblivion - Meshes.bsa` (20,182 entries, listed with `tools/bsalist.py`) ships under `characters\_male\` exactly `upperbody`, `lowerbody`, `hand`, `foot` and their `female*` twins (plus the skeletons).
    - It has **no** `lefthand.nif` / `righthand.nif`. The only `*hand*.nif` files are creature parts: zombie, and the SI gatekeeper.
    - `import_probe` on the extracted files:
      - `hand.nif` is one skinned `NiTriShape` "Hand" with both hands (x ±59, 18 `Bip01 L…` and 18 `Bip01 R…` name strings, `HandMale.dds`).
      - `lowerbody.nif` has 2 meshes (`LegMale.dds`, `GroinMale.dds`).
      - `foot.nif` has 1 mesh (`FootMale.dds`).
    - The engine asks for the two missing hand files, never asks for the lower-body or foot meshes, and has no biped mask for slot 0x08 (Lower Body) or 0x20 (Foot).
  - **Skins**:
    - The Oblivion RACE body section (after `NAM1`) authors per-race skins as ICON index 0 UpperBody, 1 Leg, 2 Hand, 3 Foot, 4 Tail. Examples: `Characters\Argonian\Male\UpperBodyMale.dds`, `Characters\Khajiit\…`, `Characters\Orc\…`, `Characters\DarkElf\…`. Redguard reuses the Imperial skin.
    - The body NIFs all author Imperial textures.
    - `parse_race` reads ICON only inside the head section, so a beast or mer torso keeps the Imperial skin.
  - **`seam_blend`**:
    - The head's own texture is the race head ICON (`state.head_texture`).
    - The neighbour texture comes from the cached `upperbody.nif` import, which is the Imperial skin.
    - So for every race whose body skin is not Imperial's, the neck tone ratio is computed against the wrong skin. It is clamped to [0.5, 2] and fades over 4 units. This is the "wrong neighbour" case the skill's `seam_blend` checklist exists for.
  - **Missing hand seam**: the hand seam pass (`is_hand_part`) never runs on Oblivion, because no hand mesh loads.
- **Evidence**: census `tools/body_slots.py` over direct CNTO items (LVLI counted as unknown):
  - **Hand slot (0x10)**: covered on 769 NPC_ (738 placements), not covered on 254 (212 placements), and LVLI-dependent on 1,459 (1,240 placements). Gauntlets and gloves are rare in Oblivion leveled outfits.
  - **Lower Body (0x08)**: 87 NPC_ have no covering item.
  - **Foot (0x20)**: 159 NPC_ have no covering item.
  - Both of those miss as well, and the player body has the same gaps.
- **Impact**:
  - Hands are missing on most Oblivion humanoids. Legs and feet are missing wherever clothing does not cover them, which includes naked or partly dressed prisoners, beggars and bandits.
  - Beast and mer torsos render human skin.
  - Necks of non-Imperial-skinned races are tone-shifted.
  - Two unit tests assert the nonexistent paths, so the gap is pinned rather than guarded.
- **Related**: #793, #3419, #4794 (its "four hand-NIF re-parses" are Oblivion misses), `/audit-gameplay` NPC spawn, `/audit-fnv` Dim 4 (`seam_blend` checklist).
- **Suggested Fix**:
  - Give Oblivion its own arm:
    - Male: `[upperbody, lowerbody, hand, foot]`.
    - Female: the `female*` twins.
    - Masks: 0x04 / 0x08 / 0x10 / 0x20.
  - Decode the body-section ICONs (index 0..=4, per gender) and swap them in at body import, before `build_seam_context` reads the neighbour textures.
  - Replace the two tests with a real-data pin that every Oblivion body path exists in `Oblivion - Meshes.bsa`.

### Dimension 4: Exterior & Lighting Data (Tamriel / SI streaming)

**Delta reviewed with no finding:**
- **#5374 / #5388 rung order on data**:
  - Tamriel children: rung 1 (the stamped parent bits) reaches Tamriel, which has no `CNAM`. The region rung returns `None` (0 of 211 REGN author a `CNAM`). Chain naming then finds TamrielClimate.
  - SEWorld children: rung 1 gives SEWorldClimate.
  - Water: the children resolve their parent's `NAM2` at Z=0.
- **#5423**: `terrain_lod_layout(Oblivion) = OblivionLegacy`, so Oblivion stays on the descent path, as it was before the `game ==` gate.
- **#5387**: the prune is reached only through the FalloutLegacy index.
- **#5421**: REGN `WNAM` rename. It has no consumer.
- **#5422**: PKIN (FO4).
- **#5424**: a shared rule; Oblivion's `WLST` resolves before the stand-in.
- **Oblivion `XCCM`**: 55 occurrences, all on interior CELLs (8 CLMT targets), so the exterior per-cell override path is unused on Oblivion.
- **Already reported this suite (not re-reported)**:
  - EXT-D1-2026-10-09-01: inherit-all is implemented twice. The parse stamp at `wrld.rs:228` is untested, and `/audit-exterior` routes it here as an Oblivion item; D4-01 below adds the consumer gap.
  - EXT-D1-04: REGN `CNAM` doc rot, including `env_translate.rs:503-504` and the dead `named_or_richest_climate` link.
  - ECS-2026-10-09-D7-01: every Tamriel↔city crossing is a worldspace teardown, so it is reachable on every Oblivion city gate.
- **Note, not filed**: `byroredux/src/cell_loader/exterior.rs:1963` still holds a stray line, `/// The climate the center cell's regions carry (Oblivion's region→climate`. It was orphaned when #5423 moved `region_climate_for_center` to `env_translate`, and rustdoc now attaches it to `load_one_exterior_cell`. It belongs to EXT-D1-04's doc-rot sweep from the same fix wave, so it is folded there rather than filed separately.

#### OBL-2026-10-09-D4-01: Oblivion child worldspaces draw no ground outside their own LAND footprint. The CS documents that a child uses its parent's landscape, and #5374 stamps the land and LOD inherit bits, but neither the parent's LAND nor its terrain LOD has a consumer.
- **Severity**: MEDIUM. Terrain and the horizon are missing around all 30 child worldspaces (every city, every Imperial City district, New Sheoth). The cities' own footprints are intact, and nothing crashes.
- **Dimension**: Exterior & Lighting Data (Tamriel). The mechanism is EXAL terrain / LOD (`/audit-exterior`).
- **Location**:
  - `byroredux/src/cell_loader/exterior.rs:2082-2100`: terrain spawns only from the child cell's own `cell.landscape`.
  - `byroredux/src/cell_loader/terrain_lod.rs:450-457`: the LOD textures are keyed on the child's own form ID ("Small worlds (AnvilWorld) ship none, so their synth blocks resolve no texture and are suppressed (#1745)"). They are holed against the child's cell map.
  - `crates/plugin/src/esm/cell/wrld.rs:228-235`: the parse stamp sets `INHERIT_LAND | INHERIT_LOD` for Oblivion children, "so the bit-gated walks resolve the parent's water/climate/LOD".
  - The only readers of those two bits are the `DNAM` water height and the `NAM3`/`NAM4` LOD water (`env_translate.rs:209-298`). Oblivion authors neither.
- **Status**: NEW.
  - `gh` all-states searches for "child worldspace terrain", "city worldspace", "parent landscape", "child worldspace LOD", "INHERIT_LAND" and "use land data" find only #5374 / #5388 (water and climate, closed), #2735 (`DNAM` land data) and #5335 (Oblivion LOD water, open; related but a different gap).
  - AUDIT_EXTERIOR_2026-10-09 does not cover it.
- **Description**:
  - **Source**: the CS wiki (`cs-uesp-wiki/Category/World Spaces.wiki`) says: "Parent Worldspace: If you select NONE, the worldspace will have its own, editable landscape. Otherwise, this worldspace will use the landscape of its parent." It adds that Climate / Water / Map / Usable Dimensions are "Sharable Data … only available if this worldspace has no Parent". That is also the first in-tree source for #5374/#5388's inherit-all rule.
  - **Data** (`tools/child_land.py`; the structural persistent CELL is excluded):
    - There are 30 children: 25 of Tamriel and 5 of SEWorld.
    - Between them they author 1,989 grid cells. 946 have their own LAND and **1,043 have none**. The parent has LAND at **1,040** of those 1,043.
    - The pattern is a city footprint with its own LAND, inside an empty, LAND-less ring that has no refs (`tools/gridmap.py`). Examples:
      - BrumaWorld: 13 / 43.
      - AnvilWorld: 22 / 35.
      - SkingradWorld: 21 / 34.
      - KvatchPlaza: 21 / 50.
      - TGTempleOfTheEmperorZero: 3 / 296.
    - `landscapelod\generated` ships for 18 root worldspaces (Tamriel 60, SEWorld 40728 in the SI archive, and 16 Oblivion planes) and for **0 of the 30 children**.
    - The children do ship their own object LOD, for example `distantlod\brumaworld_*.lod`. The placement ring already resolves that per worldspace key.
- **Evidence**:
  - A `grep` for any parent-LAND fallback in `cell_loader/`, `streaming/` and `crates/plugin/src/esm/cell/` finds none.
  - `stream_lod_blocks` uses `index.worldspaces[worldspace_key].form_id`, which is the child's.
- **Impact**:
  - From inside every Oblivion city or Imperial City district, nothing exists beyond the authored footprint: neither the parent's terrain nor its terrain LOD.
  - This is visible over the walls, from towers and hillside cities (Bruma, Skingrad, Chorrol, Kvatch), and at the horizon.
  - The stamped `INHERIT_LAND`/`INHERIT_LOD` bits suggest the inheritance is modelled when it is not.
  - **Unsourced**: whether vanilla fills the ring with the parent's full LAND or only with its LOD. Do not guess this. Capture it from the game before choosing.
- **Related**: #5374, #5388, EXT-D1-2026-10-09-01, #5335, #1745, #2735.
- **Suggested Fix**:
  - At the EXAL boundary, resolve a child's terrain source through the WNAM chain: where the child has no LAND, use the parent's LAND for that cell and/or the parent's terrain-LOD quads (form ID 60 / 40728), holed against the child's own LAND cells.
  - Let the stamped bits gate it, which retires the "dead" reading of `INHERIT_LAND`/`INHERIT_LOD` on Oblivion.
  - Pin it with a real-data test: every LAND-less child cell resolves a parent LAND.

### Dimension 5: Gameplay & UI Data Slice (M47.3, MenuXml HUD)

**Delta reviewed with no finding:**
- **`5c53ad9c7` (#5400)**:
  - The 16-bpp A4R4G4B4 / R5G6B5 files decode through the generic masks, and both are pinned.
  - The corpus census decodes 1,975 headers with 0 undecodable.
- **`6a132a967` (#5399)**: the include budget and fetch budget hold. All 89 Oblivion menus still parse.
- **`015e10549` (#5377)**: the mask arithmetic is covered by the census.
- **`431f74ce7` (#5278)**: doc only.
- **No changes**: `obscript_vm.rs`, `obscript_quests.rs` and `attach.rs`.
- **Still open, not re-filed**: #5033 (the `hud.rs` doc) and #5351 (the HNAM docs).

#### OBL-2026-10-09-D5-01: Phase V dialogue voice never resolves on Oblivion. The lookup needs NPC `VTCK` → `VTYP` and composes the FNV `.ogg` / 10-25-truncation name, but Oblivion authors neither record and ships its voices as race\sex\*.mp3. Every Oblivion greeting, which Phase G makes live on each activation, is silent.
- **Severity**: LOW (enhancement). The subtitle-estimate fallback works, and no wrong behaviour results.
- **Dimension**: Gameplay & UI Data Slice. The mechanism is owned by `/audit-gameplay` / `/audit-audio`.
- **Location**:
  - `byroredux/src/systems/dialogue_voice.rs:235-246`: the voice-type resolve.
  - `byroredux/src/systems/dialogue_voice.rs:71-100`: `voice_path_candidates`, which hard-codes `.ogg`, the FNV truncation and the voice-type folder.
  - `byroredux/src/systems/dialogue_voice.rs:1-8` and `docs/engine/dialogue-trees.md:178-188`: both call the FNV shape "Bethesda's authored convention".
- **Status**: NEW.
  - `gh` searches for "Oblivion voice", "mp3 voice" and "Phase V voice" find #5433 (open, a sibling: the FO3 profile mounts no sound archives) and #5395/#5393 (closed, FO3/FNV naming).
- **Description**:
  - `Oblivion.esm` authors 0 `VTCK` on its 2,482 NPC_ and 0 `VTYP` records, so `index.voice_types.get(npc.voice_form_id)` is always `None`. The lookup returns at `debug` level.
  - The authored convention, from `Oblivion - Voices1/2.bsa` (77,780 entries: 38,898 `.mp3` and 38,882 `.lip`), is `sound\voice\oblivion.esm\<voice race full name, lowercased>\<m|f>\<quest EDID>_<topic EDID>_<formid 8-hex>_<n>.mp3`. Example: `…\high elf\f\dark17following_greeting_0000566f_1.mp3`.
    - The race folders are argonian, breton, dremora, high elf, imperial, nord and redguard. Other races borrow a folder through RACE `VNAM` voice races. That field is already decoded for Oblivion as `RaceRecord::voice_forms` (`race.rs:591`) but has no consumer.
    - Names are **not** truncated: quests run up to 22 characters, and quest + topic up to 46.
- **Impact**:
  - No Oblivion line is voiced, including the generic greetings that every activation now opens.
  - The module doc presents the FNV rule as universal, so an Oblivion implementer gets no warning.
- **Related**: #5367 Phase V, #5433, #5395, #5393, OBL-2026-10-08-D2-01 / #5397 (Phase G liveness).
- **Suggested Fix**:
  - Add a per-game voice-path table entry for Oblivion: the speaker's race, mapped through `RACE.VNAM` (by sex) to the voice race's FULL name; the `m`/`f` folder; no truncation; `.mp3`.
  - Until then, record Oblivion as out of scope in `dialogue_voice.rs` and dialogue-trees.md.

## Regression Guard List (all hold, live 2026-10-09)
- **Stride-drift family #1506-#1509**: 0 truncating, 0 unknown, histogram parity and corpus-total parity.
- **#170 dual BSStreamHeader band**, the **`NiTexturingProperty`** `u32` count, **`MORPH_LEGACY_CUTOFF`** (10) and the **#3926** contradiction guard: no commits since 10-08. The corpus lane is green.
- **BSA v103**: no commits. The brute-force sweep was not re-run; it was green on 10-08 and the reader has not changed. This run's Python lister read the Meshes, Textures, SI and Voices archives; on the two mesh archives its entry counts (20,182 and 3,017) match the reader's logged file counts.
- **`parallax_alpha_gate_tests`**: 5/5. `oblivion_apply_hilight2_binds_the_derived_normal_as_its_height_map`: pass.
- **Disney-BSDF gate**: no material commits on the Oblivion path.
- **#5397**: the positional Random stack is verified against TES4 INFO `DATA`.
- **#5374 / #5388**: inheritance is verified on data. The only test-coverage gap is the parse stamp (EXT-D1-01).
- **#5189 `__MAX_Default_Light`**: the 24 spawn-gate tests pass.
- **`placement_lod_supported_is_oblivion_only`**: plus 12 other `placement_lod` tests, all pass.
- **M47.3 quest corpus** and **MenuXml 4/4**: pass.

## Open Work
- **NPC assembly (D3-01)**: the Oblivion body paths, masks, race body skins and `seam_blend` neighbour skin.
- **AI packages**:
  - The legacy `PKDT` and 12-byte `PTDT` (D2-01).
  - FO3's 26 twelve-byte `PTDT` (route to `/audit-fo3`).
- **Exterior**:
  - Child-worldspace parent landscape (D4-01).
  - Region weather (#3301).
  - Distant water (#5335).
  - Inherit-stamp test coverage (EXT-D1-01).
- **Dialogue**:
  - Oblivion voice (D5-01).
  - Oblivion's known-topic list model (#5350).
  - INFO result scripts and Add Topics (M47.3 phase 2).
- **From ROADMAP**:
  - The CHARAL Oblivion ruleset is unwired; there is no AVIF, so the legacy actor-value resolver comes first. This also leaves the HUD bars full.
  - Navigation is PGRD-only.
- **Device-bound gates not run under suite constraints**: `m-exteriors.sh oblivion`, `p0-door-interaction.sh oblivion` and `m48-4-oblivion-hud.sh`. D3-01 and D4-01 are both visible in a city capture, so a capture would confirm them.

## Summary

| Severity | NEW | Regression | Existing (referenced) |
|---|---|---|---|
| CRITICAL | 0 | 0 | — |
| HIGH | 1 (D3-01) | 0 | ECS-D7-01 (reachable on Oblivion city crossings) |
| MEDIUM | 2 (D2-01, D4-01) | 0 | — |
| LOW | 1 (D5-01) | 0 | EXT-D1-01, EXT-D1-04 (+ the `exterior.rs:1963` note) |

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-10-09.md`. Label every finding `game:oblivion` and `legacy-compat`, plus:
- **D3-01**: `high bug gameplay import-pipeline`.
- **D2-01**: `medium bug esm-plugin ai`.
- **D4-01**: `medium bug terrain-exterior`.
- **D5-01**: `low enhancement dialogue audio`.
