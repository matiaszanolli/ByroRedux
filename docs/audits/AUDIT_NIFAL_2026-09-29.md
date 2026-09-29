**HEAD**: 9fcfdc3fc · **Baseline**: docs/audits/AUDIT_NIFAL_2026-09-21b.md (HEAD f97775ca8) · **Audited**: Dims 1, 2, 3, 5, 7, 8, 9 (delta-touched) · **Unchanged since baseline (skimmed)**: Dims 4, 6 (only adjacent refactors / a guard hardening on their paths; guards run and spot-checked)

# NIFAL Audit — 2026-09-29

This is one leg of `/audit-suite --preset comprehensive`. One auditor ran all nine dimensions in sequence, with no sub-agents.
The delta is `f97775ca8..9fcfdc3fc`, 312 commits, and every dimension's `Paths:` saw at least one commit. Scratch notes for
each dimension are in `/tmp/audit/nifal/dim_{1..9}.md`, and this report was reconciled against all nine of them.

Several NIFAL fixes landed under commit messages that name no issue. An auditor keying on `Fix #N` would miss them:

| Commit (message) | Actually contains |
|---|---|
| `b9e961eeb` ("Enhance physical lighting documentation…") | New `crates/nif/src/import/units.rs`: Starfield metres → 70 BU at the import boundary. Also the BSGeometry Z-up → Y-up basis move. |
| `2b1b7fc5c` ("Refactor material classification functions…") | #4855: the lit-dispatch glass guard now reads `from_bgsm`. #4856: the spawner guard counts per function. Spot-fixture degrees for #4859. |
| `ab255cfd2` ("Refactor and enhance shader and ABI consistency…") | #4859: `NiSpotLight` outer angle converted degrees → radians. |
| `b7491072f` ("Refactor code structure and remove redundant changes") | #4620: the #3329 sequence rate tier is now matched per instance. |

## Executive Summary

**New findings: 4 (0 CRITICAL · 0 HIGH · 0 MEDIUM · 4 LOW). No regressions.**
- 1 concurrent sibling finding on a NIFAL surface is cited, not re-counted: NIF-D4-2026-09-29-01.
- 10 open issues were re-checked. All are still open in code except as noted below.

**Every NIFAL fix in the window holds.** Each item below was verified in code, and by a green guard where one exists:
- #4579: the kitchen-sink copy test is registered again.
- #4411: both pin scans now read production code only.
- #4553: the LOD clamp is taken from the canonical material.
- #4632: `.btr` MSN (model-space normals) plus `resolve_msn_z_source`.
- #4282 / #4283: wetness and luminance sinks, and the glass-provenance split.
- #4855: the glass-promotion gate.
- #4859: spot-light angle units.
- #4636 / #4654 / #4836 / #4941 / #4428: the BGSM merge fixes.
- #4426: flipbook per-role resolve.
- #4560 / #4561 / #4620: emitter-rate attribution.
- #4563: the HKX policy is decided inside the boundary.
- #4635 / #4637 / #4965: harness fixes.

The full real-data completeness harness passed on all 8 installed games (table in the Dim 9 section).

**Headline:** nothing HIGH. The largest structural observation is NIFAL-D1-2026-09-29-02. The interior-godray work (`0572bfd5a`)
added a **mesh → `FogVolume` replacement tier**: six asset-signature converters plus the older fog-token converter. They remove
drawn submeshes on the **cell path only**. That is an undeclared category and a fourth drawn-surface exemption that `nifal.md`
§3 says does not exist. The loose-NIF viewer still draws those submeshes as painted cards.

Per-category status against spec §2:

| Category | This delta |
|---|---|
| Material (D1) | Converged. 4 production callers plus the texture-only lowering. 74/74 `material_translate` tests pass. 2 LOW: doc-rot, and the undeclared mesh→medium tier |
| Geometry/Transform (D2) | Converged. The new Starfield units boundary is applied exactly once. The BSGeometry basis is now consistent with nodes and skin. #4633 is still open |
| Skinning (D3) | Unchanged; guards green |
| Lights (D3) | #4859 verified. Spot inner angle and exponent are parsed but not translated. They are latent (0 vanilla `NiSpotLight`) and go in the ledger |
| Nodes (D4) | Unchanged; the parked-field grep is empty |
| Particles (D5) | #4560/#4561/#4620 verified; still exactly 2 overlay callers. 1 LOW doc-rot |
| Collision (D6) | Unchanged; the dispatch↔resolve guard is green and hardened |
| Animation (D7) | #4563 and #4635 verified; no third canonical-clip producer |
| Shader flags / roles (D8) | 8 merge / role fixes verified. 1 LOW latent precedence split between #4636 and #4402 |
| Completeness (D9) | Harness 2/2 green on all 8 games; ceilings hold |

