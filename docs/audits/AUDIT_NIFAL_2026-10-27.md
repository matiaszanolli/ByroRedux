**HEAD**: fa6be4a6b · **Baseline**: docs/audits/AUDIT_NIFAL_2026-10-21.md (HEAD 4a3e89d56) · **Audited**: Dims 1, 2, 4, 7, 8, 9 (delta since 4a3e89d56, read in full) · **Unchanged since baseline (skimmed)**: Dims 3, 5, 6 (each delta is refactor-only; first steps and guards run)

# NIFAL Audit — 2026-10-27

One auditor working alone, as part of the `comprehensive` suite. Scratch notes are in `/tmp/audit/nifal/dim_{1..9}.md`. The delta is 395 commits since the 10-21 baseline. Every dimension had at least one commit touching its paths. For Dims 3, 5 and 6 those commits are renames, splits or shared-helper refactors, so those dimensions were skimmed.

## Phase-1 guards run this session

- `cargo test -p byroredux --bin byroredux -- material_translate canonical_animation_completeness_harness every_hkx_sample_field every_exterior_spawner every_source_derived common_material_texture_walk`: 84 passed, 0 failed.
- `cargo test -p byroredux --bin byroredux -- every_overlay_parameter every_declared_overlay_parameter`: 2 passed.
- `cargo test -p byroredux-nif --lib -- dispatch_coverage_tests light_dispatch_coverage_tests the_scene_graph_walkers roles_covers_every_field values_covers_every_field units tangent_convention`: 31 passed.
- `cargo test -p byroredux-nif dispatch_coverage_tests`: 5 passed.
- `cargo test --release -p byroredux-nif --test translation_completeness -- --ignored cross_game_translation_completeness`: passed. Every fill rate matches 10-21 exactly. The new `consistent` column is 100% on all eight games.

```
  Oblivion   meshes=567 tex=91.4% mat_path= 0.0% m_kind= 0.0% metO=100.0% rghO=100.0% spec=100.0% gloss= 0.0% env= 0.0% nrm= 0.0% tan= 84.0%
  FO3        meshes=687 tex=92.6% mat_path= 0.0% m_kind=10.9% metO= 94.3% rghO= 94.3% spec= 16.7% gloss= 0.0% env= 4.1% nrm=79.2% tan= 99.1%
  FNV        meshes=806 tex=95.3% mat_path= 0.0% m_kind=17.6% metO= 96.7% rghO= 96.7% spec=  9.8% gloss= 0.0% env= 4.0% nrm=78.9% tan= 99.3%
  SkyrimLE   meshes=529 tex=92.1% mat_path= 0.0% m_kind=42.9% metO= 92.1% rghO= 92.1% spec= 67.5% gloss= 0.0% env= 7.0% nrm=67.5% tan= 92.4%
  SkyrimSE   meshes=515 tex=93.8% mat_path= 0.0% m_kind=35.5% metO= 93.8% rghO= 93.8% spec= 76.1% gloss= 0.0% env= 6.8% nrm=76.1% tan= 94.8%
  FO4        meshes=633 tex=91.8% mat_path=56.7% m_kind=56.2% metO= 99.4% rghO= 99.4% spec= 83.1% gloss=73.5% env=46.6% nrm=82.9% tan= 94.6%
  FO76       meshes=773 tex=11.3% mat_path=83.8% m_kind=33.1% metO= 15.8% rghO= 15.8% spec=  7.8% gloss= 0.0% env= 7.1% nrm= 8.8% tan= 96.9%
  Starfield  meshes=811 tex= 0.0% mat_path=94.9% m_kind= 3.7% metO=  5.1% rghO=  5.1% spec=  1.4% gloss= 0.0% env= 3.7% nrm= 0.0% tan=100.0%
```

Dedup sources: `/tmp/audit/open_issues_snapshot.txt` (84 open), closed-issue lookups, and SUITE_RULES §6 (safety, renderer, regression, ecs, concurrency, performance). Neither finding below overlaps them.

