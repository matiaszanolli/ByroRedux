**HEAD**: a2c24b16e · **Baseline**: docs/audits/AUDIT_NIFAL_2026-09-29.md (HEAD 9fcfdc3fc) · **Audited**: Dims 1, 2, 3, 8, 9 (delta-touched) · **Unchanged since baseline (skimmed)**: Dims 4, 5, 6, 7 (no commits on Dim 5's paths; Dims 4/6/7 saw only clippy rewrites, a test-literal regrouping, or spawn-gate commits covered under Dim 3; first steps and guards run)

# NIFAL Audit — 2026-10-05

This is one leg of `/audit-suite --preset comprehensive`. One auditor ran all nine dimensions in sequence, with no sub-agents.
The delta is `9fcfdc3fc..a2c24b16e`, 313 commits. Scratch notes for each dimension are in `/tmp/audit/nifal/dim_{1..9}.md`, and
this report was reconciled against all nine of them.

## Executive Summary

**New findings: 7 (0 CRITICAL · 0 HIGH · 1 MEDIUM · 6 LOW). No regressions.**

All four baseline findings were published, fixed and verified in code:
- NIFAL-D1-2026-09-29-01 → #5099;
- NIFAL-D1-2026-09-29-02 → #5102;
- NIFAL-D5-2026-09-29-01 → #5103;
- NIFAL-D8-2026-09-29-01 → #5106.

Two of those fixes are incomplete (D9-01, and D1-01 as a sibling of #4441), and so is one renderer-filed NIFAL fix (#5196 → D8-01).

**Headline (MEDIUM, D8-01):** #5196 routes CDB `IsGlass` into the glass classifier's authoritative `bgem_glass` input, but that input
is still behind the classifier's `has_alpha || alpha_test` coverage gate. A Starfield shape gets `has_alpha` only from a
`NiAlphaProperty`, and the CDB merge forwards no blend state. A full-corpus census (188,936 imported Starfield shapes) found
3,449 glass-named shapes, of which only 29 have any alpha coverage. So the fix the commit describes ("an authored-glass CDB material
becomes `MATERIAL_KIND_GLASS` even when neither the texture path nor the node name carries a glass keyword") cannot fire for about
99% of the population it targets.

Cross-references to today's sibling reports (cited, not re-filed):
- **REN-D6-2026-10-05-01** (renderer): the #4441 rustdoc says Starfield stubs reach the classifier, which has been untrue for CDB
  hits since #5197. D1-01 below is the spec-side (`nifal.md`) sibling.
- **Existing #5210** plus the renderer addendum: the spawner guard now exempts the LOD-water `plane.entity` swap, while
  `nifal.md` §3 still says "exactly four" exemptions.
- **Existing #5211**: renderer-only, no NIFAL facet.
- **NIF-D4-2026-10-05-01 / -02** (nif): the #5189 `affected_node_names` doc wording, and the artifact-light count/spawn predicate
  split. D3-01 below is the NIFAL-ledger facet of the same light-scoping gap.

Per-category status against spec §2:

| Category | This delta |
|---|---|
| Material (D1) | Converged. #5099, #4634, #4873, #4912, #4917, #4441 and #5197 verified; 75/75 `material_translate` tests pass. 2 LOW: spec doc-rot and a redundant wrapper |
| Drawn mesh → medium | Declared by #5102 (§2 "Participating media", §3 fourth exemption). 1 LOW: the doc names six converters, the code chains seven |
| Geometry/Transform (D2) | #4633 verified: one sanitizer now also gates composed products. No findings |
| Skinning (D3) | Unchanged (clippy only); guards green |
| Lights (D3) | #4938, #5123 and #5189 verified. 1 LOW: affected-node scoping is untranslated for every light kind and is recorded only for ambient |
| Nodes (D4) | Unchanged; the seven parked fields still have 0 canonical consumers |
| Particles (D5) | Unchanged; still exactly 2 overlay callers; #5103 verified |
| Collision (D6) | Unchanged; 16 resolve arms; guard green |
| Animation (D7) | Unchanged (clippy-equivalent refactors); 2 production `AnimationClip` producers |
| Shader flags / roles / merge (D8) | #5106, #5012, #5190, #5196 (partial) and #5197 verified. 1 MEDIUM, 2 LOW, all on the new CDB arm |
| Completeness (D9) | Harness 2/2 green on all 8 games; the fill table is identical to the baseline |

Tier-invariant counts among the new findings:

| Invariant | Findings |
|---|---|
| single-boundary | D8-03 (CDB resolve tail copied at 4 sites; weak sense) |
| no-fabrication | 0 |
| no-leak | 0 |
| no-render-time-fallback | 0 (the `if game` grep over `triangle.frag` + `include/*.glsl` is empty) |
| parked-not-leak / translation gap | D8-01 (blend state untranslated, so glass is inert), D8-02 (emissive role zero-weighted), D3-01 (light scope) |
| doc / record-keeping | D1-01, D9-01; tech-debt D1-02 |

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary fn |
|---|---|---|---|---|---|
| Material | PASS: still 4 `translate_material` callers; texture-only family gained `_with_clamp` (in the spawner-guard needle list) | PASS: `NO_SIGNAL_NEUTRAL` is the classifier's own terminal, now named once (`material.rs:872-882`, `:1402`) | PASS | PASS | `translate_material` / `translate_texture_only_material{,_with_clamp,_with_authored_msn}` |
| Drawn mesh → medium | PASS: declared, cell path only by design | PASS: constants recorded as engine tuning | n/a | PASS (spawn-time) | `fog::*_from_mesh` via `CachedNifImport::beam_volumes` (D9-01: inventory drift) |
| Geometry/Transform | PASS: `compose_transforms` reuses `sanitize_transform_translation_and_scale` | PASS | PASS | PASS | `import/units.rs`, `import/transform.rs`, `rotation.rs` |
| Lights | PASS: one `spawn_nif_lights` for both load paths | PASS: the artifact skip is census-backed (48 carriers) | PASS | PASS | `walk/lights.rs` → `LightSource::from_legacy_world_units` (D3-01: scope untranslated) |
| Particles | PASS: 2 callers | PASS | PASS | PASS | `systems/particle.rs::apply_emitter_overlays` |
| Collision | PASS | PASS | PASS: 4 × `u32` summary | PASS | `import/collision/` |
| Animation | PASS | PASS | PASS | PASS | `convert_nif_clip` + `convert_hkx_clip` |
| Shader flags / roles / merge | PARTIAL (D8-03, weak) | PASS | PASS | PASS | `slot_role.rs::slot_to_role`, `merge.rs::{fill, apply_cdb_material}` |

## Findings

### MEDIUM

#### NIFAL-D8-2026-10-05-01: #5196's `IsGlass` → `bgem_glass` route is gated out for ~99% of Starfield glass, because the CDB merge forwards no blend state and Starfield shapes almost never carry `NiAlphaProperty`
- **Severity**: MEDIUM. This uses the "translatable data dropped" row: authored blend state is never captured, and the glass renders
  opaque rather than disappearing. It is the same lineage and impact as #5196, which was MEDIUM.
- **Dimension**: Shader-flags/Effects (CDB merge boundary) → Material
- **Tier Violated**: parked-not-leak (an authored translation that is wired but cannot fire)
- **Game Affected**: Starfield
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:324-327`: `if cdb_mat.is_glass == Some(true) { material.bgem_glass = true; }`;
  - `byroredux/src/material_translate.rs:844-856`: classifier call with coverage `source.has_alpha || source.alpha_test`;
  - `byroredux/src/helpers.rs`: `classify_glass_into_material_with_provenance`, `if !has_transparent_coverage || is_decal { return; }`.
    This runs before the `bgem_glass` check, and only `is_mirror_pane` precedes it;
  - `crates/nif/src/import/material/mod.rs:1398-1417`: `effective_alpha_blend`, whose only source is `NiAlphaProperty`;
  - `crates/sfmaterial/src/index.rs:416-443`: capture of `AlphaSettingsComponent` (threshold and `HasOpacity` only) and
    `EffectSettingsComponent` (`IsGlass` only).
- **Status**: NEW. This is an incomplete fix of #5196 (closed by `978d25c19`). The blend-field capture itself falls within open #3398.
- **Description**: The classifier only promotes to glass when the surface has transparent coverage. That gate is what keeps an
  opaque `PawnShopWindow` out of the glass path, and it applies to the authoritative `bgem_glass` input too.
  - FO4 faced the same problem and solved it at the merge boundary: the BGSM and BGEM arms forward the material file's
    `alpha_blend_mode` into `has_alpha` (`merge.rs:1266-1271`, `:1560-1564`). The comment there reads "FO4+ moved per-material
    blend state out of NiAlphaProperty into BGSM … every Institute / lab pane renders fully opaque".
  - Starfield moved blend state into the CDB. `docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md:176-182` lists
    `AlphaSettingsComponent.Blender` → `AlphaBlenderSettings.Mode` and `EffectSettingsComponent.BlendingMode` (`"AlphaBlend"`).
  - `apply_cdb_material` captures neither field. It sets `alpha_test` only from `AlphaTestThreshold`, so CDB glass reaches the
    classifier with `has_alpha = false`, and the early return fires before `bgem_glass` is consulted.
- **Evidence**:
  - Per-block baseline (`crates/nif/tests/data/block_coverage_baselines/starfield.tsv`): 884 `NiAlphaProperty` against 190,549
    `BSGeometry`.
  - Full-corpus import census (a scratch tool over `Starfield - Meshes01/02/Patch.ba2` with the `.mesh` resolver, output in
    `/tmp/audit/nifal/sf_glass_census.txt`):
    - 188,936 shapes, of which `has_alpha` is set on 477 and `alpha_test` on 122.
    - **3,449 glass-named shapes** (material path or node name contains `glass`; 3,447 have a `.mat` path), of which only 23 are
      `has_alpha` and 6 are `alpha_test`.
    - Sample: `thelodgeexterior01.nif`'s `materials\…\naglasscleanopaque01_blue01.mat` (×12 sub-shapes).
  - The census cannot say which of those materials carry `IsGlass = true`. The CDB join needs the >10 GB `cdb_join_probe`.
  - The #5196 regression test `is_glass_routes_to_the_classifier_signal_without_thin_shell` asserts only the `bgem_glass` bool,
    never `MATERIAL_KIND_GLASS` out of `translate_material`, so the inert end-to-end path stays green.
- **Impact**: Authored Starfield glass (windows, visors, bottles) still renders as opaque kind-0 dielectric with the CDB colour
  texture: the exact symptom #5196 was filed for ("Authored Starfield glass is mostly not glass"). Any other alpha-blended
  Starfield material (effect cards, foliage cut-outs that use blending rather than test) is opaque for the same reason.
- **Related**: #5196 (closed), #3398 (open), #1823/#1651 (BGSM blend forwarding precedent), #4283, REN-D6-2026-10-03-02.
- **Suggested Fix**: Capture `EffectSettingsComponent.BlendingMode` and `AlphaSettingsComponent.Blender.Mode` in `MaterialIndex`,
  and forward them in `apply_cdb_material` to `has_alpha` plus `src/dst_blend_mode`, as the BGSM arm does (each enum string gets
  an explicit arm and a documented default). Add a merge → `translate_material` test asserting that a CDB `IsGlass` + `AlphaBlend`
  material classifies as `MATERIAL_KIND_GLASS`. If the blend capture is deferred to #3398, reopen #5196 or record the dependency
  there, because the current test certifies a path that cannot fire.

### LOW

#### NIFAL-D1-2026-10-05-01: `nifal.md`'s lowering step 3 still calls `resolve_pbr`'s NaN classifier arm "a backstop for future non-pre-classified sources", which is the third copy of the claim #4441 corrected in rustdoc
- **Severity**: LOW · **Dimension**: Material · **Tier Violated**: doc / record-keeping · **Game Affected**: FO4, FO76, Starfield
- **Location**: `docs/engine/nifal.md:854-859`
- **Status**: NEW. This is an incomplete sibling of #4441 (closed by `c2b67d81e`, which touched only `material_translate.rs` and
  `material.rs`). The rustdoc half is REN-D6-2026-10-05-01.
- **Description**: The spec says "For NIF-imported content … `Some(…)` is always present and `Material::resolve_pbr()` only clamps —
  its classifier arm (the `NaN` sentinel path) is a backstop for future non-pre-classified sources." Both halves are false today:
  - #2707's Starfield material-reference stubs leave both overrides `None` at NIF import;
  - the BGEM merge deliberately leaves them NaN.

  The paragraph also does not record #5197: a CDB **hit** stamps `PbrMaterial::NO_SIGNAL_NEUTRAL` (`merge.rs:219-236`), so only CDB
  misses and BGEM reach the classifier. This is the deletion-inviting text #4284 and #4441 were filed against, left standing in
  the spec that auditors read first.
- **Evidence**: `grep -n backstop docs/engine/nifal.md` → `:858`. The commit body of `c2b67d81e` lists only the two rustdoc
  statements.
- **Impact**: A contributor working from the spec concludes the classifier arm is dead for current content.
- **Related**: #4441, #4284, #5197, REN-D6-2026-10-05-01, #5210 (the same doc pass).
- **Suggested Fix**: Rewrite step 3 to name the live NaN producers (BGEM, and Starfield stubs whose `.mat` misses the CDB or that
  run with no CDB) and the `NO_SIGNAL_NEUTRAL` CDB-hit outcome. Fold this into #5210 and REN-D6-2026-10-05-01.

#### NIFAL-D1-2026-10-05-02: #4912 left two names for one texture-only lowering: `translate_texture_only_material_with_authored_msn` is a pure forward to a private `…_with_authored_msn_and_clamp` with the identical signature
- **Severity**: LOW · **Dimension**: Material · **Tier Violated**: none (tech-debt) · **Game Affected**: all (terrain/LOD)
- **Location**: `byroredux/src/material_translate.rs:1038-1067`
- **Status**: NEW (introduced by `235a90ba2`)
- **Description**: `translate_texture_only_material_with_authored_msn(texture_path, model_space_normals, texture_clamp_mode)` does
  nothing except call `translate_texture_only_material_with_authored_msn_and_clamp` with the same three arguments. The texture-only
  boundary now spans `translate_texture_only_material`, `_with_clamp`, `_with_authored_msn` and the private `_and_clamp`, plus
  `_inner`. Every public name must also be kept in `every_exterior_spawner_inserts_a_boundary_material`'s `boundary_fns` needle
  list (`:2516-2525`), which grew by one in the same commit.
- **Impact**: Maintenance only. A further variant would grow the needle list again, and a missed needle fails the guard closed.
- **Suggested Fix**: Move the body into `translate_texture_only_material_with_authored_msn` and delete `_and_clamp`, with
  `_with_clamp` calling it with `false`.

#### NIFAL-D3-2026-10-05-01: Affected-node light scoping is untranslated for every `NiLight` kind, but `nifal.md` parks it only for `NiAmbientLight`; #5189's name allowlist is the only mitigation and is recorded nowhere in the spec
- **Severity**: LOW · **Dimension**: Skinning/Lights · **Tier Violated**: parked-not-leak (record-keeping) · **Game Affected**: Oblivion, FO3, FNV, Skyrim (pre-FO4 node `effects` lists)
- **Location**:
  - `docs/engine/nifal.md:268-286` (the only scoping note, ambient-only);
  - `crates/nif/src/blocks/node.rs:22,83` (`NiNode.effects` parsed for bsver < FO4);
  - `crates/nif/src/import/walk/texture_effect.rs:18-54` (the only walker of `effects`);
  - `byroredux/src/cell_loader/spawn.rs:1099-1101` and `:1146-1163` (the allowlist skip).
- **Status**: NEW. Related to #5189 (closed) and NIF-D4-2026-10-05-01/-02 (the doc wording and the count predicate; not this gap).
- **Description**: #5189 established, and NIF-D4-2026-10-05-01 refined, that Gamebyro scopes every `NiDynamicEffect` to its
  affected-node subtrees. Oblivion-era content writes the on-light list empty and registers scope on `NiNode.effects`; 47 root
  nodes list the artifact lights there.
  - The importer parses `NiNode.effects`, but walks it only for texture effects. `ImportedLight` therefore carries no scope, and
    every point/spot/directional NIF light spawns unscoped.
  - The fix for the one known case is a consumer-side, census-backed name skip (`__MAX_Default_Light`, 48 Oblivion carriers).
  - `nifal.md`'s Lights section is marked **converged**. It mentions affected-node scoping only as an `NiAmbientLight` parking
    note ("none with `affected_node_names`"). It records neither the general gap nor the spawn-gate allowlist.
- **Evidence**: `grep -rn affected_node byroredux/src crates/renderer/src` finds no consumer. The `effects` walkers are in
  `walk/texture_effect.rs` only.
- **Impact**: There is no known vanilla population beyond the allowlisted artifact. A scoped non-artifact light (modded, or an
  unsurveyed vanilla mesh) would light the whole scene. The next audit cannot tell parked from dropped.
- **Related**: #5189, #5123, #3557, NIF-D4-2026-10-05-01/-02, the baseline ledger row for `NiAmbientLight`.
- **Suggested Fix**: Add a Lights ledger row in `nifal.md` §2: "affected-node scope (on-light list and pre-FO4 `NiNode.effects`)
  is not translated for any kind; the exporter artifact is dropped by name at `spawn_nif_lights` (#5189)". Optionally census
  `effects`-listed non-artifact lights with `crates/nif/examples/ambient_light_census.rs`'s approach.

#### NIFAL-D8-2026-10-05-02: The CDB `SLOT_EMISSIVE` → `emissive` role is zero-weighted, because `EmissiveSettingsComponent` is not captured and the stub's emissive colour and multiplier stay 0, so the texture is uploaded and bound for no contribution
- **Severity**: LOW · **Dimension**: Shader-flags/Effects (CDB merge) · **Tier Violated**: parked-not-leak · **Game Affected**: Starfield
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:258-264` (emissive `fill`);
  - `crates/nif/src/import/types.rs:911-912` (`emissive_color: [0.0; 3]`, `emissive_mult: 0.0` defaults);
  - `crates/renderer/shaders/triangle.frag:1512-1517` and `:1689-1693` (the glow sample becomes `emissiveMask`, applied only when
    `emissiveMult > 0.01 && emissiveLum > 0.01`, as `emissiveColor * emissiveMult * emissiveMask`).
- **Status**: NEW (introduced by `18fce7e43`)
- **Description**: `apply_cdb_material` presents slot 7 as a landed canonical role ("Pure role translation … 7=emissive land in
  their canonical `MaterialTextureSet` roles"). The CDB's `EmissiveSettingsComponent` (`Enabled`, `EmissiveTint`,
  `LuminousEmittance`, per the spike table) is not captured. A material-reference stub carries no inline emissive data, so the
  bound texture is multiplied by zero. Unlike the five parked single-channel kinds, this role is neither parked nor effective.
- **Impact**: No Starfield surface glows from its CDB emissive map, and every such map still costs a texture upload and bindless
  slot. Diagnostics (`tex.loaded`, `mat.dump`) show an emissive texture bound, which reads as "emission translated".
- **Related**: #3398, #4429 (the parked-kinds precedent), #5210.
- **Suggested Fix**: Either capture `EmissiveSettingsComponent` (enable plus tint/emittance → `emissive_color`/`emissive_mult`
  with an `EmissiveSource`) or skip `SLOT_EMISSIVE` and list it in `nifal.md`'s parked table until the scalars land.

#### NIFAL-D8-2026-10-05-03: The CDB resolve tail is copied at four sites in `merge.rs`, and the two arm copies skip the `trace_merge_outcome` telemetry the #4289 doc says covers every `PresenceOnly` return
- **Severity**: LOW · **Dimension**: Shader-flags/Effects (merge boundary) · **Tier Violated**: single-boundary (weak, intra-module duplication) · **Game Affected**: Starfield
- **Location**: `byroredux/src/asset_provider/material/merge.rs:482-493` (`.mat`), `:588-601` (unknown kind), `:698-709` (BGSM
  miss), `:1371-1381` (BGEM miss); the doc at `:66-85`.
- **Status**: NEW. `978d25c19` and `18fce7e43` turned the former one-line `apply_cdb_pbr_fallback` tail into this 4×12-line
  block.
- **Description**: Each site repeats `lookup_cdb_material` → `apply_cdb_material` → `Merged`/`PresenceOnly` from `touched`, with
  `apply_cdb_pbr_fallback` on a miss. The BGSM and BGEM arm copies return `Some(outcome)` without `trace_merge_outcome`. Their
  `PresenceOnly` fallback returns were also untraced before this window. The next CDB-arm change (for example D8-01's blend
  forwarding, or recording CDB texture provenance) has to land identically in four places.
- **Suggested Fix**: Extract one private `resolve_through_cdb(material, provider, pool, path, touched, texture_exists) ->
  MergeOutcome` that traces, and call it from all four sites.

#### NIFAL-D9-2026-10-05-01: #5102's declared mesh→medium boundary lists six beam converters; `CachedNifImport::beam_volumes` chains seven, and the FO4 lamp-shaft converter is the one left out
- **Severity**: LOW · **Dimension**: Completeness · **Tier Violated**: doc / record-keeping · **Game Affected**: FO4
- **Location**: `docs/engine/nifal.md:137-143` versus `byroredux/src/cell_loader/nif_import_registry.rs:389-405`;
  `byroredux/src/fog.rs:1376` (`fo4_ambient_lamp_beam_volume_from_mesh`).
- **Status**: NEW (incomplete fix of #5102, closed by `cb0aaf58c`)
- **Description**: The spec names `fnv_nellis_hangar_beam_volumes_from_mesh`, `window_…`, `oblivion_dungeon_…`,
  `authored_cone_…`, `fnv_superwide_…` and `vault_window_beam_volume_from_mesh`. The chain's seventh arm,
  `fo4_ambient_lamp_beam_volume_from_mesh`, replaces FO4 `effects\ambient\` emergency, fluorescent and dusty lamp-shaft submeshes.
  It includes `emergencylightbeam01.nif` (242/1080), the very signature the #5102 source finding cited. Its media constants
  (extinction clamped 0.001–0.3, `edge_softness` 0.5) are also absent from the doc's constant list.
- **Impact**: The declared inventory under-reports which titles lose drawn submeshes on the cell path. It omits FO4 entirely, and
  FO4 is the largest population.
- **Related**: #5102, #4809, `docs/engine/interior-godrays-status.md`.
- **Suggested Fix**: Add the seventh converter and its constants. Consider a doc-pin test that counts the `beam_volumes` chain arms
  against the named list.

## Existing open issues re-checked
- **#4246** (OPEN): still false. `bd25d805b` (#4873) **widened** the claim from "both resolvers" to "all three resolvers read their
  inputs out of that component and early-return without it". The widened claim is in the module doc
  (`material_translate.rs:48-56`) and in `nifal.md:799-806`, but `resolve_unresolved_gloss_neutral_roughness` still uses
  `.unwrap_or(0)` (`:1396-1399`) and does not early-return without handles. It still has no live effect: neither LOD caller
  invokes it. Fold into #4246.
- **#5210** (OPEN): the `merge.rs:416-422` "Phase 1 (this commit)" and `:474-476` "Phase 2 should return `Merged`" comments
  stand, as does `nifal.md:679-690` ("zero Starfield texture roles are produced"). The renderer addendum (the fifth guard
  exemption, `cell_loader/water.rs` `plane.entity`) is confirmed at `material_translate.rs:2503-2506` and `:2592-2601`.
- **#5230** (OPEN): unchanged. `helpers.rs` mirror 0.04 versus the `unresolved_gloss_neutral_roughness` floor test.
- **#4256, #4430, #3398** (OPEN): unchanged. The `#4430` comment is at `merge.rs:908-911`.

## Verified fixed in this window (for the next run's dedup)
- **#5099**: `nifal.md` §3 step 4 names `_with_provenance` and the provenance pair. The rustdoc link is fixed. `object_lod.rs` is
  in the Phase-2 exempt list.
- **#5102**: the mesh → participating medium tier is declared in §2 and §3 (with the D9-01 count gap).
- **#5103**: `nifal.md:444-446` says only the budget scan stays whole-scene.
- **#5106**: the LUT `fill` runs first, and the winner is decided from the slot change (`merge.rs:993-1011`).
- **#5012**: the near-mirror fallback is gated on `leaf.specular_enabled` (`merge.rs:1349`).
- **#5197**: `PbrMaterial::NO_SIGNAL_NEUTRAL`, stamped only where the overrides are `None`. `classify_pbr_keyword`'s terminal
  uses the same const.
- **#5190**: only a `SLOT_COLOR` replacement reaches `diffuse_color`, and only with no colour texture. Pinned by 3 tests.
- **#5196**: `thin_glass` is no longer forced, `UseSSS` is parked, and alpha test is gated on `HasOpacity`. The glass half is
  inert per D8-01.
- **#4441**: both rustdoc copies are corrected; the spec copy is not (D1-01).
- **#4634**: the four-way union doc is in place, and `translate_material_unions_all_four_effect_shader_flag_contributors` pins
  contributor 4 in its own call.
- **#4873**: the `terrain_lod_btr.rs` Phase-2 caller is documented.
- **#4917**: the stripper keeps production code after an out-of-line `#[cfg(test)] mod x;`, with a new test.
- **#4912**: the texture-only clamp is carried at the boundary.
- **#4633**: `compose_transforms` passes its output through the shared #4549 sanitizer.
- **#4938**: `spawn_nif_lights` consumes the canonical falloff lane on the cell path and gets an explicit 1.0 on the loose path.
- **#5123 / #5189**: the artifact light never spawns.
- **#5003**: the parked-kinds XOR guard is anchored to the comment span.

## Documented-limitation ledger (parked, not leaks)
The baseline ledger (`AUDIT_NIFAL_2026-09-29.md` and `AUDIT_NIFAL_2026-09-21.md`) stands, with these updates:
- **Unchanged rows**:
  - the seven parked node fields have 0 canonical consumers;
  - BhkNP blob, phantoms and undecoded constraint kinds;
  - `BhkPlaneShape` → `None`;
  - size-over-life curve and particle `initial_color`;
  - ambient/morph animation channels;
  - `NiAmbientLight` scoping;
  - `NiSpotLight` inner angle and exponent;
  - FO76/Starfield `tex`/`nrm` 0% fill is structural.
- **NEW (D3-01)**: affected-node scope for all light kinds is untranslated; the exporter-artifact name skip is the only handling.
- **NEW (D8-02)**: the CDB emissive role is filled but zero-weighted until `EmissiveSettingsComponent` is captured.
- **NEW (observation)**:
  - CDB-filled roles keep `NifTextureSet` provenance: no `record_external_texture_sources` call on CDB returns. The only
    consumers are the `commands/assets.rs` and `commands/scene.rs` diagnostics.
  - #5190 deliberately keeps a bound colour texture over an enabled `SLOT_COLOR` `TextureReplacement`. The co-occurrence is
    unmeasured.
- Starfield CDB metalness/roughness stay parked: `MaterialParamFloat` index semantics are unverified (#3398). A hit gets the
  no-signal neutral.

## Dimension 9: harness table (release, `--ignored`, all 8 games; 2 passed in 64.57 s)

Identical to the 2026-09-29 table:

| game | meshes | tex% | mat_path% | m_kind% | metO% | spec% | gloss% | env% | nrm% | tan% |
|---|---|---|---|---|---|---|---|---|---|---|
| Oblivion | 567 | 91.4 | 0.0 | 0.0 | 100.0 | 100.0 | 0.0 | 0.0 | 0.0 | 84.0 |
| FO3 | 687 | 92.6 | 0.0 | 10.9 | 94.3 | 16.7 | 0.0 | 4.1 | 79.2 | 99.1 |
| FNV | 806 | 95.3 | 0.0 | 17.6 | 96.7 | 9.8 | 0.0 | 4.0 | 78.9 | 99.3 |
| SkyrimLE | 529 | 92.4 | 0.0 | 42.3 | 92.4 | 67.7 | 0.0 | 7.0 | 67.7 | 94.0 |
| SkyrimSE | 515 | 93.8 | 0.0 | 35.5 | 93.8 | 76.1 | 0.0 | 6.8 | 76.1 | 94.8 |
| FO4 | 644 | 92.7 | 57.9 | 57.0 | 99.4 | 82.6 | 73.9 | 48.3 | 82.9 | 96.3 |
| FO76 | 773 | 11.3 | 83.8 | 33.1 | 15.8 | 7.8 | 0.0 | 7.1 | 8.8 | 96.9 |
| Starfield | 811 | 0.0 | 94.9 | 3.7 | 5.1 | 1.4 | 0.0 | 3.7 | 0.0 | 100.0 |

The dark-role census found 8 hits, all in Oblivion.

Note: the harness measures the **pre-merge** tier. All of the CDB-arm findings above (D8-01/02/03) are post-merge, so it cannot see them.

## Method notes
- **Tests run at HEAD**:
  - `cargo test -p byroredux --bin byroredux` (toolchain 1.96.0) with the NIFAL guard filters: 174 passed. This includes 75
    `material_translate` tests, 89 tests matching `merge::`, the animation harness (4), the HKX sample test, the overlay value/scan
    pins, the secondary-role walk, the tint alpha gates and the spawner/marker guards.
  - `cargo test -p byroredux-nif --lib` guard lane: 33 passed.
  - `dispatch_coverage_tests`: 4 passed.
  - `cargo test -p byroredux-nif --release --test translation_completeness -- --ignored`: 2 passed.
  - Logs: `/tmp/audit/nifal/test_byroredux.log`, `test_nif.log`, `d9_harness.log`.
- **Starfield census for D8-01**: a scratch-only cargo project under the session scratchpad (path deps on `byroredux-nif`,
  `byroredux-bsa` and `byroredux-core`; nothing added to the repo), run over the three Starfield mesh BA2s with a `.mesh`
  resolver. Output: `/tmp/audit/nifal/sf_glass_census.txt`.
- **No engine launch**, no GPU process, no plugin `--ignored` tests, and no repo edits besides this file. `git status` shows only
  untracked audit reports.
- **`_audit-validate.sh`**: OK, all path references are valid. Only the standing advisories remain
  (`/tmp/audit/nifal/validate.log`).
- **Dedup**: `/tmp/audit/issues.json` (97 open) plus `gh issue list --state all --search`. Searched: backstop, affected node,
  CDB emissive, EmissiveSettings, beam converter, `fo4_ambient_lamp`, IsGlass, BlendingMode, Starfield alpha blend, the texture-only
  wrapper name, `lookup_cdb_material`. Also checked today's sibling reports: RENDERER (REN-D6-01, #5210 addendum, #5211), NIF
  (NIF-D4-01/-02), ECS, PERFORMANCE, CONCURRENCY and SAFETY.
- **Skill staleness** (route to the next `/audit-sync`):
  - Dim 1 still says the `texture_clamp_mode` default is 0. It is 3 on both `ImportedMaterial` and `Material`, and the
    texture-only path now carries an explicit clamp (#4912).
  - Dim 9's boundary inventory should cite the seven-arm beam chain.
  - Dim 3 should mention the #5189 allowlist alongside the `NiAmbientLight` parking.

**Suggested publish labels** (domain `nifal` on all):
- **D8-01**: `medium` `bug` `import-pipeline` `game:starfield`
- **D1-01**: `low` `documentation` `doc-rot`
- **D1-02**: `low` `tech-debt`
- **D3-01**: `low` `documentation` `legacy-compat` `game:oblivion`
- **D8-02**: `low` `bug` `import-pipeline` `game:starfield`
- **D8-03**: `low` `tech-debt` `game:starfield`
- **D9-01**: `low` `documentation` `doc-rot` `game:fo4`

## Next Step

```
/audit-publish docs/audits/AUDIT_NIFAL_2026-10-05.md
```
