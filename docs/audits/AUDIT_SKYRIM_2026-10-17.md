# Skyrim (SE + LE) Compatibility Audit — 2026-10-17

**HEAD**: eec7e00cf (audited; `b88b30a96`, a gameplay-only locomotion-lock commit outside every dimension path, landed during the run) · **Baseline**: `docs/audits/AUDIT_SKYRIM_2026-10-12.md` (HEAD `5d31fd637`, 522 commits ago) · **Audited**: Dim 1, 2, 3, 4, 5 (all five, each delta-scoped with its checklist re-verified) plus a live control bench, the DLC repro and the M41 equip smoke test · **Unchanged since baseline (skimmed)**: none in full — Dim 1 and Dim 2 changed by one and three commits respectively (see below); Dim 5's SE/LE gates had test-only commits.

Run context:
- Five dimension agents (Dim 3, 4, 5, then 1, 2) plus orchestrator-run live legs. Per-dimension scratch notes are `/tmp/audit/skyrim/dim_{1..5}.md`.
- Live legs ran on an RTX 4070 Ti with desktop apps resident (not an idle machine; 1-min load average 3.5 at the start of the control bench and 2.2 at the end of the HEAD bench, so HEAD was not disadvantaged by ordering).
- The user's engine was not running (checked with `pgrep` before every launch). The control worktree and the temp build were removed afterwards.
- The `--ignored` plugin tests ran only by exact name, one at a time.

---

## Executive Summary

**Control bench is green on content, red on speed.** Whiterun BanneredMare loads with 5,837 entities against the record's 5,825, with identical draws (`1289/392b/24c`), TLAS instances (1,226), lights (18) and camera pose. A same-machine control (the record's commit `a3a7a43b0` rebuilt in a worktree and benched back-to-back with HEAD under the same harness) shows HEAD **7–13% slower** across all five upscaler configs, concentrated in the render-resolution GPU passes (SKY-4-02, MEDIUM). Nothing Skyrim-specific explains it; the renderer commit range since `a3a7a43b0` contains deliberate ReSTIR, SVGF and shader work.

**Parse, archive, shader-dispatch and equip layers are green at HEAD:**
- NIF corpus (release): SE 33,468 / 33,468 clean across 8 archives; LE 22,466 / 22,466. Counts identical to 2026-10-12, so no new CC archive is installed.
- Archive sweep: 65,637 files across SE `Meshes0/1` + `Textures0`–`8`, 0 errors, 0 magic mismatches.
- SSE packed bone indices: 19,606 weighted lanes, 0 changed by the importer.
- Unit runs: nif `--lib` Dim 1 filter 224 passed, Dim 2 filter 254 passed; plugin `--lib` 1,154 passed; bin `npc_spawn|equip|skin|prebaked` 193 passed, `load_order|lod|ragdoll_pose` 229 passed, `material_translate` 77 passed; bsa 114 passed; renderer `dds` 76 passed. No failures anywhere.
- Real-data tests (each alone): `parse_real_skyrim_esm`, `ms01_eltrys_authored_placement`, `skyrim_ltex_grass_links_resolve_to_real_gras_records`, `skyrim_default_water_promotes_underwater_tail`, `skyrim_bodt_body_templates_decode_their_biped_masks`, `skyrim_npc_wnam_skin_census_matches_the_byte_walk`, the packed-index real-data test, and the Skyrim BSA brute-force extract suites all pass.
- M41 equip smoke (`m41-equip.sh skyrim`): PASS — entities 5,837, draws 1,287, Inventory 46, EquipmentSlots 6, `tex.missing` 0. The floor of 6 is now met by five named NPCs plus the player body (SKY-3-01).
- DLC repro (`--master Skyrim.esm --master Update.esm --esm Dawnguard.esm --cell Forelhost01`): loads, 1,578 placed references → 3,678 entities; the five-plugin real order walks identically inline and in parallel (135,900 records, identical digest).

**Seven findings, none above MEDIUM:** one MEDIUM (performance), six LOW (three doc-rot, one untracked render gap, one log-noise/parser-hygiene, one test-gap).

**Baseline's open items, re-checked:** #5408, #5015, #4256 still open and unchanged. #4913 is closed as the skill states. #5750 (Hello topic selection, fn 426 uncatalogued) is still open and still true.

