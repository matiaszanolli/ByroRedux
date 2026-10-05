---
description: "Per-game audit of Starfield compatibility — BA2 v2/v3 + LZ4 block, CDB materials, BSGeometry .mesh resolution, walkable Cydonia interior"
argument-hint: "--focus <dimensions>"
---

# Starfield Compatibility Audit

Regression-and-depth audit of ByroRedux's **Starfield** support — a first-class `GameKind` with NIF + BA2 v2/v3, CDB materials and a walkable Cydonia interior — not a gap inventory.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

Read `.claude/commands/_audit-common.md` (layout, game data, dedup, finding format, SF Material / SF Smoke entries) and `.claude/commands/_audit-severity.md` (NIFAL rows gate `translate_material` at HIGH minimum).

**Scope**: *Starfield's data through the shared mechanisms*; mechanism defects go to the owner: BA2 / CDB reader discipline → `/audit-parsers`; NIF block parsing → `/audit-nif`; ESM walker → `/audit-esm`; canonical-material invariants → `/audit-nifal`; Starfield HUD (M48.8, #4470) → `/audit-ui`; NPC stat model (#4453) → `/audit-character`.

Status authority: `ROADMAP.md` compat row (100.00% over 120,543 NIFs as of 2026-09-24, #4440 — still measure, never transcribe), `docs/feature-matrix.md`, `docs/engine/starfield-esm-roadmap.md`, `docs/engine/starfield-esm-phase0-baseline.md`, `docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md`.

## Game Context

| Aspect | State |
|---|---|
| NIF | BSVER 155 (FO76 baseline) → Starfield retail extensions; mesh path is `BSGeometry` (inline data **or** external `geometries\<X>.mesh`), never `BSTriShape` |
| BA2 | v2 (zlib, 8-byte header extension) + v3 (12-byte extension with `compression_method`: 0 zlib, 3 LZ4 block) |
| Materials | CDB (`crates/sfmaterial/`) from `materials\materialsbeta.cdb` via `--materials-ba2`. Vanilla ships **zero** `.bgsm`/`.bgem` files (283 of 69,170 sampled meshes still name one → guaranteed resolver miss → CDB fallback) and no `.mat` sidecars (Creation archives can carry loose JSON `.mat`, #4277 open) — the CDB is the only real material source. Phase 2 (#3398, 224a19372 / 18fce7e43 / 978d25c19) translates colour/normal/emissive/height roles + alpha/glass flags from a streaming `MaterialIndex`; authored metal/rough scalars and five single-channel kinds are still parked |
| Cell | Walkable Cydonia interior; no runtime baseline; #3540 (frame-0 stall) fixed by `plan_static_blas_restore`, real-device re-run unconfirmed |

## Parameters / Setup

`--focus <dimensions>` (default all 6). Setup: parse `$ARGUMENTS`; `mkdir -p /tmp/audit/starfield`; `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`; confirm `Starfield/Data/` exists (else note dimensions losing real-data validation). Read `docs/audits/AUDIT_STARFIELD_2026-09-16.md` first (its premise corrections), then the latest delta report (`AUDIT_STARFIELD_2026-09-29.md` at time of writing).

## Dimensions (ordered by Starfield-specific risk)

### Dimension 1: BA2 v2/v3 + Corpus Validation
**Subagent**: `general-purpose`
**Paths**: `crates/bsa/src/ba2.rs`, `crates/nif/tests/parse_real_nifs.rs`, `crates/nif/examples/nif_stats.rs`
**First step**: `BYROREDUX_STARFIELD_DATA=… cargo test -p byroredux-nif --test parse_real_nifs parse_rate_starfield_all_meshes -- --ignored` (all 13 mesh archives; `parse_rate_starfield` = Meshes01 only)
**Guards** (`ba2.rs`): `decompress_chunk_lz4_*`, `v3_unknown_compression_method_rejected`, `extract_dx10_concatenates_a_raw_chunk_and_an_lz4_chunk_in_one_record`, the `extract_dx10_*_short_*_chunk*` pair (#4662), `dx10_open_rejects_a_non_24_chunk_hdr_len_as_invalid_data` + `dx10_open_tolerates_non_monotonic_start_mip_without_panicking` (#5008 — both were file-controlled `debug_assert`s that panicked debug builds).
**Checklist**:
- v3: `compression_method` `0` → zlib, `3` → LZ4 block, anything else a hard `InvalidData`; LZ4 gets `unpacked_size` as its output bound. GNRL and DX10 both reach `decompress_chunk`; the per-chunk selector is `packed_size == 0` = raw (v3 DX10 mips mix raw and LZ4 chunks in one texture; the sentinel is unambiguous). A DX10 non-final chunk that decodes short is rejected (it would shift every later mip); only the final chunk is lenient (#4662).
- **Corpus** — 100.00% clean over all 13 archives (120,543 NIFs, 0 truncated / recovered / `NiUnknown`; measured 2026-09-16, re-confirmed 2026-09-24 for #4440, which brought ROADMAP / `game-compatibility.md` / `nif-parser.md` in line). Confirm it stays at 0 and the texture archives extract. `BSWeakReferenceNode` still captures an **undecoded** remainder into `starfield_tail` (#3524's byte-audit was never done): the 0 is recovery, not decode — growth of that tail is the signal.
- Trace a clutter item, hull, body, weapon and landscape mesh through `import_nif_scene`; new `NiUnknown` = a block introduced since the FO76 baseline.
**Output**: `/tmp/audit/starfield/dim_1.md`

### Dimension 2: BSGeometry Mesh Extraction (Starfield's actual mesh path)
**Subagent**: `legacy-specialist`
**Paths**: `crates/nif/src/import/mesh/{bs_geometry,skeleton}.rs`, `crates/nif/src/blocks/bs_geometry.rs`, `byroredux/src/asset_provider/archive.rs` (`normalize_mesh_path`)
**First step**: `cargo test -p byroredux-nif bs_geometry` + `git log --since=<last report> -- crates/nif/src/import/mesh/bs_geometry.rs crates/nif/src/blocks/bs_geometry.rs`
**Guards**: `bs_geometry_*_tests.rs` (sentinel slot, resolve log, hint mismatch, skin, tangent, bounding sphere #4394); `normalize_mesh_path_*` in `asset_provider/tests/material_path.rs`.
**Checklist** (what the guards cannot see):
- Stage A inline (`has_internal_geom_data`) vs Stage B external `.mesh`: the canonical `geometries\<X>.mesh` path must NOT get a `meshes\` prefix (#1292; Cydonia spawn rate collapses); iterate every LOD slot, not `meshes.first()` (#1209); both stages must skip a `scale<=0` sentinel slot (empty `vertices`/`triangles`) even when it parses `Ok`/matches `Internal` first (#1828/#1829) — accepting it silently drops the whole BSGeometry.
- **Trailer EOF** — `BSGeometryMeshData::parse` treats EOF at the post-LOD meshlet/cull trailer as "no trailer" (facegen `.mesh` bodies end at the LOD array; #3777) but a body truncated *mid*-trailer still errors (`remaining() == 0` gate). Invisible to `.nif` parse-rate gates (Stage B's per-slot `Err` arm only `debug!`-logs; pre-fix it zeroed all 1,282 `FaceMeshes` heads). The gate is explicit per path (#4269): `MeshTrailer::OptionalAtEnd` for standalone `.mesh` bodies, `MeshTrailer::Required` for inline Stage A bodies, where `remaining()` counts the rest of the NIF (guards `an_inline_body_reads_its_trailer_and_stops_after_it` / `an_inline_body_missing_its_trailer_is_an_error`). Known-open: `.mesh` bone indices pass unbounded (#4268).
- **Skin chain** (#1203) — `bone_refs` are NULL on 73% of skin refs, so `solve_bone_names_with_offset` (`skeleton.rs`) fits bind-pose offsets against an externally resolved skeleton: a name is accepted only on a **unique** full match (`MIN_BONES_TO_SOLVE = 8`; offset memoised per skeleton); every decline falls back to `Bone{i}` (never worse). Zero wrong over 9,057 ground-truth bones; coverage partial by design (425/908 clothes skins, 8,288/22,663 bones at 2026-08-30, solver unchanged since — re-measure before citing).
**Output**: `/tmp/audit/starfield/dim_2.md`

### Dimension 3: CDB Material Database
**Subagent**: `renderer-specialist`
**Paths**: `crates/sfmaterial/src/`, `byroredux/src/asset_provider/material/{cdb,merge,provider}.rs`, `byroredux/src/asset_provider/tests/starfield_mat.rs`, `crates/sfmaterial/examples/cdb_key_hash_probe.rs`
**First step**: `git log --since=<last report> --format='%h %cs %s' -- crates/sfmaterial byroredux/src/asset_provider/material/cdb.rs`
**Guards**: `starfield_mat.rs` — `discovered_cdbs_accumulate_in_load_order`, `mat_path_merges_cdb_authored_textures_when_indexed` + `mat_path_lookup_miss_keeps_presence_only_and_no_textures`, `cdb_hit_stamps_neutral_scalars_instead_of_keyword_classifying` (#5197), `registered_cdb_does_not_shadow_a_resolvable_bgsm`, `unresolvable_bgsm_still_falls_back_to_cdb_pbr`, `starfield_single_channel_kinds_are_parked_by_name_or_canonical` (#4429, anchored to nifal.md's list span by #5003); `merge.rs` `non_colour_slot_replacements_stay_parked` (#5190); `reader.rs` unit tests (hostile counts, pinned vocabularies, `read_order_reorders_xmcolor_shape`, `self_referential_struct_is_rejected_instead_of_overflowing_the_stack`); `#[ignore]`d `tests/real_cdb.rs` (`material_index_resolves_a_vanilla_material`, `material_index_dlc_cdb_resolves_the_base_corpus`, streaming validator — never revert to `parse`, ~9.19 GB).
**Checklist**:
- **Discovery** — `discover_starfield_cdbs` scans every archive for the base and DLC/Creation `materials\creations\<plugin>\materialsbeta.cdb` (13 CDBs, ~232 MB; two full-size), `peek_magic` then the tolerant `probe_header` (#4273); re-hardcoding the base path drops every DLC CDB.
- **Reader** — consumption modes: `parse` (full `Value` tree; 9.19 GB on ONE full-size CDB, ~18 GB across the set), `visit_instances_with_limits` (each top-level value delivered then dropped; one `LIST`/`MAPC` can still be huge), `validate_instances_with_limits` (no tree; #4274), and the Phase-2 `MaterialIndex::build` (`crates/sfmaterial/src/index.rs`: one streaming pass, ~470 MB / ~2 s on the vanilla base CDB; `PersistentID → DBID` join, material → `LayerID` → `MaterialID` → `TextureSetID` → `MRTextureFile` slots `SLOT_COLOR`..`SLOT_TRANSMISSIVE` = 0..8). The built index is memoised per CDB in `sf_cdb_index_cache` (`cdb.rs`); a failed build fills only a vacant slot, never clobbers a concurrent success (870ea1d07); `lookup_cdb_material` is last-registered-wins across CDBs. Duplicate class/field names error (#4272); value nesting is capped at 64 (`Error::NestingTooDeep`, #4657 — an 84-byte self-referential CDB used to overflow the stack); unknown `ChunkType`/`Value` variants must error or warn-and-skip, never panic.
- **Lookup key (solved, #3398 spike)** — reflected CRC-32 (poly `0xEDB88320`, init 0, no final XOR) over the lowercased backslash path, directory and stem hashed separately (3,032/3,084 = 98.3%); `BSResource::ID` labels are rotated (`.Dir` = stem, `.Ext` = directory, `.File` = `"mat"`). Reproduce: `crates/sfmaterial/examples/cdb_key_hash_probe.rs`. Text calling the key or field vocabulary "unknown" is stale.
- **Field order is offset order (#4275, fixed in 224a19372)** — `read_user_class` walks `Class::read_order` (offset-sorted), not declaration order; `XMCOLOR` declares `r,g,b,a` at offsets 2,1,0,3, so a declaration-order read is a straight R↔B swap on every decoded colour. Regression guard: `read_order_reorders_xmcolor_shape`.
- **Try-then-fall-through (#3230)** — the key's extension column is the constant `"mat"`, so lookup ignores the reference's suffix (17 of 57 `.bgsm`/`.bgem`-named sample paths resolve to real CDB materials): `.bgsm`/`.bgem` names hit `resolve_bgsm`/`resolve_bgem` first and reach the CDB only on a miss; `.mat` keeps its early return (no JSON `.mat` resolver, #4277). On every arm a CDB hit goes through `apply_cdb_material` (`Merged`) and only a lookup miss falls to `apply_cdb_pbr_fallback` (`PresenceOnly`, `is_pbr` + `external_material_resolved`). An early `PresenceOnly` above the resolvers discards every authored role, `glass_enabled` and PBR scalar.
- **Phase-2 state (`apply_cdb_material`, `merge.rs`)** — slots 0/1/6/7 fill `base_color`/`normal`/`height`/`emissive` with NIF-first `fill` precedence; slots 2/3/4/5/8 (`_opacity`/`_rough`/`_metal`/`_ao`/`_transmissive`, ~39% of Starfield textures) are parked by name in `docs/engine/nifal.md` (#4429; the guard fails if a role lands without moving the parked row). `MaterialParamFloat` stays untranslated (index → roughness/metalness unverified, #3398 open — do not guess); a hit with no translated scalar stamps `PbrMaterial::NO_SIGNAL_NEUTRAL` so the keyword classifier never fabricates conductors from the resolved colour path (#5197: `iris_iron_color.dds` → 0.9 metal). `TextureReplacement` is per-slot (#5190): only the `SLOT_COLOR` flat colour lands, into `diffuse_color`, and only when no colour texture bound. `IsGlass` → `bgem_glass` (never `thin_glass`); `UseSSS` is parked until a subsurface-colour source exists; alpha test requires `has_opacity != Some(false)` (#5196). Any of these reverting is the regression; the parked items are scope, not new findings.
**Output**: `/tmp/audit/starfield/dim_3.md`

### Dimension 4: ESM Resolve Rate + Cell Bring-up
**Scope split with `/audit-esm`**: it owns the parser as a parser; this dimension owns Starfield's data through it. Shared-mechanism defects → `/audit-esm`.
**Subagent**: `general-purpose`
**Paths**: `byroredux/src/sf_smoke.rs`, `crates/plugin/examples/{sf_smoke,sf_parse_check}.rs`, `crates/plugin/src/esm/records/parse.rs`, `crates/plugin/src/esm/cell/{support,walkers}.rs`, `byroredux/src/cell_loader/spawn.rs`, `byroredux/src/systems/light_anim.rs`
**First step** (two different questions): `cargo run --release -- --esm Starfield.esm --sf-smoke <CELL_EDID>` (per-cell base-form resolve rate, Cydonia) and `cargo run --release -p byroredux-plugin --example sf_smoke -- <ESM_PATH> --tsv` (GRUP byte coverage vs `DISPATCH_HANDLED_FOURCCS`, diffed against `.claude/audit-baselines/sf-esm/*.tsv`).
**Guards**: `DISPATCH_HANDLED_FOURCCS` is derived beside the dispatch and pinned in `records/tests.rs` (#4278); `starfield_ligh_dat2_decodes_to_light_data`; `light_anim.rs` `starfield_light_type_enum_drives_the_spot_shape` + `starfield_shadow_technique_follows_the_light_type_enum`.
**Checklist**:
- A resolve-rate drop = the CELL handler dropped REFRs (moved subrecord, new XCLL field) or a base record failed to index → unit-cube placeholders. Check the per-type breakdown for Starfield-only base types (GBFM/GBFT/PNDT/STDT/BIOM); note frequency, don't re-report the known GBFM stub. PDCL (74.9% of unresolved Cydonia REFRs, 2026-08-30) outranks GBFM (0.081%) under the baseline doc's own promote/defer rule. Model-less STAT/BNDS/ACTI/ARMO forms (geometry in a BFCB block) still drop (#1576).
- **LIGH** — carries no `MODL`/`DATA`, only a component-block `DAT2`; `build_static_object_from_subs` must decode it or every LIGH REFR misses. Shape is the `DAT2+56` Light Type enum (0 omni, 1 shadow spot, 2 non-shadow spot) → `LightData::starfield_light_type` → `translate_light` / `canonical_light_shadow_flags`; flag `0x200` is "Focus Spotlight Beam", **not** a spot bit (pinned by `translate_light_excludes_starfield_even_with_spot_bit_set`).
- **PDCL** stays a *named* skip (`index.skipped_unconsumed_groups` + one-shot warn), not the anonymous catch-all. `XCLL_SIZES_STARFIELD = [28, 108]`: the 108-byte body shares only bytes 0–39 with Skyrim (decoded against xEdit SF1 `wbStruct(XCLL)`; "Skyrim 92 + a 16-byte tail" is stale); `parse_lgtm` reads Starfield's 108-byte LGTM DATA with the same SF layout (#5002, `starfield_lgtm_data_decodes_the_sf_xcll_layout`). The metric-unit lifts of Starfield WTHR FNAM (#5001/#5134) and WATR DNAM (#5151, pins corrected in 24cb577d2) happen at the parse boundary and belong to `/audit-exterior`. **TXST** decode covers `TX00`–`TX07`; unmodelled `TX08/09/17/19` (metal/rough/ao/opacity) warn once per FourCC (`warn_unmodelled_txst_slot`, #4438) — capture waits on canonical roles (Dim 3).
- **Spawn gates** (`cell_loader/spawn.rs`) — static-trimesh fallback gated on `base_layer`, not `final_layer` (#1294); synthesized colliders (`spawn_trimesh_collider_ghost` / `spawn_packed_collision_proxy`) carry no `MeshHandle`, so they never enter `blas_specs`; `DoorTeleport` from REFR XTEL (#1295).
**Output**: `/tmp/audit/starfield/dim_4.md`

### Dimension 5: NIF Shader Blocks — BSVER 155+
**Subagent**: `legacy-specialist`
**Paths**: `crates/nif/src/blocks/shader/{mod,lighting,effect}.rs`, `crates/nif/src/blocks/shader_tests/starfield.rs`, `crates/nif/src/shader_flags.rs`
**First step**: `cargo test -p byroredux-nif --lib -- shader::tests::starfield` — the test module mounts via `#[path = "../shader_tests/mod.rs"] mod tests` inside `blocks::shader`, so a `shader_tests::starfield` filter matches zero tests (vacuous green, #5004)
**Guards**: `starfield.rs` — `parse_bs_lighting_starfield_captures_trailing_tail`, the `..._tail_empty_without_size_or_drift` pair (LSP + effect), `every_tail_capturing_block_reports_it_and_parse_nif_records_it` (#2532). NIF mechanics are `/audit-nif`.
**Checklist**:
- CRC32 flag arrays for BSVER ≥ `FO4_CRC_FLAGS` (132) → `sf1_crcs`; SF2 for BSVER ≥ `FO76_SF2_CRCS` (152) → `sf2_crcs`; hashes are the same reflected CRC-32 as CSG/CDB, over the **uppercase** nif.xml flag name (`bs_shader_crc32`). The effect-shader `Own_Emit` additive promotion reads typed bit **or** the CRC `EMIT_ENABLED` flag via `modern_effect_shader_bit` (#4279, `own_emit_additive_promotion_reads_the_typed_bit_and_the_crc_flag`); no vanilla Starfield block sets it, so a typed-word-only read is invisible on vanilla.
- **#1510** — the `BSShaderType155` tail once over-read by 4 B, truncating ~1,036 full-body `BSLightingShaderProperty` blocks to `NiUnknown`; that count must stay 0.
- **Undocumented tails** — empty-name full-body `BSLightingShaderProperty` **and** `BSEffectShaderProperty` carry trailing bytes nif.xml does not document; both capture `block_size - consumed` opaquely into `starfield_tail` via `read_starfield_tail` (never a hardcoded length; LODMeshes drift 0). Never fabricate field names or semantics.
- **Material-reference stubs** — a non-empty `Name` at `bsver >= STARFIELD` is a stub (`is_material_reference`): census of 480,861 = 478,691 `.mat`, 1,679 `.bgsm`, 104 `.bgem`, **387 suffix-less, all the degenerate `Materials\` / `\Materials`** (editor markers, conveyors — not content-hash paths, so the "hash path" comments/fixtures are wrong, #4439). Those get `material_path = None` → `Unresolved`, the only stubs that never reach the CDB PBR route.
**Output**: `/tmp/audit/starfield/dim_5.md`

### Dimension 6: Material Flow — NIFAL Boundary + BGSM/BGEM/`.mat`
**Subagent**: `renderer-specialist`
**Paths**: `byroredux/src/material_translate.rs`, `byroredux/src/asset_provider/material/merge.rs`, `byroredux/src/cell_loader.rs` (`pack_imported_material_flags`), `crates/nif/src/import/material/slot_role.rs`, `crates/bgsm/src/{bgem,bgsm}.rs`
**First step**: `git log --since=<last report> --format='%h %cs %s' -- byroredux/src/material_translate.rs byroredux/src/asset_provider/material crates/nif/src/import/material`
**Guards**: `merge_external_material_is_the_only_exported_fn_in_this_file`; `colocated_lighting_mask_is_confined_to_the_tint_family_slot_two` (#4431); `bgsm_merge.rs` / `starfield_mat.rs`. Canonical boundary invariants are `/audit-nifal`.
**Checklist**:
- `merge_external_material` takes `&mut ImportedMaterial` (cannot touch geometry/skinning); `.mat` texture paths must land in `MaterialTextureSet` roles, never a CDB slot index; BGEM stays distinct from BGSM (`glass_enabled`, the authoritative glass signal, must not misclassify an opaque piece with a stuck flag). `pack_imported_material_flags` derives `BGSM_AUTHORED` / `PBR_BSDF` / `TRANSLUCENCY` / `MODEL_SPACE_NORMALS` / `EFFECT_PALETTE_COLOR` from the right fields; on Starfield `BGSM_AUTHORED`, `TRANSLUCENCY` and `MODEL_SPACE_NORMALS` can never be set today (zero BGSM files). `from_bgsm` stays FO4 spec-glossiness (both CDB arms leave it clear); effect-shader glass promotion reads the separate `external_material_resolved` (set by the BGSM/BGEM arms **and** both `apply_cdb_material` / `apply_cdb_pbr_fallback`, #4283) in `classify_glass_into_material_with_provenance`, while the `2..=20` authored-dispatch guard deliberately still keys on `from_bgsm` (the CDB authors scalars, not the dispatch). Swapping either signal is the regression.
- **Resolve-once** — BGEM and Starfield stubs whose CDB lookup misses leave metalness/roughness NaN so `resolve_pbr`'s keyword classifier runs (`has_no_pbr_classifier_signal`) — a live arm, not a "future backstop" (doc corrected in c2b67d81e, #4441); a CDB hit stamps the no-signal neutral instead (#5197, Dim 3). Starfield uses FO76's slot vocabulary (#3900); `slot_to_colocated_role` is Skyrim-only (#4431). Sinks (#4282): `wetness`/`luminance` reach `Material` as `WetnessShading`/`LuminanceShading` — capture-only, no `GpuMaterial` consumer yet. Water concentration normalises once at the WATAL boundary (`resolve_water_material`, #4285); a per-game token back in `water.frag` is the regression.
- **Population fact (2026-08-30 block histogram)** — vanilla Starfield ships zero `NiPSysEmitter*`, `BhkMultiSphereShape` and `BhkConvexListShape` blocks, so the NIFAL particle and per-shape collision slices have no Starfield input and a test there is vacuous; a non-zero count in a new archive is a finding. Collision is `BhkSystemBinary` blobs; Cydonia's colliders are synthesized (Dim 4).
**Output**: `/tmp/audit/starfield/dim_6.md`

## Phase 3: Merge

1. Read `/tmp/audit/starfield/dim_*.md`; combine into `docs/audits/AUDIT_STARFIELD_<TODAY>.md`:
   - **Executive Summary** — first-class `GameKind`; NIF + BA2 at the measured rate; CDB Phase 2 partial (roles + flags translated, scalars and five kinds parked); walkable Cydonia; regressions in the bring-up surface.
   - **Dimension Findings** by severity; **CRC32 Flag Table** (flag → CRC32 via `bs_shader_crc32`, read-by-import, vanilla occurrences).
   - **Remaining-Work Chain** (`starfield-esm-roadmap.md`: Phases 0+1 done, 2–4 invalidated by the 99.9%-parity measurement) — CDB Phase 2 remainder (#3398: the indexed reader, `XMCOLOR` offset order and `IsGlass` signal shipped; left are canonical roles for the five parked kinds (#4429), `MaterialParamFloat` → roughness/metalness mapping, a subsurface-colour source for `UseSSS`, the loose `.mat` JSON resolver #4277) → PDCL ahead of GBFM → exterior worldspace tiles → space-cell / planet / GBFM records. Never frame it as "BGSM parser first / ESM very far" — both shipped; the NIF truncation tail is cleared.
2. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_STARFIELD_<TODAY>.md`
(label every finding `game:starfield` + `legacy-compat`, plus its own domain label.)
