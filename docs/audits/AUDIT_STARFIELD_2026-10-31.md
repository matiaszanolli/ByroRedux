**HEAD**: 670f81917 · **Baseline**: docs/audits/AUDIT_STARFIELD_2026-10-27.md (HEAD fa6be4a6b) · **Audited**: Dims 2, 4 (delta-touched, real-data re-verified) · **Unchanged since baseline (skimmed, guards + gates run)**: Dims 1, 3, 5, 6

# Starfield Compatibility Audit — 2026-10-31

This report is one leg of `/audit-suite --preset comprehensive`. One auditor ran every dimension in sequence, with no sub-agents.

The window is `fa6be4a6b..670f81917` (136 commits). Almost all of it is M43 gameplay, audit-driven fixes and file splits for other games. The Starfield-relevant commits are:

- **Fixes for the last report:**
  - ec8bbd01f (#6473): `SkeletonBones::from_scene` composes through `compose_transforms` and `any_ni_node`. This is the composition behind the NULL-ref skin solver, cited on 10-27 as NIF-D4-2026-10-27-01.
  - 4d33e0365 (#6096, #6099): pre-baked FaceGen paths use the plugin-local id (the 10-27 report listed them as open); the facetint probe is Skyrim-only.
- **Shared mechanisms touched:**
  - abc23c102 (#6223): one Creation-era WTHR decoder, now also used by Starfield.
  - ccccbaf03 (M43): `AORU`/`KYWD DATA` parsing, `FURN`/`FLOR` keyword capture.
  - 4385d5fe8 (#6459): an unlit alpha-blended arm added to `render_geometry_can_collide`.
  - 613c7f46e and 477124477: the `npc_spawn` split and the `character_skin` flag.
- **No commits** on `crates/bsa/src/ba2.rs`, `bs_geometry.rs`, `crates/sfmaterial`, `asset_provider/material/*`, `blocks/shader/*`, `shader_flags.rs`, `crates/bgsm`, `material_flags.rs`, `sf_smoke.rs`, `lmsw.rs`, `reflection.rs` or `esm/reader.rs`.

Constraints honoured:
- No engine launch, no `--sf-smoke`, no smoke script, and no blanket `--ignored` run. Every real-data test ran alone (`--exact`) under `systemd-run --user --scope -p MemoryMax=4G..6G`.
- The full 9 GB CDB `parse` was not attempted.
- No source, skill or issue edits.
- The repo `target/` was used throughout. A read-only scratch probe crate (`…/scratchpad/skprobe`, path deps on the repo crates, `--offline`) was built into the repo `target/`. There is no private target dir.

## Guard runs and real-data gates (all green)

| Suite | Result |
|---|---|
| bsa `--lib ba2` | 44/44 |
| nif `--lib` (full) | 1449 pass, 1 ign (10-27: 1446; the delta adds tests, among them `from_scene_scales_child_offsets_and_walks_switch_node_parents`) |
| sfmaterial lib + header_smoke + integration | 51 + 6 + 1 |
| plugin `--lib` `starfield` / `lgtm` / `xcll` / `spatial_units` / `pkin` / `dat2` / `txst` / `lmsw` / `acbs` / `small_master` / `reflection` / `wths` / `bleedout` / `sf_esm_baseline` / `weather` | 30 (3 ign) / 5 / 25 (2 ign) / 10 / 12 (1 ign) / 8 / 18 / 11 / 9 (1 ign) / 3 (1 ign) / 1 / 2 / 2 / 1 / 54 — all counts equal to 10-27 |
| byroredux bin: `starfield_mat` 19, `asset_provider::material` 30, `material_translate` 78, `loose` 21, `cdb` 37, `light_anim` 31, `normalize_mesh_path` 8, `pkin` 18, `lmsw` 1, `starfield` 36, `bgsm_merge` 79, `pack_imported_material_flags` 8, `synthesize_trimesh` 17, `prebaked` 20, `tint` 25, `effect_shader` 11, `merge_external_material_is_the_only_exported_fn_in_this_file` 1, `documented_texture_role_list_matches_the_struct` 1 | all pass |
| **Dim 1 corpus** `parse_rate_starfield_all_meshes` (release, MemoryMax=5G, 8.0 s) | **13/13 archives, 120,543 NIFs, 100.00% clean, 0 truncated, 0 failed** |
| **`real_cdb.rs`**, each `--ignored --exact` alone (release, MemoryMax=4G) | 5/5 (0.7–2.0 s each): `parse_vanilla_materialsbeta_cdb`, `material_index_resolves_a_vanilla_material`, `…_inherited_glass_and_emissive_settings`, `…_dlc_cdb_resolves_the_base_corpus`, `…_inherited_texture_slots` |
| `cell_loader::pkin_expansion_tests::starfield_cydonia_pack_ins_instance_their_template_cells` (alone, MemoryMax=6G, 7.5 s) | pass. 370 pack-in REFRs → 12,372 children; **2,191 carry their own swaps**; 0 left to the default path |
| `sf_smoke --tsv --recurse` on `Starfield.esm` (release, MemoryMax=5G) | **byte-identical** to `.claude/audit-baselines/sf-esm/Starfield.tsv` (non-comment rows) |

The installed Starfield `Data` is unchanged: no file has an mtime after 2026-07-01, and there is no loose asset root.

## Executive Summary

Starfield is a first-class `GameKind`. This window brought no regressions and no new findings. The one fixed finding from the last report is verified on real data. The one open finding (#6519) still reproduces.

- **NIF + BA2**: unchanged. Re-measured at 100.00% over 120,543 NIFs.
- **BSGeometry / skin**: no `bs_geometry` commits. #6473 reworked the skeleton world-position fold behind the NULL-ref skin solver. A probe over all 122 vanilla `skeleton.nif` / `skeleton_facebones.nif` files shows the change is output-neutral for Starfield (details in the Dim 2 summary).
- **CDB Phase 2** is partial and unchanged in scope. Colour, normal and emissive roles are translated, as are the alpha, glass and blend flags, with settings and texture-set inheritance. Still parked:
  - metal/rough scalars;
  - slot-6 `_height`;
  - the five single-channel kinds;
  - emissive settings;
  - the additive blend family (#5509).
- **Cydonia bring-up**: unchanged and re-verified on real data (pack-ins, swaps, sf_smoke coverage). Starfield FaceGen path and tint (#6096/#6099) are now fixed, so the 10-27 "Actor behaviour" item narrows to bleedout (#6259) and the NPC stat model (#4453). Walkable Cydonia is not re-verified, because no engine launch is allowed.
- **Existing, still open**: #6519 (the VFX-collider gate cannot see Starfield's `BSEffectShaderProperty` stubs). Neither 4385d5fe8 nor any other commit touched it.

Counts: **0 CRITICAL / 0 HIGH / 0 MEDIUM / 0 LOW new**.

## Dimension Summaries

- **Dim 1 (BA2 + corpus)**: unchanged; no commits on `ba2.rs`. Guards and the corpus are green.
  - Cross-check of #6096 against real archives: `FaceMeshes` / `SFBGS003` / `SFBGS00D` / `ShatteredSpace` facegeom entries are named by plugin-local id (e.g. `sfbgs00d.esm\0006f626.nif`, `sfbgs003.esm\0000ba4f.nif`), matching `plugin_local_object_id`.
- **Dim 2 (BSGeometry)**: no commits on `bs_geometry.rs` or the skin conversion. The only mesh-path changes are:
  - #6473 `skeleton.rs`: the fold is now `compose_transforms` (scale term, #4549 sanitiser) over `any_ni_node` (accepts `NiSwitchNode` and other node-family parents).
    - The old fold had no scale term and a bare `NiNode` downcast, so this is a strict superset.
    - Probe over Meshes01/02/Patch/FaceMeshes: 122 skeleton NIFs, 2,006 `NiNode`s, none with `|scale − 1| > 1e-3` (only ~1e-6 float noise) and no non-`NiNode` node-family block. The output is identical within `MATCH_EPSILON` (0.002), so Starfield's solver coverage is unchanged and its failure mode is still a decline, never a wrong name.
    - NIF-D4-2026-10-27-01 is fixed.
  - A doc-only alias move in `skin.rs` (#6513).
- **Dim 3 (CDB)**: no commits on `crates/sfmaterial` or `asset_provider/material/*`. The `asset_provider` diff is `audio.rs` and the `.pex` lookup in `script.rs`. All five `real_cdb.rs` gates pass.
- **Dim 4 (ESM + bring-up)**:
  - **#6223 / abc23c102** (Starfield now decodes through `parse_wthr_creation`): behaviour is preserved.
    - A census of `Starfield.esm` WTHR shows 3 records with `NAM0` 608 B (still FO3-stride via `Nam0Layout::Fo3Stride`), `FNAM` 72 B, `DATA` 20 B, `JNAM`/`PNAM` 1024 B, and none of the dropped FO3-era `ONAM`/`DNAM`/`CNAM`/`ANAM`/`BNAM`/`00TX` arms.
    - The FNAM 72-byte height-fog read and the IMSP read are carried over. `weather` tests pass (54).
  - **M43 attraction (ccccbaf03)** adds game-agnostic `AORU` and `KYWD DATA` parsing.
    - On `Starfield.esm`: 11 `AORU` records (`AOR2` 16 B; the parser needs ≥ 14), and 14 `KYWD DATA` values that all resolve to an `AORU` form (the `CCT_Enviro_*` creature-behaviour rules, radii 15–100).
    - The layout holds and the sf_smoke TSV is unchanged (`AORU` handled = 1 was already in the baseline).
    - Whether the attraction system should run on Starfield is a gameplay-audit question. The Papyrus/Story Manager runtime does not cover Starfield, so nothing consumes it today.
  - **FURN / FLOR split** onto `extract_records_with_modl`: equivalent to the old `parse_modl_group` for single-type groups.
  - **#6096 / #6099** (4d33e0365) verified: the `0xFD` medium-master case is in `prebaked_facegen_nif_path_matches_vanilla_layout`, and `only_skyrim_probes_a_facetint_path` holds.
  - **#6464** (FO4 "Use Land Data") did not add Starfield; Starfield has no exterior tile path yet (scope).
  - **#6471** (spawn plays the idle sequence): no Starfield leg, since 0 Starfield NIFs carry a `NiControllerSequence` (10-27 probe).
  - **#6260 / #5757 / #5489**: re-verified on real data (370 → 12,372 children, 2,191 with swaps).
  - **#6519 / SF-2026-10-27-D4-01** (existing, open): `render_geometry_can_collide` still keys on `effect_shader.is_none()`.
    - The 4385d5fe8 arm needs `material_kind == MATERIAL_KIND_NO_LIGHTING` (102), which only `legacy_properties.rs` sets, so Starfield cannot reach it.
    - The stub arm in `dedicated_shader.rs` is comment-only changed.
- **Dim 5 (shader blocks)**: no commits on `blocks/shader/*` or `shader_flags.rs`; 11/11 Starfield shader guards (inside the `nif --lib` run); corpus `NiUnknown` count 0.
- **Dim 6 (material flow)**: no commits on `merge.rs`, `material_flags.rs` or `crates/bgsm`. The new `ImportedMaterial.character_skin` (#6476/#6477: `textures\characters\` minus underwear/groin) only drives the Oblivion/FO3/FNV race-skin override. Starfield authors no race body texture, so it is inert. `material_translate.rs` changed only a test constant.

## Findings

None new.

**Existing, still present (not re-filed)**:
- **#6519** (LOW): SF-2026-10-27-D4-01.
- **#6259**: Starfield bleedout has data but no recovery rule.
- **#5509**: CDB additive-family blend modes.
- **#3398**: CDB Phase 2.
- **#1576**: model-less STAT/BNDS/ACTI/ARMO forms.

## Checked and not filed

- **#6114** (NIF-registry eviction deferral, 304f3b587): performance's PERF-2026-10-31-D7-01 reports `evict_over_cap` as O(excess × N) on the main thread at batch settle. Cydonia's 12k pack-in children are a heavy user of that registry, so Starfield is exposed to it. It is not Starfield-specific, and it is cited, not re-filed.
- **Nothing else in the delta touches a Starfield-specific path.**

## CRC32 Flag Table

There is no change since the 2026-10-12 report: 0 commits on `crates/nif/src/shader_flags.rs` or `crates/nif/src/blocks/shader/` in this window. The full 32-row derivation is in `AUDIT_STARFIELD_2026-09-11.md` / `-09-16.md`.

## Remaining-Work Chain

Per `starfield-esm-roadmap.md`, Phases 0 and 1 are done, and Phases 2–4 were invalidated by the 99.9%-parity measurement.

1. **CDB Phase 2 remainder (#3398).**
   - Shipped: the indexed reader, `XMCOLOR` offset order, `IsGlass`, AlphaBlend and emissive-settings capture, the loose `.mat` decoder, settings and texture-set `Parent` inheritance, a panic-safe memoised build, and loose CDB discovery.
   - Left:
     - canonical roles for the five parked kinds (#4429);
     - a slot-6 height-blend consumer (#5381);
     - the `MaterialParamFloat` → roughness/metalness mapping;
     - a `UseSSS` colour source;
     - the inherited-glass coverage signal;
     - the additive-family blend modes (#5509). #5509 also gates VFX collider exclusion for `.mat`-authored effects (#6519).
2. **Material swaps.** LMSW / XLMS (#5757) and the pack-in child swaps (#6260) are shipped.
3. **PDCL ahead of GBFM**, then exterior worldspace tiles, then space-cell / planet / GBFM / SFTR / PNDT records. `sf_smoke` coverage is unchanged at the baseline.
4. **Actor behaviour.**
   - FaceGen path and tint (#6096 / #6099) are now fixed.
   - Bleedout data is decoded, but the recovery rule is unsourced (#6259, open).
   - The NPC stat model is #4453.
   - Corpse ragdoll pose is out of scope, because no Starfield rig is in `pose_skeleton`'s verified set.

## Coverage Notes

- **Not run**: `--sf-smoke` and the engine binary (forbidden); the full 9.19 GB generic CDB `parse`.
- **Probes** (read-only):
  - Python `-I` `mmap` walks of `Starfield.esm` for WTHR and KYWD/AORU sub-record censuses.
  - A scratch Rust probe over 122 skeleton NIFs.
  - `ba2_grep` listings of facegeom paths.

Suggest: nothing new to publish. #6519 is already filed.
