**HEAD**: 4a3e89d56 · **Baseline**: docs/audits/AUDIT_NIFAL_2026-10-12.md (HEAD 5d31fd637, last full run; the 2026-10-19 report at 75b921cf4 was area-scoped and is used for dedup only) · **Audited**: Dims 1, 6, 7, 8, 9 (delta since 5d31fd637) · **Unchanged since baseline (skimmed)**: Dims 2, 3, 4, 5 (first steps and guards run)

# NIFAL Audit — 2026-10-21

One auditor, solo, as part of the `comprehensive` suite. Scratch notes: `/tmp/audit/nifal/dim_{1..9}.md`. Delta: 784 commits since the 10-12 baseline, 155 since the 10-19 area-scoped run.

## Phase-1 guards run this session

- `cargo test -p byroredux --bin byroredux -- material_translate canonical_animation_completeness_harness every_hkx_sample_field every_exterior_spawner every_source_derived common_material_texture_walk`: 84 passed, 0 failed.
- `cargo test -p byroredux-nif --lib -- dispatch_coverage_tests light_dispatch_coverage_tests the_scene_graph_walkers roles_covers_every_field values_covers_every_field units tangent_convention`: 31 passed, 0 failed.
- `cargo test --release -p byroredux-nif --test translation_completeness -- --ignored cross_game_translation_completeness`: passed. All eight games' floors and ceilings hold.

```
  Oblivion   meshes=567 tex=91.4% mat_path= 0.0% m_kind= 0.0% metO=100.0% rghO=100.0% spec=100.0% env=0.0% nrm= 0.0% tan= 84.0%
  FO3        meshes=687 tex=92.6% mat_path= 0.0% m_kind=10.9% metO= 94.3% rghO= 94.3% spec= 16.7% env=4.1% nrm=79.2% tan= 99.1%
  FNV        meshes=806 tex=95.3% mat_path= 0.0% m_kind=17.6% metO= 96.7% rghO= 96.7% spec=  9.8% env=4.0% nrm=78.9% tan= 99.3%
  SkyrimLE   meshes=529 tex=92.1% mat_path= 0.0% m_kind=42.9% metO= 92.1% rghO= 92.1% spec= 67.5% env=7.0% nrm=67.5% tan= 92.4%
  SkyrimSE   meshes=515 tex=93.8% mat_path= 0.0% m_kind=35.5% metO= 93.8% rghO= 93.8% spec= 76.1% env=6.8% nrm=76.1% tan= 94.8%
  FO4        meshes=633 tex=91.8% mat_path=56.7% m_kind=56.2% metO= 99.4% rghO= 99.4% spec= 83.1% env=46.6% nrm=82.9% tan= 94.6%
  FO76       meshes=773 tex=11.3% mat_path=83.8% m_kind=33.1% metO= 15.8% rghO= 15.8% spec=  7.8% env=7.1% nrm= 8.8% tan= 96.9%
  Starfield  meshes=811 tex= 0.0% mat_path=94.9% m_kind= 3.7% metO=  5.1% rghO=  5.1% spec=  1.4% env=3.7% nrm= 0.0% tan=100.0%
```

The FO76 and Starfield `tex`/`nrm` zeros are the documented structural zero (raw pre-merge tier; roles come from BGSM/CDB).

Probe: a throwaway crate at `/mnt/data/tmp/suite-1021-nifal/` (path dependency on `byroredux-hkx`, repo `Cargo.lock` copied in, its own target dir) measured the D7 finding. FO4 `.hkx` files were extracted into `/tmp/audit/nifal/hkx/` with the in-tree `ba2_extract_hkx` example.

