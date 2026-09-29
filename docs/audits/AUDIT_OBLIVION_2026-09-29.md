# Oblivion (TES4) Compatibility Audit — 2026-09-29

**HEAD**: `9fcfdc3fc` · **Baseline**: `docs/audits/AUDIT_OBLIVION_2026-09-22.md` (HEAD `ee6d3fb39`) · **Audited**: Dimensions 1-5 (each had commits on its Paths since 2026-09-22; delta-reviewed) · **Unchanged since baseline (skimmed)**: none at dimension level. Within Dim 3, `byroredux/src/npc_spawn/seam_blend.rs` had no commits and was only skimmed.

This run is part of the `/audit-suite --preset comprehensive` run of 2026-09-29. It was run solo, with no sub-agents, one dimension at a time, against the real vanilla Oblivion + DLC data at `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`. The engine was not launched, and no Vulkan process or smoke script was run. Scratch files: `/tmp/audit/oblivion/dim_{1..5}.md`.

## Executive Summary

**Corpus lane: GREEN.** Command: `cargo test -p byroredux-nif --release --test parse_real_nifs --test per_block_baselines --test block_coverage_baselines --test oblivion_stream_drift_corpus -- --ignored oblivion no_block_sizes`. It was re-run with `BYROREDUX_REQUIRE_GAME_DATA=1 --nocapture` to rule out a silent skip.
- `parse_rate_oblivion`: **9,612 / 9,612 clean (100.00%)**. 0 truncated, 0 failed, across 9 archives: Meshes 8,032; Shivering Isles 1,438; Knights 75; and 6 small DLC.
- `oblivion_block_count_parity`: 9,612 whole, 0 truncating.
- `per_block_baseline_oblivion`: histogram equals `oblivion.tsv`.
- `no_block_sizes_drift_detector_has_zero_false_positives_on_real_corpus`: PASS.
- This matches the ROADMAP compatibility-matrix row (100%, 9,612 / 9,612).

**Compatibility level, measured live:**
- **NIF (including the v10.x tail)**: 100%, as above.
  - `b7491072f` changed the pre-sizing (`allocate_vec_sized` / `allocate_vec_min_bytes`) and the pre-4.2.1.0 bounding-volume layouts. Parity stays green, so neither moved an Oblivion byte.
  - Three meshes were traced through `import_nif_scene`: chandelier01, wolfhead, broadsheet01. Each imported mesh count equals the file's `NiTriStrips` count.
- **BSA v103**: 17 archives, 147,629 files, 6,770 MB, 0 errors. This survives the lock-free positional-read rewrite (`1b8b21f3f`).
- **ESM**: both parity pins are green (`clas_oblivion_knight_against_vanilla`, `race_oblivion_data_and_subs_against_vanilla`).
  - A raw census re-confirmed the SKILL authoring counts: CLOT 604, PGRD 8,228, SCPT 2,393, QUST 390, INFO 19,278, WTHR 37, LTEX 229, LVSP 306.
- **Render path**: all guards green.
  - `parallax_alpha_gate`: 5/5.
  - Dark-role census: Oblivion 8 in 6 files; every other game 0.
  - `dark_combine_is_the_pinned_bare_multiply_in_both_paths`: pass.
  - Oblivion leg of the torch-emitter guard: 272 emitters, each with params, rate and budget.
  - The new opaque early-fragment-test pipeline keeps every Oblivion alpha-tested and alpha-blended draw on the late-test module.
- **Exterior & lighting**: all five Oblivion guards green.
  - LTEX paths, default land textures, the HNAM dimmer, the pre-Skyrim falloff sentinel, and `placement_lod_supported_is_oblivion_only`.
  - `dbb265dfc` fixed Oblivion's north/south-mirrored distant-terrain UVs. Visual acceptance still needs `m-exteriors.sh oblivion static`, which was not run.
- **Gameplay & UI**:
  - The M47.3 quest-script corpus gate is green, and the ObScript VM gained a recursion cap (32) on nested If chains and nested `X` call arguments.
  - The MenuXml Oblivion corpus is 3/3.

