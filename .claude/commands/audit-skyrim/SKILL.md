---
description: "Per-game audit of Skyrim (SE + LE) compatibility — BSTriShape packed geometry, BSLightingShaderProperty shader-type dispatch, NPC equip/FaceGen, multi-master load order"
argument-hint: "--focus <dimensions>"
---

# Skyrim Compatibility Audit (SE + LE)

Deep audit of ByroRedux readiness for **Skyrim Special Edition** (BSVER 100, BSA v105) and the **2011 Skyrim LE** (BSVER 83, BSA v104).

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

Read `.claude/commands/_audit-common.md` (layout, game data, dedup, finding format) and `.claude/commands/_audit-severity.md`. This file carries only Skyrim-specific context.

**Scope**: Skyrim SE is the renderer **control bench** (Whiterun BanneredMare — cell load and rendering both work), so this is regression coverage plus Skyrim-specific risk, not readiness scoping. The skill owns *Skyrim's data through the shared mechanisms*; mechanism defects go to the owner: NIF block/dispatch parsing → `/audit-nif`; canonical material invariants → `/audit-nifal`; ESM walker / FormID remap / ESL / `.STRINGS` → `/audit-esm` (Dim 3 + 6); BSA/hkx/facegen reader discipline → `/audit-parsers`; `.btr`/`.bto` distant LOD and VWD culling (#3307) → `/audit-exterior`; Scaleform AVM1 HUD + `GameDelegate` catalog (74 methods) → `/audit-ui`; NPC spawn / AI-package / loot gameplay → `/audit-gameplay`; runtime baseline (`.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv`) → `/audit-runtime`.

Live numbers (parse rate, bench FPS/entities) come from a fresh harness run or ROADMAP's compat matrix / Bench-of-record — never transcribe them here.

## Game Context

