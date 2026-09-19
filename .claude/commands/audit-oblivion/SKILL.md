---
description: "Per-game audit of Oblivion (TES4) compatibility — NIF v20.0.0.4 + the v10.x NetImmerse family, BSA v103, live ESM path"
argument-hint: "--focus <dimensions>"
---

# Oblivion Compatibility Audit

Oblivion is the **oldest** title and exercises code no other game reaches. This audit owns **Oblivion's data through the shared mechanisms** (routing: `.claude/commands/_audit-owners.md`): only dimensions whose Paths are Oblivion-specific stay here; a defect in the shared mechanism goes to its owner audit.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

Read `.claude/commands/_audit-common.md` (layout, methodology, dedup, finding format) and `.claude/commands/_audit-severity.md` for shared protocol.

## What Makes Oblivion Different

Two NIF eras share `Oblivion - Meshes.bsa`: the **retail body** (v20.0.0.4, `bsver` 11, plus a v20.0.0.5 minority; inline strings, u16 flags) and a **v10.x NetImmerse tail** (down to pre-Gamebryo v3.3.0.13) with tight nif.xml version bands. **Neither has a per-block size table**: one N-byte under-read discards the rest of the file (no `block_size` recovery as on 20.2.0.7+), so drift is silent and Oblivion-only. The v10.x stride-drift family (#1506-#1509) is resolved — a **regression-guard set**. The 2026-09-05 CRITICAL (a `NiSkinPartition` reservation: clean rate 100% -> 92.41%, 730 of 9 612 meshes lost) is what a skipped corpus run looks like.

| Aspect | State (verify; pull live figures) |
|---|---|
| NIF | Baselines: `crates/nif/tests/data/per_block_baselines/oblivion.tsv`, `crates/nif/tests/data/block_coverage_baselines/oblivion_truncations.tsv` (`truncating=0`; 9 612 NIFs = base + 8 DLC archives) |
| BSA | v103; 17 archives / 147 629 files extract with 0 errors. Folder record is 16 B for v103 **and** v104 (24 B only for v105, `open.rs`) |
| Exterior | Tamriel `(0,0)` radius 1 renders on-device; live figure in `docs/engine/exterior-readiness-plan.md` (#2377 / #2368 closed); `ROADMAP.md` Oblivion row |
| Runtime baseline | `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv` |
| Reference data | `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/` |

**Authoring census** (byte-scan of `Oblivion.esm`, 2026-09-19): 2 393 SCPT, 390 QUST, **604 CLOT**, 8 228 PGRD, **0 NAVI / NAVM**, 19 278 INFO with **0 `PNAM` / 0 `ANAM`**, 306 LVSP, 229 LTEX, 37 WTHR (all with `HNAM`). No `locomotion\` directory (the walk clip is `handtohandforward.kf`); 9 889 `.lod` names in `Oblivion - Meshes.bsa`.

## Parameters (from $ARGUMENTS)

`--focus <dimensions>`: comma-separated numbers (e.g. `1,3`). Default: all 5.

## Phase 1: Setup

1. Parse `$ARGUMENTS`; `mkdir -p /tmp/audit/oblivion`.
2. Dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
3. Confirm `Oblivion/Data/` exists; if not, name the dimensions that lose real-data validation.
4. **Run the corpus lane first** (release build; the plain `cargo test` lane never runs it): `cargo test -p byroredux-nif --release --test parse_real_nifs --test per_block_baselines --test block_coverage_baselines --test oblivion_stream_drift_corpus -- --ignored oblivion no_block_sizes`. Any red gate is the headline finding.
5. Scope by delta: `git log --since=<last AUDIT_OBLIVION date> --format='%h %cs %s' -- <dimension Paths>`.
6. Toolchain: the `byroredux` bin crate needs rustc >= 1.94 — rustup cargo per `docs/contributing.md` § Toolchain note (#4466).
7. Always name `Oblivion.esm` real-data tests (cheap: ~3 s): a whole-crate `cargo test -p byroredux-plugin -- --ignored` spikes >20 GB and can kill the session.

## Phase 2: Launch Dimension Agents (parallel)

### Dimension 1: NIF Version Handling & Corpus Integrity (v20.0.0.4 + the v10.x tail)
**Subagent**: `legacy-specialist`
Routing: generic stream position / dispatch / version gating -> `/audit-nif`; collision translation -> `/audit-nifal`, `/audit-physics`. This dimension owns the Oblivion-only bands and the corpus.
**Paths**: `crates/nif/src/header.rs`, `crates/nif/src/version.rs`, `crates/nif/src/stream.rs`, `crates/nif/src/lib.rs`, `crates/nif/src/blocks/` (controller/, properties.rs, legacy blocks), `crates/nif/tests/`
**First step**: the Phase 1 corpus lane; `git log --since=<date> --format='%h %cs %s' -- crates/nif/src/header.rs crates/nif/src/blocks crates/nif/src/lib.rs`
- Corpus gates are the guard: 0 truncating, 0 unknown, per-block histogram equal to `oblivion.tsv` across base + 8 DLC archives (`per_block_baseline_oblivion`, `oblivion_block_count_parity`, `no_block_sizes_drift_detector_has_zero_false_positives_on_real_corpus`). `unknown` growth or `parsed` shrinkage is a stride-drift regression — escalate, do not re-derive; diff the histogram for new block types.
- Header quirks (each pinned; aim at what the pin cannot see — a *new* off-band version): `user_version` exists only for >= v10.0.1.8; BSStreamHeader is the nif.xml dual band `V10_0_1_2 || (user_version >= 3 && (V20_2_0_7 || V20_0_0_5 || (V10_1_0_0..=V20_0_0_4 && user_version <= 11)))` (`bs_stream_header_not_read_for_off_spec_version`, #170); v10.0.0.0..10.1.0.114 blocks carry a leading 4-byte group_id (mixing stream-relative and file offsets is the classic false trail); pre-5.0.0.1 files inline block-type names, decided by version, not by an empty type table (`uses_inline_block_type_names`, #4257).
- `NiTexturingProperty` reads a `u32` shader-map count directly — no leading `Has Shader Textures` bool (nif.xml is wrong; Gamebryo 2.3 is authoritative). Pin: `parse_ni_texturing_property_with_zero_shader_maps` (`crates/nif/src/blocks/properties_tests.rs`, #149).
- `NiGeomMorpherController` trailing field gates on `bsver > 9` (`blocks/controller/morph.rs`): `doghead.nif` (v10.2.0.0 bsver 9) must skip it, bsver-11 rigs (`obgatemini01.nif`) keep it (`path_lookat_tests.rs`, #1509).
- The sizeless runtime-size-cache recovery must refuse a skip that contradicts the failed block (#3926, `crates/nif/src/lib.rs`) — a plausible-looking recovery is silent mis-alignment.
- Scene walk: `as_ni_node` (`crates/nif/src/import/walk/mod.rs`) unwraps every NiNode subclass (`BSOrderedNode`, `NiBillboardNode`, `NiSwitchNode`, `NiLODNode`, `BsFaceGenNiNode`); a missed subclass drops the whole subtree.
- Deep-trace 3 meshes (Heinrich Oaken Halls chandelier, a book, a creature head) through `import_nif_scene`: mesh count + material chain. Tools: `nif_stats`, `recovery_trace` (`crates/nif/examples/`).
**Output**: `/tmp/audit/oblivion/dim_1.md`

### Dimension 2: BSA v103 & ESM Data Slice (Oblivion.esm)
**Subagent**: `general-purpose`
Routing: reader discipline `/audit-parsers`; GRUP walk / `SubReader` / remap / dispatch `/audit-esm`. This dimension owns TES4-only decode branches and their real-data pins.
**Paths**: `crates/bsa/src/archive/`, `crates/plugin/src/esm/records/` (`actor/`, `items.rs`, `climate.rs`, `pathgrid.rs`, `misc/dialogue.rs`), `crates/plugin/src/esm/cell/walkers.rs`
**First step**: `cargo test -p byroredux-bsa --release --test bsa_real -- --ignored oblivion`, then `cargo test -p byroredux-plugin --release --test parse_real_esm -- --ignored clas_oblivion_knight race_oblivion` (single named tests).
- **BSA v103** is a regression guard (#699): `oblivion_all_bsas_v103_brute_force_extract_zero_errors` sweeps all 17 archives; `embed_file_names` gates on `>= BSA_V_FO3_SKYRIM`, and the "Xbox archive" flag several v103 archives set must not change name handling.
- **16-byte ACBS** (#1650): Oblivion `NPC_`/`CREA` ACBS is 16 B (flags u32 @0, level i16 @10), distinct from the >=24 B FNV/FO3/Skyrim layout, so `parse_npc` needs the `GameKind::Oblivion` arm gated on `len >= 16` *before* the FNV arm (else level defaults to 1 and every actor reads Male). Pins: `oblivion_16byte_acbs_parses_level_and_gender`, `fnv_ignores_16byte_acbs`.
- Other TES4 branches: MGEF-by-code map (`oblivion_mgef_populates_magic_effects_by_code`), CONT 4-byte payload guard, CLMT three-entry WLST keyed on `GameKind`, RACE/CLAS `is_oblivion` / `flags_oblivion` arms (real-data pins above), XCLL sizes `[28, 32, 36]` with the >= 92 ambient-cube arm **game-validated** to Skyrim/FO4/FO76 (`crates/plugin/src/esm/cell/tests/cell.rs`).
- **CLOT enters inventory** as a wearable `ItemKind::Armor` (`parse_clot`: BMDT slots, MODL/MOD3 male/female meshes; SLGM likewise). Pin: `oblivion_clothing_enters_inventory_and_gender_aware_equipment` (`records/tests.rs`). The `EsmIndex.clothing` doc says "~150 records"; the master has 604.
- **Navigation is PGRD-only** (`pathgrid.rs`, 8 228 grids; layout census in its header). As of 2026-09-19 `CellData.pathgrids` has no consumer beyond the walker: `docs/engine/navmesh-pathfinding.md` covers NAVM only, so Oblivion packages have no path graph.
- DIAL/INFO: with zero `PNAM` / `ANAM`, speakers resolve through the `GetIsID` condition fallback and `build_conversation_tree` must order without PNAM chains (#3600); TCLF / NAME / CTDT and multi-segment responses are kept (#3614, #3616); LVSP (306) dispatches (#3617); SBSP (33) / ROAD (2) are recorded in `RecordType` as deliberate non-goals (#3619).
**Output**: `/tmp/audit/oblivion/dim_2.md`

### Dimension 3: Legacy-Property Rendering Path (highest historical yield)
**Subagent**: `renderer-specialist`
Routing: the single material boundary / no render-time fallback / emitter translation `/audit-nifal`; Disney-BSDF gate (`MAT_FLAG_PBR_BSDF` must be 0, no BGSM/`.mat`) `/audit-renderer`.
**Paths**: `crates/nif/src/import/material/` (`legacy_properties.rs`, `walker.rs`), `byroredux/src/cell_loader/spawn/mesh_instance.rs`, `byroredux/src/scene/nif_loader.rs`, `byroredux/src/render/static_meshes.rs`, `byroredux/src/asset_provider/texture.rs`, `crates/renderer/shaders/include/material_sampling.glsl`
**First step**: `cargo test -p byroredux --bin byroredux parallax_alpha_gate` (rustup toolchain, see Phase 1) and `git log --since=<date> --format='%h %cs %s' -- crates/nif/src/import/material byroredux/src/render byroredux/src/cell_loader/spawn`
- **`APPLY_HILIGHT2` is Oblivion's parallax flag.** The importer records `parallax_height_in_alpha` on the intent alone (`parallax_map.is_none()`) because Oblivion authors no normal slot (14 `bump_texture` uses in the whole mesh archive pair); normals resolve by filename (`derive_normal_map_path`, #1303) downstream of `MaterialInfo`, and the asset provider binds that `_n.dds` into the height slot (`mesh_instance.rs`, `nif_loader.rs`, #3596). Height is the normal's **alpha**: when the bound normal has none (BC1/BC4/BC5 return `A = 1.0`, sliding the whole surface) the render site must withhold `PARALLAX_ALPHA_HEIGHT_BIT` **and zero the height slot** (#3562, #4260). Guard: `byroredux/src/render/parallax_alpha_gate_tests.rs`; it cannot see the real alpha-format split — 2026-09-11 census: 1 274 `APPLY_HILIGHT2` properties, 100 with a BC1 `_n.dds`; re-measure.
- The bit is bit 31 of `parallaxMapIndex`: every shader reader must mask it (`textures[0x8000000N]` is out of bounds) and **both** POM marchers (`material_sampling.glsl`, secondary-ray `ray_hit.glsl`) must honor the channel. Scale defaults are the named `DEFAULT_PARALLAX_*` constants, never literals (#4263).
- Apply modes 1 (`APPLY_DECAL`) and 3 (`APPLY_HILIGHT`) are decoded and unconsumed: 681 of 30 121 instances are non-default and their PC semantics are unsourced (#3625, 2026-09-11) — no value is guessed for them.
- Legacy properties (pins in `crates/nif/src/import/material/`: `emissive_source_tests.rs`, `alpha_flag_tests.rs`, `double_sided_tests.rs`, `stencil_state_capture_tests.rs`): raw monitor-space `NiMaterialProperty` colors; all 11 AlphaFunction values routed, per-slot blend fallback (#4262); `NiWireframeProperty` -> LINE pipeline and `NiShadeProperty.flat_shading` -> fragment shader (#869); `NiStencilProperty` dormant except the two-sided promotion (#930).
- **Emitters**: 140 of 208 emitter-bearing Oblivion NIFs carry several `NiPSysEmitter`s (`obgatemini01.nif` 11, `transformation.nif` 13); each `ImportedParticleEmitter` needs its **own** kinematics / curve / rate, not the first match (#4261, then #4467 when that fix lost rates behind opaque sibling controllers). Oblivion's leg of `real_archive_torch_meshes_surface_particle_emitters` (`--ignored`) is the guard; the pre-Skyrim emitter layout is version-gated (#1239).
**Output**: `/tmp/audit/oblivion/dim_3.md`

### Dimension 4: Exterior & Lighting Data (Tamriel)
**Subagent**: `general-purpose`
Routing: WTHR / sun / terrain / LOD / water translation mechanism -> `/audit-exterior`. This dimension checks the Oblivion-only inputs.
**Paths**: `byroredux/src/cell_loader/` (`placement_lod.rs`, `terrain.rs`), `byroredux/src/env_translate.rs`, `byroredux/src/systems/light_anim.rs`, `crates/plugin/src/esm/cell/`, `docs/engine/exterior-readiness-plan.md`
**First step**: `docs/smoke-tests/m-exteriors.sh oblivion static` (needs a Vulkan device), and `git log --since=<date> --format='%h %cs %s' -- byroredux/src/cell_loader byroredux/src/env_translate.rs`
- **`_far.nif` placement LOD is real only here** (`placement_lod_supported_is_oblivion_only`; FO3/FNV ship none and use `ObjectLodScheme::FalloutLegacyBlocks`): confirm placements and LOD textures still resolve on a Tamriel exterior.
- **`WTHR.HNAM`** (Oblivion-only HDR block, 37 / 37 weathers): the sunlight dimmer rides the EXAL boundary onto `WeatherDataRes` and multiplies the sampled sun colour; absence is a neutral 1.0 and a short/degenerate block must clamp, not go negative (`sunlight_dimmer_translates_from_the_hnam_block`, #4057).
- **LTEX / default land textures**: Oblivion `LTEX.ICON` is relative to `landscape\` and rooted there at the parse boundary (226 of 229 resolve; three ship nowhere), default land texture is `Default.DDS` (`DefaultLandTexture::for_game`), and layers without `TX01` take the diffuse's `_n` sibling. Pins (`--ignored`, `byroredux/src/cell_loader/terrain.rs`): `oblivion_ltex_paths_exist_in_vanilla_archives`, `default_land_textures_exist_in_vanilla_archives`. A checkerboard on lake/sea beds means a missing key.
- LIGH falloff: an authored 0.0 means "default", which is the quadratic 2.0 pre-Skyrim (`falloff_exponent_sentinel_resolves_per_layout_generation`, `light_anim.rs`) — not the Skyrim 1.0.
- Embedded `NiControllerSequence` animation (423 files / 792 sequences: gates, banners) must reach the ECS on cell load (#3602); frozen animated statics are the symptom. Repeatable gate: `m-exteriors.sh oblivion static|boundary|soak|cycle|water` and `/audit-runtime --game oblivion`.
**Output**: `/tmp/audit/oblivion/dim_4.md`

### Dimension 5: Gameplay & UI Data Slice (M47.3 ObScript quests, MenuXml HUD)
**Subagent**: `general-purpose`
Routing: quest-script runtime `/audit-scripting`; HUD drivers / MenuXml eval-layout-raster `/audit-ui`, parse side `/audit-parsers`; gameplay mechanism `/audit-gameplay`.
**Paths**: `crates/scripting/src/obscript_vm.rs`, `crates/scripting/src/obscript_quests.rs`, `byroredux/src/hud.rs`, `crates/menuxml/`, `byroredux/src/cell_loader/references/attach.rs`
**First step**: `cargo test -p byroredux-scripting --release -- --ignored vanilla_oblivion_quest_scripts_execute_and_setstage` (`BYROREDUX_OBLIVION_DATA`, ~160 MB resident) and `git log --since=<date> --format='%h %cs %s' -- crates/scripting crates/menuxml byroredux/src/hud.rs`
- **M47.3** (mechanism `/audit-scripting`): 255 vanilla QUST scripts install once per load order and tick every 5 s (`fQuestProcessInterval`); command ids were derived empirically from `Oblivion.esm` and 2 349 of 2 393 SCPT decode clean (residue: per-command string-argument signatures); unknown commands are counted, never faked. Oblivion's dialect is `Obse` (`obscript_dialect_follows_the_profile_not_the_game_kind`). Object-script blocks, `Message` UI and actor-state functions are phase 2 (ROADMAP M47.3).
- **MenuXml HUD** (`--hud`, `HudGameProfile::oblivion`): the XML *authors* the art, so the driver pushes only trait overrides (`hudmain_health_full` / `hudmain_magic_full` / `hudmain_fatigue_full`, `hudmain_compass_window` `user0`, `HUDMainMenu` `user3`); XML + 5 font slots come from `Oblivion - Misc.bsa`, art from `Oblivion - Textures - Compressed.bsa` (three `Menus` / `Menus80` / `Menus50` sets). Oblivion has no stamped index-keyed ActorValues, so bars read the Skyrim-keyed `0x3E8` / `0x3E9` / `0x3EA` and fall back to full. Guards: `crates/menuxml/tests/vanilla_corpus.rs` (`BYROREDUX_OBLIVION_DATA`), `docs/smoke-tests/m48-4-oblivion-hud.sh`. The driver, throttle, overlay and eval/layout/raster are shared -> `/audit-ui`.
- Playable gate: `docs/smoke-tests/p0-door-interaction.sh oblivion` is the only gate `oblivion.env` declares; an undeclared gate exits 2 (unmeasured, not covered).
**Output**: `/tmp/audit/oblivion/dim_5.md`

## Phase 3: Merge

1. Read all `/tmp/audit/oblivion/dim_*.md`; combine into `docs/audits/AUDIT_OBLIVION_<TODAY>.md`:
   - **Executive Summary** — compatibility level (NIF parse incl. v10.x tail, archive extract, ESM parse, render); state the corpus-lane result first; cite ROADMAP / feature-matrix numbers.
   - **Dimension Findings** — grouped by severity per dimension.
   - **Regression Guard List** — stride-drift family (#1506-#1509), #170 dual band, `NiTexturingProperty` u32 count, BSA v103 sweep, `parallax_alpha_gate_tests`, Disney gate stays 0.
   - **Open Work** — from `ROADMAP.md` Known Issues; interiors and the Tamriel exterior already render, so neither is a blocker.
2. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_<TODAY>.md`
(label every finding `game:oblivion` + `legacy-compat`, plus its own domain label.)