Dedup: checked against `/tmp/audit/issues.json` (119 open), closed-issue searches, and `/tmp/audit/SUITE_FINDINGS.md` (esm, renderer, scripting, nif, gameplay). Neither finding below overlaps those lists. The 10-19 findings NIFAL-SD-01..04 are all closed (#6128 1a8c05139, #6129 4cfde44d2, #6130 7822695e4, #6131 4617306bc). Their fixes were verified in code; see "Verified clean".

## Executive Summary

**New findings: 0 CRITICAL, 0 HIGH, 1 MEDIUM, 1 LOW.** The material boundary itself has no violation.

| Category | Status | Boundary fn | Note |
|---|---|---|---|
| Material | converged | `translate_material` / `_with_provenance` | The only `pub` delta on `Material` since 10-12 is `source_shader_type` (2266f7778; write-only, tracked as open #6138). The MSWP fix (#5755/#5756) and the new Starfield LMSW/XLMS swap path (#5757) both re-merge onto the pre-merge snapshot and pass through the same boundary. |
| Geometry / Transform | converged | import (`coord`, `units`, `tangent`) | The Z-up tangent synthesizer is now a wrapper over the Y-up producer (89a0b6ccd). A proper rotation preserves sign. |
| Skinning / Lights | converged | `skin.rs`, `walk/lights.rs` | `find_named_node_offset` generalisation (f6a64fe20) only. |
| Nodes | triaged (by design) | none | Parked fields still have zero canonical consumers. The `bs_sub_index` ledger fix (#6129) has landed. |
| Particles | converged | `apply_emitter_overlays` | Still two call sites. The LUT release fix (#6109) is outside the overlay. |
| Collision | converged | `collision/` resolve + `extract_np_ragdoll` | The FO4 NP ragdoll uses `NP_*` constants only as `properties.map_or` fallbacks, as the checklist requires. |
| Animation | **pending** | `convert_nif_clip` + `convert_hkx_clip` | NIFAL-D7-2026-10-21-01: the HKX reference-frame yaw is dropped. |
| Shader flags / texture roles | triaged | `slot_to_role`, merge arms | NIFAL-D8-2026-10-21-01: the runtime `SwapTexture` re-resolve bypasses the per-role resolve. |
| Completeness | converged | `translation_completeness` | Green on all eight games (table above). |

Violations by invariant: single-boundary 1 (D8), parked-not-leak 1 (D7), no-fabrication 0, no-leak 0, no-render-time-fallback 0.

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|
| Material (`translate_material`) | pass | pass | pass | pass (`if game` grep of `triangle.frag` + includes: empty) |
| Geometry / Transform | pass | pass | pass | pass |
| Skinning / Lights | pass | pass | pass | pass |
| Nodes | N-A (no boundary, by design) | pass | pass | pass |
| Particles (`apply_emitter_overlays`) | pass | pass | pass | pass |
| Collision | pass | pass (NP fallbacks documented) | pass | pass |
| Animation (`convert_nif_clip`, `convert_hkx_clip`) | pass | pass | **fail (D7: decoded yaw parked with a refuted rationale)** | pass |
| Shader flags / texture roles | **fail (D8: second runtime role re-resolve)** | pass | pass | pass |

## Findings

### NIFAL-D7-2026-10-21-01: `convert_hkx_clip` drops the Havok reference-frame yaw, so Fallout 4 turn clips do not turn
- **Severity**: MEDIUM
- **Dimension**: Animation
- **Tier Violated**: `parked-not-leak` (decoded data dropped at the canonical boundary, with a stated rationale that measurement refutes)
- **Game Affected**: Fallout 4 (any Havok 2014 clip carrying `hkaDefaultAnimatedReferenceFrame`)
- **Location**: `byroredux/src/asset_provider/animation.rs:467-478` (`reference_frame_position`), `:579-584` (travel laid on the root bone); `crates/hkx/src/animation.rs:74-78` (`ReferenceFrame.samples` = `(x, y, z, yaw)`); `byroredux/src/systems/graph_locomotion.rs:205-214`; `crates/core/src/animation/root_motion.rs:16` (`RootMotionDelta(Vec3)`)
- **Status**: NEW (searched "root motion yaw", "graph_root_motion", "turn in place": no match)
- **Description**: e8026e737 reads FO4's extracted motion (`hkaDefaultAnimatedReferenceFrame`) and lays its position on the root bone, where the accumulation root splits it back out as `RootMotionDelta`. The hkx decoder also keeps each sample's yaw (`samples[i][3]`). `convert_hkx_clip` ignores it, and the comment gives the reason as "yaw is not applied (a clip that turns its root plays the turn in the pose)". The canonical root-motion channel is translation-only (`RootMotionDelta(pub Vec3)`), and `graph_root_motion_system` repeats "Yaw in the extracted motion is not applied". So the turn has no carrier anywhere in the canonical tier.
- **Evidence**: Measured on `meshes\actors\character\animations\mt\neutral\turninplaceright90.hkx` against `meshes\actors\character\characterassets\skeleton.hkx`. The reference frame's yaw runs 0 → −1.5708 rad (exactly 90°) while its translation stays below 5e-5. The `Root` track's rotation is constant (first = last = identity, 0.00° change), and so is `COM`'s (0.00°). The turn exists only in the dropped yaw, so the comment's premise is false for this clip. `Fallout4 - Animations.ba2` holds 662 `turninplace` clips out of 15,320 `.hkx`.
- **Impact**: The extracted motion only drives the actor where the graph moves the actor: `bGraphDriven`/`bAnimationDriven` transitions, which the commit message names as "walk it into a seat or turn it on the spot". In those transitions a turn-in-place plays as feet stepping with no rotation, and the actor ends facing its original heading. Where the AI mover owns the heading (not graph-driven), dropping the yaw is correct, because the mover already rotates the actor and applying it again would double the turn. So the loss is confined to graph-driven transitions, but there it is visible.
- **Related**: e8026e737; #5894 (refused reference frames); #5897 (additive clips played as an absolute pose). `docs/engine/nifal.md` still describes `convert_hkx_clip` as Skyrim cart/furniture idles only (lines 622-638). The reference-frame translation, the yaw drop and the additive-as-absolute conversion are absent from the ledger.
- **Suggested Fix**: Give root motion a yaw component, for example `RootMotionDelta { translation, yaw }` or a sibling channel, filled at `convert_hkx_clip` from the reference frame's yaw delta. `graph_root_motion_system` would then rotate the actor by it. Until then, correct the comment and record the yaw park, with this measurement, in `nifal.md` §Animation beside the HKX boundary paragraph.

### NIFAL-D8-2026-10-21-01: ObScript `SwapTexture` re-resolves the base colour outside the per-role resolve: authored clamp mode dropped, provenance left stale
- **Severity**: LOW
- **Dimension**: Shader flags / texture roles
- **Tier Violated**: `single-boundary`
- **Game Affected**: Oblivion, FO3, FNV (ObScript `SwapTexture`; e.g. the FO3 end-game slides `interface\endgame\endscrn_*`)
- **Location**: `byroredux/src/asset_provider/texture.rs:1388-1432` (`apply_texture_swap`), driven by `byroredux/src/app_step.rs:1115` (`step_texture_swaps`); compare `byroredux/src/cell_loader/spawn/mesh_instance.rs:1153-1158` (spawn) and `:1372` (`MaterialTextureDebugInfo` insert)
- **Status**: NEW. The sibling issues #6110 (ref counting, closed) and #6111 (upload flush, open) are about different defects in the same function.
- **Description**: The spawn path resolves the base-colour handle with `resolve_texture_with_clamp(.., material.texture_clamp_mode)`. `apply_texture_swap` (07bd22a6b) replaces the same role at runtime with `resolve_texture(ctx, tex_provider, Some(&path))`, which hard-codes clamp mode 3 (WRAP_S_WRAP_T). It never reads `Material.texture_clamp_mode`, which is on the entity it already holds. Flipbooks had the same class of defect and were routed through `resolve_flip_texture_for_role` (#4426). The swap also rewrites `Material.texture_path` but leaves the entity's `MaterialTextureDebugInfo` untouched, so `paths.base_color` keeps the old path and `sources.base_color` keeps `nif-texture-set`. Dim 8's rule is that an override which keeps the NIF slot's label is a finding. `MaterialTextureSource::RuntimeOverride` already exists for this case (#6131).
- **Evidence**: `texture.rs:630-637` (`resolve_texture` → `resolve_texture_with_clamp(.., 3)`). `apply_texture_swap` has no `texture_clamp_mode` read, and `MaterialTextureDebugInfo` is not mentioned anywhere in the function.
- **Impact**: A CLAMP-authored swap target, such as a full-screen slide quad or a decal, samples with REPEAT and can bleed its opposite edge along the border. `mat.dump` / `tex.*` show the pre-swap path and label for a swapped mesh, which undercuts the "chrome means missing textures" triage. No crash and no data loss. Reachable only through the ObScript command.
- **Related**: #4426 (flipbook per-role resolve), #6131 (`RuntimeOverride`), #6110, #6111.
- **Suggested Fix**: Resolve with `resolve_texture_with_clamp(ctx, tex_provider, Some(&path), material.texture_clamp_mode)`, or through a shared single-role helper like `resolve_flip_texture_for_role` for `base_color`. In the same loop, update `MaterialTextureDebugInfo.paths.base_color` and set `sources.base_color = MaterialTextureSource::RuntimeOverride`.

## Verified clean (delta-scoped)

- **10-19 fixes in code.** `RestorePart.body_texture` is applied on restore through `set_body_base_color`, the same function the spawn hook calls (#6128). The FO3/FNV hand seam blend on restore is still unapplied, as the fix's commit message records. `override_base_color` sets path and `RuntimeOverride` together (`npc_spawn/loot_appearance.rs:59-66`, #6131). `ImportedTextureSource::RuntimeOverride` has no importer producer: its only writers are `override_base_color` and the FaceGen `diffuse_override` arm at `nif_loader.rs:1362`. `bake_fallout_hair_tint` keys on `ImportedMaterial::fallout_hair`, which is set from `fo3nv_f1::HAIR` in `legacy_properties.rs` (#6130). The raw-tier field has no canonical consumer, as its doc says.
- **Material swap path (#5755/#5756/#5757).** `mswp_source_matches` normalises both spellings through `normalize_material_path`. The `*` wildcard handles a trailing star correctly (`pattern[p..].iter().all(|&c| c == b'*')`). The FNAM filter is gone from the overlay, and swap targets are stored canonical. The per-shape re-merge goes through `merge_external_material` on the pre-merge snapshot (`merge_external_materials` snapshots every mesh with a `material_path`, `.mat` included). The glass-classifier provenance (`external_material_resolved` / `from_bgsm`) rides on the re-merged `ImportedMaterial`, and the result enters `translate_material_with_provenance` like any other placement. `fill_from_bgsm` on a `.mat` target matches no arm and contributes nothing, so the CDB re-merge carries the roles. One ledger gap, not filed: this spawn-time per-REFR re-merge, now live on FO4 (where it never fired before #5755) and Starfield (689,041 refs per #5757), is not described in `nifal.md`.
- **BGSM merge split (#5735, a6cef30a5).** The greyscale-LUT interlock (#3898 / #4402 / #5106) and the specular-disabled gating (#4654 / #4836 / #5012) survive the split verbatim in `apply_bgsm_texture_slots` and `merge_bgsm_arm`.
- **Dim 4 first step**: empty for all six parked fields.
- **Dim 5 first step**: `apply_emitter_overlays(` outside `systems/particle.rs` appears at `scene/nif_loader.rs:1822` and `cell_loader/spawn.rs:1344` only.
- **Dim 6, FO4 NP ragdoll** (e1bd40bcf, fc4a15104, 019b087e3). Bodies take decoded `NpBodyProperties`, and the Skyrim-derived `NP_*` constants apply only when a body has no rows (`ragdoll.rs:421-428`). The ragdoll-side code-quality and doc items are already filed by `/audit-nif` (NIF-D5-2026-10-21-01..03) and are not re-reported.
- **Dim 2.** `synthesize_tangents` (Z-up) converts positions and normals through `zup_to_yup_pos` and calls `synthesize_tangents_yup`. The axis map is a proper rotation, so the bitangent sign is invariant.
- **Dim 3.** `find_attach_light_offset` and `find_projectile_node_offset` share `find_named_node_offset`, one resolver as the checklist describes. The #5792 cull-gate residual is NIF-D4-2026-10-21-01 and is not re-reported.

Observed, out of scope, not filed: `studio_host.rs:406-416` (`StudioCommand::SetMaterial`) clamps a Studio-edited `roughness` to `[0, 1]`, while the canonical floor `resolve_pbr` keeps is 0.04. The code predates the baseline (2026-08) and belongs to `/audit-tooling`.

## Documented-limitation ledger (restated so it is not re-reported)

Parked, not leaks:
- Node and mesh passthroughs: `bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`. `bs_sub_index` / `triangle_body_parts` are consumed raw→raw by `hide_skin_partitions`; only the dismemberment system is deferred.
- FO4+ `bhkNPCollisionObject` shape blob (the ragdoll blob *is* decoded), phantoms, and the decoded-but-unimported constraint kinds.
- Particle size-over-life curve and `initial_color`.
- Per-light ambient and morph-weight animation channels.
- Affected-node light scoping (incl. `NiAmbientLight`).
- CDB parked slots and `EmissiveSettingsComponent`.
- Starfield additive-family blend modes (#5509) and the coverage half of #5502.
- `source_shader_type` write-only (#6138).
- FO4 streaming partial `attach_points` = `None` (#1594).
- FO3/FNV hair tint mask has no canonical field (raw `fallout_hair` only).
- HKX additive clips played as an absolute pose (#5897, warned at conversion).
- The HKX reference-frame yaw (NIFAL-D7-2026-10-21-01, above) until it is fixed or recorded.

## Could not check

- Live engine behaviour of a graph-driven FO4 turn transition (no engine launch; the impact is inferred from the measured clip data and the consumer code).
- Whether vanilla FO3/FNV `SwapTexture` targets author CLAMP (no ObScript corpus scan was run for the swap targets).

Suggest `/audit-publish docs/audits/AUDIT_NIFAL_2026-10-21.md` (domain label `nifal`, plus `animation` and `game:fo4` for D7; `scripting` and `legacy-compat` for D8).
