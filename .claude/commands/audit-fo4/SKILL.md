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
| ESM parser        | Stub + SCOL / MOVS / PKIN / TXST records added in session 10. TXST DODT decal-data sub-record + DNAM flags now parsed (#813 / #814, 6941da6 — was silently dropping authoring on 207/382 (DODT) + 382/382 (DNAM) vanilla TXSTs). SCOL / PKIN placements now **expand into individual statics** in the cell loader (`byroredux/src/cell_loader/refr.rs::expand_scol_placements` / `expand_pkin_placements`, depth-capped by `MAX_PKIN_DEPTH = 4`, #1180 / #1182), wired via `cell_loader/references.rs`. |
| Parse rate        | 100.00% (34995 / 34995)                                                              |
| Rendering         | Individual mesh + material diagnostics; FO4 cell loading is **wired** (`Fallout4.esm` SCOL/PKIN expansion + `cell_loader/precombined.rs` PreCombined-Mesh spawn, exterior Phase-3a). BGSM/BGEM materials parsed (`crates/bgsm/`) and merged. |
| Reference data    | `/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/`                            |

### Known Specifics

- **Half-float vertices** — `VF_FULL_PRECISION` bit controls whether positions/normals are stored as f32 or u16 half-precision. FO4 defaults to half.
- **FO4 shader flags** — `BSLightingShaderProperty` flags are a **u32 pair** (two 32-bit masks: shader_flags_1 + shader_flags_2), not the single mask of FO3/FNV/Skyrim.
- **Trailing fields** — FO4 adds subsurface_color, subsurface_strength, rimlight_power, backlight_power, wetness params (Unknown 1 from BSVER > 130).
- **BGSM / BGEM materials** — FO4 stores PBR-ish material parameters in external `.bgsm` / `.bgem` files. `BSLightingShaderProperty.net.name` holds the path; the parser **stops and returns a material-reference stub** when BSVER ≥ 155 and Name is a BGSM/BGEM path. BGSM parser itself is out of scope — the Name path flows through `ImportedMesh.material_path` for diagnostics.
- **Architecture records** — `SCOL` (static collections / prefabs), `MOVS` (movable statics), `PKIN` (packins), `TXST` (texture sets). Added in session 10; SCOL / PKIN placements now expand into individual statics in `byroredux/src/cell_loader/refr.rs` (`expand_scol_placements` / `expand_pkin_placements`, recursion bounded by `MAX_PKIN_DEPTH = 4`, #1180 / #1182; tests at `cell_loader/{scol_expansion_tests,pkin_expansion_tests}.rs`). MOVS physics semantics are still parse-only.
- **Specialty blocks** — BSSubIndexTriShape, BSClothExtraData, BSConnectPoint::Parents/Children, BSBehaviorGraphExtraData, BSInvMarker, BSSkin::Instance / BSSkin::BoneData.
- **Inline per-vertex tangents** (#795 / #796, b63ab0c) — when `VF_TANGENTS | VF_NORMALS` are both set on the packed-vertex flag, FO4+ BSTriShape ships tangents inline in the packed-vertex blob, NOT in a separate `NiBinaryExtraData`. Distinct from the Skyrim-via-`NiBinaryExtraData` path of Oblivion / FO3 / FNV (#786 / 5dde345). Any FO4 audit proposing to consolidate the two paths into one is a regression of #795/#796.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 9.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/fo4`.
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
4. Confirm `Fallout 4/Data/` exists; if not, note which dimensions lose real-data validation.

## Phase 2: Launch Dimension Agents (parallel)

### Dimension 1: NIF BSVER 130 + Half-Float Vertices
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs` (BSTriShape parser; split out into `tri_shape/bs_tri_shape.rs` post-#1118), `crates/nif/src/import/mesh/bs_tri_shape.rs`, `crates/nif/src/import/collision.rs` (FO4 collision translate)
**Checklist**: VF_FULL_PRECISION flag resolution — default-half unless set. Half-float decode matches IEEE 754 binary16 (including denormals and NaN). BSSubIndexTriShape segment data walked correctly (FO4 uses this extensively for actors). Skinned-vertex bone indices + weights extraction honors packed layout. BSVER 130 trailing fields on BSLightingShaderProperty: subsurface, rimlight, backlight, fresnel, wetness (Unknown 1 for BSVER > 130). Next-Gen patch NIFs (slightly different BSVER values) still dispatch correctly. **FO4 collision (regression pin)**: `BhkMultiSphereShape` (`collision.rs:301`) + `BhkConvexListShape` (`collision.rs:397`) now translate to `CollisionShape` instead of falling through to the "unsupported" log and being dropped — FO4 power-armor / settlement / destructible debris collision depends on this; if either downcast arm reverts to a drop it's a regression target.
**Output**: `/tmp/audit/fo4/dim_1.md`

### Dimension 2: BA2 Reader — GNRL + DX10 Variants (Exhaustive Version Match)
**Subagent**: `general-purpose`
**Entry points**: `crates/bsa/src/ba2.rs`
**Checklist**: Version dispatch is now an **exhaustive `match` over `{1, 2, 3, 7, 8}`** (#811 FO4-D2-NEW-01, f480337) — unknown majors bail at `open()` time, no silent v1-fallback. Audit any reintroduction of cascading `if v == 1 { ... } else if v == 7 { ... }` chains: that pattern is the regression. Mirrors the BSA reader's allowlist discipline now at `crates/bsa/src/archive/open.rs` (the `archive.rs` file was split into the `crates/bsa/src/archive/` dir — `{open,extract,hash,mod,tests}.rs`; the in-code `archive.rs:165-173` comment in `ba2.rs` is stale and points at the pre-split file). GNRL (general files — meshes, scripts): extract a mesh and a script and verify byte-exact. DX10 (textures): DDS header reconstruction from width/height/format/mip count. DXT1/DXT5/BC5/BC7 format encoding in `dxgi_format`. Mip chunk assembly — mip0 (largest) vs mip_last (smallest) ordering. Compression flag per-archive (zlib vs uncompressed). Verify against the ROADMAP claim of 100% on 34995 NIFs + the Textures archives.
**Output**: `/tmp/audit/fo4/dim_2.md`

### Dimension 3: FO4 Shader Flags & BGSM Material Reference + Disney BSDF Path
**Subagent**: `renderer-specialist`
**Entry points**: `crates/nif/src/blocks/properties.rs` (BSLightingShaderProperty), `crates/nif/src/import/material/` (mod, walker, shader_data), `crates/renderer/shaders/triangle.frag` (Disney lobe gate sites)
**Checklist**: Shader flags — u32 pair read correctly (shader_flags_1 + shader_flags_2, two separate fields). Flag bit positions for FO4 differ from Skyrim — verify decal, alpha-test, skinned, glow, window, refraction, parallax, facegen bits are read from the correct mask + bit. BSLightingShaderProperty stopcond on BGSM Name path — when the material is external, parser returns early without reading the Phong trailing fields (those belong in the BGSM). `ImportedMesh.material_path` flows through to `Material.material_path` so diagnostics show BGSM references. `mesh.info <entity_id>` debug command surfaces material_path when texture_path is absent (session 10 behavior). **BSShaderTextureSet slot routing on `BSLightingShaderType`** (#563, d9bc363): SkinTint and HairTint sample from different slots than the default LSP path — verify the per-shader-type slot map is in lockstep with what the fragment shader reads.
**Disney BSDF / PBR path (FO4 is PBR-canonical, see audit-renderer Dim 21)**:
- **FO4 BGSM authoring sets `MAT_FLAG_PBR_BSDF`** at import — verify `crates/nif/src/import/material/walker.rs` routes BGSM through the PBR flag (NOT the legacy Phong/Lambert path). #1241 (`a82366e9`) surfaces `smoothness / IOR / specular_strength` from `BSLightingShaderProperty` into `MaterialInfo` so the Disney lobe has data to consume
- **Regression pattern (FO4-specific)**: BGSM-authored material that **falls back to Lambert** (does NOT set the PBR flag) — the opposite of the FNV/FO3 regression. Audit pattern: scan an FO4 cell, count materials with `material_kind` matching BGSM authoring, confirm the PBR flag count matches
- **BGSM smoothness → glossiness conversion** (`98383caf`): BGSM authors `smoothness` (0..1, 1=mirror); the Disney lobe consumes `roughness` (0..1, 0=mirror). Verify the conversion `roughness = 1.0 - smoothness` happens ONCE at parse and not double-applied at consume
- **#1242 `FO4_ENV_SCALE` renamed to `FO4_DLC_UPPER`** (`9883ed88`): the BSVER constant that was misnamed now reflects its actual semantic (DLC bsver upper bound). Any code that referenced `FO4_ENV_SCALE` is a regression target — verify nothing in `crates/nif/` or `crates/plugin/` references the old name
- **#1244 `BSShaderPropertyBaseOnly` consumer wired at import** (`6163b5f7`): bare-stub BSShaderProperty without subclass data is now routed through MaterialInfo. Verify FO4 fallback meshes don't render with default-everything
- The per-game→canonical material handoff (BGSM PBR flag packing, smoothness→roughness, metalness/roughness resolution) now lives at the single NIFAL boundary — covered in depth by **Dimension 7** below. **See also `/audit-nifal`** for the cross-game canonical-translation invariants (single boundary, no fabrication, no render-time fallback)
**Output**: `/tmp/audit/fo4/dim_3.md`

### Dimension 4: ESM Architecture Records (SCOL / MOVS / PKIN / TXST)
**Subagent**: `general-purpose`
**Entry points**: `crates/plugin/src/esm/records/`, `crates/plugin/src/esm/cell/` (post-Session-34/35 split: `cell/{mod,walkers,support,wrld}.rs` + `cell/tests/`)
**Checklist**: SCOL record structure — a prefab that contains placements of child statics + scale/rotation per instance. Are these parsed into a structure the cell loader can expand? MOVS — movable statics (physics-driven). PKIN — packins (grouped content bundles). TXST — texture sets referenced by NIF material paths. **TXST DODT + DNAM** (#813 / #814, 6941da6): decal-data sub-record + flags now parsed via `DecalData`. Pre-fix: 207/382 (DODT) + 382/382 (DNAM) vanilla TXSTs silently dropped their authoring. Audit any path that reads TXST without `DecalData` — that's the regression pattern. Post-Session-34/35, `cell.rs` was split into `crates/plugin/src/esm/cell/{mod,walkers,support,wrld}.rs` + `cell/tests/` (8 per-topic test siblings); the TXST match arms live in `cell/walkers.rs`. If an `unreachable_patterns` warning surfaces there (or `grep -n 'unreachable_patterns' crates/plugin/src/esm/cell/` returns anything) it suggests `b"TXST"` matches before reaching the intended arm — investigate. How many SCOL / MOVS / PKIN / TXST records exist in `Fallout4.esm`? Minimum record set needed for a hello-world FO4 cell load. **FO4-architecture map exposure**: 5 FO4-architecture maps must surface in `categories()` index (#817 FO4-D4-NEW-05, af9f4de) — missing entries hide the records from any caller iterating by category. Real-data FO4 ESM parse-rate harness lives at `#819 FO4-D4-NEW-07` (d8f859d) — re-run before reporting parse-rate findings.
**Output**: `/tmp/audit/fo4/dim_4.md`

### Dimension 5: Real-Data Validation
**Subagent**: `general-purpose`
**Entry points**: `crates/nif/examples/nif_stats.rs`, `crates/nif/tests/parse_real_nifs.rs`
**Checklist**: Parse rate holds at 100% on 34995 FO4 NIFs via `BYROREDUX_FO4_DATA=... cargo test -p byroredux-nif --test parse_real_nifs -- --ignored fo4`. Load a settlement object (workshop crafted item). Load a creature (deathclaw, super mutant). Load a power armor frame (heavy skinning + connect points). Load a weapon (receiver + barrel + stock — uses BSConnectPoint to attach modular parts). For each, trace `import_nif_scene` output: mesh count, material_path (BGSM reference or null), skinned / rigid classification, connect-point extra data presence. Spot-check BSBehaviorGraphExtraData string (references external havok behavior graph — parse-only for now).
**Output**: `/tmp/audit/fo4/dim_5.md`

### Dimension 6: Forward Blockers (post-BGSM / post-cell-load)
**Subagent**: `general-purpose`
**Entry points**: `ROADMAP.md`, `crates/plugin/src/esm/records/` (FO4 records live alongside FNV / FO3 / Skyrim — the per-game legacy/{fo4,tes4,tes5}.rs stubs were removed under `#390`)
**Checklist**: The BGSM/BGEM parser is **shipped** as `crates/bgsm/` and the SCOL→static expansion is **implemented** (covered by Dim 8 / Dim 9 respectively — do NOT re-frame either as an unimplemented blocker). Remaining forward scope: **FO4 ESM cell coverage beyond architecture records** — what's still needed for a richer cell load (REFR with more complex placement data, LIGH with power state, CONT with leveled items, NPC_ with face morph data)? **Havok Next-Gen integration** — mostly out of scope but `BSBehaviorGraphExtraData` is parse-only today; verify nothing pretends to drive it. **FO4 PreCombined CSG companion** — `cell_loader/precombined.rs` spawns the `_oc.nif` precombines, but the CSG reader that would tell the loader which original REFRs the precombine subsumes is still pending; verify the REFR-fallback gate (Dim 9) is the honest interim state. **MOVS physics** — movable-static records parse but the physics-driven runtime is not wired.
**Output**: `/tmp/audit/fo4/dim_6.md`

### Dimension 7: NIFAL Canonical Material Translation (FO4 is PBR-canonical)
**Subagent**: `renderer-specialist`
**Entry points**: `byroredux/src/material_translate.rs` (`translate_material` — the single boundary), `crates/core/src/ecs/components/material.rs` (`Material`, `Material::resolve_pbr`), `byroredux/src/cell_loader.rs` (`pack_bgsm_material_flags`). Spec: `docs/engine/nifal.md`. **See also `/audit-nifal`** for the full canonical-tier audit.
**Checklist**: `translate_material(mesh, paths, extra_material_flags)` is the **single** site turning a per-game `ImportedMesh` into the canonical `Material` — both the cell-loader REFR-spawn path and the loose-NIF (`scene`) path must route through it, not the old verbatim ~110-line struct literals. **BGSM PBR flag routing**: `effect_shader_flags` is the OR of `pack_effect_shader_flags` + `pack_bgsm_material_flags(mesh)` + caller `extra_material_flags`; verify BGSM-authored FO4 meshes get `BGSM_PBR` (and, where authored, `BGSM_TRANSLUCENCY` / `BGSM_MODEL_SPACE_NORMALS`) set in `cell_loader.rs::pack_bgsm_material_flags` (maps to the shader-side `MAT_FLAG_PBR_BSDF`). **Resolve-once contract**: `Material.metalness` / `Material.roughness` are now plain `f32` (not `Option` + per-draw classify); they are seeded from `mesh.metalness_override` / `mesh.roughness_override` (or `f32::NAN` sentinel for legacy inline-shader content), then `material.resolve_pbr()` fills any NaN from the keyword classifier and clamps (`metalness` 0..1, `roughness` 0.04..1). `resolve_pbr` is idempotent (tests `resolve_pbr_is_idempotent`, `resolve_pbr_preserves_upstream_translator_values` in `material.rs`) — audit any consumer that re-derives roughness at draw-time or re-runs a classifier; that's the regression. Glass is classified **after** `resolve_pbr` (so forced glass roughness wins) via `helpers::classify_glass_into_material`.
**Output**: `/tmp/audit/fo4/dim_7.md`

### Dimension 8: BGSM / BGEM Consumption Correctness
**Subagent**: `general-purpose`
**Entry points**: `crates/bgsm/src/` (`lib`, `base`, `bgsm`, `bgem`, `reader`, `template`), `byroredux/src/asset_provider.rs` (`merge_bgsm_into_mesh`)
**Checklist**: The BGSM/BGEM parser ships as crate `crates/bgsm/` (package `byroredux-bgsm`) — **not** a stub and **not** living under `crates/plugin/src/esm/records/`. `asset_provider.rs::merge_bgsm_into_mesh` is the actual FO4 material data source: it merges parsed BGSM fields into `ImportedMesh` (albedo/diffuse tint, specular, emissive, smoothness→glossiness/roughness, translucency suite #1147, model-space-normals bit, texture-slot paths) before `translate_material` runs. **smoothness→roughness applied exactly once at parse**: `merge_bgsm_into_mesh` writes `mesh.roughness_override = Some((1.0 - leaf.smoothness).clamp(0.04, 1.0))` (`asset_provider.rs:1079-1081`) and separately normalizes `mesh.glossiness = bgsm.smoothness * 100.0`; verify the `1.0 - smoothness` inversion is NOT re-applied downstream. **Magic-vs-extension reconciliation** (the #758 footgun guard): `merge_bgsm_into_mesh` dispatches the `.bgsm` / `.bgem` / `.mat` arm by file content, and logs when the extension disagrees with the detected magic — audit for any path that trusts the extension blindly. **Cycle-aware template resolve** (#1148): `template.rs::resolve_depth` tracks `visited` lowercase keys and breaks `A→B→A` self-references (vanilla `defaulttemplate_wet.bgsm` self-refs) instead of overflowing — verify the depth-limit / cycle-break still fires.
**Output**: `/tmp/audit/fo4/dim_8.md`

### Dimension 9: FO4 Cell Load End-to-End (SCOL/PKIN Expansion + PreCombined Mesh)
**Subagent**: `general-purpose`
**Entry points**: `byroredux/src/cell_loader/refr.rs` (`expand_scol_placements` / `expand_pkin_placements`), `byroredux/src/cell_loader/references.rs` (wires the expanders), `byroredux/src/cell_loader/precombined.rs` (`spawn_precombined_meshes`), `byroredux/src/cell_loader/exterior.rs` (Phase-3a), `crates/plugin/src/esm/cell/mod.rs` (`precombined_mesh_hashes`)
**Checklist**: **SCOL / PKIN expansion** — `expand_scol_placements` / `expand_pkin_placements` turn a prefab/packin into per-instance synthetic REFRs with composed transform (parent × child), recursion bounded by `MAX_PKIN_DEPTH = 4` (shared by both, #1180 / #1182; vanilla FO4 has zero nesting, the cap guards modded cross-recursion). `references.rs` fires the first matching expander and composes the placements. Tests: `cell_loader/{scol_expansion_tests,pkin_expansion_tests}.rs`. **PreCombined Mesh spawn** — `precombined.rs::spawn_precombined_meshes` loads the `_oc.nif` precombines referenced by `cell.precombined_mesh_hashes` (populated by the CELL walker, parsed in `cell/mod.rs`), for both interior (Phase-3a) and exterior (`exterior.rs`, #1221 / #1222 world-coord transform). **REFR-fallback gate** (#1188): when precombined geometry is NOT rendered, the XPRI-absorbed REFRs (`cell.absorbed_refs`) are honored as the only carrier of that architecture — verify the gate renders the original REFRs when `pc_spawned == 0` and suppresses double-draw when the precombine spawns. CSG companion is still pending; this gate is the honest interim.
**Output**: `/tmp/audit/fo4/dim_9.md`

## Phase 3: Merge

1. Read all `/tmp/audit/fo4/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_FO4_<TODAY>.md` with structure:
   - **Executive Summary** — Current state: NIF + BA2 at 100%, ESM architecture records landed, BGSM parser (`crates/bgsm/`) + SCOL/PKIN expansion + PreCombined-Mesh spawn all landed; PreCombined CSG companion + deeper cell coverage (LIGH/CONT/NPC_) + MOVS physics pending.
   - **Dimension Findings** — Grouped by severity per dimension.
   - **BGSM Consumption Table** — BGSM field × merged-into-ImportedMesh-by-`merge_bgsm_into_mesh` / surfaced-on-canonical-`Material` (post-NIFAL).
   - **Forward Blocker Chain** — What still must land for richer "FO4 interior renders" (PreCombined CSG reader → REFR de-dup → deeper REFR/LIGH/CONT/NPC_ cell coverage → MOVS physics → ...). Do NOT list BGSM-parser or SCOL-expansion as pending; both are shipped.
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_FO4_<TODAY>.md`