---

## Dimension Findings

### MEDIUM

### SKY-4-02: Whiterun BanneredMare is 7–13% slower than the bench-of-record's own commit, same machine, same scene
- **Severity**: MEDIUM (performance regression; no correctness impact observed)
- **Dimension**: 4 (control bench)
- **Location**: renderer range `a3a7a43b0..eec7e00cf` (`crates/renderer/shaders/`, `crates/renderer/src/vulkan/`, `byroredux/src/render/`)
- **Status**: NEW (no open issue; closed #2367 / #5117 are earlier, unrelated perf regressions)
- **Description**: Per the standing methodology the outgoing record's commit `a3a7a43b0` was rebuilt in a worktree (`--release`) and run through `scripts/fsr-bench-matrix.sh` (harness `a4ede5fa0`, `renderer-stepped/orbit`, 3 runs × 300 frames, Whiterun only), then HEAD was run the same way immediately after. The camera pose is identical to the record and to each other (`177.223,188.000,1048.424`).
- **Evidence** (medians; ms is wall frame time):

  | Config | Control `a3a7a43b0` | HEAD `eec7e00cf` | Δ |
  |---|---:|---:|---:|
  | TAA native | 83.8 FPS / 11.94 ms | 74.3 FPS / 13.46 ms | +1.52 ms (+12.7%) |
  | FSR native-AA | 79.9 / 12.52 | 72.6 / 13.77 | +1.25 ms |
  | FSR Quality | 131.2 / 7.62 | 120.4 / 8.30 | +0.68 ms (+8.9%) |
  | FSR Balanced | 157.7 / 6.34 | 143.8 / 6.95 | +0.61 ms |
  | FSR Performance | 189.7 / 5.27 | 178.2 / 5.61 | +0.34 ms (+6.5%) |

  Per-pass (TAA): `gpu_main` 11.161 → 12.546 ms (+1.385), `gpu_svgf` 0.506 → 0.856 (+0.350, +69%), `gpu_volumetrics` 0.818 → 0.902, `gpu_volumetrics_inject` 0.790 → 0.877. At Quality: `gpu_main` +0.664, `gpu_svgf` 0.265 → 0.443. Entities 5,825 → 5,837, draws / TLAS / lights equal. Run-to-run spread is ±0.01–0.35 ms, well below the delta. An earlier HEAD-only pass the same hour agreed (TAA 76.3 FPS / 13.10 ms, Quality 122.0 / 8.20).
- **Impact**: About 1.5 ms of a 12 ms frame at TAA native on the project's Skyrim control scene; the record in ROADMAP (92.4 FPS TAA) is no longer reproducible (today's control build itself reads 83.8, so the machine is also ~9% slower than at record time — only the same-session A/B is meaningful).
- **Related**: ROADMAP bench-of-record; `/audit-performance`, `/audit-renderer`. Candidate commits in the range (not bisected): `3f8194117` (SVGF progressive accumulation no longer waits for every NPC), `1092b5b9e` / `24eb7dba8` (SVGF history age / invalidation), `c2647d468`, `17e0b7464`, `6110b6078`, `2e7d5bead` (ReSTIR reservoir ownership / temporal reuse), `triangle.frag` +253 lines with `triangle.frag.spv` 432,576 → 444,880 B, and a new `shadow_filter.comp` pass (its GPU time is not in the harness columns, so it is unmeasured here).
- **Suggested Fix**: Bisect `a3a7a43b0..eec7e00cf` on this exact harness (the control worktree recipe above takes ~2 minutes per build); decide per commit whether the cost is the intended price of the feature. Refresh the bench-of-record once the number is understood.

### LOW

