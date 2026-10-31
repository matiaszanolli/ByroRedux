**HEAD**: 670f81917 · **Baseline**: docs/audits/AUDIT_NIFAL_2026-10-27.md (HEAD fa6be4a6b) · **Audited**: Dims 1, 3, 6, 7, 8 (delta since fa6be4a6b, read in full) · **Unchanged since baseline (skimmed)**: Dims 2, 4, 5, 9 (no commits on their paths; first steps and guards run)

# NIFAL Audit — 2026-10-31

One auditor working alone as part of the `comprehensive` suite. Scratch notes are in `/tmp/audit/nifal/dim_{1..9}.md`. The delta is 136 commits since fa6be4a6b, of which about 30 touch NIFAL paths (`git log fa6be4a6b..HEAD -- crates/nif byroredux/src/{material_translate,anim_convert,cell_loader,asset_provider,scene,helpers}* byroredux/src/npc_spawn/loot_appearance.rs byroredux/src/systems/particle.rs crates/core/src/ecs/components/material.rs`). Most are M43 gameplay, exterior (EXAL) or doc-citation commits that sit outside this audit.

**Result: 0 CRITICAL / 0 HIGH / 0 MEDIUM / 0 LOW.** Both Dim-3/Dim-8 changes that moved a boundary (race-skin identity, skeleton transform fold) landed as single-site translations with regression pins.

## Phase-1 guards run this session

- `cargo test -p byroredux --bin byroredux -- material_translate canonical_animation_completeness_harness every_hkx_sample_field every_exterior_spawner every_source_derived common_material_texture_walk every_overlay_parameter every_declared_overlay_parameter`: 86 passed, 0 failed.
- `cargo test -p byroredux-nif --lib -- dispatch_coverage_tests light_dispatch_coverage_tests the_scene_graph_walkers roles_covers_every_field values_covers_every_field units tangent_convention`: 31 passed.
- `cargo test -p byroredux --bin byroredux -- character_skin race_skin spawn_starts_the_idle unlit_blended`: 6 passed.
- `cargo test --release -p byroredux-nif --test oblivion_body_skin_flag -- --ignored` (new, real Oblivion data): passed.
- `cargo test --release -p byroredux-nif --test translation_completeness -- --ignored cross_game_translation_completeness`: passed. Every fill rate matches 10-27 exactly, `consistent` is 100% on all eight games.

```
  Oblivion   tex=91.4% m_kind= 0.0% metO=100.0% spec=100.0% nrm= 0.0% tan= 84.0%
  FO3        tex=92.6% m_kind=10.9% metO= 94.3% spec= 16.7% nrm=79.2% tan= 99.1%
  FNV        tex=95.3% m_kind=17.6% metO= 96.7% spec=  9.8% nrm=78.9% tan= 99.3%
  SkyrimLE   tex=92.1% m_kind=42.9% metO= 92.1% spec= 67.5% nrm=67.5% tan= 92.4%
  SkyrimSE   tex=93.8% m_kind=35.5% metO= 93.8% spec= 76.1% nrm=76.1% tan= 94.8%
  FO4        tex=91.8% m_kind=56.2% metO= 99.4% spec= 83.1% nrm=82.9% tan= 94.6%
  FO76       tex=11.3% m_kind=33.1% metO= 15.8% spec=  7.8% nrm= 8.8%  tan= 96.9%
  Starfield  tex= 0.0% m_kind= 3.7%  metO=  5.1% spec=  1.4% nrm= 0.0%  tan=100.0%
```

Dedup sources: `/tmp/audit/issues.json`, `/tmp/audit/open_issues_snapshot.txt`, SUITE_RULES §6 (safety, renderer, ecs, concurrency, nif), and the 10-27 report. Nothing found, so nothing to dedup.

## 10-27 findings