Tier-invariant counts among the new findings:

| Invariant | Findings |
|---|---|
| single-boundary | D1-02 (load-path divergence), D8-01 (two precedence decisions for one slot) |
| no-fabrication | D1-02 (uncited media constants; weak sense) |
| no-leak | 0 |
| no-render-time-fallback | 0 (`if game` grep over `triangle.frag` + `include/*.glsl` is empty) |
| doc / record-keeping | D1-01, D5-01 |

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary fn |
|---|---|---|---|---|---|
| Material | PASS: production `Material {` literals exist only at `material_translate.rs:645` and `:1025`. Callers: `nif_loader.rs:1294`, `mesh_instance.rs:1110`, `placement_lod.rs:552`, `object_lod.rs:334` | PASS | PASS: `pbr_classified_at_import` is now the #4229 provenance signal | PASS | `translate_material` / `translate_texture_only_material{,_with_authored_msn}` |
| Drawn mesh → medium | **FAIL (D1-02)**: cell path only | PARTIAL: constants uncited | n/a | PASS (spawn-time) | `fog::*_volume_from_mesh` via `CachedNifImport::beam_volumes` (undeclared) |
| Geometry/Transform | PASS: `units::meshes/hierarchy/animation` run once per entry | PASS: 70 BU/m and HAVOK_SCALE are both sourced | PARTIAL: #4633 still open | PASS | `import/units.rs`, `zup_to_yup_pos`, `rotation.rs` |
| Lights | PASS | PASS: #4859 is sourced from nif.xml `#F_DEG#` and `NiSpotLight.inl` | PASS | PASS | `walk/lights.rs` → `LightSource::from_legacy_world_units` |
| Particles | PASS: 2 callers | PASS | PASS | PASS | `systems/particle.rs::apply_emitter_overlays` |
| Collision | PASS | PASS | PASS: the summary carries 4 × `u32` | PASS | `import/collision/` |
| Animation | PASS: no post-conversion policy override | PASS | PASS | PASS | `convert_nif_clip` + `convert_hkx_clip` |
| Shader flags / roles | **PARTIAL (D8-01)** | PASS: #4636 now records its precedence as an engine choice | PASS | PASS | `slot_role.rs::slot_to_role`, `merge.rs::fill` |

## Findings

### LOW

#### NIFAL-D1-2026-09-29-01: The spec names a `#[cfg(test)]`-only glass classifier as the live boundary step, and #4855's `from_bgsm` provenance input is recorded nowhere
- **Severity**: LOW · **Dimension**: Material · **Tier Violated**: doc / record-keeping · **Game Affected**: all
- **Location**:
  - `docs/engine/nifal.md:776` (§3 step 4) and `:788` (Layering note);
  - `byroredux/src/material_translate.rs:584`, an intra-doc link in `translate_material`'s rustdoc;
  - `docs/engine/asset-pipeline.md:387` and `docs/engine/material-abstraction.md:168`;
  - comments at `asset_provider/material/merge.rs:555` and `:1341`.
