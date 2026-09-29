# Skyrim (SE + LE) Compatibility Audit — 2026-09-29

**HEAD**: `9fcfdc3fc` · **Baseline**: `docs/audits/AUDIT_SKYRIM_2026-09-22.md` (HEAD `ee6d3fb39`) · **Audited**: Dim 1 (BSTriShape packed geometry + SSE reconstruction), Dim 2 (shader-type dispatch + Skyrim material slice), Dim 3 (NPC equip + FaceGen), Dim 4 (multi-master load order + TES5 cell load), Dim 5 (archives + corpus gates) · **Unchanged since baseline (skimmed)**: none. Every dimension had commits since 09-22 and was audited in delta mode against them.

Run context: one leg of `/audit-suite --preset comprehensive`. All five dimensions were analysed synchronously in this session, with no sub-agents. Per-dimension scratch is at `/tmp/audit/skyrim/dim_{1..5}.md`, and this report is reconciled against all five. Constraints: no engine launch and no GPU process, so the Whiterun control bench, the DLC repro and the live render trace were not re-driven. Real-data evidence comes from single named `--ignored` tests and byte scans of the installed SE/LE data.

---

## Executive Summary

Skyrim SE is still the renderer control bench, and **its parse and archive layers stay fully green**:
- SE NIFs: 33,468 / 33,468 clean across 8 archives.
- LE NIFs: 22,466 / 22,466 clean.
- v105 LZ4-frame BSA: brute-force extract has 0 errors, and `declared_size` matches extract on all 19,443 `Meshes0` entries.
- `Skyrim.esm` parse: 590 cells, 590/590 Skyrim XCLL.
- DIAL QNAM ownership: 117 MS01 topics carry `quest_refs`.

The SSE packed-bone-index trap (no palette remap) holds on real data: 19,606 weighted lanes, 0 changed. The #4622 block-boundary hard error added to `BsTriShape::parse` drops no vanilla Skyrim mesh.

The window's new Skyrim-facing surface is gameplay content flowing through shared mechanisms. That is where this cycle's findings are:
- **The P3 player body is headless on Skyrim** (SKY-D3-2026-09-29-01, MEDIUM). Vanilla ships no FaceGen for the player record, and the prebaked spawn path has no head fallback. Both docs claim the miss "leaves the race-default head".
- **#4814's Starts Dead decode fires on Skyrim, which is the FNV-D2-01 contrast: 1,151 corpses in `Skyrim.esm` are decoded.** But the authored corpse pose (`XRGD`, on 1,445 of those corpses across Skyrim + Dawnguard + Dragonborn) is never read. Every placed corpse ragdoll-collapses from its spawn pose (SKY-D4-2026-09-29-01, MEDIUM).
- The ROADMAP compat matrix still lists the pre-#3712 Skyrim SE count and has no Skyrim LE row (SKY-D5-2026-09-29-01, LOW).

Regression coverage: 0 regressions. Two matched-existing items are re-verified and unchanged: #4256 (MEDIUM, `shader_type` discriminator still not canonical) and #4628 (LOW, `per_block_baseline_skyrim_se` red from data drift, identical `BSDynamicTriShape 21140 -> 21054`). One prior cross-cut HIGH is now closed: NIFAL-D1-2026-09-21b-01 / #4632, Skyrim `.btr` MSN normals now shade model-space.