The 10-27 report had two LOW findings. Both are fixed and checked in code:
- NIFAL-D4-2026-10-27-01 (stale legacy-furniture "can never resolve to Sleep" comments) is fixed by b2d17ccf9 (#6475). The `FurnitureMarkerKind` comment in `cell_loader/references/attach.rs` now says beds arrive as animation type 2.
- NIFAL-D8-2026-10-27-01 (race-skin filter copying the whole `StringPool` per body piece) is fixed by 477124477 (#6476, #6477). The skin flag is decided at the import boundary (`ImportedMaterial::character_skin`) and `skin_texture_symbols` is deleted.

## Delta review by dimension

### Dim 1 — Material (reviewed in full)
- `translate_material` remains the only raw-material → `Material` producer. Production callers are unchanged: `scene/nif_loader.rs:1384` and `cell_loader/spawn/mesh_instance.rs:1136` (both via `translate_material_with_provenance`), `placement_lod.rs:615`, `object_lod.rs:400`, plus the Cornell fixture. The texture-only spawners (`terrain.rs`, `terrain_lod.rs`, `terrain_lod_btr.rs`, `water.rs`, `tree_lod.rs`) still use `translate_texture_only_material*`.
- `SPAWNER_ROOTS` shrank from six entries to five. Commit 613c7f46e (#6357) split `npc_spawn.rs` into `npc_spawn/`, and the directory entry `npc_spawn` still covers the new files, so nothing escaped the scan.
- `Material::source_shader_type` is now documented as reserved with no reader (#6138, closed). Grep confirms it is only written (`material_translate.rs:734`) and saved. `nifal.md` records it as parked-not-leak. This corrects the skill's "consumers read it" framing.
- No new `f32` on `Material`, so `sanitize_finite` is unaffected.
- New raw-tier field `ImportedMaterial::character_skin` (see Dim 8). It has no canonical consumer, which is the correct shape for a raw-tier spawn-hook fact.

### Dim 3 — Skinning and lights
- `SkeletonBones::from_scene` (`crates/nif/src/import/mesh/skeleton.rs`, #6473) used a hand-rolled rotation+translation fold with no scale term and a bare `NiNode` downcast. It now folds through `compose_transforms` (the single composition site, which includes the #4549 sanitiser) and walks parents via `any_ni_node`. Pinned by `from_scene_scales_child_offsets_and_walks_switch_node_parents`. This removes a second transform-composition site, so it is a `single-boundary` improvement.
- `skin.rs` is a doc/type-alias move only (#6513). Lights are untouched, and `light_dispatch_coverage_tests` passes.

### Dim 6 — Collision
- `crates/nif/src/import/collision/` is unchanged and `dispatch_coverage_tests` passes.
- `render_geometry_can_collide` (`cell_loader/spawn.rs`, #6459) gained an exclusion for unlit (`MATERIAL_KIND_NO_LIGHTING`) alpha-blended sheets. It reads only the raw `ImportedMaterial`, adds no `MeshHandle` and no per-game branch, and is pinned by `unlit_blended_sheets_do_not_synthesize_collision` plus an FNV corpus test. The skill's gate list (decals, alpha-test, effect shader, fire-refraction) is now one item short. That is skill drift, not a code finding.

### Dim 7 — Animation
- `anim_convert.rs`, `asset_provider/animation.rs` and `crates/nif/src/anim` are unchanged apart from a comment. The completeness harness and hkx guards pass.
- `NamedSequences::spawn_handle` (`crates/core/src/animation/named_sequences.rs`, #6471) chooses which authored sequence starts at spawn: `Idle`, else `SpecialIdle`, else none. This is consumer-side selection among already-converted clips, not a translation, and the two-clips-on-two-players rule is intact.

### Dim 8 — Shader flags / texture roles
- `is_character_skin_texture` (`crates/nif/src/import/material/mod.rs`) decides `ImportedMaterial::character_skin` once, in `into_imported_material`, from the authored base-colour path (#6476, #6477). The string-pool-snapshot path (`skin_texture_symbols`) is deleted, and `set_body_base_color` paints exactly the flagged meshes. Pinned by `the_skin_family_excludes_oblivions_underwear_sub_meshes`, the hook test `race_skin_overrides_only_the_skin_meshes`, and the new ignored real-data test `oblivion_body_skin_flag`, which passes.
- The dark-role census pin now scans production text (#6474), so its needles can no longer match themselves.
- Remaining nif material deltas are comment/citation fixes (#6514). `asset_provider` changes are audio and script only. The role-list, `values()` and `common_material_texture_walk` guards pass. `grep -n 'if game'` over `triangle.frag` and `include/*.glsl` is empty.

### Dims 2, 4, 5, 9 — unchanged
- Dim 2: no commits on its paths. The `units` and `tangent_convention` tests pass.
- Dim 4: the first-step grep (zero canonical consumers of `bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`) is empty.
- Dim 5: `apply_emitter_overlays` callers are still exactly `scene/nif_loader.rs:1824` and `cell_loader/spawn.rs:1430`. The overlay guards pass.
- Dim 9: fill rates are identical to 10-27 (see above).

## Observations (not findings)

- `nifal.md` does not record the raw-tier spawn-hook facts `ImportedMaterial::fallout_hair` or `character_skin`. They are documented on the fields themselves, have no canonical consumer, and the "canonical field for the mask" step is already named in the skill. This is a doc nicety. No tier invariant is violated.
- The skill text for Dim 6 (collision-proxy gate list) and Dim 1 (`source_shader_type` consumers) should be refreshed at the next `/audit-sync`.

## Per-category tier matrix

| Category | Boundary | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|---|
| Material | `translate_material` | pass | pass | pass (`source_shader_type` parked) | pass |
| Geometry/Transform | import/mesh + `coord` | pass | pass | pass | pass |
| Skinning/Lights | `skin.rs`, `walk/lights.rs`, `SkeletonBones::from_scene` | pass (fold now via `compose_transforms`) | pass | pass | pass |
| Nodes | by design (raw-tier parked) | N-A | N-A | pass (zero consumers) | N-A |
| Particles | `apply_emitter_overlays` | pass | pass | pass | pass |
| Collision | `collision/` + `summarize_collision_authoring` | pass | pass | pass | pass |
| Animation | `convert_nif_clip` / `convert_hkx_clip` | pass | pass | pass | pass |
| Shader flags / roles | `slot_to_role`, `merge.rs` | pass | pass | pass | pass |
| Completeness | `translation_completeness` | pass | pass | pass | pass |

## Documented-limitation ledger (unchanged, do not re-report)

- Node passthroughs: `bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`.
- FO4+ NP collision blob, `BhkPCollisionObject` phantoms, `BhkPlaneShape` (no collider), and decoded-but-not-imported constraint kinds.
- Particle size-over-life and colour-curve deferrals, and legacy `NiParticleSystemController` keeping the preset rate.
- Animation: per-light ambient colour channels, morph-weight channels, and `NiPathInterpolator` follow-axis/banking.
- Light affected-node scoping for every `NiLight` kind, and `NiAmbientLight` parked by census.
- Starfield CDB parked slots: `EmissiveSettingsComponent`, translucency, the five single-channel kinds, additive blend modes (#5509).
- `Material::source_shader_type`: canonical-tier parked, reserved with no reader (#6138).
- `fallout_hair` and `character_skin`: raw-tier facts with no canonical consumer.