| Aspect | State |
|---|---|
| NIF | v20.2.0.7; SE BSVER 100 (`BSTriShape` packed geometry), LE BSVER 83 (classic `NiTriShape` under Skyrim's shader + skin blocks — the SE gate does not exercise this path) |
| Archives | SE BSA v105 = LZ4 **frame** (`lz4_flex::frame::FrameDecoder`, `crates/bsa/src/archive/extract.rs`) — NOT `lz4_flex::block` (BA2/Starfield's codec). LE BSA v104 = zlib. Guard: `synthetic_v105_block_codec_payload_is_rejected_by_frame_reader` (`crates/bsa/src/archive/tests.rs`, #1558) |
| Releases | LE is selected by the files on disk, not a flag: `[[profiles.skyrim_se.releases]]` in `assets/debug_profiles.toml` → `GameProfileEntry::for_data_dir` (`crates/core/src/ecs/game_profiles.rs`); unnumbered `Skyrim - Meshes.bsa` / `Textures.bsa` |
| ESM | Unified `esm/` walker; SE `Skyrim.esm` form v44 / HEDR 1.71, LE form v40 / HEDR 0.94 |
| Data | SE `/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/`; LE in the Wine prefix (`BYROREDUX_SKYRIMLE_DATA`, see `_audit-common.md`) |

## Parameters / Setup

`--focus <dimensions>` (default all 5). Setup: parse `$ARGUMENTS`; `mkdir -p /tmp/audit/skyrim`; `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`; confirm the Data dir exists, else note which dimensions lose real-data validation.

## Dimensions (ordered by risk, highest first)

### Dimension 1: BSTriShape Packed Geometry + SSE Skinned Reconstruction
**Subagent**: `legacy-specialist`
**Paths**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs`, `crates/nif/src/import/mesh/{bs_tri_shape,sse_recon,skin,tangent}.rs`, `crates/nif/src/blocks/mod.rs` (dispatch)
**First step**: `git log --since=<last report> --format='%h %cs %s' -- crates/nif/src/import/mesh crates/nif/src/blocks/tri_shape`
**Guards**: `sse_skin_geometry_reconstruction_tests.rs`, `bs_tri_shape_partition_remap_tests.rs`, `tangent_convention_tests.rs`, `alpha_flag_tests.rs`; `bs_lod_tri_shape_skyrim_consumes_ni_tri_shape_body_plus_3u32_trailer` (`crates/nif/src/blocks/tri_shape_skin_vertex_tests.rs`, #838). Real-data (`#[ignore]`d — run explicitly): `sse_skin_index_space_tests.rs::packed_sse_indices_match_partition_palette_expansion_on_real_data`.
**Checklist** (what the guards cannot see):
- **Packed bone indices are already global** — `widen_packed_bone_indices` (`skin.rs`) widens the `BSTriShape` / SSE global-buffer `[u8; 4]` channel with **no** partition-palette remap; only `NiSkinPartition`'s separate `bone_indices` is partition-local. The old #613/#2577 remap corrupted 4,893 of 11,669 weighted lanes on SSE Draugr/body/hands — re-introducing it (or any second remap) is the regression; a finding proposing it is a false premise.
- **SSE reconstruction** (`try_reconstruct_sse_geometry`, empty-inline `BSTriShape` + populated `NiSkinPartition` global buffer, #559/#638): positions/normals Z-up→Y-up converted; partition triangles are global-buffer indices (concatenate directly, ignore `vertex_map`); the on-disk "bitangent" triplet is routed as the Y-up tangent (∂P/∂U) so bodies do not read magenta/chrome. `BSDynamicTriShape` (facegen heads) has `VF_VERTEX` clear on every real block — positions come from external data.
- **`BSLODTriShape` ≠ `BSMeshLODTriShape`** — `BSLODTriShape` inherits `NiTriBasedGeom` and routes to `NiLodTriShape::parse`; `BSMeshLODTriShape` (FO4) inherits `BSTriShape`. Folding the former into `BsTriShape` over-reads every Skyrim tree LOD (#838). `BSSubIndexTriShape` (segments, Arc-shared) and the four `BsTriShapeKind` variants must not be confused.
- **Alpha-property cascade** — gated on `alpha_property_consumed`; skinned geometry inherits the parent `NiAlphaProperty` exactly once.
- Specialty blocks: `BsLagBoneController` / `BsProceduralLightningController` have dedicated parsers (without them a by-design `block_size` WARN burst fires per mesh sweep); `BSTreeNode` wind bones; `BSPackedCombined[Shared]GeomDataExtra` distant batches; `BSFadeNode` / `BSBlastNode` / `BSMultiBoundNode` unwrapped by the import walker.
**Output**: `/tmp/audit/skyrim/dim_1.md`

### Dimension 2: Shader-Type Dispatch + Skyrim Material Slice (NIFAL)
**Subagent**: `renderer-specialist`
**Paths**: `crates/nif/src/blocks/shader/{lighting,effect}.rs`, `crates/nif/src/blocks/shader_tests/skyrim.rs`, `crates/nif/src/import/material/{mod,dedicated_shader,shader_data}.rs`, `byroredux/src/material_translate.rs`, `crates/renderer/shaders/include/pbr.glsl`
**First step**: `git log --since=<last report> --format='%h %cs %s' -- crates/nif/src/blocks/shader crates/nif/src/import/material byroredux/src/material_translate.rs`
**Guards**: `shader_tests/skyrim.rs` (one wire-level test per trailing-data type + `parse_bs_lighting_remaining_no_trailing_shader_types_consume_nothing_extra`, #4252); `lighting_shader_pbr_tests.rs`, `emissive_source_tests.rs`, `shader_type_data_tests.rs` (`crates/nif/src/import/material/`). Single-boundary / resolve-once invariants are `/audit-nifal`.
**Checklist**:
- `parse_shader_type_data` dispatches numeric types 1/5/6/7/11/14/16 to `EnvironmentMap` / `SkinTint` / `HairTint` / `ParallaxOcc` / `MultiLayerParallax` / `SparkleSnow` / `EyeEnvmap`; every other type (0/2/3/4/8–10/12/13/15/17–20) reads no trailing data — confirm none over-reads. FO76 (`BSShaderType155`, `parse_shader_type_data_fo76`) uses different numbering (4 = `Fo76SkinTint` Color4, 5 = HairTint) — the two enums must not cross-contaminate; the FO4 arm adds fields only at BSVER 130–139.
- Skyrim flag-bit positions differ from FO4; verify the Skyrim decode of decal / alpha-test / skinned / glow / facegen bits. `BSEffectShaderProperty`: soft-falloff depth, greyscale texture, lighting influence, env-map min LOD, falloff angle/opacity.
- **Unauthored fields stay unauthored** — Skyrim's `BSEffectShaderProperty` has no `env_map_scale` wire field, so import must not write one (#4393: #4250's fabricated 1.0 flipped every inline Skyrim effect shader into the "authored environment mapping" PBR arm); the `env_map_scale_consumed` latch must be set by the Skyrim+ dedicated writers (#4251).
- **PBR lobe unreachable for vanilla** — the Disney lobe is gated on `MAT_FLAG_PBR_BSDF`; vanilla Skyrim materials never author BGSM, so vanilla parse runs must set 0 instances (modded BGSM opting into PBR is the one legitimate path).
- **`EmissiveSource`** — Skyrim `BSLightingShaderProperty.emissive_multiple` maps to `Lighting`, not `Effect` (#1280).
- **Detail / tint inputs** — vanilla Skyrim's blank detail map and alpha-less BC1 `_sk.dds` skin tint used to darken every NPC face (#4422 `detail_neutral`, #4423 `TINT_ALPHA_WEIGHT_BIT`, GpuMaterial 432 B). A Skyrim NPC rendering near-black skin is these arms regressing; `_msn` maps in source basis is open #3922; authored-shader-type split of identical glass is open #4392.
- #1241 PBR scalars (`refraction_strength`, `lighting_effect_1/2` for BSVER < FO4) flow `MaterialInfo` → `ImportedMesh.material`.
**Output**: `/tmp/audit/skyrim/dim_2.md`

### Dimension 3: NPC Equip + FaceGen (M41)
**Subagent**: `general-purpose`
**Paths**: `byroredux/src/npc_spawn.rs` + `npc_spawn/`, `crates/plugin/src/equip.rs`, `byroredux/src/scene/nif_loader.rs` (`select_facegen_diffuse`), `crates/facegen/src/`, `byroredux/src/render/skinned.rs`
**First step**: `git log --since=<last report> --format='%h %cs %s' -- byroredux/src/npc_spawn crates/plugin/src/equip.rs`
**Guards**: `docs/smoke-tests/m41-equip.sh skyrim` — hard equip floor 6 on WhiterunBanneredMare (`Inventory` + `EquipmentSlots` on saadia, brenuin, mikael, sinmir, amaundmotierreend, hulda); `crates/plugin/src/equip_template_tests.rs` (#4086); `npc_spawn/tests.rs`.
**Checklist**:
- **Armor chain** — `resolve_armor_mesh` walks ARMO → ARMA → worn mesh; `humanoid_body_paths` returns `&[]` for Skyrim+ (the `upperbody.nif` pre-scan is Oblivion/FO3/FNV only). Body coverage: the race default skin (`RACE.WNAM`) equips first as the lowest-priority layer (#2093), then a post-loop occupancy filter drops any queued mesh — including the skin's — whose slot a higher-priority OTFT/CNTO entry displaced (#2094). Gendered RACE skeleton paths are stored separately from body models. LVLI flattening (`expand_leveled_form_id`) is level-gated: single-pick (highest eligible) vs multi-pick; pre-fix, LVLI outfits spawned gearless.
- **Template chain (#4086)** — absent TPLT fields resolve down the whole chain (`resolve_inherited_*` / `walk_inherited_records`), not across its two ends; nested actor lists feed every inherited category, level-gate tie-order preserved, cycles share one budget and retain the shell.
- **FaceGen is pre-baked on Skyrim** — heads come from `meshes\actors\character\facegendata\facegeom\<plugin>\<formid>.nif` (`BSDynamicTriShape`) plus the per-NPC `facetint` DDS; there is no runtime EGM/EGT morph on this path (the `crates/facegen` EGM evaluator serves the runtime-recipe games; `.egt`/`.tri` have no consumer at all). The tint override applies **only to the FaceTint (kind 4) head mesh** (#4421) — Skyrim's kind-5 shapes are overlays with their own textures (gashes, Argonian hair, Orc tusks); keying on SkinTint replaces them with atlas fragments.
- `BSDismemberSkinInstance` partition data routes into the skinning pipeline. NPC pool composition (#4454) and the rest of spawn/AI belongs to `/audit-gameplay`.
**Output**: `/tmp/audit/skyrim/dim_3.md`

### Dimension 4: Multi-Master Load Order + TES5 Cell-Load Regression
**Scope split**: `/audit-esm` owns FormID remap, ESL `0xFE` space, tombstones and `.STRINGS`; this dimension owns Skyrim's data through them (record counts, DLC authoring, this title's masters).
**Subagent**: `general-purpose`
**Paths**: `byroredux/src/cell_loader/load_order.rs`, `crates/plugin/src/esm/{reader,cell/}`, `crates/plugin/src/esm/cell/tests/integration.rs`
**First step**: `git log --since=<last report> --format='%h %cs %s' -- byroredux/src/cell_loader/load_order.rs crates/plugin/src/esm/reader.rs crates/plugin/src/esm/cell`
**Guards**: `parse_real_skyrim_esm` (`#[ignore]`d; walks `Skyrim.esm`, finds `SolitudeWinkingSkeever`, 92-byte XCLL); ESL / tombstone / strings tests live under `/audit-esm`.
**Checklist**:
- **DLC repro** (engine launch — not while another instance runs): `cargo run -- --master Skyrim.esm --master Update.esm --esm Dawnguard.esm --cell Forelhost01 …` (Dawnguard's real MAST list is `[Skyrim.esm, Update.esm]`; a single `--master` 404s the second, #2583). Confirm cross-plugin REFRs land under merged global FormIDs, last-write-wins, unresolved REFRs name the missing plugin, DLC-owned names/dialogue resolve (not raw string IDs), and a DLC-deleted base REFR does not over-render.
- TES5 compressed-record groups decompress and interiors render; the minimum interior set parses (CELL, REFR, STAT, LIGH, WEAP, ARMO + Skyrim LAND heightmap scale, LTEX, TXST, ADDN); NAVM, HDPT, `BSBehaviorGraphExtraData` parse without error (out of scope to consume).
- **Control bench** — Whiterun BanneredMare entity count + FPS vs ROADMAP's Bench-of-record. Skyrim ships real `bhk` collision, so entity count is flat across collider-gate changes; any entity drop, or an FPS drop at flat entities, is a regression.
**Output**: `/tmp/audit/skyrim/dim_4.md`

### Dimension 5: Archives + Corpus Gates
**Subagent**: `general-purpose`
**Paths**: `crates/bsa/src/archive/`, `crates/bsa/src/naming.rs`, `byroredux/src/asset_provider/archive.rs`, `crates/nif/tests/{parse_real_nifs,common/mod}.rs`, `crates/nif/src/corpus.rs`
**First step**: `BYROREDUX_SKYRIMSE_DATA=… cargo test -p byroredux-nif --test parse_real_nifs -- --ignored skyrim` (`parse_rate_skyrim_se`, `parse_rate_skyrim_le`)
**Guards**: `numeric_sibling_paths` cases in `byroredux/src/asset_provider/tests/archive_siblings.rs` (`siblings_skyrim_zero_start_offers_1_through_9`).
**Checklist**:
- v105 header, hash table, folder-record size, embedded-name flag, compressed-flag priority (archive vs per-file — which wins on disagreement); verify a known-good mesh (sweetroll) and a full sweep of `Skyrim - Meshes0/1.bsa` + `Textures0`–`8`.
- **Sibling auto-load** — a zero-based `Foo0.bsa` pulls `Foo1`..`Foo9`, so distant-LOD diffuse in `Textures7.bsa` and `.btr` in `Textures8.bsa` reach the loader; re-narrowing starves distant terrain (`/audit-exterior`) of textures.
- **Corpus gate** — SE sweeps the two base mesh archives plus the account-dependent Creation Club / Anniversary set (`Game::optional_mesh_archives`, present-only, kept out of the count-keyed baseline); the corpus counts `.bto`/`.btr` (`NIF_ENTRY_EXTENSIONS`) — filtering on `.nif` alone hid 10,662 distant-LOD files. Expected: 100% clean, 0 truncated / recovered / realignment WARNs; any WARN on a clean Skyrim corpus is a regression. Report the measured count; LE has its own per-block baseline (`crates/nif/tests/data/per_block_baselines/skyrim_le.tsv`).
- Real-data render trace: one creature (dragon skeleton / NPC head), one tree LOD, one `BSEffectShaderProperty` magic effect → `import_nif_scene` → `translate_material` → `render/static_meshes.rs` / `render/skinned.rs`; verify mesh count, material extraction, texture handle resolution.
**Output**: `/tmp/audit/skyrim/dim_5.md`

## Phase 3: Merge

1. Read `/tmp/audit/skyrim/dim_*.md`; combine into `docs/audits/AUDIT_SKYRIM_<TODAY>.md`:
   - **Executive Summary** — control bench state; regression coverage vs Skyrim-specific geometry/shader/equip risk.
   - **Dimension Findings** — grouped by severity per dimension.
   - **Shader-Type Coverage Matrix** — numeric type → `ShaderTypeData` arm × parse-complete / import-complete / render-complete (note types that map to `None`).
   - **Cell-Load Regression Status** — TES5 cells through the unified walker; Whiterun entity count + FPS vs ROADMAP.
2. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_SKYRIM_<TODAY>.md`
(label every finding `game:skyrim` + `legacy-compat`, plus its own domain label.)