Both 10-21 findings are fixed, and the fixes were checked in code:
- NIFAL-D7-2026-10-21-01: the HKX reference-frame yaw is now carried. Fixed by 051179b7b (#6241), which adds `AnimationClip::root_yaw` via `reference_frame_yaw_keys`.
- NIFAL-D8-2026-10-21-01: the `SwapTexture` clamp mode and provenance are fixed by 64cbfb983 (#6305). The swap now calls `resolve_texture_with_clamp` with the mesh's `texture_clamp_mode`, and `mark_base_color_swapped` records `RuntimeOverride`.

## Executive Summary

**New findings: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 2 LOW.** The translation boundaries themselves have no violation this cycle.

| Category | Status | Boundary fn | Note |
|---|---|---|---|
| Material | converged | `translate_material` / `_with_provenance` | `Material` / `material_translate` have no `pub` delta. The normal-alpha gate helpers drop unused parameters (4ad1bbda7). Outside the translate module, `Material` struct literals appear only in test modules and the exempt Cornell builders. |
| Geometry / Transform | converged | import (`coord`, `units`, `tangent`, shader-type positions) | The EyeEnvmap centres cross Y-up once (52e48bd4c). The consumer `pack_eye_fields` does not convert them again. |
| Skinning / Lights | converged | `skin.rs`, `walk/lights.rs` | The satellite walkers share `node_is_culled` (088821edd). Lights now carry `EmitterKind` directly (178e1e697). |
| Nodes | triaged (by design) | none | Parked fields still have zero canonical consumers. The legacy furniture-bed decode is new and sourced (0c6cd46dc); some downstream comments are stale (D4-01). |
| Particles | converged | `apply_emitter_overlays` | Still two call sites. The emitter walker now skips editor-marker subtrees. |
| Collision | converged | `collision/` resolve + `extract_np_ragdoll` | TAG0 tagfiles are declined (#6302). Ragdoll parents now compose through `compose_transforms` (#6303). The `NP_*` constants remain `map_or` fallbacks only. |
| Animation | converged | `convert_nif_clip` + `convert_hkx_clip` | `root_yaw` is produced only by `convert_hkx_clip`. All other `AnimationClip` literals are in test modules. The sequence player and sinks are per-sequence (#6253, #6254). |
| Shader flags / texture roles | triaged | `slot_to_role`, merge arms, runtime overrides | Pack-in children keep their own swaps (#6260). The race-skin override's mesh filter is LOW (D8-01). |
| Completeness | converged | `translation_completeness` | Green on all eight games (table above). |

Violations by invariant: single-boundary 0, no-fabrication 0, no-leak 0, no-render-time-fallback 0, harness-gap 0. The two LOW findings are a doc-rot item (D4) and a fragile, costly runtime override filter (D8).

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|
| Material (`translate_material`) | pass | pass | pass | pass (`if game` grep of `triangle.frag` + `include/*.glsl`: empty; `render/static_meshes.rs` split has no classifier) |
| Geometry / Transform | pass | pass | pass | pass |
| Skinning / Lights | pass | pass | pass | pass |
| Nodes | N-A (no boundary, by design) | pass (bed range 1–10 cites the CS *GetFurnitureMarkerID* page + a 75-NIF census) | pass | pass |
| Particles (`apply_emitter_overlays`) | pass | pass | pass | pass |
| Collision | pass | pass (NP fallbacks documented) | pass | pass |
| Animation (`convert_nif_clip`, `convert_hkx_clip`) | pass | pass | pass | pass |
| Shader flags / texture roles | pass | pass | pass | pass |

## Findings

### LOW

### NIFAL-D4-2026-10-27-01: Three comments still say legacy furniture markers can never resolve to Sleep
- **Severity**: LOW
- **Dimension**: Nodes
- **Tier Violated**: parked-not-leak (the documentation of the passthrough is stale; the code is right)
- **Game Affected**: Oblivion, FO3, FNV
- **Location**: `byroredux/src/systems/eat_sleep.rs:532-540`, `byroredux/src/cell_loader/references/attach.rs:66-68`, `crates/core/src/ecs/components/furniture.rs:23-30`
- **Status**: NEW (#5390 is closed; no open issue covers this)
- **Description**: 0c6cd46dc taught `imported_furniture_marker` (`crates/nif/src/import/mod.rs:392-404`) to map a legacy `position_ref_1` of 1..=10 to `animation_type` 2 (Sleep). The source for that range is the CS *GetFurnitureMarkerID* page: "1 - 10 for beds". The same commit rewrote the `eat_sleep.rs` module doc and the `FurnitureMarker.animation_type` field doc. Three nearby statements still describe the old behaviour:
  - `eat_sleep.rs:532-536`: "on FO3/FNV this is not 'no bed found' but 'no bed can EVER resolve': legacy markers author animation_type 0 and the translate boundary reads them all as Sit … until Phase C decodes furnituremarkerNN". The debug log text at 537-540 repeats it: "legacy cells author none (animation_type 0 over-match, #5390)". This contradicts the module doc 20 lines above it.
  - `attach.rs:66-68` (the `_ =>` arm of `furniture_component`): "0 = legacy (Oblivion/FO3/FNV, no AnimationType authored at all)".
  - `furniture.rs:23-30` (`FurnitureMarkerKind` doc): "the position kind is implied by the referenced `furnituremarkerXX.nif`, which isn't resolved at this layer — so legacy markers take … Sit … A real per-marker decode is deferred to Phase C".
- **Impact**: None at runtime. However, a live debug log now misreports the decision whenever the sit fallback fires on a legacy cell. A future auditor or fixer would also conclude that the bed decode is still missing.
- **Suggested Fix**: Reword the three sites to match the module doc. Legacy beds (1–10) resolve to Sleep at the importer, and every other legacy marker is the v0 Sit default. Change the log text to "no Sleep marker in range".

### NIFAL-D8-2026-10-27-01: The race-skin override's skin-mesh filter copies the whole StringPool for every body piece, and it works only because an earlier pre-parse interned the NIF's paths
- **Severity**: LOW
- **Dimension**: Shader-flags/Effects (spawn-time `RuntimeOverride` of the `base_color` role)
- **Tier Violated**: single-boundary (the role-override decision depends on hidden spawn ordering) / performance
- **Game Affected**: Oblivion, FO3, FNV (humanoid body-section ICON override). Every game pays the cost.
- **Location**: `byroredux/src/npc_spawn/loot_appearance.rs:78-100` (`skin_texture_symbols` / `skin_texture_symbols_from`), call sites `byroredux/src/npc_spawn/resumable/runtime.rs:726-727` and `loot_appearance.rs:914`
- **Status**: NEW (introduced by 9d1a262ed, #6090)
- **Description**: #6090 limits `set_body_base_color` → `override_base_color` to skin meshes. A mesh counts as skin when its authored `base_color` symbol is in a `HashSet` built by `pool.dump()` over the entire world `StringPool`, keeping every string that starts with `textures\characters\`. There are two problems:
  1. **Cost, paid for nothing in most cases.** The snapshot is taken unconditionally for every body piece, before the code checks `body_texture.is_some()`. That includes creatures (`body_textures: vec![None; …]`, runtime.rs:614) and every NPC with no race ICON. The hook it serves is attached only when `body_texture.is_some()` or a hand seam applies (runtime.rs:757-764). `dump()` allocates a `String` per interned symbol (`crates/core/src/string/mod.rs` `dump`), and each one is then lowercased. That is O(pool size) on the main-thread spawn unit, three or four times per NPC. The corpse-restore path (`loot_appearance.rs:914`) does the same.
  2. **Order dependence.** The snapshot is taken *before* `load_nif_extracting_with_skeleton`. On a cache miss, that call's import (`scene/nif_loader.rs` `cached_scene_lazy` → `import_nif_scene_with_resolver` with the world pool) is what interns the body NIF's own skin path. The filter currently gets the right answer only because `build_seam_context` already ran in the Skeleton phase. On Oblivion/FO3/FNV humanoids with a skeleton it calls `peek_or_parse_scene` on every body path (`seam_blend.rs:94-125`), which interns those paths earlier. If that pre-parse does not happen (no skeleton root, `blend_skin_seams` false, or a later reordering), the snapshot lacks the NIF's skin symbol on that NIF's first import. Every mesh then reads as non-skin, and the race skin is silently not applied. No test covers the ordering; `race_skin_overrides_only_the_skin_meshes` builds the pool by hand.
- **Evidence**: runtime.rs:719-727 (intern the race texture, then snapshot) is followed by the load at runtime.rs:767. `skin_texture_symbols_from` iterates `pool.dump()`.
- **Impact**: Today this is a CPU cost per body piece in NPC spawn units. If the spawn order ever changes, the #6090 fix silently stops working (Ghoul and child races would render with the default human skin on the first spawn per body NIF).
- **Suggested Fix**: Decide skin-ness from the scene being overridden. Inside the hook, resolve each mesh's `base_color` symbol and test the prefix: give the hook a pool reader, or record the resolved prefix on `ImportedMesh` at import. Skip all of the work when `body_texture` is `None`.

## Documented-limitation ledger (parked, not leaks: do not re-report)

- Nodes: `bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs` have zero canonical consumers (first-step grep empty). `bs_sub_index` is consumed raw→raw by `hide_skin_partitions`. The legacy furniture *heading* (ushort `Orientation`) stays deferred. Only the bed/non-bed kind is decoded.
- Lights: affected-node scope is untranslated for every `NiLight` kind (nifal.md, #5282). `NiAmbientLight` is parked by census.
- Collision: the `BhkNPCollisionObject` blob shape is not decoded. FO76/Starfield TAG0 ragdoll tagfiles are declined (#6302). `BhkPCollisionObject` phantoms and the decoded-but-not-imported constraint kinds remain as listed.
- Particles: the size-over-life bell curve keeps magnitude only, and `initial_color` is intentionally not applied.
- Animation: per-light ambient colour channels are parked. Morph weights reach `AnimatedMorphWeights` but have no GPU consumer (#2221). The `NiPathInterpolator` follow-axis, banking, constant-velocity and closed-wrap behaviours are parked. The loose `nif_loader` path plays only the standalone-controller clip; it never played sequences. The HKX completeness harness fixture has `reference_frame: None`, so `root_yaw` is pinned by the dedicated tests (`reference_frame_yaw_becomes_unwrapped_root_yaw_keys`, plus real-data `fallout_4_turn_in_place_clip_carries_its_turn_as_root_yaw`) rather than by the kitchen-sink harness.
- Starfield CDB: `EmissiveSettingsComponent` is parked (#5503). The additive-family `BlendingMode`s are parked by name with census counts (#5509 open; Additive/Multiply factors are sourced but not landed). The single-channel kinds (`_rough`/`_metal`/`_ao`/`_opacity`/`_transmissive`) and slot 6 `_height` are parked.
- Still-open NIFAL issues, re-checked and not re-reported: #4246 (third Phase-2 resolver doc), #6138 (`Material::source_shader_type` is still written but never read outside tests), #5509, #3398.

Publish with `/audit-publish docs/audits/AUDIT_NIFAL_2026-10-27.md` (labels: `nifal`, `low`; D4-01 adds `doc-rot`, D8-01 adds `performance`; `game:fo3` / `game:fnv` / `game:oblivion` are optional).
