---
description: "Per-game audit of Skyrim SE compatibility — BSTriShape, BSLightingShaderProperty 8 variants, BSA v105"
argument-hint: "--focus <dimensions>"
---

# Skyrim SE Compatibility Audit

Deep audit of ByroRedux readiness for **The Elder Scrolls V: Skyrim Special Edition** content.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, game data locations, methodology, deduplication rules, and finding format.

## Game Context

| Aspect            | State                                                                              |
|-------------------|------------------------------------------------------------------------------------|
| NIF format        | v20.2.0.7 (BSVER 83 / 100)                                                         |
| BSA format        | v105 ✓ (LZ4 compression)                                                           |
| ESM parser        | Stub — Skyrim.esm not yet parsed                                                   |
| Parse rate        | 100.00% (18862 / 18862) — Meshes0 sweep is `100.00% clean / 0 truncated / 0 recovered / 0 realignment WARN` post #836–#838 |
| Rendering         | Individual meshes ✓ — Sweetroll demo ~3000-5000 FPS (2026-04-22, RTX 4070 Ti @ 1280×720) |
| Cell loading      | Not wired (requires Skyrim ESM parser)                                             |
| Reference data    | `/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/`             |

### Known Specifics

- **BSTriShape** — packed vertex format with u16 half-precision positions/normals, optional skinning (VF_SKINNED), optional full-precision (VF_FULL_PRECISION). Per-vertex tangents ship inline in the packed-vertex blob when `VF_TANGENTS | VF_NORMALS` are set (Skyrim convention; FO4+ shares the same inline path — see #795 / #796).
- **BSLightingShaderProperty** — 8 shader-type variants: None, EnvironmentMap, GlowShader (SkinTint for hair?), HairTint, ParallaxOcc, MultiLayerParallax, SparkleSnow, EyeEnvmap.
- **BSEffectShaderProperty** — soft falloff depth, greyscale texture, lighting influence, env map min LOD.
- **BSDynamicTriShape** (facegen) + **BSLODTriShape** (DLC LOD — see architectural note below) + **BSMeshLODTriShape** + **BSSubIndexTriShape**.
- **`NiLodTriShape`** (#838 SK-D5-NEW-07, 8d416cc) — **architecturally distinct** from BSTriShape: inherits from `NiTriBasedGeom` per nif.xml, NOT from BSTriShape. Routed through a dedicated `NiLodTriShape` wrapper with `NiTriShape + 3 LOD-size u32s`. Pre-#838 dispatch through BSTriShape produced a 23-byte over-read on every Skyrim tree LOD. Audit guard: any audit that proposes "fold BSLODTriShape back into BSTriShape" is a regression of #838.
- **`BsLagBoneController`** + **`BsProceduralLightningController`** (#837 SK-D5-NEW-03) — both have dedicated parsers. Without them, ~120 by-design `block_size` WARN events fire per Meshes0 sweep.
- **BSTriShape `data_size` warning gate** (#836 SK-D5-NEW-02) — gated on `num_vertices != 0`. Removing the gate fires 67 false-positive WARNs/parse on the SSE skinned-body reconstruction path.
- **BSTreeNode** — SpeedTree wind-bone lists.
- **BSPackedCombined[Shared]GeomDataExtra** — distant LOD batches.
- **BsDismemberSkinInstance** — dismemberment data on skinned meshes.
- Windowed shader trailing fields are fully parsed (N23.2).
- **Meshes0 sweep baseline (post #836–#838)**: `100.00% clean / 0 truncated / 0 recovered / 0 realignment WARNs`. Any audit observing realignment WARNs on a clean Skyrim Meshes0 corpus has hit a regression.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 6.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/skyrim`.
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
4. Confirm `Skyrim Special Edition/Data/` exists; if not, note which dimensions lose real-data validation.

## Phase 2: Launch Dimension Agents (parallel)

### Dimension 1: BSTriShape Vertex Format
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/blocks/bs_tri_shape.rs` (if present, else the BSTriShape parser in `blocks/`), `crates/nif/src/import/mesh.rs`
**Checklist**: Vertex format flag bits (`VF_*`) mapped correctly — VERTEX, UV, NORMAL, TANGENT, COLOR, SKINNED, FULL_PRECISION, EYE_DATA. Half-precision u16 → f32 conversion numerically correct (IEEE 754 binary16 decode). Packed normals → tangent-space reconstruction. Vertex index stride (u16 vs u32) chosen from the BSVER or packed-vertex flag. `extract_bs_tri_shape` in `import/mesh.rs` handles all flag combinations. Skinned-vertex `bone_indices` / `bone_weights` extraction matches the #178 skinning pipeline.
**Output**: `/tmp/audit/skyrim/dim_1.md`

### Dimension 2: BSA v105 (LZ4)
**Subagent**: `general-purpose`
**Entry points**: `crates/bsa/src/archive.rs`
**Checklist**: BSA v105 header format. LZ4 block decompression via `lz4_flex::block` — verify against known-good Skyrim mesh (e.g. sweetroll). Hash table layout differences vs v104. Folder record size. Embedded-name flag for files. Compressed-file flag priority (archive-level flag vs per-file flag — which wins when they disagree). Full-archive extraction sweep: `Skyrim - Meshes0.bsa` + `Skyrim - Textures*.bsa` all extract without error.
**Output**: `/tmp/audit/skyrim/dim_2.md`

### Dimension 3: BSLightingShaderProperty Shader Variants
**Subagent**: `renderer-specialist`
**Entry points**: `crates/nif/src/blocks/properties.rs` (BSLightingShaderProperty), `crates/nif/src/import/material.rs`, `crates/renderer/shaders/triangle.frag`
**Checklist**: All 8 shader-type enum values dispatch to the correct trailing-field reader (EnvironmentMap adds env strength + env map, HairTint adds hair color, ParallaxOcc adds height map params, MultiLayerParallax adds inner-layer fields, SparkleSnow adds sparkle params, EyeEnvmap adds eye-specific fields). Flag bits 0–31 — which are decal, alpha-test, skinned, etc. in Skyrim (differ from FO4 flag bit positions). SkinTint color for HairTint variant. Environment map slot in BSShaderTextureSet[4]. Alpha mask threshold. Subsurface scattering params parsed but not yet routed to the renderer (noted as M38 wetness/subsurface deferred).
**Output**: `/tmp/audit/skyrim/dim_3.md`

### Dimension 4: BSEffectShaderProperty + Specialty Nodes
**Subagent**: `renderer-specialist`
**Entry points**: `crates/nif/src/blocks/properties.rs` (BSEffectShaderProperty), `crates/nif/src/import/walk.rs`, `crates/nif/src/blocks/mod.rs` (NiLodTriShape / BsLagBoneController / BsProceduralLightningController dispatch)
**Checklist**: BSEffectShaderProperty soft_falloff_depth, greyscale_texture, lighting_influence, env_map_min_lod. BSDynamicTriShape (facegen dynamic verts). **`NiLodTriShape`** (Skyrim DLC tree LOD): inherits from NiTriBasedGeom per nif.xml — distinct wrapper, NOT routed through BSTriShape (#838 regression guard, 8d416cc). BSLODTriShape vs BSMeshLODTriShape vs BSSubIndexTriShape — distinct block types with distinct trailing data; dispatch + import must not confuse them. **`BsLagBoneController`** + **`BsProceduralLightningController`** (#837): dedicated parsers — without them ~120 by-design `block_size` WARN events fire per Meshes0 sweep. BSTreeNode wind-bone list parsing (SpeedTree). BSPackedCombined[Shared]GeomDataExtra — distant LOD batch layout. `as_ni_node` walker unwraps Skyrim NiNode subclasses (BSFadeNode, BSBlastNode, BSDamageStage, BSMultiBoundNode, BSTreeNode).
**Output**: `/tmp/audit/skyrim/dim_4.md`

### Dimension 5: Real-Data Validation & Rendering
**Subagent**: `general-purpose`
**Entry points**: `crates/nif/examples/nif_stats.rs`, canonical CLI demos
**Checklist**: Parse rate holds at 100% (18862 / 18862). `cargo run -- --bsa "Skyrim - Meshes0.bsa" --mesh meshes\clutter\ingredients\sweetroll01.nif --textures-bsa "Skyrim - Textures3.bsa"` still renders correctly at ≥3000 FPS (2026-04-22 baseline: ~3000-5000 FPS on RTX 4070 Ti @ 1280×720; regression if substantially below 3000). Pick: one creature (dragon skeleton, NPC head), one landscape (tree LOD), one magic effect (BSEffectShaderProperty). Trace each through `import_nif_scene` → verify mesh count, material extraction, and texture handle resolution. Pick a FaceGen head — parses but expected visual fidelity is limited (no FaceGen runtime).
**Output**: `/tmp/audit/skyrim/dim_5.md`

### Dimension 6: ESM Readiness & Forward Blockers
**Subagent**: `general-purpose`
**Entry points**: `crates/plugin/src/legacy/tes5.rs`, `ROADMAP.md`
**Checklist**: TES5 ESM parser status (stub). What record types would a minimum "interior cell renders" require (CELL, REFR, STAT, LIGH, WEAP, ARMO)? TES5 group structure differences vs TES4 (Skyrim uses compressed records extensively — groups can be compressed). Skyrim-specific record types needed for proper rendering: NAVM (navmesh — not needed for render), LAND (different heightmap scale), LTEX, TXST, ADDN (addon nodes). Dialog/Quest format (TES5 introduced radiant story). FaceGen head part (HDPT) records — metadata for facegen but runtime is out of scope. Animation system (havok behavior graphs — out of scope, but BSBehaviorGraphExtraData should parse without error).
**Output**: `/tmp/audit/skyrim/dim_6.md`

## Phase 3: Merge

1. Read all `/tmp/audit/skyrim/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_SKYRIM_<TODAY>.md` with structure:
   - **Executive Summary** — Current state: individual mesh rendering works; cell loading blocked on ESM parser. Shader coverage across 8 BSLightingShaderProperty variants.
   - **Dimension Findings** — Grouped by severity per dimension.
   - **Shader Variant Coverage Matrix** — 8 BSLightingShaderProperty variants × parse-complete / import-complete / render-complete.
   - **Forward Blocker Chain** — What must land for "interior cell renders" (TES5 ESM subset → compressed record decompression → ...).
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_SKYRIM_<TODAY>.md`
