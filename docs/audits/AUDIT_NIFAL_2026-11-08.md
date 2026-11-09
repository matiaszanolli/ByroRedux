**HEAD**: 51ce1c2f9 · **Baseline**: docs/audits/AUDIT_NIFAL_2026-10-31.md (HEAD 670f81917) · **Audited**: all 9 dimensions (delta since 670f81917 read in full; unchanged-path dimensions had their first steps and guards re-run)

# NIFAL Audit — 2026-11-08

Nine dimension auditors ran under one orchestrator. Per-dimension scratch notes are in `/tmp/audit/nifal/dim_{1..9}.md`. No repo files were modified by the audit.

**Result: 0 CRITICAL / 0 HIGH / 0 MEDIUM / 7 LOW.** Every finding is LOW. Two are real wiring or boundary gaps in code that landed this week (env-mask classifier, synthetic static height). One is an unmeasured constant, and the other four are a test-strength gap and doc drift. No `translate_material` output was shown to be wrong on a verified shipped path, so the HIGH row of `_audit-severity.md` is not triggered. The synthetic-height prototype is off by default.

## Phase-1 guards run this session

| Command | Result |
|---|---|
| `cargo test -p byroredux --bin byroredux -- material_translate every_exterior_spawner both_spawn_paths every_source_derived nif_importer_material_kind markers_and_layer no_source_file_frames` | 81 passed, 0 failed |
| `… -- documented_texture_role_list common_material_texture_walk starfield_single_channel tint_alpha_gate cdb_blend_mode` | all pass |
| `… -- canonical_animation_completeness_harness every_hkx_sample_field` | 5 passed (the tests live in the bin crate; without `--bin` they match 0) |
| `cargo test -p byroredux-nif --lib -- dispatch_coverage_tests light_dispatch_coverage_tests the_scene_graph_walkers units tangent_convention` | 29 passed |
| `cargo test -p byroredux-nif dispatch_coverage_tests` / `import::collision` | 5 / 92 passed |
| `systems::particle` overlay guards | 27 passed |
| `cargo test --release -p byroredux-nif --test translation_completeness -- --ignored cross_game_translation_completeness` | passed, all 8 games |

The completeness fill rates match the 10-31 baseline table exactly on every recorded column. `consistent` is 100% on all eight games, and the FO76 / Starfield `tex` / `nrm` near-zero values are the documented structural zeros. Floors and ceilings held.

Dedup sources: `/tmp/audit/issues.json`, `gh issue list --search` per finding, and the 10-31 report. None of the findings below match an existing issue.

## Executive summary — per-category status vs the spec §2 inventory

| Category | Status |
|---|---|
| Material | converged (one new LOW wiring/measurement gap, one new LOW prototype boundary gap) |
| Geometry/Transform | converged |
| Skinning & Lights | converged |
| Nodes | triaged (raw-tier parked, zero canonical consumers, grep confirmed empty) |
| Particles | converged (one LOW harness gap) |
| Collision | converged |
| Animation | converged |
| Shader flags / texture roles | converged |
| Completeness signal | green |

Violations per invariant: single-boundary 2 (-02; the -D9-02 declaration gap); no-fabrication 3 (-01 scope, -02 constants, -03); no-leak 0; no-render-time-fallback 0; harness-gap / doc 5 (-01 wiring, -D5-01, -D2-02, -D9-01, and -D9-02).

## Per-category tier matrix

| Category (boundary fn) | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|
| Material (`translate_material`) | **-02** | **-01, -03** | pass | pass |
| Geometry (`import/mesh/*`, `coord`, `units`) | pass | pass | pass | pass |
| Skinning/Lights (`skin.rs`, `walk/lights.rs`) | pass | pass | pass | pass |
| Nodes (parked passthroughs) | N-A | N-A | pass | N-A |
| Particles (`apply_emitter_overlays`) | pass | pass | pass | pass |
| Collision (`resolve_shape_inner`, `summarize_collision_authoring`) | pass | pass | pass | N-A |
| Animation (`convert_nif_clip` / `convert_hkx_clip`) | pass | pass | pass | N-A |
| Textures/flags (`slot_to_role`, `merge.rs`) | pass | pass | pass | pass |

