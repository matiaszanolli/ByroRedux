---
description: "Per-game audit of Skyrim SE compatibility — BSTriShape packed geometry, BSLightingShaderProperty shader-type dispatch, NPC equip/FaceGen, multi-master load order"
argument-hint: "--focus <dimensions>"
---

# Skyrim SE Compatibility Audit

Deep audit of ByroRedux readiness for **The Elder Scrolls V: Skyrim Special Edition** content.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, game-data locations,
methodology, deduplication rules, and finding format. See
`.claude/commands/_audit-severity.md` for the severity scale. Do not duplicate
those here.

## Why Skyrim is the hardest geometry case

Skyrim SE is the engine's renderer **control bench** — cell-load and rendering
both work (Whiterun BanneredMare). So this audit is *not* readiness scoping; it
is **regression coverage** plus the genuinely Skyrim-specific risk surface:

1. **BSTriShape packed geometry** — half-float vertex pool with a `vertex_desc`
   bitfield, inline tangents, and a separate SSE skinned-reconstruction path
   that is uniquely prone to silent magenta/chrome corruption.
2. **`BSLightingShaderProperty` shader-type dispatch** — the trailing-field
   reader branches on ~18 numeric shader types; an off-by-one drops or over-reads
   geometry on a whole material class.
3. **NPC equip + FaceGen** — Whiterun ships 6 named equipped NPCs via M41
   OTFT/LVLI; this is the only vanilla cell that exercises the full outfit chain.
4. **Multi-master load order** — DLC interiors via `--master` need cross-plugin
   FormID remap.

Dimensions below are ordered by that risk, highest first.

## Game Context

Pull live numbers from `ROADMAP.md` (compat matrix + Bench-of-record) and
`docs/feature-matrix.md` rather than trusting any figure transcribed here —
benches refresh every `/session-close`.

| Aspect       | State (cite ROADMAP, do not re-transcribe) |
|--------------|---------------------------------------------|
| NIF format   | v20.2.0.7 (BSVER 83 / 100) |
| BSA format   | v105 ✓ (LZ4 block compression) — `crates/bsa/src/archive/` |
| ESM parser   | Unified `esm/` walker ✓ — `Skyrim.esm` cells parse (`parse_real_skyrim_esm`, finds `SolitudeWinkingSkeever`) |
| Parse rate   | 100% clean on the Meshes0 sweep (cite ROADMAP compat matrix for the exact ratio) |
| Rendering    | Cells + meshes ✓ — Whiterun BanneredMare is the renderer **control bench** (entity/FPS figures: ROADMAP Bench-of-record, currently R6a-stale-14) |
| NPC equip    | 6 named NPCs equipped via M41 OTFT/LVLI (`byroredux/src/npc_spawn.rs`) |
| Reference data | `/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/` |

### Known Specifics (verified against live code)