**Findings: 0 CRITICAL, 1 HIGH, 0 MEDIUM, 3 LOW.** The headline is OBL-2026-09-29-D2-01, which answers the question the LC audit left open. The LC audit found 0 of 2,190 ACHR carrying `0x200`. Oblivion marks a corpse on its *base* `NPC_`/`CREA` record: header flag `0x80000` "Starts Dead" (xEdit TES4), authored in the CS as 0 Health. 135 bases carry the flag, and 132 of them also author 0 Health. This is census-confirmed: all 787 refs placing a flagged base are the authored corpses, and they carry 771 of the 775 `XRGD` refs. The engine reads neither the flag nor the base health, so all 787 spawn alive.

## Dimension Findings

### Dimension 1: NIF Version Handling & Corpus Integrity — clean
- Corpus lane: green (see above).
- 13 commits reviewed:
  - `b7491072f`: pre-sizing, `NiGeomMorpherController` weights at 4 B before 20.1.0.3, BASE_BV / LOZENGE / version-split HALF_SPACE.
  - `e2f99ad55`: bulk-array checked copy.
  - `d98769436`: dead constraint-stub histogram removed. It is block_size-only, so it is inert on Oblivion.
  - `a6bcec6e2` (#4560): legacy `NiPSysEmitterCtlr.data_ref`, pre-10.1.0.104.
  - `9281eebd7` (#4561): particle-leaf `APP_CULLED`.
  - The remaining commits touch BSTriShape, Starfield or the renderer.
- Pins still in source:
  - #170 dual-band pin (`header.rs:743`).
  - `uses_inline_block_type_names` (`lib.rs:172`).
  - `parse_ni_texturing_property_with_zero_shader_maps` (`properties_tests.rs:465`).
  - `MORPH_LEGACY_CUTOFF` gate (`morph.rs:116`, `:226`).
  - #3926 contradiction guard (`lib.rs:643-725`).
  - `as_ni_node` unwraps all 12 NiNode-wrapper structs.
- Tooling note: `target/release/examples/import_probe` was stale (2026-09-23) and printed no per-mesh lines until it was rebuilt.

### Dimension 2: BSA v103 & ESM Data Slice

#### OBL-2026-09-29-D2-01: Oblivion marks corpses on the *base* `NPC_`/`CREA` record (header flag `0x80000` "Starts Dead", authored with 0 Health), not on the placement, so all 787 placed Oblivion corpses spawn alive
- **Severity**: HIGH. This is the same impact class as #4814 (HIGH) and FNV-2026-09-29-D2-01.
- **Dimension**: BSA v103 & ESM Data Slice. The decode is owned by `/audit-esm` and consumption by `/audit-gameplay`; the Oblivion data half is owned here.
- **Location**:
  - `crates/plugin/src/esm/cell/walkers.rs:1190-1195`: `starts_dead = ACHR && Tes5Plus && flags & 0x200`, a placement-only signal.
  - `crates/plugin/src/esm/reader.rs:106-111`: `FLAG_STARTS_DEAD = 0x200`, cited from the TES5 list only.
  - `crates/plugin/src/esm/cell/mod.rs:569-574`: the field doc says "Oblivion is excluded because … no Oblivion source was checked".
  - `crates/plugin/src/esm/records/actor/mod.rs`: the `NPC_`/`CREA` parsers keep no record-header flags; the only `0x80000` uses in `crates/plugin/src` are unrelated MGEF, SOUN and quest-alias bits. `:1167` decodes creature `DATA` for `Fallout3NV` only, and the Oblivion `NPC_` `DATA` arm (`:1953`) reads no health.
  - `byroredux/src/cell_loader/references/mod.rs:747-754`: the only consumer, `apply_starts_dead`.
- **Status**: NEW.
  - `gh` searches for "corpse", "starts dead" and "XRGD" find only #4814, which is closed and fixed the Skyrim/FO4 bit.
  - This is the Oblivion member of the FNV-2026-09-29-D2-01 / SKY-D4-2026-09-29-01 family, but its marker is different: a flag on the base record, not `XRGD` presence and not the ACHR bit.
  - LEGACY_COMPAT today showed that the `0x200` exclusion drops nothing. It did not establish what Oblivion uses instead.
- **Description**:
  - #4814 decodes "Starts Dead" as ACHR record-flag `0x200` on the 24-byte header family only.
  - TES4 puts "Starts Dead" on the base actor instead. xEdit's TES4 flag lists give `19, 'Starts Dead'` for both `CREA` (`wbDefinitionsTES4.pas:1942`) and `NPC_` (`:2783`), which is header bit `0x80000`.
  - The Construction Set's authoring rule is the same fact from the tool side: a base actor with 0 Health spawns dead.
  - The engine decodes neither the base flag nor the base health, so every Oblivion actor placement spawns alive.
- **Evidence**:
  - Sources:
    - xEdit `wbDefinitionsTES4.pas:1942` and `:2783` (local copy `/tmp/audit/esm/xedit/`).
    - CS wiki, "Stats Tab - Creatures" (cs.uesp.net): *"If you specify 0 health for a creature, it will automatically spawn in the cell as dead, just as NPCs do."*
    - Health layouts: UESP `Oblivion_Mod:Mod_File_Format/NPC_` gives `DATA` Health as `ulong` @21; `.../CREA` gives it as `ushort` @6.
  - Raw census of `Oblivion.esm`, this run:
    - `0x200` appears on 0 of 2,190 ACHR and 0 of 1,473 ACRE. The flags seen are `0x400`, `0x800` and `0x8000`.
    - Base flag `0x80000` is set on 135 bases: 87 `NPC_` and 48 `CREA`. 132 of them also author 0 Health; 3 `CREA` have non-zero health. No base without the flag authors 0 Health.
    - Refs placing a flagged base: 156 ACHR + 631 ACRE, for **787** in total; 771 of them carry `XRGD`.
    - Only 4 `XRGD` refs sit on an unflagged base.
    - Examples: `MS12SkeletonDeadState` `CREA` header flags `0x80000`, `DeadSkeleton` `0x80400`, `DeadBanditMale01` `NPC_` `0xC0400`.
    - Top bases: DeadSkeleton 253, DeadSkeleton2 108, DeadZombieHeadless 49, DeadZombie 37, RatDead 30, SE09DeadFailedExperiment03 29, MS12SkeletonDeadState 24, DeadGoblin 15.
- **Impact**:
  - Every authored Oblivion dungeon corpse (skeleton piles, dead adventurers and bandits, Shivering Isles experiment and war dead) spawns as a live actor.
  - It stands or idles, runs its AI packages, and is offered as a container rather than a corpse (`interaction.rs:1216` keys `InteractionKind::Corpse` on `Dead`).
  - Its `XRGD` pose is also unused (the SKY-D4-2026-09-29-01 half).
- **Related**: #4814 (closed), FNV-2026-09-29-D2-01, SKY-D4-2026-09-29-01, LEGACY_COMPAT 2026-09-29 Dim 3 census.
- **Suggested Fix**:
  - At the parse boundary, keep the Oblivion `NPC_`/`CREA` record-header bit `0x80000` ("Starts Dead", per xEdit TES4) on the actor record.
  - Derive `starts_dead` for Oblivion placements whose resolved base carries it. The `PlacedRef` cannot see its base, so do this at the translate step that already resolves `record_index.actor`, not in the walker.
  - Pin a real-master census: 135 flagged bases and 787 corpse refs.

#### OBL-2026-09-29-D2-02: #4415's magic runtime and consumables look up Oblivion `EFID` codes by FormID; `magic_effects_by_code` has no consumer
- **Severity**: LOW. It is latent: Oblivion has no AVIF records and `resolve_actor_value` has no Oblivion arm, so nothing would apply today even with the right MGEF.
- **Dimension**: BSA v103 & ESM Data Slice.
- **Location**:
  - `crates/scripting/src/magic.rs:92`: `index.magic_effects.get(&effect.effect_form_id)`.
  - `crates/plugin/src/consumables.rs:115`, `:142`, `:179`.
  - `crates/plugin/src/esm/records/dispatch_misc_gameplay_b.rs:36-50`: the side index was built "so the (pending) magic-system runtime can resolve EFID lookups on Oblivion".
  - `crates/plugin/src/esm/records/index.rs:223`: "not yet read by any consumer".
- **Status**: NEW.
  - Related to ESM-2026-09-29-D2-02 (the `EFID` remap warnings). That finding already exists and is not re-filed here.
  - Its routing note gives this consumer requirement to `/audit-oblivion`.
- **Description**:
  - `cd4fc019a` (#4415) landed the "pending" magic runtime, but translated spells through `magic_effects` keyed by FormID.
  - On Oblivion the `EFID` is a 4-char code, so every constant-effect lookup misses and `continue`s silently. `consumables::restoration` misses the same way through `?`.
- **Evidence**:
  - `Oblivion.esm` `SPEL` `EFID`s are 149 distinct codes (FOAT 136, REAT 112, DRAT 108, REHE 92, SEFF 89).
  - As little-endian u32 values (for example `0x5441_4F46`) none of them can equal an Oblivion MGEF FormID.
  - `rg magic_effects_by_code` outside `index.rs` and `dispatch_misc_gameplay_b.rs` finds nothing.
- **Impact**: when the legacy actor-value resolver lands, Oblivion abilities, birthsign and racial constant effects, and potions will still translate to nothing, with no log.
- **Related**: ESM-2026-09-29-D2-02, #4415, #969.
- **Suggested Fix**: add a single `EsmIndex` accessor that routes an effect id through `magic_effects_by_code` on Oblivion, and use it in `magic.rs` and `consumables.rs`. Pin it with an Oblivion `EFID` fixture (`b"FOAT"`).

#### OBL-2026-09-29-D2-03: `EsmIndex::clothing` doc says "~150 vanilla records"; `Oblivion.esm` has 604 CLOT
- **Severity**: LOW (doc-rot).
- **Dimension**: BSA v103 & ESM Data Slice.
- **Location**: `crates/plugin/src/esm/records/index.rs:445-448`.
- **Status**: NEW. The SKILL text records it, but no issue was ever filed.
- **Evidence**: raw census, CLOT = 604.
- **Suggested Fix**: state 604 (`Oblivion.esm`), and state that CLOT enters inventory as `ItemKind::Armor` via `parse_clot`.

The other Dim 2 deltas were reviewed with no finding:
- #4638 LVLO/LVLD pins are present (`container.rs:418`, `:462`).
- #4813 `0x800` is decoded on Oblivion.
- The GRUP skip clamp.
- The TES4 light/medium-master bits (Oblivion has none).
- DIAL QSTI is kept.
- IMGS is None on Oblivion.
- RACE head-part textures (the RACE pin is green).
- `CellData.pathgrids` still has no consumer. This is known-open per the SKILL and is not re-filed.

### Dimension 3: Legacy-Property Rendering Path — clean
- All guards are green (Executive Summary).
- Texture prefetch (`a3632909a`): `texture_prefetch_paths` predicts the derived `_n` normal and height map when the normal slot is empty (`nif_import_registry.rs:363-387`).
- `DrawCommand::allows_early_fragment_tests` (`types.rs:374`) admits only kind-0 draws with no alpha test, no alpha blend and no decal flag. POM writes no `gl_FragDepth`.
- The Oblivion dungeon light-beam → `FogVolume` converter (`fog.rs:926`) is already filed today as NIFAL-D1-2026-09-29-02. It is cross-referenced here, not re-filed.
- #4965 greened the Oblivion PBR-override lane. The Disney gate is still 0.
- The APPLY_HILIGHT2 BC1 `_n.dds` census (1,274 / 100) was not re-measured, because there is no in-tree census tool.

### Dimension 4: Exterior & Lighting Data (Tamriel) — no new findings
- All five Oblivion guards are green.
- #4898: Oblivion LOD quads now run U east / V north, and the corner UVs are pinned.
- #4899: LTEX editor-ID cover affinity. It changes the classification of 34 of 229 Oblivion LTEXs.
- Cross-references, not re-filed:
  - Existing **#4910** and EXT-D5-2026-09-29-01: Oblivion's Cartesian WATR wind vector still takes the +90° bearing rotation.
  - Existing **#4909**: the froxel sun ignores the HNAM dimmer.
  - NIFAL-D1-2026-09-29-02: the beam FogVolume.

### Dimension 5: Gameplay & UI Data Slice (M47.3, MenuXml HUD)

#### OBL-2026-09-29-D5-01: The M47.3 real-data gate silently passes without `Oblivion.esm`, and no CI lane runs it
- **Severity**: LOW (test-gap).
- **Dimension**: Gameplay & UI Data Slice.
- **Location**:
  - `crates/scripting/src/obscript_quests.rs:398-409`: `if !path.is_file() { return; }`, with no strict-mode failure.
  - `.github/workflows/real-data-gates.yml:186`: the strict parser lane covers `bsa bgsm sfmaterial hkx facegen menuxml`, not `scripting`.
- **Status**: NEW. It is the same class as PAR-D4-2026-09-29-01 (the Oblivion EGM test in the facegen crate), but it is a different crate and test, and that finding does not cover it.
- **Evidence**:
  - The test resolves `BYROREDUX_OBLIVION_DATA` by hand and returns green when the file is absent, even under `BYROREDUX_REQUIRE_GAME_DATA=1`.
  - The scheduled strict lane never builds `byroredux-scripting`.
- **Impact**: this is the only real-data guard over the 255 vanilla quest scripts (ObScript framing and command ids). A regression there lands green on any run without the data.
- **Suggested Fix**:
  - Resolve the path via `byroredux_plugin::esm::test_paths::oblivion_esm()`.
  - Under `BYROREDUX_REQUIRE_GAME_DATA`, fail instead of returning, as the per-crate `require_game_data` helpers do (`crates/bsa/tests/bsa_real.rs:38`, `crates/facegen/tests/parse_real_facegen.rs:68`).
  - Add the single named test to the strict lane. It uses about 160 MB resident, unlike the whole-crate plugin `--ignored` run.

Also reviewed:
- `2f8538334` adds a nesting cap of 32 on If chains and nested `X` arguments, with 2 new pins. The real-data gate stays green.
- The MenuXml hardening commits (#4650-#4652, #4715, #4716, #4718) belong to `/audit-ui` and `/audit-parsers`.
- Cross-references, not re-filed:
  - UI-D7-2026-09-29-02: stale `hud.rs` comments on the Skyrim-keyed bar source.
  - Existing #4723: `--menu` + `--hud` focus orphan.
  - SCR-D3-01 (GetIsID base vs placed): the Oblivion DIAL speaker fallback path.
- The Oblivion bars drawing full is known-open: no AVIF, and the legacy actor-value resolver is unbuilt (ROADMAP Status).

## Regression Guard List (all hold, live 2026-09-29)
- Stride-drift family #1506-#1509: corpus lane green; 0 truncating, 0 unknown, histogram parity.
- #170 dual BSStreamHeader band: pin present and unchanged.
- `NiTexturingProperty` reads an unconditional `u32` shader-map count: pin present.
- BSA v103 sweep: 147,629 files, 0 errors.
- `parallax_alpha_gate_tests`: 5/5.
- Disney-BSDF gate (`MAT_FLAG_PBR_BSDF` == 0 for Oblivion): unchanged. The #4965 PBR-override lane is now meaningful.
- 16-byte ACBS (#1650) and Oblivion LVLO/LVLD (#4638): pins present.
- Dark-role census: 8 meshes in 6 files, Oblivion only.

## Open Work
- **Placed corpses** (OBL-2026-09-29-D2-01) are the only Oblivion-specific HIGH. The decode is small and census-verifiable.
- From ROADMAP Status:
  - Oblivion's CHARAL ruleset is built but unwired. `Oblivion.esm` has no AVIF, so a legacy actor-value resolver comes first. That blocks the HUD bars, the magic runtime (OBL-2026-09-29-D2-02) and regen.
  - M47.3 phase 2: object-script blocks, Message UI and actor-state functions.
  - PGRD-only navigation has no path-graph consumer.
- Device-bound gates not run under this audit's constraints:
  - `m-exteriors.sh oblivion static`, which gives visual acceptance for the #4898 LOD UV fix.
  - `p0-door-interaction.sh oblivion`.
  - `m48-4-oblivion-hud.sh`.
- Interiors and the Tamriel exterior already render, so neither is a blocker.

## Statistics
- Dimensions audited: 5/5.
- Commits reviewed: about 130 unique on Oblivion Paths (232 in the repo since the baseline).
- Real-data gates run live: 4 corpus-lane tests, 2 BSA, 2 ESM, parallax 5, dark-role census, dark-combine, torch emitters, 5 exterior guards, the quest corpus, and MenuXml 3.
- Raw censuses (Python walk of `Oblivion.esm`): ACHR/ACRE flags, `XRGD`, base health, record counts, SPEL `EFID`s.
- Findings: 0 CRITICAL / 1 HIGH / 0 MEDIUM / 3 LOW. All are NEW. There are no regressions.

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-09-29.md`. Label every finding `game:oblivion` + `legacy-compat`, plus its domain:
- D2-01: `high bug esm-plugin gameplay`.
- D2-02: `low bug esm-plugin`.
- D2-03: `low documentation doc-rot`.
- D5-01: `low bug test-gap scripting`.
