# Skyrim (SE + LE) Compatibility Audit — 2026-10-21

**HEAD**: 4a3e89d56 · **Baseline**: Dims 1, 2, 5 against `docs/audits/AUDIT_SKYRIM_2026-10-17.md` (HEAD `eec7e00cf`, the last full run); Dims 3, 4 against `docs/audits/AUDIT_SKYRIM_2026-10-19.md` (HEAD `2139ae20e`, area-scoped streaming-deep run) · **Audited**: Dim 2 (shader dispatch + the #5961 EyeEnvmap branch), Dim 3 (NPC equip + FaceGen, 26 commits), Dim 4 (multi-master + TES5 cell load, 17 commits), Dim 5 (archives + corpus gates, real-data sweeps re-run) · **Unchanged since baseline (skimmed)**: Dim 1 (one doc-only commit, `d81f7501f`; guards and real-data corpus re-run green).

Run context: part of the 2026-10-21 comprehensive suite. Solo, static analysis plus tests, no engine launch (suite rule), no sub-agents. Tests ran in the repo `target/`; no private target dir. Scratch notes in `/tmp/audit/skyrim/`.

---

## Executive Summary

Skyrim parsing and archives are fully green on real data. The SE corpus parses 33,468 of 33,468 NIFs and LE 22,466 of 22,466, all clean. Both per-block baselines match. SE has 0 `NiUnknown` blocks out of 856,103. The constraint-drift corpus logs 0 events across all five Bethesda titles, so the 2026-10-17 motor-byte finding (#5960) is fixed and holds. The BSA sweeps extract every entry with 0 errors: SE 23 archives / 172,961 entries, LE 9 archives / 132,755 entries.

The window's Skyrim-visible fixes check out:
- The Use-Traits FaceGen re-key (#6113, `1dfac76f6`).
- The verified-creature XRGD posing (#6068/#6069). The census re-run poses exactly 730 of 1,077 corpses, as pinned.
- The REFR-move and CELL-override folds (#6084/#6086).
- The Starfield FormID scheme (#6118). Every SE and LE plugin on disk still lands on the positional scheme.

**New this run: the EyeEnvmap shading branch (`b4d073454`, #5961) cannot produce the iris-centred reflection on vanilla Skyrim eyes.** It has two defects of its own, separate from the renderer audit's REN-H-01 (render-origin mixing):
- **SKY-D2-01 (HIGH, NIFAL floor)**: the authored eye reflection centres reach the GPU still in Gamebryo Z-up. They never pass `zup_to_yup_pos`, but the shader transforms them with the Y-up instance matrix. On `eyesmale.nif` the computed centre is about 12 units from the real one, around an eyeball about 1 unit across.
- **SKY-D2-02 (MEDIUM)**: the branch samples the eye cube at LOD `roughness * 8`. Vanilla eyes carry the classifier's no-signal roughness of 0.85, which asks for LOD 6.8. The shipped 32×32 `eyecubemap.dds` has only six mips, so every eye reads the 1×1 mip: one flat colour per face, with no glint.

All three defects (REN-H-01 and the two above) must be fixed before kind-16 eyes look any different from the generic path.

Open, not re-filed: #6096 (DLC / Creation Club facegeom paths built from the global FormID; `prebaked_facegen_nif_path` is unchanged) and #5959 (BanneredMare bench). Cited from this suite: ESM-2026-10-21-01 (PSDT minutes as hours, 1,536 of 1,545 Skyrim schedules), CHAR-2026-10-21-M-02 (Use-Traits sex/VTCK read off the shell), UI-D4-2026-10-21-01 (Loader SWF ceiling bypass, `BethesdaNetLogin.swf`), REN-H-01.

| Severity | NEW | Existing |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 1 | 0 |
| MEDIUM | 1 | 0 |
| LOW | 0 | 0 |

| ID | Severity | Title | Status |
|---|---|---|---|
| SKY-D2-01 | HIGH | EyeEnvmap reflection centres cross NIFAL in Gamebryo Z-up; the shader places them with the Y-up instance matrix | NEW |
| SKY-D2-02 | MEDIUM | EyeEnvmap branch samples the eye cubemap at LOD `roughness * 8` = 6.8 on vanilla eyes, which is the 1×1 mip of the 32² cube | NEW |

---

## Dimension Findings

### Dimension 2 — Shader-Type Dispatch + Skyrim Material Slice

#### HIGH

### SKY-D2-01: EyeEnvmap reflection centres cross the NIFAL boundary in Gamebryo Z-up, and the shader places them with the Y-up instance matrix
- **Severity**: HIGH. This is the `_audit-severity.md` floor for a wrong canonical `Material` out of `translate_material`. The impact is visual only, limited to kind-16 eyes (3,251 SE / 2,628 LE blocks per the 2026-10-17 matrix census; FO4 / FO76 share type 16).
- **Dimension**: 2 (Skyrim material slice) / NIFAL coordinate boundary
- **Location**:
  - `crates/nif/src/import/material/shader_data.rs:201-209`: `ShaderTypeData::EyeEnvmap` is copied verbatim into `ShaderTypeFields::eye_{left,right}_reflection_center`.
  - `byroredux/src/material_translate.rs:744-747`: `shader_type_fields` is cloned, not converted.
  - `byroredux/src/render/static_meshes.rs:300-313`: `pack_eye_fields` packs the values verbatim.
  - `crates/renderer/shaders/triangle.frag`, the `if (isEyeEnvmap)` block: `leftWS = (instances[…].model * vec4(mat.eyeLeftCenterX, …, 1.0)).xyz`.
- **Status**: NEW. Introduced with #5961 (`b4d073454`, closed). It is not REN-H-01: that finding is the missing `renderOrigin` term in the same two lines. This one is the axis convention of the inputs, and it survives REN-H-01's suggested fix. Searched open and closed issues for "eye centre", "EyeEnvmap", "reflection center": only #5961 and #430 (field plumbing) match.
- **Description**: every other positional quantity a NIF authors (vertices, node translations, bound centres) is converted once at import with `zup_to_yup_pos` (`(x, y, z) → (x, z, -y)`), so `GpuInstance.model` maps Y-up local space to world. The two eye reflection centres are the exception. They are authored in the eyeball shape's Z-up local space (nif.xml `BSLightingShaderProperty` Eye Envmap: Left/Right Eye Reflection Center), and no tier converts them, so the shader multiplies a Z-up point by a Y-up matrix. The centre's authored height becomes its forward depth, and its authored forward offset becomes height.
- **Evidence** (real data, `meshes\actors\character\character assets\eyesmale.nif` from `Skyrim - Meshes0.bsa`, hand-decoded from `trace_block`):
  - Block 7 `BSLightingShaderProperty`, type `0x10`. Trailer: `eye_cubemap_scale` 1.3; left centre (−2.397, 7.572, 3.750); right centre (2.398, 7.572, 3.750).
  - Block 3 `BSDynamicTriShape`, which has no shape transform: the first iris vertex is (−2.34, 8.54, 4.47) Z-up. The centre sits about 1 unit behind the iris surface, as expected.
  - The correct Y-up centre is (−2.40, 3.75, −7.57). The shader uses (−2.40, 7.57, 3.75), about 11.9 units away, on the far side of the head along the facing axis.
  - Once REN-H-01 is fixed, `normalize(fragWorldPos - centerWS)` is ≈ (0, −0.24, −0.97) for every iris fragment. That is a near-constant direction, which gives the flat reflection the branch exists to avoid.
  - `material_dump` confirms the import (`kind 16`). `grep zup_to_yup crates/nif/src/import/material/` returns nothing.
- **Impact**: the "iris-centred" reflection degenerates to an almost fixed view direction per eye on every Skyrim (and FO4) eye with an authored cube. It is masked today by REN-H-01's larger origin error, so fixing REN-H-01 alone will look like the eye still does not work.
- **Related**: REN-H-01 (same block, different defect), SKY-D2-02 (same block), #5961, #430.
- **Suggested Fix**: convert both centres with `zup_to_yup_pos` where `ShaderTypeData::EyeEnvmap` is unpacked in `shader_data.rs`. That is the single import boundary, so FO4 / FO76 get the same fix. Add a test that feeds a known Z-up centre and asserts the Y-up value reaches `ShaderTypeFields`.

#### MEDIUM

### SKY-D2-02: The EyeEnvmap branch samples the eye cubemap at LOD `roughness * 8`; vanilla eyes carry the classifier's no-signal roughness 0.85, so every eye reads the 1×1 mip of the 32² cube
- **Severity**: MEDIUM (visual only; the effect is erased, nothing crashes)
- **Dimension**: 2 (render-complete column of the shader matrix)
- **Location**:
  - `crates/renderer/shaders/triangle.frag`, the `if (isEyeEnvmap)` block: `textureLod(cubemaps[nonuniformEXT(mat.envMapIndex)], cubeDirection, roughness * 8.0)`.
  - Roughness source: `crates/core/src/ecs/components/material.rs` `classify_pbr_keyword` (terminal `PbrMaterial::NO_SIGNAL_NEUTRAL`), reached from `crates/nif/src/import/material/mod.rs` `classify_legacy_pbr`.
- **Status**: NEW (from #5961, `b4d073454`).
- **Description**:
  - The branch's own comment says "No roughness attenuation: the cube IS the eye's reflection, and eye smoothness values in the wild straddle the gate". The LOD term contradicts it.
  - Vanilla eyes have no roughness signal, so `roughness` is the classifier's no-signal neutral of 0.85. The texture path `eyebrown.dds` matches no keyword, there is no gloss map, and type 16 authors no `env_map_scale`. The authored `glossiness` of 479 is ignored without a gloss map.
  - 0.85 × 8 gives LOD 6.8.
  - The vanilla cube `textures\cubemaps\eyecubemap.dds` (`Skyrim - Textures3.bsa`) is DXT1, 32×32, with 6 mips. LOD 6.8 clamps to mip 5, which is one texel per face.
- **Evidence**:
  - `material_dump eyesmale.nif` prints `kind 16, rghO 0.85, env 0.00, gloss 479`.
  - The DDS header reads `h 32 w 32 mips 6 fourcc DXT1 caps2 0xfe00` (all six cube faces).
  - `triangle.frag` changes `roughness` only through the gloss-map mix and the weather terms (lines 495, 1539, 1631, 1636). None of them applies to an eye.
- **Impact**: every vanilla kind-16 eye reflects one averaged colour per cube face, a uniform tint over the iris. There is no glint, and the specular "alive eye" look is lost even after SKY-D2-01 and REN-H-01 are fixed.
- **Related**: SKY-D2-01, REN-H-01, #5961. The no-signal neutral itself is deliberate (#2707). The defect is only that this branch reads it as an eye smoothness.
- **Suggested Fix**: sample the eye cube at LOD 0, as the comment already describes, or at an LOD derived from an authored eye value. Do not use the classifier fallback. Add a source-shape pin so the eye `textureLod` does not take `roughness`.

### Dimension 1 — BSTriShape packed geometry + SSE reconstruction
Unchanged since the 2026-10-17 baseline except `d81f7501f` (#5963, docs only), which closed SKY-1-01. Guards re-run (see Verified Holding). No findings.

### Dimension 3 — NPC Equip + FaceGen
No new findings. #6113 (the 2026-10-19 SKY-SD-02) and #6108 (SKY-SD-03) are closed and verified in code. #6096 (SKY-SD-01's Skyrim half, filed as FO4-SD-01 with `game:skyrim`) is still open: `byroredux/src/npc_spawn.rs:1705-1729` still formats the global FormID, and `byroredux/src/npc_spawn/tests.rs:470` still pins `Dawnguard.esm` with `0x0001684C`. CHAR-2026-10-21-M-02 already covers the remaining shell-read Use-Traits fields (sex, VTCK).

### Dimension 4 — Multi-Master Load Order + TES5 Cell Load
No new findings. See Verified Holding and Cell-Load Regression Status.

### Dimension 5 — Archives + Corpus Gates
No new findings. #5960 (SKY-5-01) is closed and holds on the drift corpus.

---

## Verified Holding (no finding)

- **#6113 Use-Traits FaceGen (`1dfac76f6`)**:
  - Both production `NpcSpawnJob::prebaked` callers (`cell_loader/references/mod.rs:723`, `player_body.rs:180`) feed `prepare_prebaked_state`.
  - The facegeom and facetint ids and the PNAM fallback now read `resolved.r#traits`.
  - The plugin segment comes from `plugin_for_form_id(traits_terminal.form_id, …)`.
  - The cell loader's `resolve_inherited_traits(npc, effective_actor_level(npc), …)` and `ResolvedNpc::resolve` (`crates/plugin/src/equip.rs:717`) use the same shell level and the same walk policy. A Use-Traits chain that ends in an `LVLN` therefore picks the same terminal for the plugin segment and for the id.
- **#6124 lazy mesh extract (`8879b67b4`)**: `apply_spawn_hook` (`scene/nif_loader.rs:254`) runs the per-NPC `hide_skin_partitions` hook on `Arc::make_mut`. The shared cached head-part scene is never mutated, so one NPC's helmet mask cannot leak to the next NPC through the cache.
- **#6128 / #6107 / #6103 / #6105**: the prebaked arm sets `body_texture: None` (correct: Skyrim carries no body-section ICON). The skinned static-BLAS gate, the lock order and the subtree walk do not change Skyrim output.
- **XRGD (`dfbcf7e2c`, #6068/#6069)**: the `xrgd_entry_counts_match_the_humanoid_ragdoll_bodies` census was re-run on SE (1.1 GB RSS). It poses 730 corpses: human male 273, female 100, draugr 165, draugr F 35, SkeletonS 53, falmer 20, skeever 54, deer 30. Every unverified rig (beast races at 23 bodies, wolf, horse, spiders, dragon, …) poses 0, which is refused by design.
- **#6084 REFR move (`765e387a0`)**: the sweep covers interior, exterior and worldspace-persistent cells and exempts the authoring cell. An Update.esm temporary→persistent re-home therefore leaves one copy.
- **#6118 Starfield FormID scheme (`0a2955d33`)**: the scheme is chosen per plugin from `GameKind::from_header`. Measured on disk: every SE plugin (Skyrim, Update, Dawnguard, HearthFires, Dragonborn, `_ResourcePack.esl`, four CC files) is HEDR 1.70 or 1.71 with record version 44, and LE `Skyrim.esm` is 0.94 with v40. All fall in the Skyrim band, so the positional remap is unchanged for Skyrim.
- **LTEX Havok (`c53d87679`)**: Skyrim LTEX names its material by `MATT` FormID. The commit decodes it and deliberately keeps the default surface, because no source maps MATT to the bhk material id. #6048 stays open for that. Not a Skyrim regression.
- **Loose tier (#6200)**: the SE `Data` holds only `screenshots/` and `Video/`, and LE only `Interface/`. None of these is a `LOOSE_ASSET_ROOTS` folder, so no tier opens and no Skyrim baseline moves. Observation, not filed: the tier covers `textures`/`meshes`/`materials` only. Loose `scripts\*.pex` (the script provider has no loose backend), `interface\*.swf` and `sound\voice` stay archive-only, which matters for modded Skyrim, not vanilla.
- **#4256 `source_shader_type` (`2266f7778`)** and **#6130 `fallout_hair` (`7822695e4`)**: the type survives the glass reassignment (guard passes). `fallout_hair` is written only by the FO3/FNV `BSShaderPPLightingProperty` arm, so Skyrim hair is untouched.
- DIAL `QNAM` → `quest_refs` (`crates/plugin/src/esm/records/misc/dialogue.rs:528`) is intact.

## Shader-Type Coverage Matrix

Skyrim `BSLightingShaderType`, BSVER 83–129. Block counts (SE / LE) come from the 2026-10-12 census and were not re-measured; the Skyrim parse path is byte-unchanged since then (per-block baselines match). Changes from the 2026-10-17 matrix are rows 11 and 16 only.

| # | nif.xml name | ShaderTypeData | Parse | Import | Render | SE / LE blocks |
|---|---|---|---|---|---|---|
| 0 | Default | None | yes, 0 B | n/a | default lit | 45,458 / 36,963 |
| 1 | Environment Map | EnvironmentMap | yes, 4 B | `env_map_scale` + latch | forced env reflection; alchemy glass → glass (#4392) | 6,726 / 3,176 |
| 2 | Glow Shader | None | yes, 0 B | glow slot by flag | `glowMapIndex` | 1,395 / 729 |
| 3 | Parallax | None | yes, 0 B | height → `parallax_map` | POM | 11 / 14 |
| 4 | Face Tint | None | yes, 0 B | tint slots, `detail_neutral` | `TINT_ALPHA_WEIGHT_BIT` | 3,158 / 2,554 |
| 5 | Skin Tint | SkinTint | yes, 12 B | `skin_tint_color` | albedo tint | 1,631 / 1,423 |
| 6 | Hair Tint | HairTint | yes, 12 B | `hair_tint_color` | multiply | 10,817 / 8,748 |
| 7 | Parallax Occ | ParallaxOcc | yes, 8 B | passes / scale | POM | 0 / 12 |
| 8 | Multitexture Landscape | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 9 | LOD Landscape | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 10 | Snow | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 11 | MultiLayer Parallax | MultiLayerParallax | yes, 20 B | all 4 fields | inner layer + strength; `refraction_scale` now offsets the inner UV (#5961, interpretive `MLP_REFRACTION_UV_PER_UNIT` 0.1) | 662 / 502 |
| 12 | Tree Anim | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 13 | LOD Objects | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 14 | Sparkle Snow | SparkleSnow | yes, 16 B | `sparkle_parameters` | glint | 19 / 19 |
| 15 | LOD Objects HD | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 16 | Eye Envmap | EyeEnvmap | yes, 28 B | 3 fields, **centres left in Z-up** (SKY-D2-01) | branch exists (#5961) but **not render-complete**: REN-H-01 + SKY-D2-01 + SKY-D2-02 | 3,251 / 2,628 |
| 17 | Cloud | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 18 | LOD Landscape Noise | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 19 | Multitexture Landscape LOD Blend | None | yes, 0 B | n/a | n/a | 0 / 0 |
| 20 | FO4 Dismemberment | None | yes, 0 B | n/a | n/a | 0 / 0 |

`None` types read no trailing data (#4252 guard passes in the nif lib run). #4256 is closed (`2266f7778`).

## Cell-Load Regression Status

- **TES5 cells through the unified walker**: static only (no engine launch). The walker delta is the #6084 / #6086 override folds (verified above), #6043 XCLC hide-quad flags, #6047 EDID-less WRLD and #6077 XESP. These are ESM / exterior mechanisms, and `/audit-esm` and `/audit-exterior` reported nothing Skyrim-specific on them. The plugin guard subset (BODT, WNAM, equip template, dialogue override folds, `skyrim_race_base_head_parts`, `a_refr_moved…`, `cell_record_fields_come_from_the_winning_override`, XCCM) passes: 15 passed, 1 ignored.
- **Whiterun BanneredMare control bench**: **not measured** (suite rule: no engine launch). #5959 (7–13% slower than the record at flat entities, 2026-10-17) remains open. The DLC repro (`Forelhost01`) and `m41-equip.sh skyrim` were also not run.

## Tests run (repo `target/`)

| Command | Result |
|---|---|
| `cargo test --release -p byroredux-nif --test parse_real_nifs -- --ignored skyrim` | SE 33,468/33,468 clean (8 archives incl. 6 optional); LE 22,466/22,466 clean |
| `… --test per_block_baselines -- --ignored skyrim` | SE 145 types OK; LE 146 types OK |
| `… --test block_coverage_baselines -- --ignored skyrim` | SE 856,103 blocks, 0 `NiUnknown` |
| `… --test constraint_drift_corpus -- --ignored` | 0 drift events, 8 types (Obl/FO3/FNV/LE/SE) |
| `cargo test --release -p byroredux-bsa --test bsa_real -- --ignored skyrim fo3_fnv_and_skyrimse` | SE 172,961 / LE 132,755 entries, 0 errors; Meshes0 brute force 18,862 NIFs, 0 errors; `.fuz` shape ok (FO3 / FNV census legs also 0 errors) |
| `cargo test -p byroredux-nif --lib` | 1,439 passed |
| `cargo test -p byroredux --bin byroredux -- prebaked facegen npc_spawn ragdoll_pose skyrim eye_pack source_shader_type lod_settings loose_override archive_siblings` | 236 passed, 22 ignored |
| `cargo test -p byroredux --bin byroredux -- --ignored xrgd_entry_counts_match_the_humanoid_ragdoll_bodies` | passed; 730 posed |
| `cargo test -p byroredux-plugin --lib -- bodt equip_template wnam dialogue_override …` | 15 passed, 1 ignored |

Not run: engine legs (bench, `Forelhost01`, `m41-equip.sh`, `p6-loading-model.sh`, `sm1-story-manager.sh`, `dt2-skyrim-forcegreet.sh`): suite rule. Plugin `--ignored` real-data tests: forbidden (OOM). The real-data render trace (dragon skeleton / tree LOD / effect shader through the renderer) needs a device. Only its import half was exercised, on `eyesmale.nif`.

## Stale skill-text notes (for `/audit-sync`)

- Dim 2 "Shaded, was packed-only" says type 16 is verified by finding `isEyeEnvmap` in `triangle.frag`. Reading the branch is not enough: the matrix row should stay "not render-complete" until REN-H-01, SKY-D2-01 and SKY-D2-02 close. Add the centre basis and the LOD to the checklist.
- Dim 3: #6096 is the open Skyrim DLC / CC facegeom tracker (filed under FO4-SD-01); the skill does not name it.
- Dim 4: the XRGD census test reads race and gender off the placed shell (`npc.race_form_id`, `npc.acbs_flags`), not the Use-Traits terminal that production spawns with. Its pinned per-skeleton counts are a census of shell races. Harmless today, but it is the #6113 pattern in a test.

Suggest: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-10-21.md` (labels: `game:skyrim` + `legacy-compat`, plus `nifal`/`shaders` for SKY-D2-01 and `renderer`/`shaders` for SKY-D2-02; both also `game:fo4`).
