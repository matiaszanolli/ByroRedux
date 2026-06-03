---
description: "Per-game audit of Starfield compatibility — BA2 v2/v3 + LZ4 block, CRC32 flag arrays, no ESM"
argument-hint: "--focus <dimensions>"
---

# Starfield Compatibility Audit

Deep audit of ByroRedux readiness for **Starfield** content.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, game data locations, methodology, deduplication rules, and finding format.

## Game Context

| Aspect            | State                                                                                       |
|-------------------|---------------------------------------------------------------------------------------------|
| NIF format        | BSVER ≥ 168 retail (Starfield adds further extensions on top of the BSVER ≥ 155 baseline)    |
| BA2 format        | v2 ✓ / v3 ✓ (12-byte header extension with `compression_method` field — zlib or LZ4 block)  |
| ESM parser        | **Live** — first-class `GameKind::Starfield` (HEDR 0.96 classifier); parses end-to-end       |
| Mesh path         | `BSGeometry` (inline + external `.mesh` companion), not `BSTriShape`                          |
| Materials         | CDB (`crates/sfmaterial`) + BGSM/BGEM (`crates/bgsm`) — both shipped, wired `--materials-ba2` |
| Parse rate        | NIF aggregate 98.6% (Meshes01 97.21% / 31 058); BA2 extract 100%                            |
| Cell loading      | **Walkable Cydonia interior** (Session 42: 93 547 entities + 91 698 static colliders)        |
| Reference data    | `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/`                                   |

### Known Specifics

- **CRC32-hashed shader flag arrays** (BSVER ≥ 132) — `BSLightingShaderProperty` / `BSEffectShaderProperty` store shader flags as arrays of CRC32 hashes of named flag strings instead of bit masks.
- **SF2 array** (BSVER ≥ 152) — additional shader flag array.
- **BSVER == 155 (FO76 baseline)** — adds `BSShaderType155` dispatch with distinct skin-tint / hair-tint layouts, `BSSPLuminanceParams`, `BSSPTranslucencyParams`, `BSTextureArray` lists.
- **BGSM / BGEM material references** — when `Name` is a non-empty BGSM/BGEM path the NIF parser short-circuits and returns a material-reference stub; the real material lives in the external file. That external file is now parsed: `crates/bgsm/` reads BGSM/BGEM and `asset_provider::merge_bgsm_into_mesh` folds it into the `ImportedMesh`.
- **CDB material database** — vanilla Starfield ships all material data inside a single `materials\materialsbeta.cdb` Component Database (in `Starfield - Materials.ba2`), parsed by the dedicated `crates/sfmaterial/` consumer and extracted via the `--materials-ba2` flag (`asset_provider.rs`).
- **WetnessParams** — extended with `Unknown 1` (BSVER > 130) and `Unknown 2` (BSVER == 155).
- **BSEffectShaderProperty** — adds Reflectance / Lighting / Emittance / Emit Gradient textures.
- **BA2 v3 compression** — header has a 12-byte extension (vs. 8 for v2) with `compression_method`: 0 = zlib, 3 = LZ4 block. GNRL + DX10 both dispatch through `decompress_chunk()` that selects based on archive-level method.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 9.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/starfield`.
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
4. Confirm `Starfield/Data/` exists; if not, note which dimensions lose real-data validation.

## Phase 2: Launch Dimension Agents (parallel)

### Dimension 1: NIF BSVER 155–172+ Shader Blocks
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/blocks/properties.rs` (BSLightingShaderProperty, BSEffectShaderProperty), `docs/legacy/nif.xml`
**Checklist**: CRC32 flag-array parsing for BSVER ≥ 132 — array count + per-element u32 CRC. Mapping from CRC32 hash → flag semantics (do we have a table?). SF2 array for BSVER ≥ 152. BSShaderType155 dispatch — skin-tint vs hair-tint variant layouts differ from Skyrim's 8-variant enum. BSSPLuminanceParams layout. BSSPTranslucencyParams layout. BSTextureArray — variable-length texture list vs fixed BSShaderTextureSet. WetnessParams Unknown 1 (BSVER > 130) + Unknown 2 (BSVER == 155). Refraction power on BSEffectShaderProperty (FO76-style). BSEffectShaderProperty new textures: Reflectance, Lighting, Emittance, Emit Gradient.
**Output**: `/tmp/audit/starfield/dim_1.md`

