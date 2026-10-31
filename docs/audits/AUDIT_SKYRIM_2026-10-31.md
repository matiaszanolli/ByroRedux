# Skyrim (SE + LE) Compatibility Audit — 2026-10-31

**HEAD**: 670f81917 · **Baseline**: `docs/audits/AUDIT_SKYRIM_2026-10-27.md` (HEAD `fa6be4a6b`, 136 commits since) · **Audited**: Dim 3 (NPC equip + FaceGen; npc_spawn split, #6096, #6515, new Skyrim armor rating), Dim 4 (multi-master + TES5 cell load; new census of Skyrim "Use Land Data" child worldspaces), Dim 5 (archives + corpus gates, real-data sweeps re-run) · **Unchanged since baseline (skimmed)**: Dim 1 (only #6473 skeleton compose, #6513/#6514 doc moves), Dim 2 (only #6514 doc text; `blocks/shader/*`, `shader_data`, `material_translate.rs`, `triangle.frag` untouched for Skyrim)

Run context: part of the 2026-10-31 comprehensive suite. Solo, static analysis plus cargo tests and a Python byte-walk of the SE masters. No engine launch, no sub-agents, repo `target/` only, no private target dir. Scratch notes are in `/tmp/audit/skyrim/dim_{1..5}.md`.

---

## Executive Summary

All five 2026-10-27 findings are fixed and verified in the current source:

| 10-27 ID | Fix | Check |
|---|---|---|
| SKY-D1-01 | e03dff819 (#6513) | `skin.rs:634-645`: the alias now has its own two-line doc; the no-remap contract sits on `decode_sse_skin_payload` |
| SKY-D2-01 | 974d00657 (#6514) | `import/material/mod.rs:145` says bit 21 = `Anisotropic_Lighting` on Skyrim and FO4 |
| SKY-D3-01 | 26850aa5f (#6515) | `TEMPLATE_FLAG_USE_KEYWORDS` + `ResolvedNpc.keywords` terminal, read by `KeywordTable` |
| SKY-D4-01 | 3da35c683 (#6516) | `faction.rs:8-13`: `CRVA_MIN_LEN = 12`, `CRVA_STEAL_LEN = 16`, `CRVA_TAIL_LEN = 20` |
| SKY-D5-01 | 90c36f4c5 (#6517) | `bsa_real.rs:99-106`: LE sweep resolves through `data_dir` with the Wine-prefix fallback |

#6096 (DLC facegeom named by the global FormID) was also fixed, in 4d33e0365, via the shared `plugin_local_object_id`.

Parse and archive coverage is green on real data (see Tests run). The 10-27 cited items EXT-D6 (spawn_lod_block OOB) and LC-D2-01 (spawn-time sequence) are fixed upstream per the suite notes.

One new finding, from a census of the Skyrim masters that the #6464 commit invited ("Skyrim has not been measured"): **SKY-2026-10-31-D4-01 (MEDIUM)**. Skyrim's seven "Use Land Data" city worldspaces are co-located with Tamriel, but Skyrim is outside the `needs_exterior_processing` gate, so 308 exterior cells in them (all fillable) load with no terrain and no ground collision.

| Severity | NEW | Existing / cited |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 1 | #5959 (bench) open; CHAR-2026-10-31-M-01 (armor GMSTs) cited |
| LOW | 0 | #4137 open |

---

## Dimension Findings

### Dimension 1 — BSTriShape + SSE reconstruction (skimmed, unchanged)
No findings. In the window only ec8bbd01f (#6473, `SkeletonBones::from_scene` composes through `compose_transforms`; vanilla scales are 1.0) and the doc commits touched `import/mesh`. The real-data SSE skin-index guard passes.

### Dimension 2 — Shader-type dispatch + material slice (skimmed, unchanged)
No findings. 477124477 adds `ImportedMaterial::character_skin` (path under `textures\characters\`, minus `underwear*`/`groin*`). Skyrim keeps body skins under `textures\actors\character\`, so it is false on Skyrim, and the only consumer (`set_body_base_color`) is on the Oblivion/FO3 race-skin path. The coverage matrix from 2026-10-27 stands (parse path byte-unchanged; per-block baselines match).

### Dimension 3 — NPC equip + FaceGen
No findings.
- **#6096 / #6099** (4d33e0365): `prebaked_facegen_nif_path` and `prebaked_facegen_tint_path` take `plugin_local_object_id(form_id)` (regular 24 / light 12 / medium 16 bit); `prebaked_tint_path_for` gates the facetint probe to `GameKind::Skyrim`. Tests pin the three slot kinds.
- **#6515**: the 0x1000 Use Keywords terminal is in. 0x0800 Use Attack Data remains an unparsed `ATKD` block, which the skill already documents.
- **613c7f46e** (npc_spawn split): behaviour-neutral. The `npc_spawn::tests` concatenated-source scans pass (257 bin tests ok).
- **31aca901d** (Skyrim armor rating, `byroredux/src/armor.rs`): re-verified against the local UESP wiki (`Skyrim/Armor.wiki` lines 141-154, `Skyrim/Smithing.wiki` chest/other columns). Reduction is 0.12 % per displayed point plus 3 % per worn piece (shields included, clothing excluded), capped at 80 %. The player skill coefficient is 0.4 and the NPC one is 1.5. The `BOD2` armor-type order 0 light / 1 heavy / 2 clothing matches xEdit `wbArmorTypeEnum`. The hardcoded GMST-style constants are CHAR-2026-10-31-M-01 and are not re-filed here.

### Dimension 4 — Multi-master load order + TES5 cell load

Verified: `parse_real_skyrim_esm` and `ms01_eltrys_authored_placement` (DIAL `QNAM`) are green. FACT `CRVA` now cuts at 12/16/20. #6464 (FO4 Use Land Data) touched `load_order.rs` and `cell/index.rs` and is the source of the finding below.

#### SKY-2026-10-31-D4-01: Skyrim's seven "Use Land Data" child worldspaces never inherit Tamriel's landscape — 308 exterior cells load with no terrain or ground collision
- **Severity**: MEDIUM
- **Dimension**: Multi-master / TES5 cell load (exterior; mechanism owner `/audit-exterior` + `/audit-esm`)
- **Location**: `byroredux/src/cell_loader/load_order.rs:607-615` (`needs_exterior_processing`, Oblivion | Fallout3NV | Fallout4 only); test `use_land_data_children_inherit_on_oblivion_fo3_fnv_and_fo4_only` (`load_order.rs` ~:1959-1967) asserts `!needs_exterior_processing(GameKind::Skyrim)` with the comment "Skyrim has not been measured"; stale doc `docs/engine/exal.md:148-154` ("not measured on the Skyrim+ children")
- **Status**: NEW. Same mechanism as closed #5505 (Oblivion), #6089 (FO3) and #6464 (FO4); no open issue covers Skyrim. Distinct from EXT-D1-2026-10-31-01, which is the exal.md doc-rot for the same sentence after #6464 and does not measure Skyrim.
- **Description**: `inherit_parent_landscapes` copies the `WNAM` parent's LAND into a child's own LAND-less cells at the same grid, for worldspaces whose `PNAM` has bit 0x01 (Use Land Data). Skyrim stamps `PNAM` (all children parent Tamriel `0x3C`), and the parse already stores `parent_flags` (`cell/wrld.rs:116`). But the pass runs only for games in `needs_exterior_processing`, and Skyrim is not one, so the step and the `exterior_ready` re-parse are skipped.
- **Evidence** (Python byte-walk of `Skyrim.esm`, LAND matched to its preceding CELL's `XCLC`, `VHGT` decoded to heights, median |dz| in game units, 33×33 samples):

  | Child worldspace | PNAM | Exterior cells | With own LAND | LAND-less | Parent LAND exists for LAND-less |
  |---|---|---|---|---|---|
  | WhiterunWorld | 0x7f | 113 | 7 | 106 | 106 |
  | WindhelmWorld | 0x7f | 57 | 2 | 55 | 55 |
  | RiftenWorld | 0x77 | 29 | 12 | 17 | 17 |
  | SolitudeWorld | 0x7f | 62 | 20 | 42 | 42 |
  | WhiterunDragonsreachWorld | 0x7f | 48 | 0 | 48 | 48 |
  | WindhelmPitWorldspace | 0x5f | 29 | 1 | 28 | 28 |
  | KatariahWorld | 0x57 | 12 | 0 | 12 | 12 |
  | **Total** | | | | **308** | **308** |

  Co-location, child LAND vs Tamriel LAND at the same grid, against the same cell shifted by one cell or (2,2):

  | Child | Same grid | Shifted control |
  |---|---|---|
  | Whiterun (7 cells) | 0 | 520 |
  | Windhelm (2) | 488 | 2376 |
  | Riften (12) | 24 | 364 |
  | Solitude (20) | 364 | 1316 |
  | WindhelmPit (1) | 0 | 312 |

  Whiterun and the pit are identical to Tamriel; the others are Tamriel plus city edits. The grids are co-located as in Oblivion/FO3. Update/Dawnguard/Dragonborn/HearthFires only re-author these worlds with a few extra LAND-less cells (e.g. Dawnguard: Whiterun 2, Windhelm 2).
- **Impact**: A Skyrim city exterior (`--wrld WhiterunWorld`, or a gate door transition into it) renders with no terrain in 106 of 113 cells and has no ground collision there (placed statics keep their own colliders). The FO3 equivalent (#6089) was the same defect. Interiors, Tamriel itself and the `WhiterunDragonsreach` interior bench are unaffected, which is why the control bench cannot see this. Not verified in a running engine (suite rule); the claim rests on the data census and the unchanged gate.
- **Related**: #5505, #6089, #6407, #6464, EXT-D1-2026-10-31-01.
- **Suggested Fix**: Add `GameKind::Skyrim` to `needs_exterior_processing`, flip the Skyrim assertion in the gate test, add an `#[ignore]`d `Skyrim.esm` test (WhiterunWorld: 106 cells filled), and update `exal.md:148-154` with this measurement. Confirm visually on Whiterun gate approach. Check the SE-only `WhiterunDragonsreachWorld` and `KatariahWorld` worlds separately: they have no LAND of their own, so any gap there is the loader's.

### Dimension 5 — Archives + corpus gates
No findings. No `crates/bsa/src/archive` change; #6517 verified. All real-data sweeps pass (below). The 10-27 SKY-D5-01 silent-pass is gone: the LE sweep ran this time without `BYROREDUX_SKYRIMLE_DATA` set and reported 132,755 extracted.

---

## Shader-Type Coverage Matrix

Unchanged from `AUDIT_SKYRIM_2026-10-27.md` (parse path byte-unchanged; `parse_shader_type_data` arms, `shader_tests/skyrim.rs` and the per-block baselines are green). Summary: types 1/5/6/7/11/14/16 carry `ShaderTypeData` (`EnvironmentMap`, `SkinTint`, `HairTint`, `ParallaxOcc`, `MultiLayerParallax`, `SparkleSnow`, `EyeEnvmap`) and are parse-, import- and render-complete (type 16 since #5961/#6214/#6261). Types 0/2/3/4/8-10/12/13/15/17-20 map to `None` and read no trailing data. Block counts were not re-measured.

## Cell-Load Regression Status

- **TES5 cells through the unified walker**: `parse_real_skyrim_esm` green (finds `SolitudeWinkingSkeever`, 92-byte XCLL). Walker delta since baseline is FO3/FNV/FO4 and the CRVA decode only. The one Skyrim gap is SKY-2026-10-31-D4-01 (city worldspaces, exterior).
- **Whiterun BanneredMare control bench / DLC repro (`Forelhost01`) / `m41-equip.sh skyrim`**: not run (no engine launch). #5959 still open.

## Tests run (repo `target/`, `CARGO_INCREMENTAL=0`)

| Command | Result |
|---|---|
| `cargo test --release -p byroredux-nif --test parse_real_nifs -- --ignored skyrim` | `parse_rate_skyrim_se`, `parse_rate_skyrim_le` ok |
| `… --test per_block_baselines -- --ignored skyrim` | SE + LE ok |
| `… --test block_coverage_baselines -- --ignored unknown_ceiling_skyrim_se` | ok |
| `… -p byroredux-nif --lib` | 1,449 passed, 1 ignored |
| `… --lib -- --ignored packed_sse_indices_match_partition_palette_expansion_on_real_data` | ok |
| `… -p byroredux-bsa --test bsa_real -- --ignored` (SE census, SE Meshes0 brute force, LE full extract) | SE 23 archives / 172,961 entries, 0 errors; Meshes0 18,862 NIFs, 0 errors; LE 9 / 132,755, 0 errors |
| `… -p byroredux-plugin -- --ignored` (`skyrim_bodt_body_templates_decode_their_biped_masks`, `parse_real_skyrim_esm`, `skyrim_npc_wnam_skin_census_matches_the_byte_walk`, `ms01_eltrys_authored_placement`) | all ok |
| `… -p byroredux --bin byroredux -- prebaked facegen npc_spawn ragdoll_pose skyrim armor` | 257 passed, 24 ignored |

Censuses (Python, scratchpad `wrld_census.py` / `land_cmp.py` / `fill.py` under the session scratchpad): Skyrim.esm and DLC WRLD `WNAM`/`PNAM`, per-world CELL/LAND counts, VHGT co-location.

Suggest: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-10-31.md` (label `game:skyrim` + `legacy-compat`, plus `esm-plugin` / `exterior`).
