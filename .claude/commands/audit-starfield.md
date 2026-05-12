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
| NIF format        | BSVER 172+ (Starfield adds further extensions on top of the BSVER ≥ 155 baseline)           |
| BA2 format        | v2 ✓ / v3 ✓ (12-byte header extension with `compression_method` field — zlib or LZ4 block)  |
| ESM parser        | **None** — Starfield ESM format not spec'd / not parsed                                     |
| Parse rate        | 100.00% (31058 / 31058) via shader-blocks route                                             |
| Rendering         | Individual mesh with material-reference stub; cell loading not possible                     |
| Reference data    | `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/`                                   |

### Known Specifics

- **CRC32-hashed shader flag arrays** (BSVER ≥ 132) — `BSLightingShaderProperty` / `BSEffectShaderProperty` store shader flags as arrays of CRC32 hashes of named flag strings instead of bit masks.
- **SF2 array** (BSVER ≥ 152) — additional shader flag array.
- **BSVER == 155 (FO76 baseline)** — adds `BSShaderType155` dispatch with distinct skin-tint / hair-tint layouts, `BSSPLuminanceParams`, `BSSPTranslucencyParams`, `BSTextureArray` lists.
- **BGSM / BGEM material references** — when `Name` is a non-empty BGSM/BGEM path the parser short-circuits and returns a material-reference stub; the real material is in the external file (out of scope for NIF parsing).
- **WetnessParams** — extended with `Unknown 1` (BSVER > 130) and `Unknown 2` (BSVER == 155).
- **BSEffectShaderProperty** — adds Reflectance / Lighting / Emittance / Emit Gradient textures.
- **BA2 v3 compression** — header has a 12-byte extension (vs. 8 for v2) with `compression_method`: 0 = zlib, 3 = LZ4 block. GNRL + DX10 both dispatch through `decompress_chunk()` that selects based on archive-level method.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 6.

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
**Checklist**: Stopcond check — BSVER ≥ 155 && Name is non-empty BGSM/BGEM path → return material-reference stub + **do NOT read the Phong trailing fields** (they belong in the BGSM). Name path flows through `ImportedMesh.material_path` → `Material.material_path`. Validate on a Starfield mesh that references a BGSM: `mesh.info` debug command shows the material path and `tex.missing` lists it as expected-missing (correct behavior). Check whether the BGEM variant is handled distinctly from BGSM (different texture set conventions). Count unique BGSM / BGEM paths in a Starfield archive — this is the work queue for the eventual BGSM parser.
**Output**: `/tmp/audit/starfield/dim_3.md`

### Dimension 4: Vertex Format & Mesh Variants
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/blocks/bs_tri_shape.rs` (or equivalent)
**Checklist**: BSTriShape vertex format for Starfield. Any new VF_* flag bits introduced beyond FO4's set. Tangent-space reconstruction from packed normals. Vertex count limits (Starfield has much higher-detail meshes than FO4). BSGeometrySegmentData — current `block_size` skip is correct; any change in presence or layout for Starfield (the N23.9 note said full parsing deferred until segment metadata surfaces to rendering). Check for new `BSMeshLODTriShape` / `BSSubIndexTriShape` variants or replacements.
**Output**: `/tmp/audit/starfield/dim_4.md`

### Dimension 5: Real-Data Validation
**Subagent**: `general-purpose`
**Entry points**: `crates/nif/examples/nif_stats.rs`, `crates/nif/tests/parse_real_nifs.rs`
**Checklist**: Parse rate holds at 100% on 31058 Starfield NIFs via `BYROREDUX_STARFIELD_DATA=... cargo test -p byroredux-nif --test parse_real_nifs -- --ignored starfield`. Verify all 22 Starfield texture archives + 53 vanilla + patch BA2s listed in the roadmap extract cleanly (session 7 validated ~128K DX10 textures + 0 failures — make sure that still holds). Pick 5 representative meshes: a clutter item, a ship hull, a character body, a weapon, a landscape feature. Trace each through `import_nif_scene`. Watch for `NiUnknown` placeholders in the block histogram — these would indicate new block types introduced since N23.9.
**Output**: `/tmp/audit/starfield/dim_5.md`

### Dimension 6: ESM Roadmap & Forward Blockers
**Subagent**: `general-purpose`
**Entry points**: `ROADMAP.md`, `crates/plugin/src/legacy/` (no starfield.rs exists yet)
**Checklist**: Starfield ESM format — is there any public documentation yet? What's the closest prior (FO4 ESM) and where would it diverge (planet/system/space-cell records are entirely new)? BGSM material parser — still out of scope; estimate effort. Without ESM, what demos are possible (individual mesh rendering via `--mesh` is supported today). What would the minimum "render a Starfield interior" involve: ESM parser → BGSM parser → space-cell concept → everything else. Is Starfield's use of physics-based ship assembly realistic to render without the ESM form-linker? Lowest-effort visible-progress milestone for Starfield (e.g., "render an arbitrary BSLightingShaderProperty mesh with a stub white material instead of the missing-texture magenta").
**Output**: `/tmp/audit/starfield/dim_6.md`

## Phase 3: Merge

1. Read all `/tmp/audit/starfield/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_STARFIELD_<TODAY>.md` with structure:
   - **Executive Summary** — Current state: NIF parse + BA2 extract 100%, everything above that (ESM, cells, materials) is unshipped. This audit is mostly a gap inventory.
   - **Dimension Findings** — Grouped by severity per dimension.
   - **CRC32 Flag Table** — Known/unknown flag-name → CRC32 mappings for the shader flag arrays (anything we can derive empirically from observed hashes).
   - **Forward Blocker Chain** — What must land for "Starfield mesh renders with real material" (BGSM parser first) vs "Starfield cell renders" (ESM parser + space-cell concept — very far).
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_STARFIELD_<TODAY>.md`
