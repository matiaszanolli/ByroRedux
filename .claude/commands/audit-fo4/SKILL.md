---
description: "Per-game audit of Fallout 4 compatibility — BA2, half-float verts, BGSM materials, M49 precombines (CSG)"
argument-hint: "--focus <dimensions>"
---

# Fallout 4 Compatibility Audit

Deep audit of ByroRedux readiness for **Fallout 4** content.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

Read `.claude/commands/_audit-common.md` (layout, game data, dedup, finding format) and `.claude/commands/_audit-severity.md`. This file carries only FO4-specific context.

**Scope**: this skill owns *FO4's data through the shared mechanisms*. Defects in the mechanism itself go to the owner: BA2/CSG/BGSM/`.uvd` reader discipline → `/audit-parsers`; NIF block parsing → `/audit-nif`; ESM walker → `/audit-esm`; canonical-material invariants → `/audit-nifal`; Scaleform HUD (`--hud`, `byroredux/src/scaleform_hud.rs`, `docs/smoke-tests/m48-7-fo4-hud.sh`, AVM2/`BGSCodeObj`) → `/audit-ui`; runtime telemetry (`.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv`) → `/audit-runtime` — run it after any precombine / SCOL / spawn change.

Everything below is **shipped** — audit as regression guard, never re-propose as pending: NIF BSVER 130+ incl. Next-Gen, BA2 v1/v7/v8, the BGSM/BGEM parser, SCOL/PKIN expansion, M49 CSG precombines, BSConnectPoint attach graph.

## Game Context

| Aspect | State |
|---|---|
| NIF | BSVER 130 + Next-Gen patches (`FALLOUT4..FO4_DLC_UPPER` = 130..=139 carries the DLC trailing fields, `crates/nif/src/version.rs`) |
| Shader flags | typed u32 pair at BSVER 130 **only**; BSVER 131 (`FO4_SHADER_GAP`) has neither; BSVER ≥ 132 uses CRC arrays. Guard: `bsver_shader_flag_band_tests` (`version.rs`), gates `carries_typed_shader_flags` / `carries_crc_shader_flags` |
| Materials | PBR params live in external `.bgsm`/`.bgem`; the NIF carries a path stub. `crates/bgsm/` parses; `merge_external_material` folds into `ImportedMesh.material` before `translate_material` |
| Precombines | `crates/bsa/src/csg.rs` → `crates/nif/src/import/precombine.rs` → `byroredux/src/cell_loader/precombined.rs`. Spec `docs/engine/fo4-csg-format.md` |
| Data | `/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/`; bench `--cell MedTekResearch01` (ROADMAP) |

## Parameters / Setup

`--focus <dimensions>` (comma-separated, default all 5). Setup: parse `$ARGUMENTS`; `mkdir -p /tmp/audit/fo4`; `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`; confirm `Fallout 4/Data/` exists (else note dimensions losing real-data validation).

## Dimensions

