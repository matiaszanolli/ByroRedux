---
description: "Per-game audit of Fallout 4 compatibility — BA2, half-float verts, BGSM materials, M49 precombines (CSG)"
argument-hint: "--focus <dimensions>"
---

# Fallout 4 Compatibility Audit

Deep audit of ByroRedux readiness for **Fallout 4** content.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, game data locations, key reference docs, methodology, deduplication rules, and finding format. See `.claude/commands/_audit-severity.md` for severity. This file only carries FO4-specific context — do not duplicate the common map here.

## What's Landed (audit as regression guards, not blockers)

FO4 is one of the more complete game paths. Treat each item below as **shipped** — the audit job is to confirm it still holds, not to re-propose it.

- **NIF** — BSVER 130 + Next-Gen patch range parses. Half-float vertices, BSTriShape, BSGeometry, inline tangents, BSSubIndexTriShape.
- **BA2** — exhaustive version match `{1, 2, 3, 7, 8}` (GNRL + DX10); unknown majors bail at `open()`.
- **BGSM/BGEM materials** — full external-material parser crate `crates/bgsm/`, merged into the mesh before NIFAL translate.
- **ESM architecture records** — SCOL / MOVS / PKIN / TXST parsed; SCOL/PKIN placements expand into individual statics.
- **M49 precombined geometry (CSG)** — **closed Session 45, 2026-06-02** (#1351 / #1188 Stage A). `.csg` reader, NIF precombine decode, cell-loader spawn, per-tier LOD selection, owning-REFR texture slot routing all shipped. ROADMAP language describing M49 as "blocked / no spec" is stale — the format was cracked from first principles, spec written, pipeline landed.
- **FO4 metalness from spec chromaticity** (#1476) — legacy spec-glossiness BGSMs derive metalness from spec-color **saturation**, not luminance (luminance made all vanilla concrete read chrome).

Still open (legitimate forward scope): `_precomb.nif` collision, `.uvd` occlusion volumes, MOVS physics runtime, FaceGen NIF truncation tail, deeper cell coverage (LIGH power state / CONT leveled items / NPC_ face morph), Havok behavior-graph driving.

## Game Context

| Aspect       | State |
|--------------|-------|
| NIF format   | BSVER 130 + Next-Gen patches; `FALLOUT4..FO4_DLC_UPPER` (130..=139) carries DLC trailing fields (`crates/nif/src/version.rs`) |
| BA2 format   | Exhaustive `match` over `{1, 2, 3, 7, 8}` in `crates/bsa/src/ba2.rs` (consts `BA2_V_FO4=1`, `BA2_V_FO4_NEXT_GEN_TEX=7`, `BA2_V_FO4_NEXT_GEN_MESH=8`); GNRL + DX10 |
| ESM records  | SCOL / MOVS / PKIN / TXST in `crates/plugin/src/esm/records/`; SCOL/PKIN expand in `byroredux/src/cell_loader/refr.rs` |
| Precombines  | M49 CSG pipeline landed — `crates/bsa/src/csg.rs` → `crates/nif/src/import/precombine.rs` → `byroredux/src/cell_loader/precombined.rs` |
| Parse rate   | 100.00% clean on both vanilla mesh archives per #1457 (`parse_rate_fo4_all_meshes`, 2026-06-14); the FaceGen truncation tail the 2026-06-02 ROADMAP snapshot recorded is gone. ROADMAP compat-matrix now also reads 100.00% (159 866/159 866, #1593) — still re-run the harness before citing a parse rate. |
| Rendering    | Interior cells render end-to-end (MedTekResearch01 bench, ~21k entities). BGSM/BGEM merged; precombined entities spawned interior + exterior. |
| Reference    | `/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/` |
| Bench        | `--cell MedTekResearch01` (see ROADMAP for the full `--bsa`/`--textures-ba2`/`--materials-ba2` invocation) |

### Known FO4 Specifics

- **Half-float vertices** — `VF_FULL_PRECISION` controls f32 vs u16 binary16 positions/normals; FO4 defaults to half.
- **FO4 shader flags** — `BSLightingShaderProperty` flags are a **u32 pair** (shader_flags_1 + shader_flags_2), not the single mask of FO3/FNV/Skyrim.
- **DLC trailing fields** — subsurface, rimlight, backlight, fresnel, wetness in the `FALLOUT4..FO4_DLC_UPPER` BSVER band.
- **External materials** — PBR-ish params live in `.bgsm`/`.bgem` files; the NIF carries only the path. The block parser returns a material-reference stub when BSVER ≥ 155 and Name is a BGSM/BGEM path; `crates/bgsm/` parses the file.
- **Inline per-vertex tangents** (#795 / #796) — when `VF_TANGENTS | VF_NORMALS` are both set, FO4+ BSTriShape ships tangents **inline** in the packed-vertex blob, NOT via a separate `NiBinaryExtraData` (the Skyrim/FO3/FNV path, #786). Consolidating the two into one is a regression of #795/#796.
- **Architecture records** — SCOL (prefab collections), MOVS (movable statics), PKIN (packins), TXST (texture sets w/ DODT decal-data + DNAM flags).

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 9.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/fo4`.
3. Dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
4. Confirm `Fallout 4/Data/` exists; if not, note which dimensions lose real-data validation.

## Phase 2: Launch Dimension Agents (parallel)

Dimensions are ordered by FO4 risk: the precombine pipeline, BGSM material translation, and the BA2 reader are the hot, recently-churned areas.

### Dimension 1: M49 Precombined Geometry — CSG + Decode + Spawn (highest FO4 risk)
**Subagent**: `general-purpose`
**Entry points**: `crates/bsa/src/csg.rs` (`CsgArchive::open`, `read_psg`), `crates/nif/src/import/precombine.rs` (`decode_shared_geom_object`, `psg_vertex_stride`, `PrecombineGeometry::into_imported_mesh`, `collect_precombine_geom_refs`, `PrecombineMaterial::apply`), `byroredux/src/cell_loader/precombined.rs` (`spawn_precombined_meshes`), `byroredux/src/cell_loader/load.rs` (the `pc_spawned` / `absorbed_refs` gate), `crates/plugin/src/esm/cell/wrld.rs` + `crates/plugin/src/esm/cell/mod.rs` (`precombined_mesh_hashes`). Spec: `docs/engine/fo4-csg-format.md`.
**Checklist**:
- **Vertex stride** — `psg_vertex_stride(vertex_desc)` must match the `BSPackedGeomObject` packed layout; an off-by-N stride silently shifts every vertex. Cross-check against the spec doc.
- **Y-up convert** — `into_imported_mesh` applies the Z-up→Y-up + per-instance transform. Confirm the instance transform composes (no double-apply with `cell_origin`).
- **CSG resolve** — `spawn_precombined_meshes` opens the companion `<Plugin> - Geometry.csg` once per cell; missing CSG must fall back to per-REFR (return 0 spawns) rather than panic.
- **Owning-plugin resolution (#1592→#1590, `d6bf8437`)** — the CSG path AND the `_oc.nif` precombine-object path key off the cell's **owning** plugin (form-id high byte), NOT the last-loaded `--esm` (`precombined.rs::open_geometry_csg` is fed the owning path). A DLC / multi-master cell must open `<owning-plugin> - Geometry.csg`, else every DLC-owned precombine silently falls back to per-REFR (or reads the wrong CSG blob). Tests: the `#1590` cases in `precombined.rs`.
- **REFR de-dup gate** (#1188) — `cell.absorbed_refs` are suppressed **only when `pc_spawned > 0`**; verify the `load.rs` gate renders absorbed REFRs when the precombine spawns nothing (no double-draw, no holes).
- **LOD selection** — one tier per object, not all three (the `a30c088a` fix); a regression here triples precombine geometry.
- **Texturing** — precombine shapes texture through the owning REFR's shape-slot indices (`2900de70`); a regression renders untextured/checker precombines.
- **Opaque-architecture alpha-blend guard (`887aae52`)** — on the precombine path only, `spawn_precombined_meshes` keeps the BGSM merge's two_sided/decal/alpha_test/texture flags but **restores the pre-merge (NIF-shape) alpha-blend state**. FO4 authors the "Standard" blend mode identically on transparent lab glass and opaque Institute metal architecture, so forwarding the merged `has_alpha` makes every precombined wall `MATERIAL_KIND_GLASS` → see-through, mirror-hazy walls. A regression that re-applies the merged alpha-blend on the precombine path reintroduces the transparent/reflective-wall bug.
- **Exterior path** — `cell_origin = cell_grid_to_world_yup(gx, gy)` for exterior (#1221 / #1222); without it exterior precombines stack at world origin.
**Output**: `/tmp/audit/fo4/dim_1.md`

### Dimension 2: BGSM / BGEM Consumption + Metalness-from-Chromaticity (regression pin #1476)
**Subagent**: `general-purpose`
**Entry points**: `crates/bgsm/src/` (`lib`, `base`, `bgsm`, `bgem`, `reader`, `template`), `byroredux/src/asset_provider.rs` (`merge_bgsm_into_mesh`).
**Checklist**:
- **Single data source** — `merge_bgsm_into_mesh` is the FO4 material merge: albedo/diffuse tint, specular, emissive, smoothness→roughness, translucency suite (#1147), model-space-normals bit, texture-slot paths. It runs **before** `translate_material`.
- **Metalness from saturation, NOT luminance** (#1476, `08ed03be`) — for legacy spec-glossiness BGSMs (`leaf.pbr == false`, ~all vanilla architecture), metalness = `(max-min)/max` of `specular_color` (mult-invariant saturation): white spec `[1,1,1]` → 0 (concrete is dielectric), tinted spec → metallic. **Any reversion to luminance (`0.2126·r + …`) for the non-pbr branch is the #1476 regression — it makes vanilla concrete read chrome.** The luminance path is correct *only* for `leaf.pbr == true`.
- **smoothness→roughness applied exactly once at parse** — `mesh.roughness_override = Some((1.0 - leaf.smoothness).clamp(0.04, 1.0))`. Audit any downstream re-application of the `1.0 - smoothness` inversion.
- **Magic-vs-extension reconciliation** (#758 footgun) — the `.bgsm`/`.bgem`/`.mat` arm dispatches on file content; verify it logs and does not blindly trust the extension.
- **Cycle-aware template resolve** (#1148) — `template.rs::resolve` / `resolve_depth` track `visited` lowercase keys and break `A→B→A` self-refs (vanilla `defaulttemplate_wet.bgsm`); verify the cycle-break + `DEPTH_LIMIT` still fire.
- **Blend-factor enum translation (#1651, `ada75ee3`)** — `merge_bgsm_into_mesh` must translate the BGSM/BGEM `alpha_blend_mode` src/dst from the **GL-style** enum (Zero=0, One=1, …) into the renderer's **Gamebryo** `NiAlphaProperty` nibble (ONE=0, ZERO=1, then 2..=10 shared) before writing `mesh.src_blend_mode`/`dst_blend_mode` — the two tables are inverted exactly at 0 and 1. Standard mode (6,7) coincides and masks the bug; additive effect/glow BGEM cards author (One,One)=(1,1) which, forwarded raw, the renderer reads as (ZERO,ZERO) → invisible instead of additive. Any verbatim `as u8` forward is the regression.
**Output**: `/tmp/audit/fo4/dim_2.md`

### Dimension 3: BA2 Reader — GNRL + DX10 (Exhaustive Version Match)
**Subagent**: `general-purpose`
**Entry points**: `crates/bsa/src/ba2.rs`.
**Checklist**:
- **Exhaustive dispatch** (#811, `f480337`) — version handling is a `match` over the supported set; unknown majors (0, 4, 5, 6, 9, …) bail at `open()`. Any reintroduction of a cascading `if v == 1 { … } else if v == 7 { … }` chain with a silent v1 fallback is the regression. Mirrors the BSA allowlist now at `crates/bsa/src/archive/open.rs` (the old `archive.rs:165-173` comment in `ba2.rs` referencing the pre-split file is stale).
- **GNRL** — extract a mesh and a script, byte-exact.
- **DX10** — DDS header reconstruction from width/height/`dxgi_format`/mip count; DXT1/DXT5/BC5/BC7 encoding; mip chunk assembly (mip0 largest → mip_last); per-archive zlib-vs-uncompressed flag.
- **Cross-version** — `BA2_V_FO4` (1) / `BA2_V_FO4_NEXT_GEN_TEX` (7) / `BA2_V_FO4_NEXT_GEN_MESH` (8) all force zlib; confirm the Next-Gen mesh/tex archives extract.
**Output**: `/tmp/audit/fo4/dim_3.md`

### Dimension 4: NIF BSVER 130 + Half-Float Vertices + FO4 Collision
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs`, `crates/nif/src/import/mesh/bs_tri_shape.rs`, `crates/nif/src/import/mesh/bs_geometry.rs`, `crates/nif/src/import/collision.rs`.
**Checklist**:
- VF_FULL_PRECISION resolution (default-half unless set); half-float decode matches IEEE 754 binary16 incl. denormals/NaN.
- BSSubIndexTriShape segment data walked (FO4 actors lean on it); skinned bone indices/weights honor packed layout.
- DLC trailing fields on `BSLightingShaderProperty` in the `FALLOUT4..FO4_DLC_UPPER` band (subsurface/rimlight/backlight/fresnel/wetness). Next-Gen patch BSVER values still dispatch.
- **#1242 constant rename** — the BSVER bound is `FO4_DLC_UPPER` (= 140), not the old misnamed `FO4_ENV_SCALE`. Any reference to the old name anywhere under `crates/nif/` or `crates/plugin/` is a regression target.
- **FO4 collision (regression pin)** — `BhkMultiSphereShape` and `BhkConvexListShape` translate to `CollisionShape` in `collision.rs` (search the two `downcast_ref` arms) instead of falling through to the "unsupported" log + drop. Power-armor / settlement / destructible-debris collision depends on this; either arm reverting to a drop is a regression (NIFAL "parsed then dropped" leak class).
**Output**: `/tmp/audit/fo4/dim_4.md`

### Dimension 5: FO4 Shader Flags & BGSM PBR Routing (Disney path)
**Subagent**: `renderer-specialist`
**Entry points**: `crates/nif/src/blocks/shader.rs` (BSLightingShaderProperty), `crates/nif/src/import/material/` (`mod`, `walker`, `shader_data`), `crates/renderer/shaders/include/lighting.glsl` + `include/pbr.glsl` (Disney lobe gates).
**Checklist**:
- **u32 flag pair** read as two separate fields (shader_flags_1 + shader_flags_2). FO4 flag bit positions differ from Skyrim — verify decal / alpha-test / skinned / glow / window / refraction / parallax / facegen bits read from the correct mask + bit.
- **Render-affecting bits consumed, not just parsed (#1592, `f7fbbed5`)** — the walker (`crates/nif/src/import/material/walker.rs`) must OR Model_Space_Normals (F4SF1 bit 12), Alpha_Test (F4SF2 bit 25), and Glow_Map (F4SF2 bit 6) into `MaterialInfo`; vanilla FO4 is BGSM-backed so this only bites inline/loose/modded NIFs, but a regression decodes an object-space normal map as tangent-space or renders an inline cutout opaque. Tests: `crates/nif/src/import/material/fo4_shader_flag_tests.rs`, `alpha_flag_tests.rs`.
- **BGSM Name stopcond** — when the material is external, the block parser returns before reading the inline Phong trailing fields (those belong to the BGSM). `ImportedMesh.material_path` flows to `Material.material_path` for diagnostics; `mesh.info <id>` surfaces it when texture_path is absent.
- **BSShaderTextureSet slot routing** (#563) — SkinTint / HairTint `BSLightingShaderType` variants sample different slots than the default LSP path; verify the per-type slot map is in lockstep with what `triangle.frag` reads.
- **PBR flag, not Lambert** — `crates/nif/src/import/material/walker.rs` routes BGSM-authored material through the PBR path; the FO4-specific regression is a BGSM material **falling back to Lambert** (opposite of the FNV/FO3 regression). #1241 surfaces smoothness/IOR/specular_strength into `MaterialInfo` so the Disney lobe has data.
- **#1244 `BSShaderPropertyBaseOnly` consumer** wired at import — bare-stub BSShaderProperty routes through MaterialInfo; verify FO4 fallback meshes aren't default-everything.
- The per-game→canonical handoff is covered in depth by **Dimension 7**; cross-game canonical invariants live in `/audit-nifal`.
**Output**: `/tmp/audit/fo4/dim_5.md`

### Dimension 6: ESM Architecture Records (SCOL / MOVS / PKIN / TXST)
**Subagent**: `general-purpose`
**Entry points**: `crates/plugin/src/esm/records/` (`scol.rs`, `movs.rs`, `pkin.rs`, `mswp.rs`; TXST lives in the misc set), `crates/plugin/src/esm/cell/` (`mod`, `walkers`, `support`, `wrld` + `cell/tests/`).
**Checklist**:
- SCOL — prefab of child-static placements w/ per-instance scale/rotation, parsed into an expandable structure. PKIN — packins (grouped bundles). MOVS — movable statics (parse-only; physics runtime not wired). TXST — texture sets referenced by NIF material paths.
- **TXST DODT + DNAM** (#813 / #814) — decal-data sub-record + flags parsed via `DecalData`; pre-fix, 207/382 (DODT) + 382/382 (DNAM) vanilla TXSTs dropped authoring. Any TXST read path without `DecalData` is the regression. The TXST match arms live in `cell/walkers.rs`; an `unreachable_patterns` warning there suggests `b"TXST"` matches before the intended arm.
- **Category index exposure** (#817) — the FO4-architecture maps must surface in `categories()`; a missing entry hides records from category iteration.
- Re-run the real-data ESM parse harness (#819) before reporting any parse-rate finding.
**Output**: `/tmp/audit/fo4/dim_6.md`

### Dimension 7: NIFAL Canonical Material Translation (FO4 is PBR-canonical)
**Subagent**: `renderer-specialist`
**Entry points**: `byroredux/src/material_translate.rs` (`translate_material` — the single boundary), `crates/core/src/ecs/components/material.rs` (`Material`, `Material::resolve_pbr`), `byroredux/src/cell_loader.rs` (`pack_bgsm_material_flags`, `pack_effect_shader_flags`). Spec: `docs/engine/nifal.md`. **See also `/audit-nifal`.**
**Checklist**:
- **Single boundary** — `translate_material(mesh, paths, extra_material_flags)` is the only `ImportedMesh → Material` site; both the cell-loader REFR-spawn path and the loose-NIF `scene` path must route through it, not verbatim struct literals.
- **BGSM PBR flag routing** — `effect_shader_flags` ORs `pack_effect_shader_flags` + `pack_bgsm_material_flags(mesh)` + caller `extra_material_flags`. Verify BGSM meshes get `BGSM_PBR` (and where authored `BGSM_TRANSLUCENCY` / `BGSM_MODEL_SPACE_NORMALS`) → shader-side `MAT_FLAG_PBR_BSDF`. Tests: `pack_bgsm_material_flags_tests` in `cell_loader.rs`.
- **Resolve-once contract** — `Material.metalness` / `Material.roughness` are plain `f32` (not `Option` + per-draw classify), seeded from `mesh.metalness_override` / `mesh.roughness_override` (or `f32::NAN` for legacy inline-shader content), then `resolve_pbr()` fills NaN from the classifier and clamps (metalness 0..1, roughness 0.04..1). `resolve_pbr` is idempotent (tests `resolve_pbr_is_idempotent`, `resolve_pbr_preserves_upstream_translator_values`, `resolve_pbr_fills_only_missing_slot` in `material.rs`). Any consumer re-deriving roughness or re-running a classifier at draw-time is the regression.
- Glass is classified **after** `resolve_pbr` (forced glass roughness wins) via `helpers::classify_glass_into_material`.
**Output**: `/tmp/audit/fo4/dim_7.md`

### Dimension 8: FO4 Cell Load End-to-End (SCOL/PKIN Expansion)
**Subagent**: `general-purpose`
**Entry points**: `byroredux/src/cell_loader/refr.rs` (`expand_scol_placements`, `expand_pkin_placements`), `byroredux/src/cell_loader/references.rs` (wires the expanders), `byroredux/src/cell_loader/exterior.rs` (Phase-3a).
**Checklist**:
- `expand_scol_placements` / `expand_pkin_placements` turn a prefab/packin into per-instance synthetic REFRs with composed transform (parent × child), recursion bounded by `MAX_PKIN_DEPTH = 4` (shared, #1180 / #1182; vanilla has zero nesting, the cap guards modded cross-recursion). `references.rs` fires the first matching expander and composes placements.
- Tests: `cell_loader/scol_expansion_tests.rs`, `cell_loader/pkin_expansion_tests.rs`.
- Precombine spawn (Dim 1) and SCOL/PKIN expansion are independent carriers of the same cell — confirm no double-draw between an expanded SCOL static and a precombine that subsumed it (the `absorbed_refs` gate in Dim 1 is the de-dup mechanism).
- **BSConnectPoint attach-graph consumer (#1594, `c16600a5`)** — the import lifts `BSConnectPoint::Parents`/`::Children` into `AttachPoints` / `ChildAttachConnections` ECS components (`crates/core/src/ecs/components/attach_points.rs`); the cell-load spawn path (`cell_loader/spawn.rs` + `references.rs`, cached in `nif_import_registry.rs`) must materialize them onto spawned entities. Pre-fix the chain dead-ended at the import boundary and modular weapons / power-armor frames spawned as base-receiver only. Regression target: a spawn path that imports the connect graph but attaches no `AttachPoints` component. Tests: `cell_loader/attach_points_spawn_tests.rs`.
**Output**: `/tmp/audit/fo4/dim_8.md`

### Dimension 9: Real-Data Validation + Forward Scope
**Subagent**: `general-purpose`
**Entry points**: `crates/nif/tests/parse_real_nifs.rs` (`parse_rate_fallout_4`, `parse_rate_fo4_all_meshes`), `crates/nif/examples/nif_stats.rs`, `ROADMAP.md`, `docs/feature-matrix.md`.
**Checklist**:
- Re-run `BYROREDUX_FO4_DATA=… cargo test -p byroredux-nif --test parse_real_nifs -- --ignored fo4` (`parse_rate_fo4_all_meshes` covers both `Fallout4 - Meshes.ba2` + `Fallout4 - MeshesExtra.ba2`) — both archives parse 100.00% clean per #1457. Report the measured number; do not cite the stale ROADMAP figure without re-running.
- Trace `import_nif_scene` on: a settlement workshop item, a creature (deathclaw / super mutant), a power-armor frame (heavy skinning + BSConnectPoint), a modular weapon (receiver/barrel/stock via BSConnectPoint). For each: mesh count, material_path (BGSM ref or null), skinned vs rigid, connect-point extra-data presence.
- **Forward scope** (do NOT re-file as blockers): `_precomb.nif` collision, `.uvd` occlusion volumes, MOVS physics runtime, FaceGen NIF truncation tail, deeper cell coverage (LIGH power state / CONT leveled items / NPC_ face morph), `BSBehaviorGraphExtraData` parse-only (verify nothing pretends to drive it). The BGSM parser, SCOL/PKIN expansion, and the M49 CSG pipeline are **all shipped** — never list them as pending.
**Output**: `/tmp/audit/fo4/dim_9.md`

## Phase 3: Merge

1. Read all `/tmp/audit/fo4/dim_*.md`.
2. Combine into `docs/audits/AUDIT_FO4_<TODAY>.md`:
   - **Executive Summary** — NIF + BA2 + BGSM parser + SCOL/PKIN expansion + **M49 CSG precombines** all landed; parse rate per the latest `parse_rate_fo4_all_meshes` run. Pending: precombine collision / `.uvd` volumes, MOVS physics, deeper cell coverage (LIGH/CONT/NPC_).
   - **Dimension Findings** — grouped by severity per dimension.
   - **BGSM Consumption Table** — BGSM field × merged-into-`ImportedMesh`-by-`merge_bgsm_into_mesh` / surfaced-on-canonical-`Material` (post-NIFAL).
   - **Forward Scope Chain** — precombine collision / `.uvd` → MOVS physics → deeper REFR/LIGH/CONT/NPC_ coverage. Do NOT list the CSG reader, BGSM parser, or SCOL expansion as pending.
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_FO4_<TODAY>.md`