### Dimension 2: BA2 v2 / v3 — LZ4 Block Decompression
**Subagent**: `general-purpose`
**Entry points**: `crates/bsa/src/ba2.rs`
**Checklist**: v2 header format (standard Starfield — 8-byte extension). v3 header format (12-byte extension with `compression_method` at the correct offset). Dispatch: compression_method == 0 → zlib, == 3 → LZ4 block, others → error. `lz4_flex::block::decompress` usage — does it need a specific `max_size` hint, and does BA2 supply it? GNRL + DX10 both go through the unified `decompress_chunk()`. DX10 chunk layout unchanged from FO4 v1 — the per-chunk-layout-difference diagnosis for v3 was wrong; the real issue was the 4-byte compression_method field offset. Parse-rate sweep across all v2 and v3 archives.
**Output**: `/tmp/audit/starfield/dim_2.md`

### Dimension 3: BGSM Material Reference Flow
**Subagent**: `renderer-specialist`
**Entry points**: `crates/nif/src/blocks/properties.rs` (stopcond), `crates/nif/src/import/material/` (mod, walker, shader_data)
**Checklist**: Stopcond check — BSVER ≥ 155 && Name is non-empty BGSM/BGEM path → return material-reference stub + **do NOT read the Phong trailing fields** (they belong in the BGSM). Name path flows through `ImportedMesh.material_path` → `Material.material_path`. Validate on a Starfield mesh that references a BGSM: `mesh.info` debug command shows the material path and `tex.missing` lists it as expected-missing (correct behavior). The external BGSM/BGEM file is now parsed by `crates/bgsm/` and merged in `asset_provider::merge_bgsm_into_mesh` (`byroredux/src/asset_provider.rs`); confirm the BGEM variant (`crates/bgsm/src/bgem.rs`) is handled distinctly from BGSM (`bgsm.rs`) — different texture-set conventions, plus the BGEM `glass_enabled` flag. **Disney BSDF / PBR (#1248-#1252)** is the canonical lobe (adapted from GLSL-PathTracer MIT + Burley 2012, attribution block at top of `crates/renderer/shaders/triangle.frag`). For Starfield, BGSM/BGEM authoring flows through `cell_loader::pack_bgsm_material_flags` (`byroredux/src/cell_loader.rs`), which sets `material_flag::BGSM_PBR` / `BGSM_AUTHORED` from `ImportedMesh.is_pbr` / `from_bgsm`. The classification itself happens at the **single NIFAL boundary** — `material_translate::translate_material` (`byroredux/src/material_translate.rs`), never per-draw in the shader; `Material.metalness` / `Material.roughness` are plain resolved `f32` set there (see also `/audit-nifal`). **#1232 empty BSGeometry tangents** (`293db681`): Starfield meshes with absent / zero-length tangent blobs now route through `synthesize_tangents_yup` (Mikkelsen) at `crates/nif/src/import/mesh/bs_geometry.rs:139` — verify the fallback is reached and produces unit-length tangents.
**Output**: `/tmp/audit/starfield/dim_3.md`

### Dimension 4: BSGeometry Mesh Extraction (Starfield's actual mesh path)
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/import/mesh/bs_geometry.rs` (geometry extraction), `crates/nif/src/blocks/bs_geometry.rs` (block parse), with `crates/nif/src/import/mesh/bs_tri_shape.rs` as the FO4/Skyrim contrast (Starfield does NOT use `BSTriShape`)
**Checklist**: `BSGeometry` is the Starfield geometry path — Stage A inline geometry (`has_internal_geom_data()`) vs Stage B external `.mesh` companion. **#1292** — external `.mesh` resolved via the canonical `geometries\<X>.mesh` path (`bs_geometry.rs:58`); without this the spawn rate collapses (Cydonia 75 → 93 547 entities). **#1209** — iterate every LOD slot, not `meshes.first()` (a `None` short-circuit when LOD 0 was `External` despite later `Internal` slots). **#1203** — skin chain resolved via `BSSkin::Instance` + `BSSkin::BoneData` (`bs_geometry.rs:171`). **#1232** — empty-tangent fallback to `synthesize_tangents_yup` (also pinned in Dim 3). PBR scalars: `metalness_override` / `roughness_override` are forwarded from the BGSM-resolved `legacy_pbr` at `bs_geometry.rs:254-255`. Watch for new VF_* / vertex-attribute bits beyond FO4's set. Vertex count limits (Starfield meshes are far higher-detail than FO4).
**Output**: `/tmp/audit/starfield/dim_4.md`

### Dimension 5: Real-Data Validation
**Subagent**: `general-purpose`
**Entry points**: `crates/nif/examples/nif_stats.rs`, `crates/nif/tests/parse_real_nifs.rs`
**Checklist**: Parse rate holds at the compat-matrix figure (Meshes01 97.21% / 31 058; aggregate 98.6% — see `docs/engine/game-compatibility.md`) via `BYROREDUX_*_DATA=... cargo test -p byroredux-nif --test parse_real_nifs parse_rate_starfield_all_meshes -- --ignored` (walks all 5 vanilla mesh archives; `parse_rate_starfield` covers Meshes01 only). The residual truncation tail in Meshes01/MeshesPatch is tracked at #746/#747 — confirm it has not grown. Verify Starfield texture archives matching `Starfield - *Textures*.ba2` (30 as of 2026-05-21 post-Shattered-Space, was 22 in Session 7 — per #1185) extract cleanly (session 7 validated ~128K DX10 textures + 0 failures — make sure that still holds). Pick 5 representative meshes: a clutter item, a ship hull, a character body, a weapon, a landscape feature. Trace each through `import_nif_scene` (`crates/nif/src/import/mod.rs:116`). Watch for `NiUnknown` placeholders in the block histogram — these would indicate new block types introduced since N23.9.
**Output**: `/tmp/audit/starfield/dim_5.md`

### Dimension 6: ESM Phase Roadmap & Forward Blockers
**Subagent**: `general-purpose`
**Entry points**: `ROADMAP.md`, `docs/engine/starfield-esm-roadmap.md`, `docs/engine/starfield-esm-phase0-baseline.md`
**Checklist**: The ESM parser is **live** (not a forward blocker) — Starfield is a first-class `GameKind` and Cydonia is walkable. Re-scope this dimension to the remaining phase work, not a from-scratch gap inventory. Read `starfield-esm-roadmap.md` and confirm the phase ordering matches shipped state (Session 42 collapsed the original 7–11-session estimate to 3–4 sessions). Open follow-ups to scope: #1293 (16-byte SF XCLL tail decode), #746/#747 (NIF truncation tail). (The #1290 forward-blocker re-ordering is DONE — the chain below + `starfield-esm-roadmap.md` already reflect #762 + #1289 shipped.) Space-cell / planet / system records that have no FO4 analogue — which are parsed vs stubbed in `crates/plugin/src/esm/records/mod.rs`? Is Starfield's physics-based ship assembly (MSWP / module records) realistic to render via the form-linker? What is the lowest-effort next visible-progress milestone now that Cydonia renders (e.g., a vanilla exterior worldspace tile)?
**Output**: `/tmp/audit/starfield/dim_6.md`

### Dimension 7: Starfield ESM + Cell Bring-up
**Subagent**: `general-purpose`
**Entry points**: `crates/plugin/src/esm/reader.rs` (`GameKind::Starfield` HEDR-0.96 classifier), `crates/plugin/src/esm/records/mod.rs` (FourCC dispatch), `crates/plugin/src/esm/cell/walkers.rs` (XCLL + per-cell NAVM), `byroredux/src/cell_loader/spawn.rs` (REFR placement)
**Checklist**: HEDR-0.96 → `GameKind::Starfield` classification (`reader.rs:140`). FourCC dispatch coverage in `records/mod.rs` — which record types are parsed vs unrecognised; cross-check against the `sf_smoke` / `sf_parse_check` baseline (`crates/plugin/examples/sf_smoke.rs`, `sf_parse_check.rs`). **#1291** — `XCLL_SIZES_STARFIELD = [28, 108]` (`walkers.rs:30`), split off the FNV-era `[28, 40]` bucket; the 108-byte body is Skyrim's 92-byte layout plus a 16-byte tail (#1293 decode follow-up). Per-cell NAVM collection (`walkers.rs:621`, #1272). Walkable-Cydonia spawn regressions in `cell_loader/spawn.rs`: **#1294** static-trimesh fallback gated on `base_layer` not `final_layer` (synthesized collider count 0 → 91 698); **#1235** `SceneFlags::from_nif` (`crates/core/src/ecs/components/scene_flags.rs:57`) attached at spawn; **#1212/#1213/#1214** `FormIdComponent` / `LocalBound` / `BSXFlags` at spawn; **#1284** `SkinSlotPool` ceiling raise (`crates/core/src/ecs/resources.rs:650`) + `WorldBound::ZERO` seed for Cydonia's skinned density.
**Output**: `/tmp/audit/starfield/dim_7.md`

### Dimension 8: NIFAL Canonical Material Translation for Starfield
**Subagent**: `renderer-specialist`
**Entry points**: `byroredux/src/material_translate.rs` (`translate_material` — the single boundary), `crates/core/src/ecs/components/material.rs` (`Material::resolve_pbr`)
**Checklist**: `translate_material` (`material_translate.rs:65`) is the **single** raw `ImportedMesh` → ECS `Material` boundary — per-game / per-material classification happens here, never per-draw in the shader (see also `/audit-nifal`). Verify BSGeometry/BGSM/CDB-resolved Starfield meshes land with `Material.metalness` / `Material.roughness` as **plain resolved `f32`** (`material.rs:216,222`), set once — no `Option<f32>` / per-draw `classify_pbr` plumbing (that pattern was removed by the NIFAL refactor). Confirm `Material::resolve_pbr` and the `EmissiveSource` discriminator (#1280, `crates/core/src/ecs/components/material::EmissiveSource`, tagged in `crates/nif/src/import/material/walker.rs`) behave for SF content. NIFAL particle slice reaching SF NIFs: typed `NiPSysEmitter` / `NiPSysEmitterCtlr` (`crates/nif/src/blocks/particle.rs`) → `extract_emitter_params` / `extract_emitter_rate` (`crates/nif/src/import/walk/mod.rs:670,713`) → `systems::particle::apply_emitter_params` (`byroredux/src/systems/particle.rs:29`). NIFAL collision slice (relevant to Cydonia's 91 698 colliders): `BhkMultiSphereShape` + `BhkConvexListShape` translate to `CollisionShape` in `crates/nif/src/import/collision.rs:301,397`.
**Output**: `/tmp/audit/starfield/dim_8.md`

### Dimension 9: Starfield CDB + BGSM Material Database
**Subagent**: `renderer-specialist`
**Entry points**: `crates/sfmaterial/src/reader.rs` (`materialsbeta.cdb` reader, #762), `crates/bgsm/src/` (external BGSM/BGEM parser), `byroredux/src/asset_provider.rs` (`--materials-ba2` wiring)
**Checklist**: Replaces the old "BGSM out of scope" assumption — both material paths have shipped. CDB: `ComponentDatabaseFile::parse` (`reader.rs:29`) consumes `materials\materialsbeta.cdb`, extracted from `Starfield - Materials.ba2` via `--materials-ba2` (`asset_provider.rs:492,507`). BGSM/BGEM: `crates/bgsm/src/bgsm.rs` + `bgem.rs` parsed, then `asset_provider::merge_bgsm_into_mesh` (`asset_provider.rs:922`) folds the result into `ImportedMesh`; `cell_loader::pack_bgsm_material_flags` (`byroredux/src/cell_loader.rs:177`) packs `material_flag::{BGSM_AUTHORED, BGSM_PBR, BGSM_TRANSLUCENCY, BGSM_MODEL_SPACE_NORMALS}` (#1147 / #1077 / #1076). BGEM `glass_enabled` (`bgem.rs:34`) as the authoritative glass signal (#1280, consumed in `byroredux/src/helpers.rs:202`). Count unique CDB material handles vs BGSM/BGEM file references in a Starfield archive — confirm the CDB path supersedes loose-file BGSM for vanilla content.
**Output**: `/tmp/audit/starfield/dim_9.md`

**Pin — #762 / sfmaterial CDB chunk index**: guard `crates/sfmaterial/src/reader.rs::index_chunks` (`reader.rs:142`) against the chunk-index regression already referenced at `byroredux/src/asset_provider.rs:2461`.

## Phase 3: Merge

1. Read all `/tmp/audit/starfield/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_STARFIELD_<TODAY>.md` with structure:
   - **Executive Summary** — Current state: Starfield is a first-class `GameKind` with NIF + BA2 at the compat-matrix rate (98.6% aggregate), CDB + BGSM/BGEM materials shipped, and a **walkable Cydonia interior** (Session 42). This is a depth/correctness audit, not a gap inventory — focus on regressions in the bring-up surface (spawn, NIFAL translation, CDB chunk index) and the remaining ESM phase work.
   - **Dimension Findings** — Grouped by severity per dimension.
   - **CRC32 Flag Table** — Known/unknown flag-name → CRC32 mappings for the shader flag arrays (anything we can derive empirically from observed hashes).
   - **Forward Blocker Chain** (re-ordered per #1290, closed 2026-05-29 — both the CDB parser #762 AND the consumer wiring #1289 Phase 1 have shipped) — Remaining work for full Starfield coverage, in order: per-field CDB extraction (#1289 Phase 2 follow-up — `.mat`-resolved materials currently reach the Disney lobe with NIF defaults), exterior worldspace tiles, space-cell / planet records, #1293 16-byte XCLL tail decode, and the #746/#747 NIF truncation tail — NOT the "BGSM parser first / ESM very far" chain (both have shipped).
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_STARFIELD_<TODAY>.md`