- **Status**: NEW (introduced by `2b1b7fc5c`)
- **Description**: `2b1b7fc5c` made `helpers::classify_glass_into_material` a `#[cfg(test)]` shim that passes
  `from_bgsm = false`. Production now calls `classify_glass_into_material_with_provenance`, which takes 9 arguments,
  including the new `from_bgsm`. The live boundary therefore consumes two distinct provenance signals:
  - `external_material_resolved` gates keyword promotion of effect carriers (#4283);
  - `from_bgsm` gates overriding an authored lit dispatch `2..=20` (#4855).

  Every spec and doc site still names the shim, and none of them records the two-signal split. The same §3 paragraph lists
  `placement_lod.rs` as "the third production caller" exempt from Phase 2. It omits `object_lod.rs`, which has been a
  `translate_material` caller since #4245 and also attaches no `MaterialTextureHandles`.
- **Evidence**: `rg 'classify_glass_into_material\b' docs/engine byroredux/src` finds the 5 non-test sites above. The
  definition sits at `helpers.rs:70-71` (`#[cfg(test)] fn classify_glass_into_material`).
- **Impact**: An auditor or contributor following the spec reads a function that production never calls. The rustdoc link
  resolves only under `cfg(test)`.
- **Related**: #4873 covers the `terrain_lod_btr.rs` Phase-2 omission and the ground-cover wording in the same section; fold
  this in there or publish it separately. See also #4246 and #4855.
- **Suggested Fix**:
  - Rename the references at those sites.
  - Add the provenance pair to §3 step 4.
  - Add `object_lod.rs` to the Phase-2 caller and exemption list.

#### NIFAL-D1-2026-09-29-02: Mesh → `FogVolume` beam/fog replacement runs on the cell path only and is an unrecorded drawn-surface exemption
- **Severity**: LOW · **Dimension**: Material (drawn-surface boundary) / Completeness
- **Tier Violated**: single-boundary (load-path divergence); no-fabrication (weak sense)
- **Game Affected**: Oblivion, FO3, FNV, FO4
- **Location**:
  - `byroredux/src/cell_loader/nif_import_registry.rs:389-402`: `CachedNifImport::beam_volumes`, which calls 7 converters in
    `byroredux/src/fog.rs:864-1477`;
  - `byroredux/src/cell_loader/spawn/mesh_instance.rs:560-575` (`prepare_fog_mesh_instance` → `fog::fog_volume_from_mesh`)
    and `:723-740` (`FogGroup`);
  - `scene/nif_loader.rs` calls none of them.
- **Status**: NEW. `fog_volume_from_mesh` dates from `733dff8f1` (2026-07-28) and the beam converters from `0572bfd5a`
  (2026-09-23). Neither has been reviewed by a NIFAL audit (grep of `docs/audits/`).
- **Description**: `0572bfd5a` added six asset-signature converters. Each matches a file name plus exact vertex/index counts:
  - `NVNellisHangarInteriorLightBeam.nif`: 110 verts / 120 indices;
  - FO4 `emergencylightbeam01.nif`: 242/1080;
  - Oblivion `lightbeam01.nif`: 14/36.

  On a match the drawn submesh is **replaced** before upload: no `Material`, no raster entity, no BLAS. One or more canonical
  `FogVolume`s are emitted instead, and the fog-token converter does the same for `dst_blend == 7` fog/smoke quads. This is a
  new NIF-data → canonical-type translation. It has no declared boundary in `nifal.md` §2. `nifal.md` §3 says the drawn-surface
  exemptions are "exactly three" (Cornell, save, ground cover), so this would be a fourth. The media parameters are uncited:
  - extinction 0.12 m⁻¹;
  - albedo 0.9;
  - `edge_softness` 0.35–0.65;
  - an 80-BU beam half-width;
  - extrusion of span × 0.3 or width × 0.75.

  `docs/engine/interior-godrays-status.md` calls them "asset-specific conversions". A painted card has no extinction to
  translate, so this is recorded as weak-sense no-fabrication, not invented data.
- **Evidence**: `rg 'beam_volume_from_mesh|beam_volumes_from_mesh|fog_volume_from_mesh' byroredux/src` finds only fog.rs,
  its tests, `nif_import_registry.rs` and `mesh_instance.rs`.
- **Impact**: The same NIF renders two ways. `cargo run -- effects/ambient/windowlightbeam.nif` draws the painted card, while
  that NIF placed in a cell becomes a medium. The real game path (cells) is unaffected. The main cost is that the population
  is invisible to the §3 exemption ledger and to the NIFAL boundary inventory.
- **Related**: EXT-D1-02 / EXT-D4-0x (`AUDIT_EXTERIOR_2026-09-27.md`, godray lighting side), #4809 (beam classification
  caching), `docs/engine/interior-godrays-status.md`. `fog.rs` is owned by exterior and renderer; this is the NIFAL facet only.
- **Suggested Fix**:
  - Record "mesh → participating medium (cell path)" in `nifal.md` §2/§3 as a declared boundary. Cite the converters and the
    status doc.
  - Either route the loose-NIF spawn through `beam_volumes` and `fog_volume_from_mesh`, or state the viewer divergence as
    deliberate.
  - Cite or measure the media constants.

#### NIFAL-D5-2026-09-29-01: `nifal.md` §2 Particles still calls the legacy-data and #3329 sequence rate tiers "whole-scene" after #4560 and #4620 made both per-instance
- **Severity**: LOW · **Dimension**: Particles · **Tier Violated**: doc / record-keeping · **Game Affected**: Oblivion, FO3, FNV, Skyrim
- **Location**: `docs/engine/nifal.md:406-410`, the Attribution paragraph: "Still whole-scene: the legacy
  `NiParticleSystemController` fallback and the #3329 sequence tier (a sequence's controlled block names an emitter
  controller, not an emitter instance)".