**Skyrim-relevant findings filed today by sibling audits, cited rather than re-filed:**
- LC-D3-01 (AUDIT_LEGACY_COMPAT): DIAL `DATA` byte 0 is Topic Flags on Skyrim; the category is byte 1. The MS01 real-data run prints `type=0` for every topic, consistent with it.
- GAME-D2-2026-09-29-01: topic ownership ignores speaker/category; Eltrys owns all 117 MS01 topics.
- ESM-2026-09-29-D2-03: the DIAL quest-ownership docs name only `QSTI`.
- SAVE-D5-02 / SAVE-D1-02 (AUDIT_SAVE): the Helgen cart cinematic survives load; alias-injected factions (MQ101).
- SCR-D4-01 (AUDIT_SCRIPTING): scripted Enable of the Helgen keep actors.
- UI-D1-2026-09-29-01 / #4757: `canonical_menu_name` mixes Skyrim/FO4 menu names.
- AUD-2026-09-29-D5-03 (#4743 residual): the Draugr swing is silent during an active take.
- REN-D10-2026-09-29-01 (HIGH): the back-light lobe self-shadows, including on Skyrim SLSF2 Back_Lighting materials.
- PAR-D1-2026-09-29-01 (MEDIUM): the HKX frame bound is bypassable; Skyrim-only on the main thread.
- The mid-life gear-import set touching the Skyrim P3 route: GAME-D1-2026-09-29-01/-02/-03, PERF-D7-2026-09-29-02, REN-D5-2026-09-29-02, CONC-D4-2026-09-29-01, CHAR-2026-09-29-D4-02.
- FNV-2026-09-29-D2-01: `XRGD` as the FO3/FNV corpse marker. Skyrim evidence is in Dim 4.

**Totals: 3 NEW** (0 CRITICAL, 0 HIGH, 2 MEDIUM, 1 LOW) · 0 regressions · 2 matched-existing (open, unchanged).

---

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction

**Clean — 0 findings.**

- `b7491072f` ("Refactor code structure…") actually lands the #4622 fix (CLOSED). `BsTriShape::parse` now takes `block_size`, and the #621 derived-stride override must fit the block. A new hard `UnexpectedEof` ("BSTriShape geometry exceeds block boundary") fires if `num_vertices*stride + num_triangles*6` overruns the block. `parse_dynamic` / `parse_sub_index` pass the full size; `parse_lod` subtracts the 12-byte LOD triplet, which only affects FO4. Facegen `BSDynamicTriShape` (`data_size == 0`) skips the payload check by construction. Risk checked against the corpus: no vanilla drop (SE 33,468 / LE 22,466 clean).
- `1ab08644a` (#4617) adds tangent pre-size tests only. `b9e961eeb` / `4039a1f3a` are not Skyrim-reachable.
- Checklist:
  - `widen_packed_bone_indices` (`crates/nif/src/import/mesh/skin.rs:551`) is the sole path, and `remap_bs_tri_shape_bone_indices` / `remap_one` have 0 hits.
  - The `BSLODTriShape` → `NiLodTriShape::parse` / `BSMeshLODTriShape` → `BsTriShape::parse_lod` split is intact (`blocks/mod.rs:484/489`).
  - The specialty arms are present: BSTreeNode, BSPackedCombined[Shared]GeomDataExtra, BSLagBoneController, BSProceduralLightningController.
- Tests:
  - 228 passed: `-p byroredux-nif --lib -- bs_tri_shape sse_recon sse_skin_geometry tangent_convention alpha_flag bs_lod_tri_shape tri_shape_skin_vertex tangent_presize dispatch_tests`.
  - Real-data `packed_sse_indices_match_partition_palette_expansion_on_real_data` → ok (19,606 lanes, 0 changed).

### Dimension 2 — Shader-Type Dispatch + Skyrim Material Slice

**Clean — 0 new; 1 matched-existing.**

No commits to `crates/nif/src/blocks/shader/` or `shader_tests/` since baseline, so the Skyrim wire decode and the `parse_shader_type_data` dispatch are unchanged. Window commits on the import/translate side, and why each leaves Skyrim alone:
- `d3e043d3e` (#4282/#4283): Skyrim `parse_skyrim` writes `wetness/luminance: None`, and inline Skyrim shaders set neither `external_material_resolved` nor `from_bgsm`.
- `2b1b7fc5c` (#4855/#4856): `lit_carrier_authored_dispatch = (2..=20).contains(&kind) && !from_bgsm` is intact (`byroredux/src/helpers.rs:171`).
- `2f8538334`: `pbr_classified_at_import` behaves the same as the old `metalness_override.is_some()` for NIF-keyword materials.
- `ba094b93e` (#4279): Skyrim's CRC arrays are empty, so this reduces to the typed SLSF1 Own_Emit bit.
- `7ddc1b03c` / `2935e9da3` (#4632): these close the Skyrim `.btr` MSN cross-cut.
- `9ad808374` (#4553): the `.btr` texture-sampling residual is open as #4912, owned by /audit-exterior.
- `b978bb5a1` (#4973): IOR floor; Skyrim authors no IOR.

PBR lobe: `material_flag::PBR_BSDF` is set only by `cornell.rs` in production, so vanilla Skyrim has 0 instances. #4422 `detail_neutral` and #4423 `TINT_ALPHA_WEIGHT_BIT` are present, and `GpuMaterial` is still 432 B.

Tests:
- 124 passed: `-p byroredux-nif --lib -- skyrim shader_type_data_tests emissive_source_tests lighting_shader_pbr_tests lighting_shader_mat_tests effect_shader_capture_tests msn_basis_pin`.
- 224 passed / 9 ignored: `-p byroredux --bin byroredux -- glass_classification material_translate npc_spawn:: scene::nif_loader`.

#### SKY-D7-2026-09-11-02 — `ImportedMaterial.shader_type` never crosses the NIFAL boundary
- **Severity**: MEDIUM
- **Dimension**: 2
- **Location**: `byroredux/src/material_translate.rs` (0 hits for `source.shader_type`); raw-tier read-back at `byroredux/src/cell_loader/spawn/mesh_instance.rs:317`
- **Status**: Existing: #4256 (OPEN). Re-verified this session: unregressed and unchanged.
- **Suggested Fix**: as filed: a canonical `source_shader_type` on `Material`.

### Dimension 3 — NPC Equip + FaceGen (M41)

**1 NEW (MEDIUM).** Commits reviewed (28), grouped by what they touch:
- **P3 player body** — `a070baaad`, `db8351587`, `0182fc5e8`. The mid-life gear defects are sibling-filed (see summary).
- **Skyrim Draugr combat marker** — `3978b5184` (#4700 CLOSED). Residual: AUD-D5-03.
- **TPLT terminals** — `e52d4a8a1` (#4812/#4696) and `3748f4cb1` (#4457); their guards pass.
- **`5570c221c`** — the RACE head-part ICON textures and `seam_blend.rs` are Oblivion/FO3/FNV only. `is_dismemberment_cap` covers only body parts 101..=113 and 201..=213; Skyrim SBP 130..=161 and 230..=261 are excluded, which is correct per nif.xml.
- **EGM/EGT/TRI decode** — `4039a1f3a` / `4b1fe9c4c`. Not on Skyrim's prebaked path.

Checklist items re-verified:
- Skin-first plus the occupancy filter.
- #4421 FaceTint-only tint (`scene/nif_loader.rs:38-59`).
- #3409 head partition hiding on the prebaked FaceGen phase.

Tests:
- 125 passed: `-p byroredux-plugin --lib -- equip_template_tests equip:: actor::`.
- Real-data `real_skyrim_bleak_falls_draugr_expands_to_one_weapon_leaf` → ok.

#### SKY-D3-2026-09-29-01: Skyrim's third-person player body has no head — the prebaked FaceGen miss skips the head entirely, but the module doc and the slice doc say it "leaves the race-default head"
- **Severity**: MEDIUM
- **Dimension**: 3 — NPC equip + FaceGen (player body through the prebaked spawn path)
- **Location**:
  - `byroredux/src/player_body.rs:28-33` (module doc) and `:168-176` (prebaked job for the player).
  - `byroredux/src/npc_spawn/resumable.rs:188-193` (`skip_missing_facegen`) and `:196-202`: `PrebakedPhase` runs Skeleton → Facegen → Armor → Finalize, with no head fallback.
  - `resumable.rs:1941-1957` (the Facegen phase).
  - `docs/engine/playable-vertical-slice.md:1295-1296`.
- **Status**: NEW. `gh` searches for player FaceGen / head found nothing. GAME-D1-2026-09-29-03 covers the same doc paragraphs' gear-import clause, not this one.
- **Description**:
  - On Skyrim the player is assembled through `NpcSpawnJob::prebaked`. Its only head source is `facegeom\skyrim.esm\00000007.nif`, and vanilla ships none: a byte scan of `Skyrim - Meshes0.bsa` / `Meshes1.bsa` finds 0 occurrences of `00000007.nif`, while other facegeom names are present.
  - `skip_missing_facegen` then jumps to `PrebakedPhase::Armor(0)`. The race-default head machinery (`head_part_path` / `head_part_entries`) is called only from `prepare_runtime_state`, the runtime-recipe path.
  - Skyrim's RACE.WNAM skin ARMO covers body, hands and feet but not the head. The third-person player therefore has no head, eyes, hair or brows.
  - Both docs describe a different outcome: `player_body.rs` says "the graceful miss leaves the race-default head", and the slice doc (1295-1296) says the same. Later paragraphs (1322, 1354) call it only "a graceful data miss".
  - The Skyrim-only gate `p3-player-body.sh` asserts meshes > 0, skeleton, part stamps, first-person hiding and the walk clip. It never checks for a head, so it stays green on a headless body.
- **Evidence**: the phase list and skip in `resumable.rs:188-202`; the BSA scan above. The data a fallback needs is already parsed: Skyrim NPC_ `PNAM` head parts are captured into `face.head_parts` (`captures_fo4_face = game.uses_prebaked_facegen()`, `crates/plugin/src/esm/records/actor/mod.rs:992, 1461-1464`), and HDPT records sit in `index.head_parts`.
- **Impact**: every Skyrim third-person view of the player shows a headless body. That is the P3 slice route. FO4's player takes the same path, but only Skyrim is gated. The docs present P3 as done except for "player FaceGen", which understates what is actually visible.
- **Related**: GAME-D1-2026-09-29-03; `playable-vertical-slice.md` P3 "player FaceGen" open item.
- **Suggested Fix**: on a prebaked FaceGen miss, assemble a runtime head from the NPC's `PNAM` head parts, falling back to the RACE default HDPTs (via `index.head_parts`) with the race skin tint. Or, at minimum, correct both docs to "no head" and add a head-mesh assertion to `p3-player-body.sh`.

### Dimension 4 — Multi-Master Load Order + TES5 Cell Load

**1 NEW (MEDIUM).**

Changes in the window:
- `382fa9296` (#3813): parallel walk with ordered fold. Guard `parallel_walk_matches_the_inline_walk_on_a_localized_master_chain` → ok.
- `f87490826` (#4813/#4814/#4820): Initially Disabled / Starts Dead.
- `9d41ecb66` (#4639): Skyrim is light only when HEDR ≥ 1.6 and flag 0x200 is set. So SE is light and LE is not. The HEDR-vs-game-mode keying was already weighed by AUDIT_ESM_2026-09-29 and is not re-raised.
- `728f1c435` (#4644 GRUP skip clamp), plus LTEX/IMGS/merge hardening.

Real-data checks:
- `parse_real_skyrim_esm`: 590 cells, 18,318 statics (+74 vs baseline; growth, not loss), 37 worldspaces, Winking Skeever 981 refs, 590/590 extended XCLL.
- `ms01_eltrys_authored_placement`: QNAM → `quest_refs` populated on the sampled MS01 topics.

Unit tests: `esm::cell esm::reader` 225 passed; `cell_loader::load_order load_order_parallel reference_state` 26 passed.

**Skyrim evidence for FNV-2026-09-29-D2-01.** On Skyrim the ACHR header bit is the authored corpse signal, and #4814 decodes it correctly: xEdit/UESP TES5 ACHR flag 9. The census:

| Plugin | ACHRs | 0x200 (Starts Dead) | 0x800 (Initially Disabled) |
|---|---|---|---|
| `Skyrim.esm` | 10,504 | 1,151 | 1,390 |
| `Dawnguard.esm` | 908 | 144 | 78 |
| `Dragonborn.esm` | 1,309 | 212 | 60 |

The FNV marker defect does not reproduce here, but the pose half does (below).

#### SKY-D4-2026-09-29-01: Skyrim's authored corpse poses (`XRGD`) are never decoded — 1,445 "Starts Dead" actors ragdoll-collapse from their spawn pose instead of lying where authored
- **Severity**: MEDIUM
- **Dimension**: 4 — TES5 cell load (Skyrim data through #4814)
- **Location**:
  - `crates/plugin/src/esm/cell/walkers.rs:1190-1195`: only the header bit is decoded, and there is no `XRGD` arm.
  - `byroredux/src/cell_loader/reference_state.rs:326-332` (`apply_starts_dead`).
  - `byroredux/src/combat.rs:543-619`: `reconcile_dead_actor` calls `activate_ragdoll`, seeded from the current bone globals.
- **Status**: NEW. The sub-record is shared with FNV-2026-09-29-D2-01, which files XRGD as the FO3/FNV dead *marker*. This is Skyrim's *pose* half. `gh` search "XRGD" finds no issue.
- **Description**:
  - Skyrim authors a placed corpse as ACHR 0x200 plus an `XRGD` ragdoll pose (xEdit `wbRagdoll`, 28 bytes per bone entry). Only the bit is decoded.
  - `apply_starts_dead` inserts `Dead` and queues reconciliation. `reconcile_dead_actor` then builds the multibody from the freshly spawned skeleton's bind/idle pose.
  - `grep -rln XRGD crates byroredux --include='*.rs'` → 0 files.
  - Every authored corpse therefore starts upright and falls, instead of lying slumped, draped over a table, hanging or impaled as authored.
- **Evidence**: byte census of the installed SE masters (24-byte TES5 headers; compressed bodies inflated):
  - XRGD is on 1,097 of 1,151 Starts Dead ACHRs in `Skyrim.esm`, 140 of 144 in `Dawnguard.esm` and 208 of 212 in `Dragonborn.esm`: 1,445 in total.
  - Every XRGD length is a multiple of 28.
  - No non-Starts-Dead ACHR carries XRGD.
- **Impact**: every vanilla authored corpse loses its placement. Bodies on ledges, tables or gibbets can move or fall, which displaces quest-clue and loot containers. It persists across save/load, because the ragdoll re-seeds the same way.
- **Related**: FNV-2026-09-29-D2-01; #4814 (CLOSED); PHYSAL `activate_ragdoll`.
- **Suggested Fix**: decode `XRGD` into `PlacedRef` as a per-bone `(bone_id, position, rotation)` list, cited to xEdit `wbRagdoll`. Pose the skeleton from it before `activate_ragdoll`, or spawn the corpse asleep in that pose. The same decode serves the FNV marker fix.

### Dimension 5 — Archives + Corpus Gates

**1 NEW (LOW); 1 matched-existing.**

Changes in the window:
- `1b8b21f3f`: positional reads.
- `dbc1c88c2`: `declared_size`.
- `6c423fc4a` (#4671): BSA full-path normalisation plus a duplicate count.
- #4670 / #4666 / #4999 / #5000 follow-ups.

None changes the v105 layout or its LZ4-frame codec.

Runs:
- `parse_rate_skyrim_se` 33,468 / 33,468 clean across 8 archives: Meshes0 18,862, Meshes1 13,847, _ResourcePack 149, CC 231/266/65/4, Animations 44.
- `parse_rate_skyrim_le` 22,466 / 22,466.
- `per_block_baseline_skyrim_le` OK (146 types).
- `byroredux-bsa --lib` 108 passed.
- v105 magic + brute-force extract: 18,862 NIFs, 1,961.8 MB, 0 errors.
- `declared_size_matches_extract_across_bsa_versions`: `Skyrim - Meshes0.bsa` 19,443 files, 0 mismatches.
- `archive_siblings` 10 passed.

#### SKY-2026-09-22-D5-01 — `per_block_baseline_skyrim_se` drift
- **Severity**: LOW
- **Dimension**: 5
- **Location**: `crates/nif/tests/data/per_block_baselines/skyrim_se.tsv`
- **Status**: Existing: #4628 (OPEN). This session's failure, `PARSED shrank BSDynamicTriShape 21140 -> 21054`, is byte-identical to the baseline's: archive-rewrite data drift, not parser loss.
- **Suggested Fix**: regenerate the baseline, as filed.

#### SKY-D5-2026-09-29-01: ROADMAP's compat matrix and game-compatibility.md still give Skyrim SE as 33,424 NIFs across 7 archives and have no Skyrim LE row; the gate measures 33,468 across 8, plus 22,466 LE
- **Severity**: LOW (doc-rot)
- **Dimension**: 5 — Archives + corpus gates (reporting)
- **Location**: `ROADMAP.md:205` (compat matrix), `ROADMAP.md:688` (stats line "Skyrim SE 33 424"), `docs/engine/game-compatibility.md:92`
- **Status**: NEW. NIF-D3-2026-09-29-02 refreshes `nif-parser.md` (which already says 33,468) and the FO76/FO4 rows, not these lines.
- **Description**: #3712 added `Skyrim - Animations.bsa` (44 NIFs) to `Game::optional_mesh_archives` (`crates/nif/tests/common/mod.rs:229-233, 266`). The gate now sweeps 8 archives and 33,468 NIFs; the delta is exactly those 44. The authoritative matrix still says "33 424 across 7 archives (2026-08-29)". It also has no Skyrim LE row, although `parse_rate_skyrim_le` has gated 22,466 LE NIFs at 100% since `fb8173fe0`.
- **Evidence**: this session's `parse_rate_skyrim_se` / `_le` output against the quoted lines.
- **Impact**: cosmetic. The authoritative matrix gives a stale SE count and does not record the LE gate.
- **Related**: NIF-D3-2026-09-29-02, #3712.
- **Suggested Fix**: in the next `/session-close`:
  - Set the SE row and the stats line to 33,468 / 8 archives.
  - Add a Skyrim LE row: BSA v104 zlib, 22,466 / 22,466, 1 archive.

---

## Shader-Type Coverage Matrix

`ShaderTypeData` arms for Skyrim (BSVER 83/100). The parse code has not changed since the 09-22 baseline. The only row changes are closed cross-cuts noted in the render column.

| Variant | Numeric type(s) | Parse | Import (→ `ImportedMesh.material`) | Render |
|---|---|---|---|---|
| `None` (no trailing data) | 0, 2, 3, 4, 8, 9, 10, 12, 13, 15, 17, 18, 19, 20 | Complete (0 bytes; #4252 pin) | N/A (kind carried as `material_kind`) | Default lit; kinds 2..=20 protected from the glass classifier (`helpers.rs:171`) |
| `EnvironmentMap` | 1 | Complete | Complete (`env_map_scale`) | Consumed; alchemy glass → glass via the #4392 carve-out |
| `SkinTint` | 5 | Complete | Complete | Consumed; #4423 tint alpha weight |
| `HairTint` | 6 | Complete | Complete | Consumed |
| `ParallaxOcc` | 7 | Complete | Complete | Consumed |
| `MultiLayerParallax` | 11 | Complete | Complete | Consumed; ice preserved through the glass classifier |
| `SparkleSnow` | 14 | Complete | Complete | Consumed |
| `EyeEnvmap` | 16 | Complete | Complete | Consumed |
| FaceTint (no payload) | 4 | Complete | `material_kind = 4` keys the FaceGen tint override (#4421) | Consumed |
| `Fo76SkinTint` etc. | FO76 table | N/A for Skyrim (separate `parse_shader_type_data_fo76`) | — | — |

Residual: #4256. The discriminator `shader_type` is still not canonical, and the raw tier is read at `mesh_instance.rs:317`.

Related render-side issue: REN-D10-2026-09-29-01. The back-light lobe on SLSF2 Back_Lighting materials self-shadows. It is owned by /audit-renderer.

---

## Cell-Load Regression Status

- **TES5 walk**:
  - `parse_real_skyrim_esm` passes: 590 cells, 18,318 statics (+74 vs 09-22; growth, no loss), 37 worldspaces.
  - Winking Skeever: 981 refs; 590/590 cells with 92-byte extended XCLL.
  - Compressed GRUPs decompress; the #4644 skip clamp is transparent to vanilla.
- **Multi-master**:
  - #3813 parallel parse with in-order fold is guarded and green.
  - #4639: the ESL light flag is SE-only (HEDR ≥ 1.6), so LE plugins never enter 0xFE space.
  - The DLC repro (`Dawnguard.esm` / `Forelhost01`) was not re-driven (no engine launch).
- **Reference state** (new since the baseline):
  - 0x800 Initially Disabled and 0x200 Starts Dead are decoded for Skyrim: 1,151 corpses and 1,390 disabled actors in `Skyrim.esm`.
  - Corpse *pose* is not decoded (SKY-D4-2026-09-29-01).
- **DIAL**: QNAM → `quest_refs` populated (117 MS01 topics). The category byte is wrong on every Skyrim topic; see LC-D3-01, cross-ref.
- **Whiterun BanneredMare control bench: INCOMPLETE, not FAILED.**
  - This session had no Vulkan device and could not launch the engine.
  - Reference: ROADMAP Bench-of-record, 5,777 entities, 83.8 FPS / 11.93 ms (`ROADMAP.md:141`).
  - No window commit touches STAT/REFR/LIGH resolution or LAND scale for that cell.
  - The new P3 player body and gear import only add entities in Character mode, not in the bench's fly-camera run.
  - `/audit-runtime` owns the live baseline.

---

## Totals

| Severity | New | Regression | Matched-existing (open) |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 2 (SKY-D3-2026-09-29-01, SKY-D4-2026-09-29-01) | 0 | 1 (#4256) |
| LOW | 1 (SKY-D5-2026-09-29-01) | 0 | 1 (#4628) |

| Dimension | New | Matched-existing |
|---|---|---|
| 1 — BSTriShape / SSE recon | 0 | 0 |
| 2 — Shader-type / material slice | 0 | 1 (#4256) |
| 3 — NPC equip + FaceGen | 1 MEDIUM | 0 |
| 4 — Load order + cell load | 1 MEDIUM | 0 |
| 5 — Archives + corpus | 1 LOW | 1 (#4628) |

Suggested: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-09-29.md`. Label every finding `game:skyrim` + `legacy-compat`, plus a domain label:
- SKY-D3-01: `character` / `gameplay`.
- SKY-D4-01: `esm-plugin` / `physics`.
- SKY-D5-01: `doc-rot`.