### SKY-5-01: Every Skyrim `bhkRagdollConstraint` / `bhkLimitedHingeConstraint` parses 1 byte short and logs a WARN per NIF load
- **Severity**: LOW (by design, but contradicts the skill's "0 realignment WARNs" corpus expectation and floods the log)
- **Dimension**: 5 (corpus gates) / NIF constraint parsing
- **Location**: `crates/nif/src/blocks/collision/constraints.rs:156-162` and `:775-784` (motor intentionally not consumed); warning emitted at `crates/nif/src/lib.rs:812-820`
- **Status**: NEW (closed #1605 is the earlier "constraint decoders hidden from the drift detector" issue)
- **Description**: The FO3+/Skyrim constraint CInfo parsers deliberately stop before the trailing `bhkConstraintMotorCInfo` and rely on the block-size table to realign. On Skyrim the motor type byte is the single missing byte on every instance, so the parse-end summary WARN fires for every actor NIF that carries a ragdoll.
- **Evidence**: `nif_stats --drift-histogram` on SE `Skyrim - Meshes0.bsa`: 1,462 events (769 `bhkLimitedHingeConstraint` + 693 `bhkRagdollConstraint`), all `drift=+1`; `Meshes1.bsa`: 113 events (54 + 59), all `drift=+1`. Totals 823 + 752 equal the `bhkLimitedHingeConstraint` / `bhkRagdollConstraint` counts in `crates/nif/tests/data/per_block_baselines/skyrim_se.tsv` — 100% of Skyrim constraint blocks, none exempt. In the live Forelhost01 run the same WARN fired 5 times (17 blocks per Draugr/Character skeleton: `bhkRagdollConstraint=9, bhkLimitedHingeConstraint=8`).
- **Impact**: No data is lost (block-size realignment is exact and the decoded prefix is what PHYSAL consumes). But a `100% clean` corpus rate does not see drift, any WARN on a Skyrim run is expected noise, and a genuine new drift in these two types would be invisible among 1,575 expected ones. The Dim 5 agent's "no realignment WARNs" statement was measured at the corpus-test level, which does not count drift; this finding corrects it.
- **Related**: #1605, #565 (drift aggregation), #939 (`--drift-histogram`).
- **Suggested Fix**: Consume the motor type byte (and its payload when non-`NONE`) so the histogram reads zero and a real regression becomes visible; or demote the aggregated WARN to debug for these two types and gate the drift histogram in the Skyrim baselines.

### SKY-2-02: EyeEnvmap (shader type 16) has no shading branch and no open tracker; MultiLayerParallax `refraction_scale` is unread
- **Severity**: LOW (visual-fidelity gap; eyes still render through the generic lit path)
- **Dimension**: 2
- **Location**: `crates/renderer/shaders/triangle.frag:1602-1606` (stub), `byroredux/src/render/static_meshes.rs:996+` (pack), `crates/renderer/shaders/include/bindings.glsl:155-156`
- **Status**: NEW as a tracker. Closed #619 (SK-D3-05) was closed by `19e7115`, which only gated the CPU pack by `material_kind`; its own comment says "shader stub today". The 2026-10-12 matrix listed type 16 as "Consumed", which overstated it.
- **Description**: `eyeLeftCenter` / `eyeRightCenter` / `eyeCubemapScale` are parsed (28 B, `lighting.rs:895-913`), imported, packed into the GPU material and instance, and never read in GLSL. The stub says it "requires a per-instance cubemap descriptor binding that doesn't exist yet". `refraction_scale` of type 11 is likewise packed but unread.
- **Impact**: About 3,251 SE / 2,628 LE eye blocks lack the authored iris-centred reflection; they fall back to the generic environment / RT reflection path. Cosmetic only.
- **Related**: #619, #623, #562 (all closed).
- **Suggested Fix**: Track as an enhancement. A per-eye reflection could reuse the existing RT reflection path rather than a new cubemap binding. Until then label the matrix "packed, not shaded".

### SKY-3-01: The M41 smoke test's equip floor of 6 is now met by five NPCs plus the player body
- **Severity**: LOW (test-gap)
- **Dimension**: 3
- **Location**: `docs/smoke-tests/m41-equip.sh:192-199` and `:248-252` (floor and comment)
- **Status**: NEW
- **Description**: The floor of 6 was set (#1560) for six named Bannered Mare residents — saadia, brenuin, mikael, sinmir, amaundmotierreend, hulda. Amaund Motierre's only placement is `ACHR 0004FA2F`, base `0004E64F`, header flags `0x00000C00`, so bit `0x800` (Initially Disabled) is set; since `f87490826` (#4813) he is correctly withheld. The sixth `EquipmentSlots` entity is now the player body (entity 5570, which carries `ActorValues`, `Perks`, `RigidBodyData` and `EquippedWeapon`; the engine log shows `Player body: assembled from NPC_ 00000007`).
- **Evidence**: smoke run: `EquipmentSlots=6` — entities 2 "saadia", 246 "brenuin", 482 "mikael", 747 "sinmir", 1022 "hulda", 5570 (player). Engine log lists `NPC` equip lines for Saadia, Hulda, Brenuin, Mikael, Sinmir and the Player only. Entity 737 (the unnamed entity with `Inventory` but no `EquipmentSlots`) is a container holding 100 gold, not an NPC.
- **Impact**: A regression that drops one NPC's gear would still pass: the guard has lost one NPC of margin.
- **Suggested Fix**: Count only entities without player components (or lower the floor to 5 NPCs and assert the player separately), and correct the comments to five residents.

### SKY-1-01: `tangent.rs` comments still describe the removed Z-up copy; the wrapper's "bit-identical" claim is wrong at the float32 last-bit level
- **Severity**: LOW (doc rot)
- **Dimension**: 1
- **Location**: `crates/nif/src/import/mesh/tangent.rs:136-147`, `:307-313`, `:385-390`
- **Status**: NEW (parents #5793 / #2828 / #3176 / #2632 closed). Re-read by the orchestrator; the stale text is present.
- **Description**: After #5793 (`89a0b6ccd`) there is one tangent producer. (1) `:307-313` still describes a "documented divergence" from the Z-up flavor's permutation. (2) `:389` says "both producers' comments". (3) `:138-139` claims bit-identical output to the old path, but the per-triangle normalize now sums `x²+z²+y²` rather than `x²+y²+z²`; float32 addition is not associative, so results can differ by about 1 ulp (the agent measured 22,786 of 100,000 random triples differ).
- **Impact**: Documentation only. The risk is an exact-equality golden test written on the strength of "bit-identical".
- **Suggested Fix**: Delete the divergence paragraph; "both producers'" → "the producer's"; reword to "mathematically identical (exact component permutation); float32 may differ in the last ulp".

### SKY-2-01: Stale shader-type comments — "ride unused", wrong enum names in the ladder header
- **Severity**: LOW (doc rot)
- **Dimension**: 2
- **Location**: `crates/nif/src/import/material/shader_data.rs:92-100`; `crates/renderer/shaders/triangle.frag:1530-1539`, `:1596-1601`
- **Status**: NEW (parents #344 / #562 / #619 closed). The "SK-D3-02 … ride unused" text at `shader_data.rs:94` and the ladder header were re-read by the orchestrator.
- **Description**: (1) `apply_shader_type_data` says the values "ride unused on the Material component"; kinds 5/6/14 are consumed at `triangle.frag:1547-1574`, kind 11 at `:1430` / `:2991` and `ray_hit.glsl:537`, parallax at `triangle.frag:305-306` and `ray_hit.glsl:322-347`. (2) The ladder header names "8 MultiIndexSnow … 19 MultiTexture"; nif.xml `BSLightingShaderType` (`nif.xml:1402-1422`) has 8 = Multitexture Landscape, 10 = Snow, 19 = Multitexture Landscape LOD Blend. (3) The type-1 bullet misstates the gate (`mat.envMapIndex != 0u || mat.materialKind == 1u` at `:2960-2961`). (4) The kind-11 stub note says the cubemap path is still needed, but it already feeds `environmentStrength` at `:2990-2992`.
- **Impact**: Misleads the next reader into thinking variants are unconsumed or mapping 8/10/19 wrongly.
- **Suggested Fix**: Rewrite the doc to name the live consumers; correct the ladder names to nif.xml; fix the type-1 bullet; trim the kind-11 note to refraction / Fresnel coupling.

### SKY-4-01: The `esm/cell` split (`d7c47d741`) and #5463 left stale source / doc pointers and a misplaced doc comment
- **Severity**: LOW (doc rot)
- **Dimension**: 4
- **Location**: `crates/plugin/src/esm/cell/helpers.rs:328-346`; `byroredux/src/cell_loader/terrain_seam.rs:7`; `docs/engine/esm-records.md:277`; `CLAUDE.md:193`
- **Status**: NEW (same class as closed #5313 / #1054 / #1059). All four sites were re-read by the orchestrator.
- **Description**: (1) `3b28dca71` inserted `xccm_targets_region` between `read_form_id` and its doc block, so the `read_form_id` doc ("Read a 4-byte FormID…", #3314) now runs into the XCCM doc and attaches to the wrong item. (2) `terrain_seam.rs:7` cites `cell/mod.rs:165-167`; the LAND orientation text now lives at `cell/landscape.rs:38`. (3) `esm-records.md:277` points to `cell/mod.rs` for the `CellData` struct, now in `cell/cell_data.rs`. (4) `CLAUDE.md:193` lists the cell submodules as "helpers / support / walkers / wrld"; the split added `base_objects`, `cell_data`, `index`, `landscape`, `lighting`, `placed_ref`, `worldspace`.
- **Impact**: Documentation only; the `include_str!` source-scan guards were verified live, not vacuous.
- **Suggested Fix**: Move `xccm_targets_region` above the `read_form_id` doc block; retarget the other three pointers.

---

## Per-dimension status

| Dim | Delta since baseline | Result |
|---|---|---|
| 1 BSTriShape + SSE recon | `89a0b6ccd` (#5793) only | Clean; SKY-1-01 (LOW doc). Packed bone indices still global, `decline_unbounded_packed_indices` still declines never clamps, `BSLODTriShape` vs `BSMeshLODTriShape` routing intact. |
| 2 Shader dispatch + material | `b99db0f11` (#5737), `d927ec8f5`, `8bd4f3b8b` | Clean at the byte level; SKY-2-01, SKY-2-02 (LOW). Skyrim `parse_shader_type_data` unchanged; FO4 arms 1/5 keep their extra fields; FO76 numbering still separate. |
| 3 NPC equip + FaceGen | ~40 commits (shared finalize tail #5794, death teardown, remove-unequips, Skyrim morality / skills) | No regression; SKY-3-01 (LOW test-gap). Finalize tail kept every step on the prebaked arm. FaceTint-only override, female male-model fallback, WNAM chain, BODT gate and TPLT chain all untouched or guarded. |
| 4 Load order + TES5 cell load | `d7c47d741` split, `0b3ae0d13`, `d6d216b91`, `3b28dca71`, others | No behavioural regression; SKY-4-01 (LOW doc), SKY-4-02 (MEDIUM perf, control bench). Split is a pure move (line-multiset compare). |
| 5 Archives + corpus | 7 commits, test hardening | Green; SKY-5-01 (LOW). Present-but-unopenable required archives now fail loudly; the Skyrim SE optional (CC) tier stays tolerant by design. |

---

## Shader-Type Coverage Matrix

Skyrim `BSLightingShaderType`, BSVER 83–129. Names per `nif.xml:1402-1422`. Block counts (SE / LE) are from the 2026-10-12 census and were not re-measured; the Skyrim parse path is byte-unchanged since then. `material_kind = shader_type` is set at `dedicated_shader.rs:464` (overridden to 103 for the Fire_Refraction pair, `:485`).

| # | nif.xml name | ShaderTypeData | Parse | Import | Render | SE / LE blocks |
|---|---|---|---|---|---|---|
| 0 | Default | None | yes, 0 B | n/a (kind 0) | default lit | 45,458 / 36,963 |
| 1 | Environment Map | EnvironmentMap | yes, 4 B | `env_map_scale` + consumed latch | `materialKind==1u` forces env reflection (`triangle.frag:2960-2961`); alchemy glass → glass (#4392) | 6,726 / 3,176 |
| 2 | Glow Shader | None | yes, 0 B | slot 2 glow by flag | by data presence (`glowMapIndex`) | 1,395 / 729 |
| 3 | Parallax | None | yes, 0 B | height slot → `parallax_map` | POM via `parallaxMapIndex` (default passes / scale) | 11 / 14 |
| 4 | Face Tint | None | yes, 0 B | tint slots; `detail_neutral`; keys FaceGen override | `TINT_ALPHA_WEIGHT_BIT` gate | 3,158 / 2,554 |
| 5 | Skin Tint | SkinTint | yes, 12 B | `skin_tint_color` | albedo tint `:1547-1553` | 1,631 / 1,423 |
| 6 | Hair Tint | HairTint | yes, 12 B | `hair_tint_color` | multiply `:1554-1559` | 10,817 / 8,748 |
| 7 | Parallax Occ | ParallaxOcc | yes, 8 B | max passes / height scale | POM consumes both | 0 / 12 |
| 8 | Multitexture Landscape | None | yes, 0 B | n/a | n/a (runtime-assigned, never in NIFs) | 0 / 0 |
| 9 | LOD Landscape | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 10 | Snow | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 11 | MultiLayer Parallax | MultiLayerParallax | yes, 20 B | all 4 fields | inner layer `:1430`, strength `:2991`; **`refraction_scale` packed, unread** | 662 / 502 |
| 12 | Tree Anim | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 13 | LOD Objects | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 14 | Sparkle Snow | SparkleSnow | yes, 16 B | `sparkle_parameters` | glint `:1560-1574` | 19 / 19 |
| 15 | LOD Objects HD | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 16 | Eye Envmap | EyeEnvmap | yes, 28 B | all 3 fields | **packed, not shaded** (SKY-2-02) | 3,251 / 2,628 |
| 17 | Cloud | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 18 | LOD Landscape Noise | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 19 | Multitexture Landscape LOD Blend | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 20 | FO4 Dismemberment | None | yes, 0 B | n/a | n/a | 0 / 0 |

Types mapped to `None` read no trailing data (the #4252 guard iterates `[2,3,4,8,9,10,12,13,15,17,18,19,20]` and passes). Every kind in `2..=20` is protected from the glass classifier (`byroredux/src/helpers.rs:194`). #4256 (`ImportedMaterial.shader_type` never crosses the NIFAL boundary) is still open. Observation, not a finding: the FO4-delegated types 6/7/11/14/16 have no FO4 wire-level test (`shader_tests/fo4.rs` covers only SkinTint and EnvironmentMap); the Skyrim tests pin the shared bytes and the FO4 corpus gate backstops.

---

## Cell-Load Regression Status

**TES5 cells through the unified walker — green.**
- `parse_real_skyrim_esm`: `Skyrim.esm` 590 cells, 18,318 statics, 37 worldspaces; WinkingSkeever 981 refs; 590 / 590 cells carry the extended XCLL.
- DIAL `QNAM` ownership: 125 MS01 topics with `quest_refs` populated; override-topic INFO folding (#5375 / #5644) passes including a 4,000-round randomized equivalence test.
- `XCCM` decodes as a REGN on TES5 (`sky_region`), a CLMT on TES4/FO3/FNV; LE (HEDR 0.94 / v40) classifies as Skyrim and takes the REGN reading.
- The five-plugin SE order (Skyrim, Update, Dawnguard, HearthFires, Dragonborn): 135,900 records, inline 3.33 s vs parallel 2.18 s, identical digest — parallel walk, ordered fold holds.

**Whiterun BanneredMare control bench** (harness `a4ede5fa0`, 3 × 300 frames, camera pose identical to the record):

| | Record `a3a7a43b0` (2026-10-13) | Control today (`a3a7a43b0` rebuilt) | HEAD `eec7e00cf` |
|---|---:|---:|---:|
| Entities | 5,825 | 5,825 | 5,837 |
| Draws / TLAS / lights | 1289/392b/24c · 1,226 · 18 | same | same |
| TAA native | 92.4 FPS / 10.82 ms | 83.8 / 11.94 | 74.3 / 13.46 |
| FSR Quality | 148.4 / 6.74 | 131.2 / 7.62 | 120.4 / 8.30 |
| FSR Performance | 215.5 / 4.64 | 189.7 / 5.27 | 178.2 / 5.61 |

Entities are flat (+12, same draw and TLAS counts; the state hash differs), so the entity gate passes and the FPS drop at flat entities is the regression in SKY-4-02. The record-vs-control gap (92.4 vs 83.8) is the machine, not the code.

**DLC repro (`--master Skyrim.esm --master Update.esm --esm Dawnguard.esm --cell Forelhost01`), run live.**
- Load order resolves (`masters=["Skyrim.esm","Update.esm"], main=Dawnguard.esm`); Dawnguard walks in 0.21 s; `Forelhost01` (form `0001527F`) loads 1,578 placed references into 3,678 entities. Duplicate-plugin and missing-master errors name the plugin (static evidence at `load_order.rs:~607`, `:370`).
- Not done: the override REFRs' final contents were not diffed against the master, so "last-write-wins on a DLC-deleted base REFR" rests on the unit and static evidence, not on this cell.
- WARNs observed (all expected; none new):
  - `FormID 0307b5b9 has mod_index 3 but plugin has 2 masters` ×2: one dangling reference in vanilla `Dawnguard.esm` (the value occurs exactly once in the file); passed through unchanged by design (`reader.rs`).
  - `XCLL with 64 bytes is non-canonical for Skyrim (expected one of [28, 92])` ×1; `SCOL GRUP encountered with GameKind::Skyrim` ×1 (skipped by design).
  - Story Manager "#5405 … do not form a single SNAM chain" ×8: placed deterministically, none dropped, by design.
  - The Skyrim constraint realignment WARN (SKY-5-01).
- One 19.1 s frame in the first run was a cold driver compile of a lazily built blend pipeline (`Blend pipeline src=1 dst=2 … 19077.750 ms`). The second run compiled the same variant in 24 ms. This is a one-off cold-cache stall after today's shader changes, not a steady-state cost, but a player with a cold cache will see it on their first blended draw.

---

## Stale skill-text notes (for `/audit-sync`)

- Dim 1: tangent synthesis is Lengyel / nifly `CalcTangentSpace`, not Mikkelsen (CLAUDE.md, `import/mesh/mod.rs:6`, `tangent.rs:3`); the `bs_lod_tri_shape_skyrim_consumes_ni_tri_shape_body_plus_3u32_trailer` guard runs as `blocks::tri_shape::bs_tri_shape::skin_vertex_tests::…`, not from `tri_shape_skin_vertex_tests.rs`.
- Dim 2: matrix row 16 (and 11's `refraction_scale`) should read "packed, not shaded"; the `msn_basis_pin` (#3922) reference was not re-verified.
- Dim 3: the guards line omits the `resumable/mod.rs` tests (`player_body_finalize_keeps_player_state_out_of_the_body_root`, `combat_disposition_is_armed_at_finalize_not_at_prepare`); the nif_loader tint tests run under `scene::nif_loader::tests` (filter `facegen_diffuse`); the smoke floor premise is stale (SKY-3-01); test counts are hard-coded in places (npc_spawn suite now 193 passed + 10 ignored).
- Dim 4: add the post-split cell files to Paths; `parse_real_skyrim_esm` lives at `crates/plugin/src/esm/cell/tests/integration.rs:309`; the guard list misses `dialogue_override_head_run_composes_in_file_order`, `batched_info_fold_matches_the_one_at_a_time_fold`, `a_large_override_folds_into_a_large_topic`; production is `fold_infos_into_topic`; `skyrim_default_water_promotes_underwater_tail` and `real_skyrim_load_order_preserves_categories_and_resolves_archive_strings` return Ok silently when the data dir is missing; the real multi-master oracle `parallel_walk_matches_the_inline_walk_on_real_load_orders` should be named.
- Dim 5: add `crates/nif/tests/block_coverage_baselines.rs` to Paths; the optional-tier tolerance is per game (Oblivion DLC panics, Skyrim SE CC skips); name the `skyrim_bsa_sweep_audit` example (it hardcodes the SE Data path); the "0 realignment WARNs" expectation is wrong for Skyrim (SKY-5-01) — the corpus gate does not count drift.
- Dim 5 not re-driven: the real-data render trace (dragon skeleton / tree LOD / `BSEffectShaderProperty` magic effect) was not run.
- Process: `--game skyrim` is not a profile key (it is `skyrim_se`); the harness `FSR_BENCH_SCENES=whiterun` form is the correct way to run the control bench.

---

Suggested next step: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-10-17.md`
(label every finding `game:skyrim` + `legacy-compat`, plus its own domain label: SKY-4-02 `performance` + `renderer`; SKY-5-01 `nif-parser`; SKY-2-02 `shaders` + `enhancement`; SKY-3-01 `test-gap`; SKY-1-01 / SKY-2-01 / SKY-4-01 `doc-rot`.)