- **Status**: NEW (introduced by `a6bcec6e2` + `b7491072f`; neither touched nifal.md)
- **Description**: Both tiers now resolve through `find_own_emitter_ctlr_refs`:
  - `crates/nif/src/import/walk/emitter.rs:473-482`: per #4560, the legacy data is `NiPSysEmitterCtlr`'s own `Data` ref, and an
    unlinked block is attributed to nobody.
  - `:642-653`: per #4620, a non-null `cb.controller_ref` must equal the system's own controller, and a null ref falls back to
    the system name.

  The paragraph's stated reason, "names an emitter controller, not an emitter instance", is exactly what #4620 disproved. The
  only remaining whole-scene piece is the budget fallback when the own `data_ref` does not resolve (`emitter.rs:352-372`).
  The audit-nifal skill's Dim 5 text carries the same stale claim.
- **Impact**: The next particle change is planned against a boundary that no longer exists. This is the same failure class as
  #4409.
- **Related**: #4560, #4620, #4409, #4261
- **Suggested Fix**: Replace the sentence with "Still whole-scene: only the emitter-budget scan when the own `data_ref` does
  not resolve", citing #4560 and #4620, and sync the skill.

#### NIFAL-D8-2026-09-29-01: #4636's dead-path yield lets a BGSM win the greyscale-LUT texture while the #4402 enable-bit rule still treats the slot as NIF-won
- **Severity**: LOW (latent; population not censused) · **Dimension**: Shader flags / texture roles (merge boundary)
- **Tier Violated**: single-boundary (two precedence decisions for one slot disagree) · **Game Affected**: FO4, FO76
- **Location**: `byroredux/src/asset_provider/material/merge.rs`:
  - `:701`: `nif_supplied_greyscale_lut` is captured before the chain walk;
  - `:785-794`: the bit rule. If the slot is empty it assigns (the BGSM wins); else if the NIF supplied the slot it ORs;
  - `:795-801`: `fill(greyscale_lut, …, texture_exists)` runs after the bit rule;
  - `:158-184`: the #4636 replacement branch of `fill`.
- **Status**: NEW (the interaction was introduced by `78d079707`)
- **Description**: Suppose the NIF's slot-3 LUT path resolves in no archive and the BGSM's does. The bit block sees a filled
  slot, takes the NIF-won branch, and ORs the BGSM bit in. `fill` then replaces the slot with the BGSM's LUT. The material
  ends up sampling the BGSM's texture under an enable that is the NIF SLSF1 bit OR the BGSM bit. That violates the
  #2108/#4402 contract documented at `:760-783`: "this BGSM wins the slot → authoritative for both the texture and the enable
  bit (assignment, including OFF)". Because `nif_supplied_greyscale_lut` stays true, every ancestor step in the chain also ORs.
- **Evidence**: The outcome diverges only when the NIF bit is ON, the BGSM bit is OFF, the NIF LUT is dead and the BGSM LUT is
  live. In that case the remap stays on although the winning material authored it off. #4636's census covered the normal slot
  only (3 dead FO4 paths out of 637 disagreements), so vanilla incidence for the LUT slot is unmeasured and likely ~0. Mods and
  retextures can reach it. The BGEM arm (`:1215-1233`) assigns the LUT without `fill`, so #4636 never applies there.