### Dimension 1: M49 Precombined Geometry — Decode + Spawn (highest FO4 risk)
**Subagent**: `general-purpose`
**Paths**: `crates/nif/src/import/precombine.rs`, `byroredux/src/cell_loader/precombined.rs`, `byroredux/src/cell_loader/load.rs` + `exterior.rs`, `byroredux/src/cell_loader/spawn/mesh_instance.rs`, `crates/bsa/src/csg.rs`, `crates/bsa/src/uvd.rs`
**First step**: `git log --since=<last report> --format='%h %cs %s' -- crates/nif/src/import/precombine.rs byroredux/src/cell_loader/precombined.rs crates/bsa/src/csg.rs`
**Guards** (confirm live, not `#[ignore]`d/vacuous): `precombine.rs` tests `decode_keeps_every_lod_band` + `lod_bands_skip_empty_bands_and_bound_the_read` (#4234); `csg.rs` `oversized_read_len_is_rejected_before_it_is_reserved` (#3758 — `read_psg` must bound its length before `Vec::with_capacity`; reader discipline is `/audit-parsers`); `precombined.rs` tests `resolve_precombine_owner_follows_form_id_mod_index`, `csg_paths_index_every_plugin_in_the_load_order`, `dedup_requires_both_a_cache_key_and_a_matching_length` (#3510); `#[ignore]`d real-data `dlc_rebake_of_a_master_owned_cell_decodes_from_the_dlc_csg`. Unguarded: the alpha-blend restore below.
**Checklist** (what the guards cannot see):
- **Stride / transform** — `psg_vertex_stride(vertex_desc)` vs the `BSPackedGeomObject` layout (off-by-N shifts every vertex silently; cross-check the spec doc). `into_imported_mesh` applies Z-up→Y-up + instance transform once — no double-apply with `cell_origin`; exterior `cell_origin = cell_grid_to_world_yup(gx, gy)` (else precombines stack at origin).
- **LOD bands (#4234)** — the three bands are disjoint sub-meshes whose union is the model (12,096/12,096 `BSMeshLODTriShape`); every populated band is decoded. Picking one band deletes geometry (12.45% of `MeshesExtra.ba2`'s baked triangles) and is not z-fighting. Distance LOD may drop tail bands, never pick one. An empty band's offset is untrusted.
- **Which CSG blob** — the `_oc.nif` *filename* keys off the cell's owning plugin (form-id mod byte; DLC bakes under `<plugin>.esm\`), but the *geometry blob* is chosen by each object's `BSPackedGeomObject::filename_hash` → `csg_paths_by_name_hash` (#2369: DLCs re-bake ~460 master-owned cells into their own blob). A blob no loaded plugin answers to, or a missing CSG, must fail closed to per-REFR (0 spawns), never read another plugin's PSG space or panic.
- **REFR de-dup gate (#1188)** — `cell.absorbed_refs` suppress per-REFR rendering only when `pc_spawned > 0` (`absorbed_refs_or_empty`, shared by interior `load.rs` and exterior `exterior.rs`); absorbed REFRs must render when nothing spawned (no holes, no double-draw with SCOL expansion).
- **Texturing** — each mesh takes its material from the *owning shape's* shader/alpha properties (`collect_precombine_geom_refs` + `precombine_material_from_shape`), then the BGSM merge; no REFR overlay on this path (`pre_merge_materials` empty by design, #4290). Regression = untextured/checker precombines.
- **Opaque-architecture blend guard** — on this path only, `spawn_precombined_meshes` keeps the BGSM merge's two_sided/decal/alpha_test/texture flags but restores the pre-merge alpha-blend triple (`has_alpha`, `src/dst_blend_mode`): FO4 authors the "Standard" blend identically on lab glass and opaque Institute metal, so forwarding the merged `has_alpha` makes walls `MATERIAL_KIND_GLASS` (see-through, mirror-hazy).
- **Archive chain precedence (#3637)** — `--bsa` / `--materials-ba2` chains resolve last-listed-wins (a DLC's re-baked `_oc.nif` must not be shadowed; 1,871 DLC mesh entries were shadowed pre-fix per the #3637 title). Guard: `byroredux/src/asset_provider/tests/archive_precedence.rs` (real-data, `#[ignore]`d — run it).
- **Upload once per object (#3510)** — `CachedNifImport::geometry_dedup` maps every instance to its object's representative; keying the cache on the flattened sub-mesh index reintroduces one upload + BLAS per instance (1.37x VRAM, 4.6x worst tile).
- **Exterior streaming** — `PrecombinedSpawnJob` yields between hashes / meshes / BLAS batches; the synchronous `spawn_precombined_meshes` wrapper must stay behaviourally identical.
- **`.uvd` previs** — envelope only (`parse_uvd_header`, incl. `exterior_cell_grid`; PVS payload uncracked, no occlusion consumer, #3810). Verify nothing pretends to consume the payload.
**Output**: `/tmp/audit/fo4/dim_1.md`

### Dimension 2: BGSM / BGEM → `ImportedMaterial` (merge + roles)
**Subagent**: `general-purpose`
**Paths**: `crates/bgsm/src/`, `byroredux/src/asset_provider/material/{merge,mod,provider}.rs`, `byroredux/src/cell_loader/spawn/mesh_instance.rs` (REFR overlay), `byroredux/src/asset_provider/tests/{bgsm_merge,material_flags}.rs`
**First step**: `git log --since=<last report> --format='%h %cs %s' -- crates/bgsm byroredux/src/asset_provider/material`
**Guards**: `bgsm_merge.rs` (`bgsm_merge_forwards_alpha_blend_mode`, `bgsm_blend_to_gamebryo_is_identity_narrowing`, the neutral-roughness pair, `bgem_glass_forwards_authored_optics_and_overlay_roles`); `material_flags.rs::bgsm_metalness_*`; `crates/bgsm/src/template.rs` `resolve_breaks_*_cycle` (#1148). Parser discipline for `crates/bgsm` is `/audit-parsers`.
**Checklist**:
- **Narrowed signature** — `merge_external_material` takes `&mut ImportedMaterial`, never `&mut ImportedMesh`, and is the only exported fn in `merge.rs` (test `merge_external_material_is_the_only_exported_fn_in_this_file`); a widened signature is a NIFAL boundary violation. It runs **before** `translate_material`.
- **Roles, not slots** — BGSM texture paths and REFR/TXST overlays land in `MaterialTextureSet` roles (`crates/nif/src/import/types.rs`); FO4 wire-slot numbers must not survive past the import boundary. Overlay output is carried as named roles, not wire slots (#4434). Texture-set slot 7 is the `_s.dds` smooth-spec map and routes to the smooth-spec role, not Specular (#4424: the BC5 map fed to the specular-colour multiply tinted every FO4 highlight warm); the gloss-sampler channel semantics are still an open question — do not guess.
- **Metalness from saturation, not luminance (#1476)** — legacy spec-glossiness BGSMs (`pbr == false`): `bgsm_metalness` = `(max-min)/max` of `specular_color` (white spec → 0). Luminance for the non-PBR branch makes vanilla concrete read chrome; luminance is correct only for `pbr == true`.
- **smoothness→roughness exactly once** at the merge (`(1 - smoothness)` clamped to the near-mirror floor); flag any downstream re-inversion.
- **Neutral-roughness fallback** — `smoothness == 1.0` with no resolvable gloss map → neutral 0.5 (`NEAR_MIRROR_NEUTRAL_ROUGHNESS`): #3639 at the merge (authored-nowhere; must not fire when a template parent supplies `smooth_spec`), #3905 at spawn via `material_translate::resolve_unresolved_gloss_neutral_roughness` (authored-but-unresolvable; gated on the shader's `glossMapIndex != 0u`). The arms cover disjoint populations — collapsing them regresses.
- **Blend factors pass through** (#1823) — BGSM/BGEM `src_blend`/`dst_blend` are already Gamebryo-native (Standard (6,7), Additive (6,0), Multiplicative (4,1)); any 0↔1 swap (or a `gl_to_gamebryo_blend`) corrupts Additive/Multiplicative while leaving Standard untouched.
- **BGEM merge** — `merge_bgem_arm` deliberately leaves metalness/roughness as NaN so `resolve_pbr`'s classifier runs; effect data merges into the NIF payload (#4425). Glass is `glass_enabled`-driven and must not misclassify opaque architecture with a stuck flag.
- **Lookup** — `.bgsm`/`.bgem`/`.mat` dispatch on content magic, not extension (#758); last-listed-wins across `--materials-ba2` (#3637); template resolve is cycle-aware and depth-limited (`DEPTH_LIMIT = 16`).
**Output**: `/tmp/audit/fo4/dim_2.md`

### Dimension 3: NIF BSVER 130 Geometry, Shader Flags, Collision (FO4 slice)
**Subagent**: `legacy-specialist`
**Paths**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs`, `crates/nif/src/import/mesh/{bs_tri_shape,bs_geometry}.rs`, `crates/nif/src/blocks/shader/lighting.rs`, `crates/nif/src/import/material/dedicated_shader.rs`, `crates/nif/src/import/collision/shape.rs`
**First step**: `git log --since=<last report> --format='%h %cs %s' -- crates/nif/src/import/material crates/nif/src/blocks/shader`
**Guards**: `fo4_shader_flag_tests.rs`, `alpha_flag_tests.rs` (`crates/nif/src/import/material/`); `bsver_shader_flag_band_tests`; `parse_rate_fo4_all_meshes` (Dim 5). Parser mechanics are `/audit-nif`; the canonical handoff (single `translate_material` boundary, resolve-once metalness/roughness, glass classified after `resolve_pbr`) is `/audit-nifal` — file there.
**Checklist**:
- `VF_FULL_PRECISION` default-half unless set; binary16 decode incl. denormals/NaN. `BSSubIndexTriShape` segments walked (actors lean on it); skinned indices/weights honour the packed layout. Inline tangents when `VF_TANGENTS | VF_NORMALS` (shared with Skyrim SE) — consolidating with the `NiBinaryExtraData` path is a regression of #795/#796.
- **Render-affecting flags consumed, not just parsed (#1592)** — `dedicated_shader.rs` ORs `Model_Space_Normals` (F4SF1 bit 12) and `Alpha_Test` (F4SF2 bit 25) into `MaterialInfo`, gated by `carries_typed_shader_flags` (BSVER 130 only). `Glow_Map` (F4SF2 bit 6) is deliberately not gated: glow sources from the `BSShaderTextureSet` slot regardless. These are lower priority than the BGSM merge (vanilla is BGSM-backed; only inline/loose/modded NIFs exercise them).
- Skin/Hair tint `BSLightingShaderType` variants sample different slots than the default path — the per-type map must match what `triangle.frag` reads (#563). A BGSM material must never fall back to Lambert (FO4's inverse of the FNV regression); bare `BSShaderPropertyBaseOnly` stubs route through `MaterialInfo` (#1244).
- **Parsed-then-dropped is FO4's recurring finding class** (`texture_clamp_mode` on 7.9% of lit materials #3507; the greyscale→palette enable bit): every decoded FO4 shader/BGSM field must reach an `ImportedMaterial` sink and the canonical `Material`, with one default across the three material tiers (#3515).
- **Collision** — `BhkMultiSphereShape` and `BhkConvexListShape` translate to `CollisionShape` in `import/collision/shape.rs` (two `downcast_ref` arms); an arm reverting to "unsupported + drop" removes power-armor / settlement / debris collision. Packed-Havok (`BhkSystemBinary`) collision is blocked on the blob decode (PHYSAL, `/audit-physics`); FO4 architecture uses synthesized ghost trimesh colliders. The precombine collision sibling is `<cell_formid:08x>_physics.nif` (not `_precomb.nif`, which appears in no archive) — forward scope.
**Output**: `/tmp/audit/fo4/dim_3.md`

### Dimension 4: ESM Architecture Records + Cell Expansion
**Scope split with `/audit-esm`**: it owns the parser as a parser (GRUP walk, `SubReader` accounting, FormID remap); this dimension owns FO4's data through it. Shared-mechanism defects → `/audit-esm`.
**Subagent**: `general-purpose`
**Paths**: `crates/plugin/src/esm/records/{scol,movs,pkin,mswp,parse}.rs`, `crates/plugin/src/esm/cell/{mod,support,walkers,wrld}.rs`, `byroredux/src/cell_loader/{refr,references/,spawn.rs,nif_import_registry.rs}`
**First step**: `git log --since=<last report> --format='%h %cs %s' -- crates/plugin/src/esm/records crates/plugin/src/esm/cell byroredux/src/cell_loader/refr.rs`
**Guards**: `cell_loader/scol_expansion_tests.rs`, `pkin_expansion_tests.rs`, `attach_points_spawn_tests.rs`, `refr_texture_overlay_tests.rs`; the real-data ESM harness (#819) before any parse-rate claim.
**Checklist**:
- SCOL/PKIN expand into per-instance synthetic REFRs with composed (parent × child) transform; recursion bounded by `MAX_PKIN_DEPTH = 4` (vanilla has zero nesting; the cap guards modded cross-recursion); SCOL-of-SCOL has its own gate. MOVS is parse-only (physics runtime unwired). The `TXST` group arm is in `records/parse.rs` and its decode in `esm/cell/support.rs` — an `unreachable_patterns` warning there means `b"TXST"` matches earlier than intended. The FO4-architecture maps must surface in `categories()` (`records/index.rs`).
- **TXST DODT (`DecalData`) has zero consumers** — 303 vanilla payloads parse onto `TextureSet.decal_data` and are never read (`grep decal_data` finds only the parser); DNAM *is* consumed (`TXST_FLAG_MODEL_SPACE_NORMALS` in `refr.rs`). A real, measured asymmetry with no tracker as of 2026-09-19 — file it as a coverage gap, not a parse bug.
- **BSConnectPoint** — the import lifts `BSConnectPoint::Parents/Children` into `AttachPoints` / `ChildAttachConnections` (`crates/core/src/ecs/components/attach_points.rs`); spawn must materialise them (modular weapons / power-armor frames otherwise spawn base-receiver only).
**Output**: `/tmp/audit/fo4/dim_4.md`

### Dimension 5: Archives + Real-Data Validation + Forward Scope
**Subagent**: `general-purpose`
**Paths**: `crates/bsa/src/ba2.rs`, `crates/nif/tests/parse_real_nifs.rs`, `crates/nif/examples/nif_stats.rs`, `ROADMAP.md`, `docs/feature-matrix.md`
**First step**: `BYROREDUX_FO4_DATA=… cargo test -p byroredux-nif --test parse_real_nifs -- --ignored fo4`
**Checklist**:
- **BA2** (FO4 slice; reader discipline is `/audit-parsers`) — version dispatch is an exhaustive `match` (v1/v7/v8 FO4 → zlib; v2/v3 Starfield → `/audit-starfield` Dim 1); unknown majors bail at `open()` and a cascading `if v == 1 … else` with a silent v1 fallback is the regression (#811). GNRL byte-exact; DX10 reconstructs the DDS header from width/height/`dxgi_format`/mips (BC1/BC3/BC5/BC7, mip0 → mip_last).
- **Parse rate** — `parse_rate_fo4_all_meshes` covers all 8 mesh-bearing archives (base pair + six DLC `Main.ba2`; `DLCUltraHighResolution` is textures-only), 235,082 NIFs incl. `.bto`/`.btr` at the last measurement (2026-08-29, #3466), 100.00% clean. Report the measured number, not ROADMAP's.
- Trace `import_nif_scene` on a workshop item, a creature (deathclaw / super mutant), a power-armor frame (heavy skinning + BSConnectPoint) and a modular weapon: mesh count, `material_path`, skinned vs rigid, connect points.
- **Forward scope** (do not file as blockers): precombine `_physics.nif` collision and packed-Havok decode, `.uvd` PVS payload / occlusion consumer, MOVS physics runtime, DecalData consumer (above), `BSBehaviorGraphExtraData` parse-only (verify nothing pretends to drive it), FaceGen NIF truncation tail, deeper cell coverage (LIGH power state / CONT leveled items / NPC_ face morph).
**Output**: `/tmp/audit/fo4/dim_5.md`

## Phase 3: Merge

1. Read `/tmp/audit/fo4/dim_*.md`; combine into `docs/audits/AUDIT_FO4_<TODAY>.md`:
   - **Executive Summary** — measured parse rate; what is shipped vs pending (above).
   - **Dimension Findings** — grouped by severity per dimension.
   - **BGSM Consumption Table** — BGSM field × merged-into-`ImportedMaterial` by `merge_external_material` / surfaced-on-canonical-`Material`. Texture columns name the `MaterialTextureSet` role, never an FO4 slot number.
   - **Forward Scope Chain** — precombine collision / `.uvd` payload → MOVS physics → DecalData → deeper REFR/LIGH/CONT/NPC_ coverage.
2. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_FO4_<TODAY>.md`
(label every finding `game:fo4` + `legacy-compat`, plus its own domain label.)
