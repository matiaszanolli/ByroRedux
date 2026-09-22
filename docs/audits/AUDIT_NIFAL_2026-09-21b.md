**HEAD**: f97775ca8 · **Baseline**: docs/audits/AUDIT_NIFAL_2026-09-21.md (HEAD 4dca737e2) · **Audited**: Dims 1, 2, 3, 5, 7 (delta-touched) · **Unchanged since baseline (skimmed)**: Dims 4, 6, 8, 9 (guards spot-checked; Dims 8 and 9 each carry one pre-existing LOW surfaced by this run's census and harness runs)

# NIFAL Audit — 2026-09-21 (b, afternoon delta)

This is one leg of `/audit-suite --preset comprehensive`. One auditor ran the nine dimensions sequentially, without
sub-agents. The delta is `4dca737e2..f97775ca8` (42 commits). Per-dimension scratch notes are in
`/tmp/audit/nifal/dim_{1..9}.md`, and this morning's scratch is preserved in `/tmp/audit/nifal/baseline_4dca737e2/`.

These delta commits reach NIFAL, mapped from each dimension's `Paths:` plus the adjacent files they touch:

| Dim | Commits |
|---|---|
| 1 Material | `c0b740ce7` (#4548 `_msn` name rule in `translate_material`), `069989e77` (#4488 FO76 `.bto` joins `object_lod` → `translate_material`), `92a633ce6` (nif_loader no-op) |
| 2 Geometry/Transform | `3e798cacd` (#4549 finite gate, `rotation.rs` + `stream.rs`), `ab8779a52` (#4206 tangent pre-size) |
| 3 Skinning & Lights | none on declared paths; `d54382415` changed the ESM light policy table (`crates/core/src/lighting.rs`, W2.10 `STATIC_PROP` flip) and the `light_anim.rs` narrative |
| 4 Nodes | `92a633ce6` (no-op); `e9aeb2706` + `521f7f14b` pure move of player spawn planning to `byroredux/src/scene/character_spawn.rs` |
| 5 Particles | `387e65616` (#4550), `a62d06d60`, `92a633ce6` (clippy, neutral) |
| 6 Collision | none |
| 7 Animation | none on declared paths; `339bdb726` (#4551 in `cell_loader/load.rs`), `92a633ce6` (combat_anim visibility), `c1f38e3da` (#4546) |
| 8 Shader flags / roles | none on declared paths; `f97775ca8` edits `triangle.frag` (debug views only) |
| 9 Completeness | none |

## Executive Summary

**New and regression findings: 6 (0 CRITICAL · 1 HIGH · 1 MEDIUM · 4 LOW).** None is a regression.
- 3 more findings match concurrent sibling-report findings and are cited, not re-counted: REN-D6-2026-09-21-01, REN-D1-2026-09-21-01, and NIF-D4-2026-09-21-02.
- 3 open issues were re-matched with new facts: #4411, #4553, #4556.
- 1 open issue, #4557, is **resolved in code** and can be closed.

**All four NIFAL fixes in the window hold**:
- #4549 (this morning's HIGH): the static-transform finite gate is in place at both `stream.rs` readers, plus the head gate and output gate of the SVD repair.
- #4550: an own `data_ref` is now decisive.
- #4551: all three clip-install routes now install the combat family.
- #4548: a `_msn` normal-slot name sets `MODEL_SPACE_NORMALS` inside the single boundary.

A fresh census over every vanilla mesh archive shows the #4548 name rule firing only on its target:
- **FO4**: 1,470 FaceCustomization heads, plus 3 inert fires on dead paths.
- **Skyrim LE/SE, FO76, FNV, FO3, Oblivion**: zero fires.

So the fix's premise, "Skyrim `_msn` maps carry both the name and the flag", holds.

**Headline (HIGH, pre-existing, surfaced by that census):** every Skyrim and FO4 `.btr` distant-terrain land shape authors
`Model_Space_Normals`. That is 9,584/9,584 on SE, 4,416/4,416 on LE, and 8,271/8,271 on FO4. The texel measurement agrees:
the maps are green-up model-space maps, in exactly the basis the #3922 MSN shader branch already consumes. But
`terrain_lod_btr.rs` documents them as tangent-space and lowers the `.btr` through `translate_texture_only_material`, which
drops the authored bit. As a result, every Skyrim distant-terrain quad samples a model-space map through `perturbNormal`.
(NIFAL-D1-2026-09-21b-01.)

**On REN-D6-2026-09-21-01** (the silenced core copy-fidelity test, found by the renderer leg), this leg adds three facts:
- The blast radius is measured: 34 of the 52 source-derived `Material` fields, and all 8 path-fed fields including
  `normal_map`, have their only assertion in the dead fn.
- The test would still pass if re-attached, because #4548 does not break it.
- The rustc warnings that flag it are printed in CI logs but never denied.

Per-category status (spec §2):

| Category | This delta |
|---|---|
| Material (D1) | #4548 verified inside the boundary. Census-confined. Guard lane 67/67, but the core copy test is silenced (REN-D6-01). **1 HIGH** (`.btr` texture-only lowering drops the authored MSN bit), 1 LOW doc/spec |
| Geometry/Transform (D2) | #4549 verified fixed. **1 MEDIUM**: finite-overflow residual. NIF-D4-02 (BSSkin bypass) cited |
| Skinning (D3) | unchanged; guards green |
| Lights (D3) | W2.10 flip made at the single policy table (single-boundary holds). #4557 resolved in code |
| Nodes (D4) | unchanged; parked-field grep empty |
| Particles (D5) | #4550 verified fixed; exactly 2 overlay callers |
| Collision (D6) | unchanged; dispatch↔resolve guard green |
| Animation (D7) | #4551 verified fixed on all 3 routes. 1 LOW (its pin self-counts) |
| Shader flags / roles (D8) | unchanged; `if game` grep empty. 1 LOW pre-existing (NIF-first precedence unsourced) |
| Completeness (D9) | FO4 fill row byte-identical to baseline. 1 LOW pre-existing (dark census false-fails on partial data) |

Tier-invariant counts among the new findings:

| Invariant | Findings |
|---|---|
| no-leak | D1b-01 (HIGH, authored MSN dropped at texture-only lowering), D2b-01 (MEDIUM) |
| no-fabrication | D1b-02 (unrecorded classifier), D8b-01 (unsourced precedence claim) |
| single-boundary | 0 |
| no-render-time-fallback | 0 new (REN-D1-01, cited, is one) |
| harness-gap | D7b-01, D9b-01 |

## Per-Category Tier Matrix (delta view; the baseline matrix otherwise stands)

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary fn |
|---|---|---|---|---|---|
| Material | PASS: #4548 rule inside `translate_material` (`:593-601`, OR'd at `:679`); 4 production callers unchanged; FO76 `.bto` enters via the existing object-LOD caller | PASS for #4548 (census-backed, table below), but its classifier is unrecorded in spec (D1b-02) | **FAIL**: `.btr` land shapes lowered texture-only, authored MSN dropped (D1b-01) | PASS | `material_translate.rs::translate_material` / `translate_texture_only_material` |
| Geometry/Transform | PASS | PASS | **PARTIAL**: #4549 closed the direct path; the overflow residual remains (D2b-01); BSSkin bypass (NIF-D4-02) | PASS | `rotation.rs::sanitize_rotation` + `sanitize_transform_translation_and_scale` at `stream.rs:753/:780` |
| Lights | PASS: W2.10 changed only in `for_legacy_local_light`'s table | PASS: evidence cited in-doc | PASS | n/a (REN-D1-01 is the renderer's occluder classifier) | `lighting.rs::for_legacy_local_light` |
| Particles | PASS: 2 callers | PASS: #4550 closes the sibling-inheritance hole | PASS | PASS | `systems/particle.rs::apply_emitter_overlays` |
| Animation | PASS: producers untouched | PASS | PASS | PASS | `convert_nif_clip` + `convert_hkx_clip` |
| Shader flags / roles | PASS | **PARTIAL**: D8b-01 (pre-existing) | PASS | PASS: `if game` grep empty | `slot_role.rs::slot_to_role`, `merge.rs::fill` |

### #4548 name-rule census (fresh; read-only probe `/tmp/audit/nifal/msnprobe`, every vanilla mesh archive)

"Effective normal" is the NIF slot, else the leaf BGSM `normal_texture`, following `merge.rs::fill` order. "Authored" is
the NIF SLSF1 bit OR the leaf BGSM bool, following the one-way OR in `asset_provider/material/mod.rs:91`.

| Game | Meshes with a normal | `_msn` and flagged | Rule fires (`_msn`, unflagged) | Flagged, not `_msn`-named |
|---|---|---|---|---|
| FO4 + DLC | 102,463 | 8,247 | **1,473** = 1,470 FaceCustomization heads + 3 dead-path bodies | 85 (DiamondCity `.btr`) |
| Skyrim SE | 84,906 | 4,060 | 0 | 9,596 (9,584 `.btr` + 12 misc) |
| Skyrim LE | 62,456 | 3,371 | 0 | 4,426 (4,416 `.btr` + 10 misc) |
| FO76 | 173,621 | 0 | 0 | 0 |
| FNV / FO3 / Oblivion | 58,135 / 49,263 / 15 | 0 | 0 | 0 |

`model_space_normals` also routes Skyrim slot 7 (`slot_role.rs:514-524`). The name rule feeds only the GPU flag. Because
zero Skyrim shapes fire, no role/flag divergence exists. FO4's slot 7 routing does not depend on the flag (`:536`).

## Findings

### HIGH

#### NIFAL-D1-2026-09-21b-01: Skyrim/FO4 `.btr` distant-terrain normal maps are model-space and authored so, but are bound through the tangent-space path, because the spawner lowers the `.btr` texture-only and drops the authored `Model_Space_Normals` bit
- **Severity**: HIGH. It hits rendering correctness on every Skyrim exterior's distant terrain, and it is always on. The magnitude below is derived from measured texels and the shader code, not from a captured frame, since engine launch is out of scope for this leg.
- **Dimension**: Material (texture-only lowering). **Owner**: `/audit-exterior` (EXAL terrain LOD) + nifal
- **Tier Violated**: no-leak and no-fabrication. The spawner imports the `.btr` NIF, so the authored
  `ImportedMaterial.model_space_normals = true` is in hand. It then replaces that authored bit with an undocumented
  tangent-space assumption.
- **Game Affected**: Skyrim LE/SE (every worldspace with `.btr`). FO4 DiamondCity (85 `_n`-named flagged quads).
  Commonwealth's `_msn` quads resolve nothing today.
- **Location**: `byroredux/src/cell_loader/terrain_lod_btr.rs`:
  - `:120-138`: the `btr_normal_path` doc, "Per-quad **tangent-space** normal map … Skyrim";
  - `:322-338`: binds `_n` into `MaterialTextureHandles.normal`;
  - `:391`: `normal_has_alpha: false`;
  - `:412`: `translate_texture_only_material(..)`, with no MSN bit.

  Render path: `byroredux/src/render/static_meshes.rs:534` / `:1019`. Shader: `crates/renderer/shaders/include/material_sampling.glsl`, `perturbNormal`.
- **Status**: NEW. Pre-existing since `d96110ebd` (2026-08-12, #2371), outside the delta, and found by the #4548 census.
- **Description**: The shader's MSN branch (`triangle.frag:599-628`, #3922) is keyed on `MAT_FLAG_MODEL_SPACE_NORMALS`.
  `.btr` entities never get that bit, so their per-quad maps take the tangent-space path. That path reconstructs z from xy
  and applies the TBN.
- **Evidence**:
  - **Authored flag** (BTR-only census): every normal-mapped `.btr` shape authors `Model_Space_Normals`, with zero
    exceptions. SE 9,584/9,584, LE 4,416/4,416, FO4 8,271/8,271.
  - **Texel basis** (`/tmp/audit/nifal/btr_basis_tamriel3.txt`, 60 Tamriel level-4 quads):
    - On 12,497 flat vertices (n.y > 0.97), the decoded texel has mean z −0.004 and mean |xy| 0.988. A tangent-space map
      would read about (0,0,1) there.
    - On 20,592 steep vertices, the best of 48 axis/sign mappings is texel (r,g,b) ↔ world (X, up, Y), i.e. renderer
      (r, g, −b). That is exactly the #3922 branch's `mn.z = -mn.z`. Cos is +0.657, against +0.142 for the identity mapping.
  - **Channel stats** (`tamriel.4.0.36_n.dds`, DXT5, alpha constant 255, so not DXT5nm): R 141.6±24.8, G 245.8±9.8,
    B 145.1±27.6. The maps are green-up.
- **Impact**: For the mean texel (0.11, 0.93), `perturbNormal` builds (0.11, 0.93, 0.35). On flat distant terrain the
  shading normal therefore leans about 69° toward the quad's V axis. Direct sun N·L then swings from about +0.9 to below
  zero depending on sun azimuth against a fixed world axis, across the whole distant landscape. A `render.debug` normal
  view on any Skyrim exterior would confirm this in seconds.
- **Related**: #3922 (MSN basis, fixed), #4548 (same defect class on FO4 faces, fixed), #2371, #2444, #3336, #4552 (same
  file). `docs/engine/exal.md:670` is also stale: it still lists the `.btr` normal map as a deferred follow-up.
- **Suggested Fix**: Carry the `.btr` shape's authored MSN bit into its canonical `Material`. Either extend the texture-only
  lowering with the authored flag, or lower the imported `.btr` material through `translate_material`. The maps are DXT5
  with an authored, signed blue axis (B 41..232), so also run the Phase-2 `resolve_msn_z_source` instead of hardcoding
  `normal_has_alpha: false`; reconstructing |z| would lose the sign. With MSN plumbed, FO4 `_msn` quads can be bound as well.
  The #2444 blocker the doc cites no longer applies, since these entities have carried a `Material` since #3336.

### MEDIUM

#### NIFAL-D2-2026-09-21b-01: #4549's gate is `is_finite()`-only on inputs — the translate step's own arithmetic can still manufacture ±inf from finite corrupt values, and nothing validates before the TLAS instance build
- **Severity**: MEDIUM. This is a defense-in-depth residual on a closed HIGH: the direct NaN/inf path is closed, and this one
  needs an overflowing product.
- **Dimension**: Geometry/Transform · **Tier Violated**: no-leak · **Game Affected**: all (corrupt or modded NIFs; vanilla incidence 0, the same premise as #4549)
- **Location**: `crates/nif/src/import/transform.rs:13-25` (`compose_transforms`); runtime `GlobalTransform` propagation
  (`crates/core/src/ecs/systems.rs`); `crates/renderer/src/vulkan/acceleration/predicates.rs:115-121`
  (`tlas_instance_transform` passes `draw_cmd.model_matrix` straight to `VkTransformMatrixKHR`).
- **Status**: NEW (a residual of closed #4549)
- **Description**: `compose_transforms` computes `scale = parent.scale * child.scale` and
  `translation = parent.rot * (parent.scale * child.trans) + parent.trans` with no check. Two finite corrupt scales
  (1e20 × 1e20) compose to +inf in f32. This happens at import on the cell path's flattened meshes and every frame on the
  loose path's propagation. A single finite scale of 1e36 overflows once the driver transforms any BLAS AABB wider than
  about 340 units. A grep finds no `is_finite`/`is_nan` in `acceleration/`, `render/static_meshes.rs`, `render/skinned.rs`,
  or the propagation system.
- **Evidence**: Among random corrupt 32-bit patterns, 1/256 (≈0.4%) are NaN/inf, which is what #4549 gates. About
  13/256 (≈5%) have an exponent ≥ 2^115: they are finite, pass the gate, and overflow on the first product. The ungated
  residual is therefore the more likely outcome of a random corruption.
- **Impact**: A non-finite TLAS instance transform or AABB, the #4166/#4549 class, re-enters through the boundary's own math.
- **Related**: #4549, #4396, #4397, #4166; NIF-D4-2026-09-21-02 (cited below)
- **Suggested Fix**: Validate the product rather than inventing a bound. Gate `compose_transforms`' output and
  `GlobalTransform` propagation on finiteness, or add one check at the convergence point (`tlas_instance_transform`, or the
  instance build) that drops the instance with a rate-limited warning. A magnitude clamp would need a measured bound.

### LOW

#### NIFAL-D1-2026-09-21b-02: The boundary's contract still describes a three-way `effect_shader_flags` union — #4548 made it four and recorded its texture-name classifier nowhere in the spec
- **Dimension**: Material · **Tier Violated**: no-fabrication (record-keeping) · **Game Affected**: all (FO4 in practice)
- **Location**: `byroredux/src/material_translate.rs`:
  - `:522-527`: the doc, which lists three contributors;
  - `:670-674`: "All three contributors";
  - `:2946-2953`: `translate_material_unions_all_three_effect_shader_flag_contributors`, name and doc.

  Also `docs/engine/asset-pipeline.md:374-386` (three contributors) and `docs/engine/nifal.md:618-640` (boundary
  pipeline; no name-keyed classifier; "Placement-only bits arrive separately through `extra_material_flags`").
- **Status**: NEW (introduced by `c0b740ce7`)
- **Description**: A path-name classifier at the canonical boundary is exactly what the spec's no-fabrication rule wants
  measured and recorded. The measurement exists now (the census table above) but lives only in this report.
- **Suggested Fix**: Update the doc, comment, and test name to four contributors. Record the rule and its census in
  `nifal.md` §Material and `asset-pipeline.md`.

#### NIFAL-D7-2026-09-21b-01: #4551's route pin counts its own string literals, so its non-vacuity floor cannot see a dropped route (and it scans only `load.rs`)
- **Dimension**: Animation · **Tier Violated**: harness-gap (the #4411 source-scan class) · **Game Affected**: Skyrim
- **Location**: `byroredux/src/cell_loader/load.rs:1306-1329` (`every_animation_route_installs_the_combat_family_too`)
- **Status**: NEW (introduced by `339bdb726`)
- **Evidence**: At HEAD each needle appears 3× in `load.rs`: walk at 636, 1019, and **1316** (the test literal); combat at
  640, 1020, and **1318** (literal).
  - Deleting a whole route (the walk and combat pair) leaves 2 == 2, and `walk >= 2` still holds. So the check the test
    labels "a dropped site is the bug this pin exists for" cannot fire.
  - A dropped combat sibling alone is still caught.
  - The exterior route (`scene/world_setup.rs:1011-1017`, spelled with different arguments) is outside the pin.
- **Suggested Fix**: Strip the `#[cfg(test)]` module before counting (as the material meta-guard does with
  `split_once("#[cfg(test)]")`) and floor at the real production count. Alternatively, fold the three installers into one
  helper called from all three routes.

#### NIFAL-D8-2026-09-21b-01: The BGSM merge's NIF-first texture precedence is an unsourced runtime claim — on FO4 it keeps a disagreeing NIF path over the leaf BGSM's on 1.1% of normal-mapped shapes, including NIF paths that exist in no vanilla archive
- **Dimension**: Shader flags / texture roles (merge boundary) · **Tier Violated**: no-fabrication · **Game Affected**: FO4
- **Location**: `byroredux/src/asset_provider/material/merge.rs:139-149` (`fill`: first-non-empty, NIF slot first) and its
  doc at `:151-158`. The doc says "NIF fields take precedence … matching Bethesda's runtime behaviour", introduced by
  `1b032b6bb` (#493) with no citation.
- **Status**: NEW. Pre-existing since 2026-04-20 and surfaced by this run's census.
- **Evidence** (`/tmp/audit/nifal/msn_fo4_precedence.txt`):
  - 56,560 FO4 shapes carry both a NIF normal slot and a leaf-BGSM `normal_texture`, and 637 (1.1%) name different files.
    For example, `bld03frontbrickres03.nif` has NIF `default_n.dds` against BGSM `concrete01a_n.dds`, and vault halls
    have `subwall02_n.dds` against `vltconcrete_08_n.dds`.
  - A hard sub-case: 3 shapes bind NIF-slot `textures\actors\character\basehumanfemale\femalebody_msn.dds`, which is
    absent from all 52 vanilla FO4 BA2s. Those shapes are `clothes\vaulttecsalesman\fcoat1stperson{,postwar}.nif` and
    `armor\raiderunderarmor\raiderunderarmorf1stperson.nif`.
  - Their BGSM `basehumanfemaleskin.bgsm` names `FemaleBody_n.DDS`, which is present. The dead path resolves to handle 0,
    so the shape gets no normal map (`asset_provider/texture.rs:712-737`). #4548 now also stamps MSN on these three shapes,
    but the bit is inert because `triangle.frag:573` gates on `normalMapIdx != 0u`.
- **Suggested Fix**: This report does not claim which side FO4's runtime honours; there is no in-repo source.
  - Source the rule (CK / NifSkope / nifly behaviour for a BGSM-named `BSLightingShaderProperty`) and cite it in the merge doc.
  - Whatever the answer, a spawn-time fallback to the BGSM chain's path when the NIF slot fails to resolve closes the
    dead-path sub-case.

#### NIFAL-D9-2026-09-21b-01: The #4523 dark-role census false-FAILS whenever Oblivion data is absent but any other game resolves — including this skill's own one-game-at-a-time Dim 9 invocation
- **Dimension**: Completeness · **Tier Violated**: harness-gap · **Game Affected**: all (the harness)
- **Location**: `crates/nif/tests/translation_completeness.rs:889-975`. The test skips only when `probed == 0`
  (`:955-958`); otherwise it asserts `total_dark == 8` (`:963`) and `oblivion_dark == 8` (`:970`) unconditionally.
- **Status**: NEW. Pre-existing since #4523; the morning run did not execute this test.
- **Evidence**: An FO4-only run panics with "left: 0, right: 8" (`/tmp/audit/nifal/d9_harness_fo4_head.log`). An
  Oblivion-only run finds 8 DARK HITs in 6 files and passes (`/tmp/audit/nifal/d9_dark_oblivion_only.log`). So the census
  holds and the failure is a partial-data artifact.
- **Impact**: Any `--ignored` run on a lane without Oblivion data reports a population "drift" that is really missing
  data. The test's own failure message points the reader at population change.
- **Suggested Fix**: Assert zero per resolved non-Oblivion game. Assert 8 only when Oblivion resolved, and print SKIP otherwise.

## Sibling-report findings on NIFAL surfaces (cited, not re-counted)

### REN-D6-2026-09-21-01 (MEDIUM, `AUDIT_RENDERER_2026-09-21.md`): `c0b740ce7` silenced `translate_material_copies_every_canonical_field` — NIFAL supplement
The new test sits between `#[test]` (`material_translate.rs:2739`) and the old fn (`:2777`). My HEAD run of
`cargo test -p byroredux --bin byroredux material_translate` gives 67 passed: baseline 66, +2 for the new test registered
twice, −1 for the silenced test. This leg adds:
1. **Blast radius, measured.** `/tmp/audit/nifal/dead_pin_analysis.py` replicates the meta-guard's own field extraction.
   - 34 of the 52 source-derived fields have their only asserting statement in the dead fn: `water_shader_flags`,
     `is_water_shader`, `emissive_*`, `specular_*`, `diffuse_color`, `ambient_color`, `glossiness`, `uv_*`,
     `env_map_scale`, `detail_neutral`, `vertex_color_mode`, `alpha_test`, `alpha_test_func`, `wireframe`,
     `flat_shading`, `z_*`, `translucency_*`, `glass_*`, `texture_clamp_mode`, `parallax_height_in_alpha`,
     `src_/dst_blend_mode`.
   - So do all 8 path-fed fields: `texture_path`, `normal_map`, `glow_map`, `detail_map`, `gloss_map`, `dark_map`,
     `greyscale_texture`, `material_path`. `normal_map` is the very field #4548's rule reads.
   - Outside the file, no test asserts these copies; the only external test caller, `cornell.rs:2084`, checks kind, PBR and IOR.
   - `every_source_derived_material_field_is_pinned_by_a_test` is green only because it counts the dead fn's statements.
     That is a new facet of open **#4411**. #4411's proposed fix (strip comments and strings) would not close it, because
     these are real `assert!` expressions in an uncalled fn. The guard must scope to `#[test]`-registered bodies.
2. **It would still pass if re-attached.** It passed at 4dca737e2 (`/tmp/audit/nifal/default-lane.log:61`). The only change
   to its function-under-test is `| msn_name_flag`, which cannot fire on its `Textures/Test/normal.dds` fixture, and its flag
   checks are bit-presence only. So the fix is a pure attribute move with nothing masked today. The harness has no negative
   MSN assertion; the new test's `_n.dds` control is the only over-fire pin.
3. **Why it landed.** Every test build prints the rustc warnings `duplicate_macro_attributes` (`:2748`) and `dead_code`
   (`:2777`), including CI's own lock-order job log (`/tmp/audit/concurrency/lockorder_job_head.log`). The only lint gate is
   `cargo clippy --workspace -- -D warnings` (`.github/workflows/ci.yml:172`), which has no `--all-targets`, so test-target
   warnings are never denied.
4. **Wrong doc provenance.** The new test cites "NIFAL-D7-2026-09-21-01 family", which is #4551 (Draugr clips); #4548 is a
   #3922 side-finding. The displaced doc block also makes the new test's rustdoc open with the old harness's contract text.

### REN-D1-2026-09-21-01 (HIGH, renderer report): `f97775ca8` removed the `alpha_blend → EFFECT` shadow-mask divert
NIFAL framing: the occluder decision is a per-instance render-time classification over canonical fields
(`acceleration/predicates.rs:976-1016`), which puts it in the no-render-time-fallback class. For FO3/FNV/Oblivion, the "not
solid" semantics (a NoLighting shader, additive blend) are only known at the translate boundary. The renderer report's
"Better" fix, a canonical non-occluder bit set at the NIFAL boundary, is the NIFAL-correct one.

### NIF-D4-2026-09-21-02 (`AUDIT_NIF_2026-09-21.md`, concurrent `/audit-nif` leg): `BSSkin::BoneData` bind transforms bypass #277 and #4549
`blocks/skin.rs:495-521` reads the raw 17 floats and passes them to `import/mesh/skin.rs:677-688`. This is the Skyrim
SE+/FO4/FO76/Starfield sibling of the `NiSkinData` path that the #4549 commit itself called out, and it is #4549's own
SIBLING checklist item. Vanilla bones are 0 non-finite (FO4 166,323; Starfield 288,208).

## Verified fixed in this window (for the next run's dedup)
- **#4549**:
  - `rotation.rs:297`: head gate.
  - `:241`/`:264`: the SVD `max_sv` gate and output-cell gate. The output gate matters because nalgebra's `max()` can skip a
    NaN element.
  - `stream.rs:753`/`:780`: translation/scale gate at both readers.
  - The 3 tests are green at HEAD (nif lib 1,342 passed).
- **#4550**:
  - `walk/emitter.rs:355-367`: an own ref that downcasts is decisive.
  - `blocks/particle.rs:1380`: the parser maps authored 0 to `None`.
  - `systems/particle.rs:139`: the overlay's `> 0` filter keeps the preset even for a synthetic `Some(0)`.
  - Both walkers share the fn, and the two-system fixture is green.
- **#4551**: `cell_loader/load.rs:640` and `:1020` install the combat family. `scene/world_setup.rs:1017`
  (`assemble_exterior_streaming`: startup, cell transition, save-load) already did.
- **#4548**: inside the single boundary, census-confined (table above).
- **#4546**: verified holding by the concurrency leg (`AUDIT_CONCURRENCY_2026-09-21.md:338`). #4547 is CI-workflow scope.
- **#4557 (OPEN) is resolved in code**:
  - `d54382415` rewrote both the #3987 narrative (`light_anim.rs:150-157`) and the test message (`:1016-1031`) to the
    current conservative set (`ARCHITECTURE | STATIC_PROP | DYNAMIC_ACTOR`).
  - A grep for the stale "ARCHITECTURE only" text finds nothing.
  - The commit did not reference the issue. **Close #4557.**

## Existing open issues re-checked
- **#4553**: the LOD clamp-unaware base resolve is unchanged, and `069989e77` (#4488) widens its scope to FO76 `.bto`
  (`object_lod.rs:495`).
- **#4556**: still stale at `types.rs:34`/`:42` and at `lighting.rs:258-259` (was `:252-254`).
- **#4411**: gains the dead-code facet above.
- **Unchanged**: #4552 (literals at `terrain_lod_btr.rs:394-395`), #4554, #4555, #4558, #4559, #4560, #4561, #4562, #4563,
  #4564, #4565, #4566, #4426, #4430, #4256, #4264, #4304, #4282.

## Documented-limitation ledger
Unchanged from the baseline's ledger (`AUDIT_NIFAL_2026-09-21.md` §Documented-limitation ledger); nothing in the delta
touches a parked item. The seven parked node fields still have zero canonical consumers (Dim 4 grep empty).

## Method notes
- **Tests run at HEAD** (1.96 toolchain, `-j 4`, cached builds):
  - `cargo test -p byroredux --bin byroredux material_translate`: 67 passed.
  - `… canonical_animation_completeness_harness every_hkx_sample_field every_animation_route_installs_the_combat_family_too`: 6 passed.
  - `… common_material_texture_walk_covers_every_secondary_role_once tint_alpha_gate`: 3 passed.
  - `cargo test -p byroredux-nif --test translation_completeness -- --ignored`: the FO4 lane passed; the dark census failed
    as D9b-01 describes, and an Oblivion-only rerun passed.
  - NIF-lib guards (dispatch coverage, light coverage, skin, slot_role, role enumeration, #4549 and #4550 tests) were read
    from the concurrent `/audit-nif` leg's HEAD run (`/tmp/audit/nif/cargo_test_nif.log`, 1,342 passed).
- **Censuses and probes**: scratch crate `/tmp/audit/nifal/msnprobe` (own target dir; reads the repo crates as path deps;
  read-only).
  - `msnprobe` for the `_msn` and precedence census, over all vanilla mesh archives of 7 games.
  - `btrbasis` for the `.btr` texel basis: 60 Tamriel quads, with the DDS decoder copied from `byroredux/examples/msn_basis_probe.rs`.
  - `f4check` for archive presence and BGSM fields.
  - No engine launch; no repo edits besides this file.
- **Scratch ID map**: dim_1 D1-b-03 → D1b-01 (HIGH); dim_1 D1-b-02 → D1b-02; dim_1 D1-b-01 = the REN-D6-01 supplement;
  dim_2 D2-b-02 → D2b-01; dim_2 D2-b-01 = NIF-D4-02.
- **`_audit-validate.sh`**: OK ("all path references valid"); only the standing advisories remain (`/tmp/audit/nifal/validate.log`).
- **Skill staleness** (route to `/audit-tech-debt`): the skill's "known-open" parentheticals still cite 11 closed issues:
  #4392, #4396, #4397, #4398, #4400, #4401, #4402, #4404, #4405, #4406, #4427. #4256, #4411, #4426 and #4430 are still open.

**Suggested publish labels** (domain `nifal` on all):
- **D1b-01**: `high` `bug` `renderer` `terrain-exterior` `game:skyrim` `game:fo4`
- **D2b-01**: `medium` `bug` `safety` `vulkan`
- **D1b-02**: `low` `documentation` `doc-rot`
- **D7b-01**: `low` `bug` `test-gap` `animation`
- **D8b-01**: `low` `bug` `import-pipeline` `game:fo4`
- **D9b-01**: `low` `bug` `test-gap`

Fold the REN-D6-01 supplement into that finding's issue body when the renderer report publishes. Close #4557.

## Next Step

```
/audit-publish docs/audits/AUDIT_NIFAL_2026-09-21b.md
```