- **BSTriShape** — packed vertex pool keyed off a 64-bit `vertex_desc` bitfield
  (`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs`, `BsTriShape` struct).
  `VF_*` attribute bits select u16 half-precision positions/normals, optional
  skinning (`VF_SKINNED`), optional full precision. Per-vertex tangents ship
  inline in the packed blob when `VF_TANGENTS | VF_NORMALS` are set (Skyrim
  convention; FO4+ shares the inline path — #795 / #796).
- **`BsTriShapeKind`** disambiguates the five wire-distinct subclasses that share
  the one `BsTriShape` Rust struct: `Plain` (BSTriShape), `LOD` (BSLODTriShape),
  `MeshLOD` (BSMeshLODTriShape), `SubIndex` (BSSubIndexTriShape, boxed
  segmentation payload, #404), `Dynamic` (BSDynamicTriShape — facegen heads).
- **`BSLODTriShape` is routed through `NiLodTriShape`, NOT `BsTriShape`** (#838).
  Per nif.xml, `BSLODTriShape` inherits `NiTriBasedGeom` (`#SKY##SSE#`) while
  `BSMeshLODTriShape` inherits `BSTriShape` (`#FO4#`) — they look identical at the
  block name but have different bodies. The dispatch in
  `crates/nif/src/blocks/mod.rs` sends `"BSLODTriShape"` to
  `NiLodTriShape::parse` and `"BSMeshLODTriShape"` to `BsTriShape::parse_meshlod`.
  Pre-#838 routing of `BSLODTriShape` through BSTriShape over-read every Skyrim
  tree LOD. **Audit guard**: any proposal to "fold BSLODTriShape into BSTriShape"
  is a regression of #838.
- **`BSLightingShaderProperty`** lives in `crates/nif/src/blocks/shader.rs`
  (NOT in `crates/nif/src/blocks/properties.rs`, where it was historically
  assumed). The shader-type-specific trailing data is the
  `ShaderTypeData` enum — **9 Rust variants** (`None`, `EnvironmentMap`,
  `SkinTint`, `HairTint`, `ParallaxOcc`, `MultiLayerParallax`, `SparkleSnow`,
  `EyeEnvmap`, `Fo76SkinTint`). The dispatch (`parse_shader_type_data`) maps
  ~18 numeric Skyrim/FO4 `BSLightingShaderType` values onto those variants
  (most fall through to `None`). FO76 uses the distinct `BSShaderType155`
  numbering (`parse_shader_type_data_fo76`). There is no `GlowShader` variant —
  glow (type 2) reads `None` trailing data.
- **`BSEffectShaderProperty`** — also in `crates/nif/src/blocks/shader.rs`: `soft_falloff_depth`,
  `greyscale_texture`, `lighting_influence`, `env_map_min_lod`, falloff
  start/stop angle+opacity.
- **`BsLagBoneController`** + **`BsProceduralLightningController`** (#837) — both
  have dedicated parsers (`crates/nif/src/blocks/controller/`). Without them a
  large by-design `block_size` WARN burst fires per Meshes0 sweep.
- **BSTriShape `data_size` warning gate** (#836) — gated on `num_vertices != 0`
  so the SSE skinned-body reconstruction path doesn't fire false-positive WARNs.
- **`BSBoneLODExtraData`** parser landed (#614, `crates/nif/src/blocks/extra_data.rs`).
- Other specialty blocks: `BsDismemberSkinInstance` (dismemberment),
  `BSPackedCombined[Shared]GeomDataExtra` (distant LOD batches), `BSTreeNode`
  (SpeedTree wind bones), and the `BSFadeNode` / `BSBlastNode` / `BSMultiBoundNode`
  NiNode subclasses unwrapped by the import walker.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 7.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/skyrim`.
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
4. Confirm `Skyrim Special Edition/Data/` exists; if not, note which dimensions lose real-data validation.

## Phase 2: Launch Dimension Agents (parallel)

### Dimension 1: BSTriShape Packed Geometry + SSE Skinned Reconstruction
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs` (`BsTriShape` parser, `vertex_desc` / `VF_*` flags, `BsTriShapeKind`), `crates/nif/src/import/mesh/bs_tri_shape.rs` (`extract_bs_tri_shape` / `_local`), `crates/nif/src/import/mesh/sse_recon.rs` (#559), `crates/nif/src/import/mesh/tangent.rs`
**Checklist**:
- `VF_*` flag bits mapped correctly (VERTEX, UVS, UVS_2, NORMALS, TANGENTS, COLORS, SKINNED, FULL_PRECISION, EYE_DATA). Half-precision u16 → f32 decode is IEEE-754 binary16 correct.
- `extract_bs_tri_shape` handles every flag combination; index stride (u16 vs u32) chosen correctly. Skinned `bone_indices` / `bone_weights` extraction matches the skinning pipeline.
- **SSE skinned-geometry reconstruction tangent path** (`crates/nif/src/import/mesh/sse_recon.rs` #559, tangent convention #1204 in `crates/nif/src/import/mesh/tangent.rs`): SSE skinned bodies ship geometry in a partition-remapped global buffer; confirm positions/normals are Z-up→Y-up converted AND the on-disk "bitangent" triplet is routed as the Y-up tangent (∂P/∂U) so reconstructed bodies don't read magenta/chrome (regression guard — mirrors the `feedback_chrome_means_missing_textures` failure mode).
- **Alpha-property cascade** gated on `alpha_property_consumed` (#1201 / #1202): set in `crates/nif/src/import/material/mod.rs` (search `info.alpha_property_consumed = true`), consulted at the two gate sites in `crates/nif/src/import/material/walker.rs` (search `!info.alpha_property_consumed`). Skinned geometry must inherit the parent `NiAlphaProperty` exactly once.
**Output**: `/tmp/audit/skyrim/dim_1.md`

### Dimension 2: BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch
**Subagent**: `renderer-specialist`
**Entry points**: `crates/nif/src/blocks/shader.rs` (`BSLightingShaderProperty`, `BSEffectShaderProperty`, `ShaderTypeData`, `parse_shader_type_data` / `_fo4` / `_fo76`), `crates/nif/src/blocks/shader_tests.rs`, `crates/nif/src/import/material/` (mod, walker, shader_data), `crates/renderer/shaders/triangle.frag`
**Checklist**:
- Every numeric Skyrim/FO4 shader type dispatches to the correct `ShaderTypeData` arm and reads the right trailing-field count (EnvironmentMap = env scale; SkinTint/HairTint = Color3; ParallaxOcc = max_passes + scale; MultiLayerParallax = inner-layer fields; SparkleSnow = 4 params; EyeEnvmap = eye cubemap + two reflection centers). Types with no trailing data (0/2/3/4/8–10/12–13/15/17–19) fall through to `None` — confirm none of those silently over-read.
- FO76 (`BSShaderType155`, `parse_shader_type_data_fo76`) uses the *different* numeric mapping (type 4 = `Fo76SkinTint` Color4, type 5 = HairTint Color3) — guard the two enums don't cross-contaminate.
- Flag bits 0–31 (decal / alpha-test / skinned / …) — Skyrim positions differ from FO4; verify the Skyrim decode.
- `BSEffectShaderProperty`: `soft_falloff_depth`, `greyscale_texture`, `lighting_influence`, `env_map_min_lod`, falloff angle/opacity. Environment-map slot in the texture set; alpha mask threshold.
- **#1241 PBR scalars surfaced at import** (`crates/nif/src/import/material/lighting_shader_pbr_tests.rs`, `crates/nif/src/import/types.rs`): smoothness / IOR / specular_strength flow into `MaterialInfo`.
- **Disney/Burley lobe pin (regression guard)**: the principled BRDF in `crates/renderer/shaders/include/pbr.glsl` (`#include`d by `triangle.frag`) is gated on `MAT_FLAG_PBR_BSDF` (`#define MAT_FLAG_PBR_BSDF 32u` in `crates/renderer/shaders/include/shader_constants.glsl`; branch sites search `MAT_FLAG_PBR_BSDF` in `include/lighting.glsl` + `include/pbr.glsl`). Vanilla Skyrim LE/SSE materials don't author the BGSM PBR flag (BGSM is FO4+), so the lobe must stay **unreachable** for vanilla content — confirm vanilla parse runs set 0 instances of the flag on the Skyrim.esm material universe. Modded BGSM that explicitly opts into PBR is the one legitimate path that flips it. See `/audit-nifal` for the canonical boundary that sets the flag (Dimension 7).
**Output**: `/tmp/audit/skyrim/dim_2.md`

### Dimension 3: NPC Equip + FaceGen (M41)
**Subagent**: `general-purpose`
**Entry points**: `byroredux/src/npc_spawn.rs` (M41 actor instantiation), `crates/facegen/src/` (`.tri`/`.egm`/`.egt` morph + texture blend), `byroredux/src/systems/character.rs` (skinning consumer for heads/bodies), `crates/nif/src/import/mesh/sse_recon.rs`
**Checklist**:
- The Whiterun BanneredMare 6 named NPCs (saadia, brenuin, mikael, sinmir, amaundmotierreend, hulda) each land `Inventory` + `EquipmentSlots` and spawn equipped (OTFT.items + LVLI dispatch). Guard that count + components don't regress.
- Skyrim+ `resolve_armor_mesh` walks ARMO → ARMA → worn-mesh; body-slot armor pre-scan skips `upperbody.nif` to kill z-fight + double bone-palette overhead.
- LVLI flattening (`expand_leveled_form_id`) gated on actor level — single-pick (highest eligible) vs multi-pick. Pre-fix, default outfits referencing LVLI spawned with no gear.
- FaceGen heads parse via `BSDynamicTriShape` + the `facegen` crate, but expected visual fidelity is limited (no FaceGen runtime morph at render time) — confirm parse, not pixel match.
- `BSDismemberSkinInstance` partition data routes into the skinning pipeline.
**Output**: `/tmp/audit/skyrim/dim_3.md`

### Dimension 4: Multi-Master Load Order + TES5 Cell-Load Regression
**Subagent**: `general-purpose`
**Entry points**: `byroredux/src/cell_loader/load_order.rs` (`--master` FormID remap), `crates/plugin/src/esm/records/` (TES5 records share the unified parser — the per-game legacy stub was removed under #390), `crates/plugin/src/esm/cell/` (CELL walker), `crates/plugin/src/esm/cell/tests/integration.rs` (`parse_real_skyrim_esm`), `ROADMAP.md`
**Checklist**:
- Repeatable `--master <path>` (M46.0 / #561): each plugin's TES4 master_files header drives a per-plugin FormID remap so cross-plugin REFRs land under merged global FormIDs; last-write-wins on collision (canonical Bethesda load order). Unresolved REFRs name the missing plugin. Repro: `cargo run -- --master Skyrim.esm --esm Dawnguard.esm --cell ForebearsHoldoutInt01`.
- **`.STRINGS` loader wired into the multi-plugin load path (`db5bb149`)** — the localized-string table loader (`crates/plugin/src/esm/strings_table.rs`) must be invoked from `cell_loader/load_order.rs` for every loaded plugin, not just the active one; a regression that resolves strings off only the last `--esm` leaves DLC-owned names/dialogue as raw string IDs.
- **ESL / light-master FormID decode (#1554, `59d3f007`)** — TES4 record flag `0x0200` (Light Master / ESL) plugins share the `0xFE` top-byte space; `crates/plugin/src/esm/reader.rs` decodes their forms as `0xFE00_0000 | ((sub & 0x0FFF) << 12) | (raw & 0x0FFF)` (12-bit load-order sub-index + 12-bit object id), driven by `light_master` on the plugin. A regression that treats an ESL like a full master (top byte = load-order index) collapses every ESL form into the wrong space and unresolves its REFRs.
- `parse_real_skyrim_esm` walks real `Skyrim.esm`, finds `SolitudeWinkingSkeever` — guard the unified walker keeps parsing Skyrim cells.
- TES5 compressed-record decompression (groups can be compressed; interiors render) stays green.
- Minimum interior-render record set parses: CELL, REFR, STAT, LIGH, WEAP, ARMO, plus Skyrim-specific LAND (heightmap scale), LTEX, TXST, ADDN.
- Out of scope but must parse without error: NAVM, HDPT (metadata), `BSBehaviorGraphExtraData`.
- **Control-bench guard**: Whiterun BanneredMare entity count + FPS vs the current ROADMAP Bench-of-record (R6a-stale-14). Skyrim ships real `bhk` collision, so entity count is flat across collider-gate changes — any drop in entity count or substantial FPS regression at the same entity count is a control-bench regression.
**Output**: `/tmp/audit/skyrim/dim_4.md`

### Dimension 5: BSA v105 (LZ4)
**Subagent**: `general-purpose`
**Entry points**: `crates/bsa/src/archive/` (mod, open, extract, hash, tests)
**Checklist**:
- v105 header format; LZ4 block decompression via `lz4_flex::block` — verify against a known-good Skyrim mesh (e.g. sweetroll).
- Hash table layout vs v104; folder record size; embedded-name flag; compressed-file flag priority (archive-level vs per-file — which wins on disagreement).
- Full-archive extraction sweep: `Skyrim - Meshes0.bsa` + `Skyrim - Textures*.bsa` (through Textures8) all extract without error. **Zero-based sibling auto-load (`821a425b`)** — `asset_provider/archive.rs::open_with_numeric_siblings` now auto-loads `<stem>2.bsa`..`<stem>9.bsa` siblings, so distant-LOD diffuse in `Textures7.bsa` and `.btr` meshes in `Textures8.bsa` drag in from a zero-based base archive; a regression that re-narrows sibling discovery starves M35 distant terrain of its LOD textures.
**Output**: `/tmp/audit/skyrim/dim_5.md`

### Dimension 6: Specialty Blocks + Real-Data Rendering
**Subagent**: `renderer-specialist`
**Entry points**: `crates/nif/src/blocks/mod.rs` (NiLodTriShape / BsLagBoneController / BsProceduralLightningController dispatch), `crates/nif/src/blocks/controller/`, `crates/nif/src/import/walk/`, `crates/nif/examples/nif_stats.rs`, `byroredux/src/render/static_meshes.rs`, `byroredux/src/render/skinned.rs`
**Checklist**:
- `BSLODTriShape` (Skyrim DLC tree LOD) routed through `NiLodTriShape`, NOT BSTriShape (#838 regression guard). `BSLODTriShape` vs `BSMeshLODTriShape` vs `BSSubIndexTriShape` — distinct bodies, must not be confused.
- `BsLagBoneController` + `BsProceduralLightningController` (#837): dedicated parsers — without them a by-design `block_size` WARN burst fires per Meshes0 sweep.
- `BSTreeNode` wind-bone list (SpeedTree); `BSPackedCombined[Shared]GeomDataExtra` distant-LOD batch layout; the import walker unwraps `BSFadeNode` / `BSBlastNode` / `BSMultiBoundNode`.
- **M35 prebaked `.btr` distant-terrain LOD (`9384d4c2`, Skyrim+/FO4)** — `byroredux/src/cell_loader/terrain_lod_btr.rs` loads prebaked `.btr` distant-terrain meshes (wired from `cell_loader/terrain_lod.rs`); confirm `.btr` quads parse + render at distance and their diffuse resolves through the zero-based sibling archives (Dim 5). A regression silently drops distant terrain to no-LOD.
- **Meshes0 sweep baseline**: 100% clean / 0 truncated / 0 recovered / 0 realignment WARNs. Any audit observing realignment WARNs on a clean Skyrim Meshes0 corpus has hit a regression.
- Real-data render trace: pick one creature (dragon skeleton / NPC head), one landscape (tree LOD), one magic effect (BSEffectShaderProperty). Trace each `import_nif_scene` → `material_translate::translate_material` → `byroredux/src/render/static_meshes.rs` (static) / `byroredux/src/render/skinned.rs` (skinned) — verify mesh count, material extraction, texture handle resolution. Single-mesh smoke: render `meshes\clutter\ingredients\sweetroll01.nif` and confirm FPS stays in the ROADMAP-documented band.
**Output**: `/tmp/audit/skyrim/dim_6.md`

### Dimension 7: NIFAL Canonical Material Translation (Skyrim slice)
**Subagent**: `renderer-specialist`
**Entry points**: `byroredux/src/material_translate.rs` (`translate_material`), `crates/core/src/ecs/components/material.rs` (`Material`, `Material::resolve_pbr`, `EmissiveSource`, `classify_pbr_keyword`, `PbrClassifierInputs`), `docs/engine/nifal.md`
**Checklist**:
- `translate_material` is the **single canonical boundary** mapping the per-game `ImportedMesh` (BSLightingShaderProperty / BSEffectShaderProperty `MaterialInfo`) into one ECS `Material` — no second translation path, no render-time fallback.
- `Material.metalness` / `Material.roughness` are plain resolved `f32` fields, seeded from BGSM/BGEM scalars or an `f32::NAN` sentinel, then filled by `Material::resolve_pbr`, which delegates to the keyword classifier `classify_pbr_keyword`. The old per-draw `Material::classify_pbr` is **deleted** — any audit proposing render-time PBR classification is a regression of the canonical boundary.
- Ordering at the boundary: `material.resolve_pbr()` runs **before** `crate::helpers::classify_glass_into_material` so forced-glass roughness wins over the keyword default.
- **`EmissiveSource` discriminator (#1280)**: `enum EmissiveSource { None, Material, Lighting, Effect }`. Skyrim `BSLightingShaderProperty.emissive_multiple` routes through the `Lighting` variant (genuine emissive scalar); `Effect` is the BSEffectShaderProperty diffuse-tint conflation. Verify Skyrim emissive maps to `Lighting`, not `Effect`.
- See `/audit-nifal` for the cross-game canonical-translation deep dive (no-fabrication / single-boundary / no-render-time-fallback invariants).
**Output**: `/tmp/audit/skyrim/dim_7.md`

## Phase 3: Merge

1. Read all `/tmp/audit/skyrim/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_SKYRIM_<TODAY>.md` with structure:
   - **Executive Summary** — Skyrim SE is the renderer control bench (Whiterun BanneredMare, 6 equipped NPCs); both loose-mesh and cell rendering work. This audit is regression coverage + Skyrim-specific geometry/shader/equip risk.
   - **Dimension Findings** — grouped by severity per dimension.
   - **Shader-Type Coverage Matrix** — the `ShaderTypeData` variants × parse-complete / import-complete / render-complete (note which numeric types map to `None`).
   - **Cell-Load Regression Status** — TES5 cells parse through the unified `esm/cell/` walker (compressed records decompress); Whiterun control-bench entity count + FPS vs the current ROADMAP Bench-of-record.
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_SKYRIM_<TODAY>.md`