- **Impact**: A wrong palette remap on the affected mesh. This is below the HIGH `translate_material` floor only because there
  is no known population.
- **Related**: #4636, #4402, #3898, #2108
- **Suggested Fix**: Decide the winner once. Run the LUT `fill` first, have it report whether this BGSM's path took the slot,
  and apply assign-vs-OR from that result.

## Sibling-report findings on NIFAL surfaces (cited, not re-counted)
- **NIF-D4-2026-09-29-01** (LOW, concurrent `/audit-nif`): after the `b9e961eeb` basis move, stale "BSGeometry decoded Y-up"
  text remains at `import/mesh/tangent.rs:377-380` and `docs/engine/per-game-translation-survey.md:163`. From the NIFAL side,
  Dim 2 re-confirmed that the code is consistent: the conversion happens once at import, and no downstream Starfield axis or
  unit compensation exists.
- **ECS Dim 8** (`AUDIT_ECS_2026-09-29.md`): canonical `Material` unchanged and clean; this report agrees.

## Verified fixed in this window (for the next run's dedup)
- **#4579**: `#[test]` is back on `translate_material_copies_every_canonical_field` (`material_translate.rs:3350-3353`), and
  the test runs in the 74-test lane.
- **#4411**:
  - the pin scan reads only `#[test]`-registered bodies;
  - the light scan (`walk/lights.rs:255`, `:308`) and the collision scan split at the first `#[cfg(test)]`;
  - in `blocks/mod.rs` the first `#[cfg(test)]` is at :1368, past every dispatch arm, so the scans are not vacuous;
  - new `resolve_scan_ignores_test_module_mentions` tests.
- **#4553**: `object_lod.rs:505` uses `translate_texture_clamp_mode`; `placement_lod.rs:574` uses
  `material.texture_clamp_mode`.
- **#4632**: `terrain_lod_btr.rs` ORs the land sub-meshes' authored MSN into
  `translate_texture_only_material_with_authored_msn`, reads `normal_has_alpha` from the DDS, and calls `resolve_msn_z_source`.
- **#4282**:
  - `WetnessShading` / `LuminanceShading` have `sanitize_finite` descents (`material.rs:1594-1601`);
  - FORMAT_MAJOR went 25 → 26 in the same commit;
  - the capture sits after the Starfield material-reference early return.
- **#4283 / #4855**: the keyword glass gate reads `external_material_resolved`; the authored lit-dispatch guard reads
  `from_bgsm`. Tests: `cdb_resolution_does_not_override_an_authored_lit_dispatch` and the helpers twin.
- **#4856**: `every_exterior_spawner_inserts_a_boundary_material` counts boundary calls per enclosing function and scans the
  `.rs` halves of the file+dir pairs.
- **#4859**: `walk/lights.rs:91` calls `.to_radians()`; the doc now says degrees; the fixture uses 45°/15°.
- **#4636 / #4654 / #4836 / #4941 / #4428 / #4279 / #4426**: verified at the lines cited in `dim_8.md`.
- **#4560 / #4561 / #4620**: per-instance rate tiers; the flat walker's culled gate.
- **#4563**: 3 `convert_hkx_clip` callers pass the policy in; no post-conversion override exists.
- **#4635**: the route pin reads production code only, with floors of 2 for `load.rs` and 1 for `world_setup.rs`.
- **#4637 / #4965 / #4566**: the harness passes on all 8 games.

## Existing open issues re-checked
- **#4633**: `compose_transforms` still has no finiteness gate on its own output (`import/transform.rs:13-25`). Still open.
- **#4634**: the doc still says "three contributors" at `material_translate.rs:566-572` and `:717-721`, and the test is still
  named `translate_material_unions_all_three_effect_shader_flag_contributors` (`:3536`). Still open.
- **#4246**: the spec paragraph claims all three Phase-2 resolvers "early-return without" `MaterialTextureHandles`. That is
  still false for `resolve_unresolved_gloss_neutral_roughness`, which runs `.unwrap_or(0)` on the gloss index
  (`material_translate.rs:1351-1354`) and so would fire without handles. Still open.
