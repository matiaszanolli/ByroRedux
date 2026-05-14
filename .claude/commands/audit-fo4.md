---
description: "Per-game audit of Fallout 4 compatibility — BA2, half-float verts, BGSM materials, SCOL architecture"
argument-hint: "--focus <dimensions>"
---

# Fallout 4 Compatibility Audit

Deep audit of ByroRedux readiness for **Fallout 4** content.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, game data locations, methodology, deduplication rules, and finding format.

## Game Context

| Aspect            | State                                                                                |
|-------------------|--------------------------------------------------------------------------------------|
| NIF format        | BSVER 130 (and Next-Gen patches)                                                     |
| BA2 format        | v1 ✓ / v2 / v3 / v7 ✓ / v8 ✓ exhaustive `match` over `{1, 2, 3, 7, 8}` (#811 FO4-D2-NEW-01, f480337) — unknown majors bail at `open()` time, no silent v1-fallback. GNRL + DX10 variants. |
| ESM parser        | Stub + SCOL / MOVS / PKIN / TXST records added in session 10. TXST DODT decal-data sub-record + DNAM flags now parsed (#813 / #814, 6941da6 — was silently dropping authoring on 207/382 (DODT) + 382/382 (DNAM) vanilla TXSTs). |
| Parse rate        | 100.00% (34995 / 34995)                                                              |
| Rendering         | Individual mesh + material diagnostics; cell loading not wired                       |
| Reference data    | `/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/`                            |

### Known Specifics

- **Half-float vertices** — `VF_FULL_PRECISION` bit controls whether positions/normals are stored as f32 or u16 half-precision. FO4 defaults to half.
- **FO4 shader flags** — `BSLightingShaderProperty` flags are a **u32 pair** (two 32-bit masks: shader_flags_1 + shader_flags_2), not the single mask of FO3/FNV/Skyrim.
- **Trailing fields** — FO4 adds subsurface_color, subsurface_strength, rimlight_power, backlight_power, wetness params (Unknown 1 from BSVER > 130).
- **BGSM / BGEM materials** — FO4 stores PBR-ish material parameters in external `.bgsm` / `.bgem` files. `BSLightingShaderProperty.net.name` holds the path; the parser **stops and returns a material-reference stub** when BSVER ≥ 155 and Name is a BGSM/BGEM path. BGSM parser itself is out of scope — the Name path flows through `ImportedMesh.material_path` for diagnostics.
- **Architecture records** — `SCOL` (static collections / prefabs), `MOVS` (movable statics), `PKIN` (packins), `TXST` (texture sets). Added in session 10; semantics of how SCOL placements expand into individual statics still needs the cell loader to understand them.
- **Specialty blocks** — BSSubIndexTriShape, BSClothExtraData, BSConnectPoint::Parents/Children, BSBehaviorGraphExtraData, BSInvMarker, BSSkin::Instance / BSSkin::BoneData.
- **Inline per-vertex tangents** (#795 / #796, b63ab0c) — when `VF_TANGENTS | VF_NORMALS` are both set on the packed-vertex flag, FO4+ BSTriShape ships tangents inline in the packed-vertex blob, NOT in a separate `NiBinaryExtraData`. Distinct from the Skyrim-via-`NiBinaryExtraData` path of Oblivion / FO3 / FNV (#786 / 5dde345). Any FO4 audit proposing to consolidate the two paths into one is a regression of #795/#796.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 6.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/fo4`.
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
4. Confirm `Fallout 4/Data/` exists; if not, note which dimensions lose real-data validation.

## Phase 2: Launch Dimension Agents (parallel)

### Dimension 1: NIF BSVER 130 + Half-Float Vertices
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/blocks/bs_tri_shape.rs` (or equivalent), `crates/nif/src/import/mesh/bs_tri_shape.rs`
**Checklist**: VF_FULL_PRECISION flag resolution — default-half unless set. Half-float decode matches IEEE 754 binary16 (including denormals and NaN). BSSubIndexTriShape segment data walked correctly (FO4 uses this extensively for actors). Skinned-vertex bone indices + weights extraction honors packed layout. BSVER 130 trailing fields on BSLightingShaderProperty: subsurface, rimlight, backlight, fresnel, wetness (Unknown 1 for BSVER > 130). Next-Gen patch NIFs (slightly different BSVER values) still dispatch correctly.
**Output**: `/tmp/audit/fo4/dim_1.md`

### Dimension 2: BA2 Reader — GNRL + DX10 Variants (Exhaustive Version Match)
**Subagent**: `general-purpose`
**Entry points**: `crates/bsa/src/ba2.rs`
**Checklist**: Version dispatch is now an **exhaustive `match` over `{1, 2, 3, 7, 8}`** (#811 FO4-D2-NEW-01, f480337) — unknown majors bail at `open()` time, no silent v1-fallback. Audit any reintroduction of cascading `if v == 1 { ... } else if v == 7 { ... }` chains: that pattern is the regression. Mirrors the BSA reader's allowlist discipline at `archive.rs:165`. GNRL (general files — meshes, scripts): extract a mesh and a script and verify byte-exact. DX10 (textures): DDS header reconstruction from width/height/format/mip count. DXT1/DXT5/BC5/BC7 format encoding in `dxgi_format`. Mip chunk assembly — mip0 (largest) vs mip_last (smallest) ordering. Compression flag per-archive (zlib vs uncompressed). Verify against the ROADMAP claim of 100% on 34995 NIFs + the Textures archives.
**Output**: `/tmp/audit/fo4/dim_2.md`

### Dimension 3: FO4 Shader Flags & BGSM Material Reference
**Subagent**: `renderer-specialist`
**Entry points**: `crates/nif/src/blocks/properties.rs` (BSLightingShaderProperty), `crates/nif/src/import/material/` (mod, walker, shader_data)
**Checklist**: Shader flags — u32 pair read correctly (shader_flags_1 + shader_flags_2, two separate fields). Flag bit positions for FO4 differ from Skyrim — verify decal, alpha-test, skinned, glow, window, refraction, parallax, facegen bits are read from the correct mask + bit. BSLightingShaderProperty stopcond on BGSM Name path — when the material is external, parser returns early without reading the Phong trailing fields (those belong in the BGSM). `ImportedMesh.material_path` flows through to `Material.material_path` so diagnostics show BGSM references. `mesh.info <entity_id>` debug command surfaces material_path when texture_path is absent (session 10 behavior). **BSShaderTextureSet slot routing on `BSLightingShaderType`** (#563, d9bc363): SkinTint and HairTint sample from different slots than the default LSP path — verify the per-shader-type slot map is in lockstep with what the fragment shader reads.
**Output**: `/tmp/audit/fo4/dim_3.md`

### Dimension 4: ESM Architecture Records (SCOL / MOVS / PKIN / TXST)
**Subagent**: `general-purpose`
**Entry points**: `crates/plugin/src/esm/records/`, `crates/plugin/src/esm/cell/` (post-Session-34 split: `cell/{mod,walkers,support,tests}.rs`)
**Checklist**: SCOL record structure — a prefab that contains placements of child statics + scale/rotation per instance. Are these parsed into a structure the cell loader can expand? MOVS — movable statics (physics-driven). PKIN — packins (grouped content bundles). TXST — texture sets referenced by NIF material paths. **TXST DODT + DNAM** (#813 / #814, 6941da6): decal-data sub-record + flags now parsed via `DecalData`. Pre-fix: 207/382 (DODT) + 382/382 (DNAM) vanilla TXSTs silently dropped their authoring. Audit any path that reads TXST without `DecalData` — that's the regression pattern. Post-Session-34, `cell.rs` was split into `crates/plugin/src/esm/cell/{mod,walkers,support,tests}.rs`; the TXST match arms live in `cell/walkers.rs`. If an `unreachable_patterns` warning surfaces there (or `grep -n 'unreachable_patterns' crates/plugin/src/esm/cell/` returns anything) it suggests `b"TXST"` matches before reaching the intended arm — investigate. How many SCOL / MOVS / PKIN / TXST records exist in `Fallout4.esm`? Minimum record set needed for a hello-world FO4 cell load. **FO4-architecture map exposure**: 5 FO4-architecture maps must surface in `categories()` index (#817 FO4-D4-NEW-05, af9f4de) — missing entries hide the records from any caller iterating by category. Real-data FO4 ESM parse-rate harness lives at `#819 FO4-D4-NEW-07` (d8f859d) — re-run before reporting parse-rate findings.
**Output**: `/tmp/audit/fo4/dim_4.md`

### Dimension 5: Real-Data Validation
**Subagent**: `general-purpose`
**Entry points**: `crates/nif/examples/nif_stats.rs`, `crates/nif/tests/parse_real_nifs.rs`
**Checklist**: Parse rate holds at 100% on 34995 FO4 NIFs via `BYROREDUX_FO4_DATA=... cargo test -p byroredux-nif --test parse_real_nifs -- --ignored fo4`. Load a settlement object (workshop crafted item). Load a creature (deathclaw, super mutant). Load a power armor frame (heavy skinning + connect points). Load a weapon (receiver + barrel + stock — uses BSConnectPoint to attach modular parts). For each, trace `import_nif_scene` output: mesh count, material_path (BGSM reference or null), skinned / rigid classification, connect-point extra data presence. Spot-check BSBehaviorGraphExtraData string (references external havok behavior graph — parse-only for now).
**Output**: `/tmp/audit/fo4/dim_5.md`

### Dimension 6: Forward Blockers & BGSM Roadmap
**Subagent**: `general-purpose`
**Entry points**: `ROADMAP.md`, `crates/plugin/src/legacy/fo4.rs`
**Checklist**: BGSM / BGEM parser — not shipped; needed for proper FO4 materials. Where would it live (new crate vs `plugin::fo4`)? File format (JSON-ish or binary — verify from a sample file). What fields does the existing shader pipeline consume that would need BGSM to source them (albedo tint, emissive color, subsurface, parallax scale, etc.)? FO4 ESM parser coverage — beyond architecture records, what's needed for cell load (REFR with more complex placement data, LIGH with power state, CONT with leveled items, NPC_ with face morph data)? Havok Next-Gen integration — mostly out of scope but BSBehaviorGraphExtraData parse-only is the current state; verify.
**Output**: `/tmp/audit/fo4/dim_6.md`

## Phase 3: Merge

1. Read all `/tmp/audit/fo4/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_FO4_<TODAY>.md` with structure:
   - **Executive Summary** — Current state: NIF + BA2 at 100%, ESM architecture records landed, BGSM parser + cell loader pending.
   - **Dimension Findings** — Grouped by severity per dimension.
   - **BGSM Readiness Table** — BGSM field × currently-parsed-from-NIF-fallback / would-read-from-BGSM.
   - **Forward Blocker Chain** — What must land for "FO4 interior renders" (TES5-style ESM for FO4 → BGSM parser → SCOL expansion in cell loader → ...).
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_FO4_<TODAY>.md`