## Findings

### NIFAL-2026-11-08-01 — Env-mask classifier change (2ed1d91ce) has no wiring test, an unmeasured non-FNV population, and is unrecorded in the spec
- **Severity**: LOW
- **Dimension**: Material (found by Dim 1 and Dim 8)
- **Tier Violated**: harness-gap (secondary: no-fabrication, measurement covers FNV only)
- **Game Affected**: FO3, FNV (intended); Skyrim type-1 EnvironmentMap and FO4 non-BGSM shapes (side effect)
- **Location**: `crates/core/src/ecs/components/material.rs:1415` (mask ceiling), `:1566` (`resolve_pbr` hardcodes `has_env_mask: false`); `crates/nif/src/import/material/mod.rs:1463`; `byroredux/src/material_translate.rs:678`; `crates/renderer/shaders/triangle.frag:1803-1809`
- **Status**: NEW
- **Description**: The change caps an env-mapped surface that authors an environment mask at roughness 0.55 (`ENV_MASK_ROUGHNESS_CEILING`), and the fragment shader blends metalness toward 0.9 by the mask texel. Three gaps. (1) The only tests of `has_env_mask` set it by hand on the classifier input (`material.rs:2344-2350`). Nothing exercises the two production wires, `classify_legacy_pbr` (`mod.rs:1463`) and the overlay recompute (`material_translate.rs:678`). Setting either to `false` leaves every test green and silently reverts the FNV fix. `resolve_pbr` also passes a bare `false` with no comment. (2) The only cited measurement is FNV (3,576 meshes, 591 reflective). The classifier arm is not game- or kind-gated, so Skyrim and FO4 shapes that fill the `environment_mask` role also change: the Skyrim SE ebony cuirass goes from roughness 0.8 to 0.55, across the renderer's `roughness < 0.6` reflection gate. That may be a fix, but it is unmeasured. The shader metalness blend is gated on `materialKind == 0u`, so kind-1 Skyrim gets the lower roughness without the metalness weighting. The census has no committed tool. (3) The GLSL constants `0.3` and `0.9` mirror Rust values with no pin, and neither `nifal.md` nor `material-abstraction.md` mentions the new classifier input.
- **Failure scenario**: A refactor drops `has_env_mask` from `classify_legacy_pbr` or the recompute arm. FNV pistols, typewriters and gurneys return to roughness 0.8, above the reflection gate, and CI stays green.
- **Suggested fix**: Add one nif-crate test (`MaterialInfo` with `env_map_scale = 1.0`, `env_mask = Some`, no authored specular, expect roughness == `ENV_MASK_ROUGHNESS_CEILING`) and one translate-level test for the recompute arm. Comment the `false` in `resolve_pbr`. Pin the 0.3 / 0.9 / 0.6 couplings the way `nif_importer_material_kind_literals_match_renderer_constants` pins kinds. Run the census on Skyrim SE / FO4, commit the tool beside `crates/nif/examples/synthetic_height_census.rs`, and either gate `has_env_mask` to the kind-0 population or record the result in `nifal.md`.