- **#4873**: the `terrain_lod_btr.rs` Phase-2 caller omission and the ground-cover "no Material" wording are both still in
  `nifal.md` §3. Ground-cover Phase C (#4413) models now take the normal Material path.
- **#4256, #4430, #4441, #4562, #4749, #3398**: unchanged.

## Documented-limitation ledger (parked, not leaks)
The baseline ledger (`AUDIT_NIFAL_2026-09-21.md` §Documented-limitation ledger) stands, with these updates:
- The seven parked node fields still have zero canonical consumers. `import/units.rs` now scales the parked
  `bs_ordered_node` / `lod_group` distances inside the raw tier for Starfield. That is a unit conversion on parked data, not
  a consumer.
- Starfield wetness/luminance (#4282) now **cross** into `Material` as capture-only fields with no `GpuMaterial` consumer, the
  same tier as `lighting_effect_1/2`.
- NEW ledger row: `NiSpotLight.inner_spot_angle` / `exponent` are parsed but not translated. The canonical `Emitter` models a
  single cone, and `lighting.glsl:203-211` uses a linear cos-ramp. This is latent: #4859's census found 0 `NiSpotLight` in
  FNV/FO3/Oblivion/SSE.
- Ground-cover blades have no `Material` (#4304). Phase C authored models do.
- FO76/Starfield tex/nrm fill of 0% remains the documented structural zero.
- FO4 raw `env_map_scale > 0.3` fill is 48.3%. It is authored shader-type-1 data, and BGSM scalars supersede it after the merge.

## Dimension 9: harness table (release, `--ignored`, all 8 games; both tests passed in 63.8 s)

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

The dark-role census found 8 hits, all in Oblivion (6 files); every other game is zero.

## Method notes
- **Tests run at HEAD** (`-j 4`, cached builds):
  - `cargo test -p byroredux --bin byroredux material_translate`: 74 passed.
  - Combined byroredux guard lane: 12 passed. It covers the overlay value/scan pins, 4 animation harness tests, the HKX sample
    test, the combat-route pin, the secondary-role walk, the tint alpha gates, and the documented-role list.
  - `cargo test -p byroredux-nif --lib` guard lane: 35 passed. It covers dispatch coverage (collision + lights),
    role/values enumeration, 22 `slot_role` tests, the decal-bit parity test, satellite independence, and the units pins.
  - `cargo test -p byroredux-nif --release --test translation_completeness -- --ignored`: 2 passed on all 8 games.
  - Logs are in `/tmp/audit/nifal/test_*.log` and `d9_harness.log`.
- **No engine launch**, no GPU process, no plugin `--ignored` tests, no `cargo fmt`, and no repo edits besides this file.
- **`_audit-validate.sh`**: OK, all path references are valid. Only the standing advisories remain (`/tmp/audit/nifal/validate.log`).
- **Dedup**: `/tmp/audit/issues.json` (open issues) plus `gh issue list --state all --search`. Searched: the glass classifier,
  greyscale, whole-scene, `fog_volume_from_mesh`, sequence tier. Also checked today's sibling reports (ECS, RENDERER,
  PERFORMANCE, CONCURRENCY, and the NIF scratch) and `AUDIT_EXTERIOR_2026-09-27.md` for the godray work.
- **Skill staleness** (route to the next skill sync / `/audit-tech-debt`):
  - Dim 1 says the `texture_clamp_mode` default is 0. `ImportedMaterial` (`types.rs:903`) and `Material` (`material.rs:834`)
    both default to 3 (WRAP_S_WRAP_T).
  - Dim 5 still says "only the #3329 sequence tier and the unresolvable-ref budget scan remain whole-scene". Only the budget
    scan remains.
  - The boundary inventory should gain the mesh → medium tier once it is declared (D1-02).

**Suggested publish labels** (domain `nifal` on all):
- **D1-01**: `low` `documentation` `doc-rot`
- **D1-02**: `low` `bug` `renderer` `terrain-exterior`, plus `game:fnv` `game:fo4` if kept title-specific
- **D5-01**: `low` `documentation` `doc-rot`
- **D8-01**: `low` `bug` `import-pipeline` `game:fo4`

## Next Step

```
/audit-publish docs/audits/AUDIT_NIFAL_2026-09-29.md
```
