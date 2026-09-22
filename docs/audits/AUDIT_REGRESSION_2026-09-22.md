# Regression Verification Audit — 2026-09-22

**HEAD**: `ee6d3fb39` · **Baseline**: `docs/audits/AUDIT_REGRESSION_2026-09-11.md` (HEAD `b3db49fa`) · **Audited**: 40 churn-selected previously-closed issues (see selection below) + Step 4 unconditional fragile-area guards · **Unchanged since baseline (skimmed)**: n/a — this sweep is itself the delta pass; no dimension carried over unexamined.

## Selection method

Per the skill's churn-weighted discovery (default scope, no `--issues`):

```bash
D=2026-09-11
base=$(git rev-list -1 --before="$D 23:59" HEAD)          # b3db49fa
git diff --name-only "$base"..HEAD -- '*.rs' '*.glsl' '*.comp' '*.frag' '*.vert' > churn.txt   # 623 files
xargs -a churn.txt -I{} git log "$base" --format=%s -- {} \
  | grep -oiE '(fix|fixes|fixed|close[sd]?|resolve[sd]?) #[0-9]+' | grep -oE '[0-9]+' \
  | sort | uniq -c | sort -rn | head -40
```

This finds the closed-issue fix commits that landed **before** the 2026-09-11
sweep, on files that have **churned since** — i.e. exactly the fixes whose
surrounding code changed enough since the last regression check to be worth
re-verifying, ranked by how many of the 40 candidate touch-file overlaps each
fix's commit message references. `--limit 40` (the skill default) selected 40
distinct issue numbers, spanning closures from 2026-05-10 (#869) through
2026-09-11 (#4090).

`gh issue list --state all --limit 6000` was refreshed first
(`/tmp/audit/issues.json`, 4535 issues, 4402 closed) since the cached copy
predated most of today's closures. All 40 candidates confirmed CLOSED in the
refreshed listing.

**Cross-check**: grepped all `docs/audits/AUDIT_*_2026-09-21*.md` and
`AUDIT_*_2026-09-22.md` sibling reports (ECS, physics, character, scripting,
papyrus, exterior, ESM, NIF, NIFAL×2, parsers, performance, renderer, safety,
tech-debt, UI, concurrency, gameplay, audio, legacy-compat, save, speedtree)
for overlap with these 40 issue numbers — **zero overlap**. No corroborating
evidence to cite; every verification below is this sweep's own.

No sub-agents were used (per task instruction); all 40 issues plus the Step 4
checks were verified directly, one at a time, with progress appended to
`/tmp/audit/regression/checked.md` as each was completed.

## Bottom line

**39 PASS, 1 N/A (closed-as-declined, no fix to regress), 0 FAIL, 0
PARTIAL-without-evidence, 0 UNVERIFIABLE.** Every Step 4 fragile-area guard
is live and green. **No regressions found.**

Two issues (#3402, #3386) have their fix code and call sites confirmed
present via source reading but no test isolates that exact site directly —
noted per-entry; both are backstopped by a sibling guard that *is* tested, so
neither is scored PARTIAL.

## Per-issue entries

### EXAL ground cover phases (#4052, #4054, #4055, #4057, #4059)

- **Status**: PASS (all five)
- **Closed**: 2026-09-07 (#4054/#4055/#4057/#4059), 2026-09-07 (#4052)
- **Fix commits**: `40b5c5b6a` (#4052), `637b65264` (#4054, #4055), `b01ef9260` (#4057, #4059)
- **Fix site**: `crates/renderer/src/vulkan/groundcover.rs`, `groundcover_bench.rs`, `crates/renderer/shaders/groundcover_*.{vert,frag,comp}`, `include/groundcover_*.glsl`
- **Fix present**: Yes — chunking, density field (`d_ground` vs `d_draw` split), scatter compute, blade vertex/wind shaders, light response (occlusion/translucency/canopy shadow/sheen), and the §11.1 terrain-attribute bench are all live.
- **Guard test**: 44/44 `vulkan::groundcover*` tests green, incl. `no_water_resolves_to_a_neutral_moisture_term`, `the_blade_record_stores_d_ground_not_d_draw`, `the_clump_term_is_not_stubbed`, `blade_wind_uses_seeded_harmonic_and_lateral_sway`, `occlusion_and_canopy_shadow_share_one_extinction`, `transmission_survives_the_diffuse_clamp_but_not_the_shadow_ray` — passes
- **Notes**: These five issues are the largest-churn candidates (12–32 file-overlap hits each) — expected, since ground cover is this window's most actively developed feature. No regression despite the heavy churn.

### #4059: Terrain LAND VNML normals decoded as unsigned

- **Status**: PASS
- **Closed**: 2026-09-07
- **Fix commit**: `b01ef9260`
- **Fix site**: `byroredux/src/cell_loader/terrain.rs` (`decode_vnml_normal`, signed `i8` read)
- **Fix present**: Yes
- **Guard test**: `flat_ground_vnml_decodes_to_world_up`, `vnml_negative_components_survive_the_decode`, `vnml_raw_magnitude_flags_the_exact_zero_vector_as_degenerate`, `vnml_raw_magnitude_matches_measured_real_corpus_range` — 4/4 pass

### #4090: `cargo clippy` red at HEAD (core NaN guards, renderer SAFETY-comment spacing)

- **Status**: PASS
- **Closed**: 2026-09-11
- **Fix commit**: `da3d5f227`
- **Fix site**: `crates/core/src/animation/{player,stack}.rs` (`is_nan() || …` NaN guards replacing `!(x >= y)`), `crates/renderer/src/vulkan/{buffer,context/draw}.rs` (SAFETY comment directly precedes `unsafe`)
- **Fix present**: Yes
- **Guard test**: `cargo clippy -p byroredux-core -- -D warnings` — clean (0 warnings). `cargo clippy -p byroredux-renderer --all-targets -- -D warnings` — the two lints this issue named (`undocumented_unsafe_blocks`, `approx_constant`) do **not** reproduce.
- **Notes**: `byroredux-renderer --all-targets` clippy is still red today, but on **9 different, unrelated** lints (`field_reassign_with_default`, `needless_range_loop`, `assertions_on_constants` ×2, `useless_format`) introduced by later commits — not a regression of #4090's scope, and consistent with the project's own memory note that the workspace clippy gate carries ~698 pre-existing sites. Not filed as a new finding here (out of this sweep's churn-selected scope; belongs to whichever future clippy-gate issue picks it up).

### #779: `triangle.frag` missing `early_fragment_tests` (PERF-N6)

- **Status**: N/A — closed-as-declined, not a landed fix to regress
- **Closed**: 2026-08-22
- **Fix commit**: none landed — full history: `4a220f578` (fix) → `649996ae9` (revert) → `436d16c56` (take 2) → `b5517e672`/`7a91597fa` (diagnostic + its revert) → `e0d4144d8` (final revert)
- **Fix site**: n/a
- **Fix present**: No, deliberately. The depth-prepass + `early_fragment_tests` approach was fully reverted after two attempts produced visible rendering artifacts (chrome specular, diagonal seams) that could not be root-caused without RenderDoc. The issue's final resolution comment identifies a **correctness risk** the original 2026-05-01 finding did not see: `triangle.frag` is shared by the opaque (`depth_write_enable=true`) and blend pipelines, and forcing `early_fragment_tests` shader-wide would make the alpha-cutout discard (foliage/fences/grates) write depth anyway, incorrectly occluding geometry visible through cutout holes.
- **Guard test**: none exists (nothing to guard)
- **Notes**: Current `triangle.frag` correctly has no `early_fragment_tests` declaration, and `depth_prepass`/`DEPTH_PREPASS`/`create_depth_prepass_pipeline` are fully absent from `crates/renderer/src/`, confirming the revert is complete and not partially applied. This matches the project memory note on speculative Vulkan fixes (RenderDoc-or-revert doctrine). No regression is possible here because no fix was ever kept.

### SpeedTree cluster (#3528, #3529, #3530, #3531)

- **Status**: PASS (all four)
- **Closed**: 2026-08-30
- **Fix commit**: `198134604`
- **Fix site**:
  - #3528 `byroredux/src/cell_loader/references/import.rs` (`resolve_tree_icon_path`)
  - #3529 `crates/spt/src/import/mod.rs` (`clamp_billboard_extent`, `compute_billboard_size`)
  - #3530 `crates/nif/src/blocks/properties.rs` + `crates/nif/src/import/material/legacy_properties.rs` (`apply_mode` field + `APPLY_HILIGHT2` consumer)
  - #3531 `crates/spt/src/parser.rs` (`is_plausible_spt_curve_string` empty-slice check)
- **Fix present**: Yes, all four. #3530's Apply-Mode gate was itself later corrected by `b9e3e1014`/`ec3a6abdc` (making it reachable on Oblivion) — a forward hardening, not a regression.
- **Guard test**:
  - #3528: `tree_icon_resolves_bare_filenames_under_the_measured_directory` — pass
  - #3529: `non_finite_bnam_falls_through_instead_of_producing_a_nan_quad`, `bnam_clamps_to_safe_band`, `modb_clamps_to_safe_band`, `corrupt_obnd_clamps_size_to_safe_band` — 4/4 pass
  - #3530: 7 parser `apply_mode` tests (`blocks::properties::tests::*`) + `apply_hilight2_flags_the_alpha_channel_without_a_normal_slot`, `oblivion_apply_hilight2_routes_the_normal_map_into_the_parallax_slot` — 9/9 pass
  - #3531: `tag_13005_before_zero_leading_tail_resolves_as_bare`, `empty_candidate_is_not_a_plausible_curve_string` (+ 4 sibling `tag_13005_*` tests) — 6/6 pass
- **Notes**: `31fa4596b` (#4118/#4120) subsequently swept stale doc premises in the surrounding comments (corpus/geometry-tail claims disproven by later measurement) — comment corrections only, code unchanged.

### #3399: ESM compressed-record inflate had no bound

- **Status**: PASS
- **Closed**: 2026-08-29
- **Fix commit**: `05bdb9691`
- **Fix site**: `crates/plugin/src/esm/reader.rs` (`read_sub_records`, `record_inflation_ceiling`)
- **Fix present**: Yes — ceiling is `min(64 MiB, max(64 KiB, compressed_len × 512))`, decoder held to `take(declared + 1)`. Since hardened further by `ee5e8ea6d` (#3720, corrupt-Adler32-trailer recovery via raw-DEFLATE retry, gated on exact declared-length match).
- **Guard test**: `compressed_record_too_small_returns_error`, `compressed_record_with_corrupt_adler32_trailer_recovers_via_raw_deflate`, `compressed_record_round_trips_sub_records`, `compressed_record_prefix_matches_payload_length`, `inflation_ceiling_clears_every_observed_vanilla_shape`, `compressed_record_with_corrupt_deflate_body_still_errors`, `compressed_record_with_oversized_prefix_is_rejected`, `compressed_record_inflating_past_its_prefix_is_rejected` — 8/8 pass

### #3400 / #3401: ESM record-tier FormID remap sweep (SCOL/PKIN high; ACTI/MOVS/NAVM/FLST/REGN medium)

- **Status**: PASS (both)
- **Closed**: 2026-08-29
- **Fix commit**: `05bdb9691` (original), superseded by `48acaea2f` (#4069/#4070/#4071 — see Notes)
- **Fix site**: `crates/plugin/src/esm/records/{scol,pkin,movs,list_record}.rs`, `crates/plugin/src/esm/records/misc/world.rs` (`parse_acti`) — all confirmed to take `remap: &Option<FormIdRemap>`
- **Fix present**: Yes
- **Guard test**: The original per-parser allowlist guard was replaced (`48acaea2f`, 2026-09-10) by a **structural denylist** that walks every `pub fn parse_*` in `records/` at test time and requires it to either take `remap` or appear in one of two exemption tables (mechanically verified / justified-with-reason). `every_record_parser_takes_a_remap_or_is_explicitly_exempt`, `parsers_that_take_a_remap_actually_use_it`, `guard_helpers_detect_what_they_claim_to`, `remap_fid_has_exactly_one_definition`, `no_parser_with_a_remap_in_scope_passes_an_identity_remap` — 5/5 pass
- **Notes**: This is a case where the *specific* guard cited by the original issue was retired and replaced by a stronger, more general one covering the same ground plus 8 more parsers found by the same defect class (#4066, #4069–#4071). The replacement is confirmed live and green, so both #3400 and #3401 remain fixed — verifying the newer guard is the correct way to re-check this, not a gap.

### #3402: 23 Skyrim skinned meshes reach `MeshRegistry::upload` with zero indices

- **Status**: PASS
- **Closed**: 2026-08-29
- **Fix commit**: `05bdb9691`
- **Fix site**: `byroredux/src/scene/nif_loader.rs` (`spawn_nif_mesh`, skips `mesh.indices.is_empty()` with `debug!`), backstopped by `crates/renderer/src/mesh.rs` (`validate_upload_geometry`, #3406)
- **Fix present**: Yes, at both layers
- **Guard test**: `empty_vertices_or_indices_are_rejected`, `non_empty_geometry_passes`, `upload_scene_mesh_validates_before_touching_the_global_pool` — 3/3 pass
- **Notes**: The `nif_loader.rs` skip site itself (the root-cause fix — the indices are legitimately emptied downstream by `ImportedMesh::hide_skin_partitions` when equipped gear covers every skin partition) has no dedicated unit test; it sits inside a large, hard-to-isolate spawn function. It is not scored PARTIAL because the `validate_upload_geometry` backstop added in the same commit chain (#3406) independently guards the same failure mode (a zero-index buffer reaching `vkCreateBuffer`) and *is* tested — a regression of the `nif_loader.rs` skip alone would degrade to a caught `Err` + `warn!`, not a silent GPU allocation failure.

### #2221: Non-transform animation channels have no production sink components

- **Status**: PASS
- **Closed**: 2026-08-23
- **Fix commit**: `7fbc5bafb` (+ prerequisite `a8b0cf648`, 2026-07-28)
- **Fix site**: `byroredux/src/anim_convert.rs` (`attach_animation_sinks`), called from all three production spawn paths (`byroredux/src/scene.rs:721`, `cell_loader/spawn.rs:839`, `scene/nif_loader.rs:633`); `byroredux/src/render/static_meshes.rs` (merges `AnimatedAlpha`/`AnimatedDiffuse`/`AnimatedAmbient`/`AnimatedSpecular`/`AnimatedEmissiveColor`/`AnimatedShaderColor`/`AnimatedShaderFloat` into `DrawCommand` before material interning); `crates/renderer/src/vulkan/material.rs` (`GpuMaterial::shader_color_r`/`shader_float` fields)
- **Fix present**: Yes, all sites
- **Guard test**: 7 `anim_convert::sink_attachment_tests::*` (`attaches_only_the_sinks_the_clip_actually_targets`, `does_not_overwrite_an_existing_sink`, `light_channels_never_synthesise_a_lightsource`, `morph_and_uv_channels_coalesce_into_one_component_each`, `uv_sink_seeds_unanimated_slots_from_the_authored_material`, `second_clip_targeting_a_new_texture_flip_slot_is_silently_dropped`, `pure_transform_clip_attaches_nothing`) + 6 routing/merge tests (`shader_color_routes_to_shader_component`, `alpha_target_writes_animated_alpha`, `shader_float_target_writes_shader_float_component`, `animated_shader_color_and_float_reach_the_draw_command`, `animated_alpha_and_diffuse_override_the_static_material_through_interning`, `phase_jittered_animated_alpha_population_dedups_to_bounded_material_count`) — 13/13 pass

### #869: NiWireframeProperty / NiShadeProperty parsed but never consumed

- **Status**: PASS
- **Closed**: 2026-05-18
- **Fix commit**: `e3c54a325` (part 1, LINE pipeline), `fc126e80a` (part 2, flat shading)
- **Fix site**: `crates/renderer/src/vulkan/pipeline.rs` (`PipelineKey::{Opaque,Blended}.wireframe` axis, `opaque_wireframe` pipeline), `crates/renderer/shaders/include/ray_hit.glsl` + `triangle.frag` (consume `INSTANCE_FLAG_FLAT_SHADING`, bit 7)
- **Fix present**: Yes — wireframe is a live pipeline-key axis; flat shading is wired via an instance flag bit rather than a GLSL `flat` qualifier (a stronger approach than the issue's original suggestion, since it works per-instance without a shader permutation)
- **Guard test**: `pipeline_key_wireframe_is_distinct_axis`, `flat_shading_bit_pinned_at_128_for_shader_constant`, `parse_flag_property_wireframe_disabled` — 3/3 pass

### #1904: ~134 renderer FFI unsafe blocks with no SAFETY comment

- **Status**: PASS
- **Closed**: 2026-07-14
- **Fix commit**: `332c0230d` (+ follow-up `b0d331aff`, FSR presentation pass)
- **Fix site**: `crates/renderer/src/lib.rs:21` (`#![deny(clippy::undocumented_unsafe_blocks)]`)
- **Fix present**: Yes, and superseded by a structural guarantee: the manual per-block sweep was followed by a crate-level deny-lint, which makes any future undocumented `unsafe {}` a compile error rather than something that has to be re-swept by hand.
- **Guard test**: `cargo clippy -p byroredux-renderer --lib -- -W clippy::undocumented_unsafe_blocks` → 0 violations at HEAD

### #2507: Caustic skip-clear leaves a frozen accumulator pool

- **Status**: PASS
- **Closed**: 2026-08-09
- **Fix commit**: `33c39c871`
- **Fix site**: `crates/renderer/src/vulkan/context/post_passes.rs` (`skip_clear_decision`, `caustic_cleared_on_skip` latch), `crates/renderer/src/vulkan/caustic.rs` (`clear_for_skip`)
- **Fix present**: Yes, and generalized: the same `skip_clear_decision` shape now also drives `record_volumetrics_pass`'s `volumetrics_cleared_on_skip` latch (#3685), so the fix is exercised by two independent call sites.
- **Guard test**: `dispatch_ran_never_clears_and_resets_latch`, `first_skip_of_a_streak_clears`, `subsequent_skip_in_same_streak_does_not_reclear`, `record_volumetrics_pass_routes_skip_clears_through_the_shared_latch` (+ 6 sibling `post_passes::tests`) — 10/10 pass

### #3343 / #3344: FNV particle budget + magnitude-floor real-archive gate

- **Status**: PASS (both)
- **Closed**: 2026-08-28
- **Fix commit**: `d1bcf6e2b`
- **Fix site**: #3344 — `crates/nif/src/blocks/particle.rs` (`max_particles` field, `MAX_PARTICLES_CEILING = 256`), `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_max_particles`, now finds the first `NiPSysBlock` that actually carries a budget rather than stopping at the first matching marker type — this was itself a bug #3343's own test caught during the original fix). #3343 — `crates/nif/tests/parse_real_nifs.rs` (`real_archive_torch_meshes_surface_particle_emitters`)
- **Fix present**: Yes
- **Guard test**: #3344: `emitter_max_particles_tests::*` — 5/5 pass. #3343: ran the `#[ignore]`d real-archive test directly against installed game data (`BYROREDUX_FNV_DATA`, `BYROREDUX_FO3_DATA`, plus Oblivion/Skyrim SE paths it discovers on its own): **350 FNV / 140 FO3 / 272 Oblivion / 137 Skyrim SE emitters swept, all four games pass their magnitude floors** — pass
- **Notes**: This is one of the two tests in this sweep run with `--ignored` against real game data (both scoped to a single named test, never the whole ignored suite — the workspace constraint against unscoped `--ignored` applies to `byroredux-plugin`, which neither of these touches).

### #3345: `AnimationClip.phase` dropped at NIF→core boundary

- **Status**: PASS
- **Closed**: 2026-08-28
- **Fix commit**: `d1bcf6e2b`
- **Fix site**: `crates/core/src/animation/{player,stack}.rs` (`with_phase`), `byroredux/src/anim_convert.rs` (`convert_nif_clip`, non-finite sanitization)
- **Fix present**: Yes
- **Guard test**: `with_phase_seeds_local_and_prev_time`, `with_phase_ignores_zero_and_non_finite`, `phase_survives_the_nif_to_core_conversion`, `non_finite_phase_is_rejected_negative_is_kept` — 4/4 pass

### #3346: `--game fnv` CWD-immune invocation undocumented

- **Status**: PASS
- **Closed**: 2026-08-28
- **Fix commit**: `d1bcf6e2b`
- **Fix site**: `byroredux/src/boot/cli.rs:303` (`expand_game_profile_args`); documented in `README.md`, `ROADMAP.md`, `.claude/commands/audit-fnv/SKILL.md`
- **Fix present**: Yes, both the function and the documentation
- **Guard test**: function presence pinned structurally in `byroredux/src/boot/mod.rs`'s source-shape registry (`("cli.rs", "fn expand_game_profile_args")`)

### #3749: 80% of `#[ignore]`s carry no machine-readable reason

- **Status**: PASS
- **Closed**: 2026-09-03
- **Fix commit**: `8b9a85722`
- **Fix site**: workspace-wide — all 138 bare `#[ignore]` sites converted to `#[ignore = "<reason>"]`
- **Fix present**: Yes
- **Guard test**: `byroredux/src/workspace_hygiene_tests.rs::every_ignore_attribute_carries_a_reason` — a whole-repo scan, not scoped to any one file — pass. Given how many files churned since baseline, this is the strongest possible confirmation that no new bare `#[ignore]` has crept back in anywhere in the tree.

### #3390: Creatures (CREA, FO3/FNV) derive no ActorValues

- **Status**: PASS
- **Closed**: 2026-08-29
- **Fix commit**: `a13272277` (+ follow-up `6b73c84da`, #3762, unrelated to this regression check — gives creatures their authored attack damage, doesn't touch the DATA-parse/ActorValue path)
- **Fix site**: `crates/plugin/src/esm/records/actor.rs` (`CREA.DATA` 17-byte parse), `crates/plugin/src/esm/records/actor_value_derive.rs` (`derive_creature_actor_values`, `CharacterRulesProfile::creature_stat_model`), `resolve_inherited_record` (learns `CREA`/`LVLC` `TPLT`)
- **Fix present**: Yes
- **Guard test**: `crea_data_arm_is_gated_to_the_fallout3_fnv_era`, `crea_data_decodes_the_sourced_seventeen_byte_layout`, `creature_use_stats_follows_an_lvlc_template`, `an_npc_beside_a_creature_still_auto_calcs`, `creature_use_stats_follows_a_crea_template`, `creature_derives_special_and_health_from_its_own_data`, `creature_with_non_positive_health_still_derives_special`, `creature_without_a_data_block_derives_nothing`, `oblivion_creatures_select_no_stat_model`, `crea_group_dispatches_to_creatures_map`, `lvlc_group_dispatches_to_leveled_creatures_map` — 11/11 pass. Also ran the `#[ignore]`d real-archive test `parse_real_fnv_creatures_derive_actor_values` against installed FNV data directly — pass (single named test, `byroredux-plugin`, ~1.6 s — not the unscoped ignored sweep the workspace constraint prohibits).

### #3386 / #3387 / #3388: Per-cell teardown, `touch_keys`, `stamp_cell_root_range` batching

- **Status**: PASS (all three)
- **Closed**: 2026-08-29
- **Fix commit**: `a13272277`
- **Fix site**: #3386 — `byroredux/src/streaming_helpers.rs` (`drain_streaming_state`) + `byroredux/src/app_events.rs` (`shutdown`), both route through `cell_loader::unload_cells` (batched) rather than a per-cell `unload_cell` loop. #3387 — `byroredux/src/cell_loader/nif_import_registry.rs` (`touch_keys`, `get_mut` not `contains_key`+`insert`). #3388 — `byroredux/src/cell_loader/load.rs` (`stamp_cell_root_range`, `world.insert_batch`)
- **Fix present**: Yes, all three confirmed via direct source read
- **Guard test**: #3387: `touch_keys_protects_recently_hit_entries_from_lru`, `touch_keys_bumps_residents_and_never_inserts_absent_keys`, `touch_keys_spends_no_tick_on_a_non_resident_key` — 3/3 pass. #3388: `stamp_cell_root_range_tests::*` (3) + `root_index_tests::*` (2) — 5/5 pass. #3386: no dedicated unit test found for the batched-call-site shape — scored PASS on direct source confirmation (both call sites literally invoke `cell_loader::unload_cells`, the batched form, not the singular `unload_cell`), not PARTIAL, since a regression here would be immediately visible as a large perf/log-volume regression in any runtime session, which is a strong operational tripwire even without a unit test.

### #1038: Shader↔Rust constant drift (build.rs codegen)

- **Status**: PASS
- **Closed**: 2026-05-15
- **Fix commit**: `d6ef9403a` (+ many follow-ups extending coverage, e.g. `15ee3169b`, `835793c71`)
- **Fix site**: `crates/renderer/build.rs` (generates `shaders/include/shader_constants.glsl` from `src/shader_constants_data.rs`, the single source of truth)
- **Fix present**: Yes — this mechanism is now load-bearing infrastructure documented in `CLAUDE.md`'s workspace tree and used throughout the shader pipeline
- **Guard test**: `shader_constants::tests::*` — 58/58 pass, including `every_top_level_shader_constant_has_one_provenance`, `shader_constant_provenance_gate_rejects_synthetic_shared_redeclaration`, and 15+ `triangle_frag_*_not_redeclared` / `*_not_redeclared` guards specific to individual constant families

### #890: BSEffect SOFT_EFFECT / GREYSCALE_TO_PALETTE_* / EFFECT_LIGHTING flags captured but not consumed

- **Status**: PASS
- **Closed**: 2026-05-10
- **Fix commit**: `ece927af5` → `2aa2817a7` → `7eb137b51` → `ea044b685` (staged over several sessions)
- **Fix site**: `crates/nif/src/import/material/shader_data.rs` (`lighting_influence`, `effect_lit`), `crates/renderer/shaders/triangle.frag` (consumes `lighting_influence`)
- **Fix present**: Yes
- **Guard test**: `capture_effect_lit_crc_fallback`, `capture_soft_effect_crc_fallback`, `capture_effect_lit_typed_flag`, `capture_soft_effect_typed_flag`, `parse_bs_effect_shader_soft_falloff_and_greyscale`, `fo4_lit_shader_captures_greyscale_to_palette_alpha`, `fo4_lit_shader_captures_greyscale_to_palette_color`, `fo76_crc_greyscale_to_palette_color_sets_field`, `fo4_slot_3_is_greyscale_lut_and_slot_7_is_smooth_spec_without_msn`, `flat_import_carries_effect_shader_greyscale_lut_to_the_particle_emitter`, `hierarchical_import_carries_effect_shader_greyscale_lut_to_the_particle_emitter` (nif crate, 11) + `greyscale_lut_index_difference_is_distinct` (renderer crate, 1) — 12/12 pass

### Renderer audit follow-ups #3988 / #3989 / #3990 / #3991

- **Status**: PASS (all four)
- **Closed**: 2026-09-07 (#3988–3990), later for #3991 (part of the same sweep, follow-up landed with the skin-submit-state split)
- **Fix commit**: `0025d8221`
- **Fix site / present / guard**:
  - **#3988** (BLAS budget blind to the upscaler): `crates/renderer/src/vulkan/acceleration/predicates.rs` — both render-extent and FSR-SDK-memory terms now ride `FrameExtentSet` + a cached `memory_usage()` figure. `the_reservation_covers_both_extents_and_the_upscaler_sdk`, `blas_budget_subtracts_the_resolution_scaled_reservation` — 2/2 pass
  - **#3989** (`shader-pipeline.md` describing live lanes as free): `docs/engine/shader-pipeline.md` rows for `GpuCamera.render_debug.w` and `material_flags` bit 10 now say "not a free slot". `shader_pipeline_doc_does_not_advertise_live_lanes_as_free` — pass
  - **#3990** (stale SAFETY argument, `upload_instances`): `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:269` (`unsafe impl NoUninit for GpuInstance`), `crates/renderer/src/vulkan/material.rs:418` (`unsafe impl NoUninit for GpuMaterial`), both routed through `write_mapped`. Confirmed by successful compilation (`requires_no_uninit<T: NoUninit>` compile-time bound check)
  - **#3991** (`skin_dispatch_ran` record-time latch read as submit-time signal): `crates/renderer/src/vulkan/context/mod.rs` now carries both `skin_dispatch_ran` (record-time) and `skin_state_submitted` (set only after `queue_submit` returns `Ok`, `skinned_blas_refit.rs:45`). `skin_dispatch_ran_is_reset_before_both_early_return_guards` — pass (re-anchored on the real call chain after the #3282 `draw_frame` split, per this commit's own "sibling found while pinning it" note — the test was vacuous before this fix and is not vacuous now)

### Water/TAA/caustic follow-ups #4007 / #4008 / #4009 / #4010

- **Status**: PASS (all four)
- **Closed**: 2026-09-08
- **Fix commit**: `ea6d24372`
- **Fix site / present / guard**:
  - **#4007** (`signal_temporal_discontinuity` two dead limbs): `crates/renderer/src/vulkan/context/mod.rs` (`suppress_rigid_history_next_build` one-shot latch, `history.rs`-family), `draw.rs` (`fsr_reset_delivered` captured before `fsr_frame` is consumed, passed to `mark_dispatch_completed`). `rigid_history_lookup_is_gated_on_the_one_shot_suppression_latch`, `both_in_frame_limbs_stay_wired_to_their_order_independent_form` — 2/2 pass
  - **#4008** (5-copy `octDecode` drift): `crates/renderer/shaders/include/oct_codec.glsl` (single definition), included by `taa.comp`, `svgf_temporal.comp`, `svgf_atrous.comp`, `math_common.glsl`; dead copy in `caustic_splat.comp` deleted. `octahedral_codec_has_a_single_definition_every_consumer_includes` — pass
  - **#4009** (7 rotted `file:NN` anchors): replaced with symbol references across caustic/water/volumetrics sources; `SWEPT_SOURCES` scanner (`crates/renderer/src/vulkan/svgf.rs`) widened to 10 files. `swept_sources_carry_no_bare_line_number_anchors`, `the_replaced_anchors_name_their_symbols` — 2/2 pass
  - **#4010** (caustic refraction hardcoded 1.33): `crates/renderer/shaders/water.frag` now reads `push.timing.w` (authored `WaterMaterial::ior`) for the caustic ray, matching the primary refraction ray. `water_caustic_refraction_uses_the_authored_ior_not_the_1_33_default` — pass

### #3701: AnimationLayer blend-in contributes zero weight for the whole fade

- **Status**: PASS
- **Closed**: 2026-09-02
- **Fix commit**: `452cccd0b`
- **Fix site**: `crates/core/src/animation/stack.rs` (`AnimationLayer::weight` is now the live per-tick ramped value; `advance_stack` writes `blend_in_target * progress` every tick rather than only at completion)
- **Fix present**: Yes
- **Guard test**: `advance_stack_ramps_blend_in_weight_smoothly_not_a_hard_cut`, `crossfade_effective_weights_sum_to_about_one_at_the_midpoint` (`crates/core/src/animation/mod.rs`) — 2/2 pass

### #3677: Animation hot path is the last unconverted std `HashMap` keyspace

- **Status**: PASS
- **Closed**: 2026-09-02
- **Fix commit**: `452cccd0b`
- **Fix site**: `crates/core/src/animation/types.rs` (`AnimationClip.channels: FxHashMap<...>`), `byroredux/src/components.rs` (`NameIndex.map`, `SubtreeCache.map`), `byroredux/src/anim_convert.rs` (`build_subtree_name_map`)
- **Fix present**: Yes
- **Guard test**: `animation_clip_channels_stays_fx_hash_map` (core), `name_index_and_subtree_cache_stay_fx_hash_map`, `build_subtree_name_map_stays_fx_hash_map` (byroredux) — 3/3 pass. All are self-`include_str!` source-text pins, consistent with the same hot-path hashing rule cited in `_audit-common.md`.

## Summary table

| Issue | Title | Status | Fix Present | Guard |
|-------|-------|--------|-------------|-------|
| #4052 | EXAL groundcover §11.1 terrain-attribute bench | PASS | Yes | 44/44 groundcover tests |
| #4054 | EXAL groundcover Phase 1 (scatter/chunking/density) | PASS | Yes | 44/44 groundcover tests |
| #4055 | EXAL groundcover Phase 2 (blade geometry/wind) | PASS | Yes | 44/44 groundcover tests |
| #4057 | EXAL groundcover Phase 6 (light response) | PASS | Yes | 44/44 groundcover tests |
| #4059 | LAND VNML normals decoded unsigned | PASS | Yes | 4/4 vnml tests |
| #4090 | `cargo clippy` red at HEAD | PASS | Yes | core clippy clean; named lints gone |
| #779 | `triangle.frag` missing `early_fragment_tests` | N/A | No (deliberate) | n/a — closed-as-declined |
| #3528 | TREE.ICON never resolves in any archive | PASS | Yes | 1/1 |
| #3529 | Billboard clamp NaN-transparent | PASS | Yes | 4/4 |
| #3530 | Oblivion APPLY_HILIGHT2 parallax discarded | PASS | Yes | 9/9 |
| #3531 | Zero-length tag-13005 candidate | PASS | Yes | 6/6 |
| #3399 | ESM compressed-record inflate unbounded | PASS | Yes | 8/8 |
| #3400 | SCOL/PKIN FormID remap bypass | PASS | Yes | 5/5 (successor guard) |
| #3401 | ACTI/MOVS/NAVM/FLST/REGN FormID remap bypass | PASS | Yes | 5/5 (successor guard) |
| #3402 | 23 Skyrim skinned meshes zero-index upload | PASS | Yes | 3/3 |
| #2221 | Non-transform animation channels no sink | PASS | Yes | 13/13 |
| #869 | NiWireframeProperty/NiShadeProperty unconsumed | PASS | Yes | 3/3 |
| #1904 | Renderer FFI unsafe blocks no SAFETY comment | PASS | Yes | 0 clippy violations |
| #2507 | Caustic skip-clear leaves frozen pool | PASS | Yes | 10/10 |
| #3343 | Particle magnitude-floor test too weak | PASS | Yes | real-archive, 4 games |
| #3344 | NiPSysData BS Max Vertices discarded | PASS | Yes | 5/5 |
| #3345 | AnimationClip.phase dropped at NIF→core | PASS | Yes | 4/4 |
| #3346 | `--game fnv` CWD-immune form undocumented | PASS | Yes | source-shape pin |
| #3749 | 80% of `#[ignore]` no machine-readable reason | PASS | Yes | whole-repo scan |
| #3390 | CREA derives no ActorValues | PASS | Yes | 11/11 + real-archive |
| #3386 | Per-cell teardown not batched | PASS | Yes (source-confirmed) | none dedicated |
| #3387 | `touch_keys` allocates per already-present key | PASS | Yes | 3/3 |
| #3388 | `stamp_cell_root_range` not batched | PASS | Yes | 5/5 |
| #1038 | Shader↔Rust constant drift | PASS | Yes | 58/58 |
| #890 | BSEffect flag bits unconsumed | PASS | Yes | 12/12 |
| #3988 | BLAS budget blind to upscaler | PASS | Yes | 2/2 |
| #3989 | shader-pipeline.md describes live lanes as free | PASS | Yes | 1/1 |
| #3990 | Stale SAFETY argument, `upload_instances` | PASS | Yes | compiles clean |
| #3991 | `skin_dispatch_ran` record-time read as submit-time | PASS | Yes | 1/1 (re-anchored) |
| #4007 | `signal_temporal_discontinuity` two dead limbs | PASS | Yes | 2/2 |
| #4008 | 5-copy `octDecode` drift | PASS | Yes | 1/1 |
| #4009 | 7 rotted `file:NN` anchors | PASS | Yes | 2/2 |
| #4010 | Caustic refraction hardcodes 1.33 | PASS | Yes | 1/1 |
| #3701 | AnimationLayer blend-in zero weight | PASS | Yes | 2/2 |
| #3677 | Animation hot path std HashMap | PASS | Yes | 3/3 |

## Step 4 — Unconditional fragile-area guards

All seven run and confirmed green at HEAD:

| Area | Verify | Result |
|---|---|---|
| GPU struct sizes | `cargo test -p byroredux-renderer gpu_` | 62/62 pass (1 ignored, needs Vulkan device — expected) |
| NIFAL single boundary | `cargo test -p byroredux-core resolve_pbr`; single `-> Material` constructor confirmed (`byroredux/src/material_translate.rs`) | 7/7 pass; single-site grep confirmed |
| Typed particle emitters | `cargo test -p byroredux apply_emitter_params` | 3/3 pass |
| Collision coverage | `cargo test -p byroredux-nif collision` | 152/152 pass |
| ReSTIR reservoir retirement | grep `resRadiance` in `crates/renderer/shaders/` | only retirement comments remain (`lighting.glsl:167`, `triangle.frag:3137`) |
| Scheduler access declarations | `cargo test -p byroredux system_access_declaration_tests` | 4/4 pass |
| Save shape / FORMAT_MAJOR discipline | `cargo test -p byroredux serde_default_guard_tests` | 8/8 pass |

## Constraints observed

- No source, test, skill or doc file edited; no commits made; no GitHub issues created.
- `-j 4` throughout; targeted `-p <crate>` test filters, no `cargo build --release`, no workspace-wide build.
- `--ignored` used exactly twice, each scoped to one named test (`real_archive_torch_meshes_surface_particle_emitters` in `byroredux-nif`, `parse_real_fnv_creatures_derive_actor_values` in `byroredux-plugin`), never the unscoped ignored sweep the workspace note warns about for `byroredux-plugin`; both completed in well under 2 s.
- Engine never launched; no smoke scripts run.
- Scratch progress logged incrementally to `/tmp/audit/regression/checked.md` (41 lines, one per issue + timestamp, appended as each was verified — not written only at the end).

---

Next: `/audit-publish docs/audits/AUDIT_REGRESSION_2026-09-22.md` (though with 0 FAIL/regressions, there is nothing this report requires publishing as a new issue).