### NIFAL-2026-11-08-02 — `static_height` (51ce1c2f9) decides material-adjacent state after the boundary from raw `mesh.material`; constants unmeasured; not in the spec
- **Severity**: LOW (opt-in prototype, `render.static_parallax_depth_m` defaults to 0)
- **Dimension**: Material / Geometry / Textures (found by Dims 1, 2, 8, 9)
- **Tier Violated**: single-boundary (secondary: no-fabrication, doc gap)
- **Game Affected**: all, when the setting is on; MSWP-swapped FO4 / Skyrim most exposed
- **Location**: `byroredux/src/static_height.rs:151-166` (`mesh_qualifies`), `:52-72` (constants), `:236-266` (`bind_synthetic_height`); `byroredux/src/scene/mesh_entity.rs:182-194` (call)
- **Status**: NEW
- **Description**: `bind_synthetic_height` runs after `translate_material_with_provenance`. It writes `handles.textures.height`, `parallax_height_scale` and `parallax_max_passes` on `MaterialTextureHandles`, so `Material.parallax_*` keeps the translate default while the render path reads the handles. The mismatch is provenance-tagged (`MaterialTextureSource::SyntheticHeight`), which is the right mitigation. The gate, though, reads `mesh.material.{is_decal, has_alpha, alpha_test, model_space_normals}` from the raw `ImportedMesh`, not the `source_material` that `spawn_mesh_entity` is handed. On the cell path `source_material` is the REFR material-swap target (`mesh_instance.rs`: `swapped_material.as_ref().unwrap_or(&mesh.material)`), so the gate judges pre-swap flags. It also misses the canonical model-space-normals bit that `translate_material` ORs in from `extra_material_flags` and the `_msn` name rule (#4548). The #6690 guard `markers_and_layer_read_the_source_material_not_the_meshes_own` scans `attach_mesh_components`, not this call, so it stays green. Constants: `MARCH_PASSES = 16.0` and `MIN_TILING_UV_EXTENT = 1.02` cite nothing, and `MAX_SEAM_EXCEEDANCE = 0.05` is self-described as "a starting point to tune by eye" (it sits just inside the measured terrain-layer p90 of 0.062). The REFR placement scale is ignored. `SyntheticHeight` and the post-boundary write appear in no spec document.
- **Failure scenario**: With the setting on, a FO4 REFR whose MSWP swaps an opaque tiling wall for an alpha-blended one still qualifies, so a height march runs on a blended surface. A tiling static whose normal slot is model-space via the REFR overlay bit gets a garbage height field.
- **Suggested fix**: Pass `source_material`, or read the translated `Material`'s flag word, into `mesh_qualifies`. Extend the #6690 source-scan guard to the call. Cite or mark provisional the three constants before the setting leaves prototype. Add a `nifal.md` §2 line recording the synthetic-height slot as an opt-in non-authored role with its post-boundary exception.

### NIFAL-2026-11-08-03 — `NEAR_MIRROR_NEUTRAL_ROUGHNESS = 0.5` has no measurement and its open NIFAL call is untracked
- **Severity**: LOW
- **Dimension**: Material (Dim 8)
- **Tier Violated**: no-fabrication
- **Game Affected**: FO4 (BGSM near-mirror materials, ~425 per #3639)
- **Location**: `byroredux/src/material_translate.rs:1366-1380`; `crates/core/src/ecs/components/material.rs:894-897` (`NO_SIGNAL_NEUTRAL` 0.85)
- **Status**: NEW (the doc-rot half closed as #6746; the 0.5 vs 0.85 question it "leaves open for NIFAL" has no issue)
- **Description**: After 80c0d01fc the constant's doc calls it "a chosen mid roughness" equal to `Material::default()`, explicitly not the classifier's 0.85 terminal. The value has no measurement or source behind it.
- **Failure scenario**: A near-mirror BGSM with an unresolvable gloss map lands at 0.5, under the 0.6 reflection gate, so it still traces RT reflections. At 0.85 it would not. Which is right is unmeasured.
- **Suggested fix**: Measure what the affected FO4 materials' `Gloss` maps resolve to elsewhere and either cite the result or switch to the classifier neutral. Link a tracker.

### NIFAL-D5-2026-11-08-01 — Particle force-field Z-up→Y-up conversion has no value assertion
- **Severity**: LOW
- **Dimension**: Particles
- **Tier Violated**: harness-gap
- **Game Affected**: all particle-bearing titles (Oblivion → Skyrim)
- **Location**: `byroredux/src/systems/particle.rs:120` (conversion), `:801` and `:988-989` (tests)
- **Status**: NEW
- **Description**: `apply_emitter_overlays` converts force fields once through `convert_force_fields_zup_to_yup`, and the code is correct today. The overlay tests carry a comment saying fields are "converted Z-up→Y-up" but assert only `force_fields.len() == 1`. `convert_force_fields_zup_to_yup` is referenced nowhere else in a test.
- **Failure scenario**: Dropping or doubling the axis swap passes CI. Wind and gravity fields then point along the wrong axis in every particle effect.
- **Suggested fix**: Author a non-symmetric Z-up force vector and assert the Y-up result component by component in `every_overlay_parameter_reaches_the_preset`.

### NIFAL-D2-2026-11-08-02 — The placement-root replacement rule is absent from `nifal.md` and the NIFAL skill
- **Severity**: LOW (doc drift)
- **Dimension**: Geometry/Transform
- **Tier Violated**: harness-gap
- **Game Affected**: FO3 / FNV / FO4 (replace) vs Oblivion / Skyrim / FO76 / Starfield (compose)
- **Location**: `docs/engine/nifal.md` §2 Geometry/Transform; `.claude/commands/audit-nifal/SKILL.md` Dim 2. The rule is in `byroredux/src/cell_loader/references/import.rs:25-47`, `crates/nif/src/scene.rs:210` (`clear_root_transform`), and the audit-fnv / audit-fo4 skills.
- **Status**: NEW
- **Description**: 62782dd3b and 7f2d98944 add one mutation site on the raw scene, applied before every importer. The cell loader decides per game through `placement_replaces_model_root` (FO3/FNV and FO4). The Dim 2 audit traced every REFR cache-fill path (sync, SCOL/PKIN children, streaming worker, precombined) and found each routes through it. The deliberate exemptions are ground cover, LOD and `_physics.nif`. The code and its pins (`placement_replaces_model_root_only_for_verified_games`, two `clear_root_transform_*` tests) are consistent. Only the spec and the skill lack a checklist line.
- **Failure scenario**: A new REFR cache-fill path skips `prepare_placed_model_scene` and nothing in the NIFAL skill says to check.
- **Suggested fix**: Add a "Placement-root replacement" paragraph to `nifal.md` §2 and one Dim 2 checklist line naming the callers and the exemptions.

### NIFAL-D9-2026-11-08-01 — `nifal.md` §3 names call sites and file homes that moved
- **Severity**: LOW (doc rot)
- **Dimension**: Completeness / cross-cutting
- **Tier Violated**: harness-gap
- **Location**: `docs/engine/nifal.md` lines 963, 1019, 1031, 1098
- **Status**: NEW (verified by reading each line)
- **Description**: Line 963 still says `pack_imported_material_flags` lives in `cell_loader.rs`, but #6712 moved it to `material_flags.rs`. Lines 1019 and 1098 name `scene/nif_loader.rs` and `cell_loader/spawn/mesh_instance.rs` as the lowering and Phase-2 resolver call sites. Since #6690 the call sites are `scene/mesh_entity.rs` (`:149`, and `:351-367` for the resolvers). Line 1031 says the terrain Phase-2 caller is only `terrain_lod_btr.rs`, but `terrain_lod.rs:1347` also calls `resolve_msn_z_source`.
- **Suggested fix**: Update the four references. `.claude/commands/_audit-validate.sh` covers path tokens in skills, not these docs.

### NIFAL-D9-2026-11-08-02 — `cornell/reference.rs:434` hand-builds a canonical `AnimationClip` outside the declared boundaries
- **Severity**: LOW (doc gap, fixture-only)
- **Dimension**: Animation / cross-cutting (found by Dims 7 and 9)
- **Tier Violated**: single-boundary (declaration only, no behavioural violation)
- **Location**: `byroredux/src/cornell/reference.rs:434`; `docs/engine/nifal.md` §3 (names only `cornell/builders.rs`)
- **Status**: NEW
- **Description**: It is the only non-test producer of the canonical `AnimationClip` besides `convert_nif_clip` (`anim_convert.rs:635`) and `convert_hkx_clip` (`animation.rs:713`). It is a Cornell-box fixture and falls inside the existing Cornell exemption in spirit, but §3 names only `builders.rs`. The Dim 9 auditor confirmed that every Cornell `MeshHandle` insert goes through `builders.rs`.
- **Suggested fix**: Extend the §3 Cornell exemption to name `cornell/reference.rs`, or route the clip through the builder module.

## Documented-limitation ledger (parked, not leaks — do not re-report)

- **Nodes** (zero canonical consumers, grep re-confirmed): `bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`. `bs_sub_index` is consumed raw→raw by `hide_skin_partitions`.
- **Lights**: affected-node scoping is untranslated for every `NiLight` kind (`affected_node_names` has zero consumers); `NiAmbientLight` is parked by census.
- **Collision**: `BhkPlaneShape → None` (counted in `plane_shapes`); phantoms; `BhkNPCollisionObject` blob (the FO4 skeleton `bhkRagdollSystem` blob is decoded); raw Havok surface ids (`surface_material`, `part_surface_materials`); NP-ragdoll constants are fallback-only. Shape arms counted fresh: 16 dispatched `bhk*Shape` structs, 16 `downcast_ref` arms, none missing.
- **Animation**: ambient-colour and morph-weight channels; path-interpolator follow-axis / banking / constant-velocity / closed-curve wrap; empty text keys on embedded controllers; the HKX cart-exit event synthesis.
- **Materials / textures**: emissive scale = no-op (§4); Starfield `EmissiveSettings` and slot-6 `_height` parked (#5503, #5381); Starfield single-channel kinds parked by name; soft additive blend variants and `TakeSmaller` parked under open #5509 (Additive and Multiply are now forwarded by 8e30e66e1 and pinned by `unsourced_modes_stay_opaque`); FO4 slot-2 `Glow_Map` gate (#4430); FO3/FNV hair-tint canonical field; `source_shader_type` written and saved with no reader (#6138).
- **Particles**: `initial_color` deliberately not applied; size-over-life bell collapses to magnitude; legacy `NiParticleSystemController` keeps the preset rate.
- **§3 exemptions**: Cornell fixtures, `crates/save` reconstructions, ground-cover blades, mesh → `FogVolume`, `scene.rs` demo primitives; scan-level LOD-water re-insert and `npc_spawn/resumable/mod.rs` mocks.

## Observations (not findings)

- **Skill drift** (fix at the next `/audit-sync`): Dim 6 says `CollisionAuthoringSummary` carries "only four `u32` counts". It now has five. The new `external` field comes from #6555 (9a0ae0817), is stamped only by the cell loader for precombine imports, and `authors_no_body()` is game-agnostic, so there is no leak. The skill's collider-gate list also omits the #6555 blended-surface term.
- Open #6549 (FNV sky planes and a waterfall sheet still get trimesh colliders) is adjacent to Dim 6. Open #6048 (terrain colliders ignore LTEX Havok data) looks partly landed in `terrain.rs::land_surface_material`, so its owner should check whether it can close.
- Dim 4's expected-empty grep has no automated guard. A tree-wide variant is also empty.
- The #6610 parked player in `named_sequences.rs` is stopped but still samples its first sequence each tick, so it does not literally hold the bind pose as the commit message says. No counter-example was measured, so it is not filed.
- `convert_hkx_clip` relies on the hkx decoder's `normalize_quaternion` (#6242) rather than `normalized_rotation_sample`. This is consistent with the 10-31 baseline.
- Unverifiable: whether any of the 182 newly-alpha Starfield Additive shapes also author `IsGlass` (it would now pass the glass classifier's coverage gate). The CDB full parse is the known 9 GB blocker.

Suggested next step: `/audit-publish docs/audits/AUDIT_NIFAL_2026-11-08.md` (domain label `nifal`; add `renderer` / `shaders` to the env-mask finding, and `nif-parser` is not needed since nothing here is parse-side).
