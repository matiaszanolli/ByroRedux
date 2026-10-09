# Regression Verification Audit — 2026-10-08

**HEAD**: 00f580e09 · **Baseline**: `docs/audits/AUDIT_REGRESSION_2026-09-29.md` (HEAD `9fcfdc3fc`) · **Audited**: 40 previously-closed issues (37 closed since the baseline that none of today's 26 sibling reports cite and no earlier sweep verified, plus 3 older churn-ranked fixes: #4941, #3865, #4413), the Step 4 unconditional fragile-area guards, and the three findings of the 09-29 sweep · **Unchanged since baseline (skimmed)**: n/a. This sweep is itself the delta pass.

Run context: the `/audit-suite --preset comprehensive` run of 2026-10-08. Scratch files: `/tmp/audit/regression/{dim_1..dim_5,selection.txt,tests_a.txt,tests_b.txt,run_*.txt}`.

## Summary

| Result | Count |
|---|---|
| Closed issues checked | **40** |
| PASS (fix present, guard present and run green) | 39 |
| PARTIAL (fix present, but the guard has a blind spot) | 1 (#3865) |
| **FAIL (regression)** | **0** |
| Step 4 fragile-area guards | 7/7 green |
| 09-29 sweep findings (REG-01/02/03 → #5117/#5118/#5119) | all closed and fixed; #5119's guards run green |

**Findings**: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 1 LOW (NEW). This sweep found no `Regression of #N`.

- REG-2026-10-08-01: LOW. The #3865 consolidation missed a fourth-name copy of the `SubRecord` builder, and the guard cannot see it.

### Regressions and incomplete fixes found today by sibling reports (cross-referenced, not re-derived, not counted)

| Issue | Kind | Reported by | ID |
|---|---|---|---|
| #1171 | regression (`assert_send::<PartialNifImport>` dropped) | nif | NIF-D6-01 |
| #4283 | regression on the loose-`.mat` path | nifal | NIFAL-D8-02 |
| #2968 | regression (third SWF inflate) | ui | UI-D1-01 |
| #3817 | regression from its own fix (player stamped `CellRoot`) | save | SAVE-D5-01 |
| #3817 | incomplete | ecs | D7-01 |
| #4277 | incomplete | nifal | NIFAL-D8-01 |
| #5294 | incomplete | tooling | TOOL-D4-01 |
| #5095 | incomplete | skyrim | SKY-D3-01 |
| #5304 | incomplete | fo3 | FO3-D2-01 |
| #5239 | fix contradicts the FO4 CK | character | CHAR-D4-01 |
| #5066 | "no cycle today" premise now false | concurrency | CONC-D3-01 |
| #4756 | guard `debug_cli_component_counts_match_the_registry` is **red at HEAD** (`00f580e09` registered 3 components, so the doc count needs 67 → 70). This sweep reproduced it. | concurrency (also cited by tooling, ecs) | CONC-D3-2026-10-08-04 |

None of the 40 issues below overlaps with this list or with any issue number a sibling report cites.

## Selection method

```bash
D=2026-09-29; base=$(git rev-list -1 --before="$D 23:59" HEAD)     # 8f7acf3a7
git diff --name-only "$base"..HEAD -- '*.rs' '*.glsl' '*.comp' '*.frag' '*.vert'   # 534 files
# churn-ranked fix candidates (skill Step 1) -> /tmp/audit/regression/candidates.txt
gh issue list --state closed --search "closed:>=2026-09-29" --limit 500   # 465 closed (464 COMPLETED, 1 NOT_PLANNED)
# minus the 361 numbers today's 26 sibling reports cite, minus the 163 issues the
# 09-29 / 09-22 / 09-11 sweeps verified -> 289 uncited (18 high, 78 medium, 187 low, 6 unlabelled)
```

Of the churn top 40, 22 had already been verified by an earlier sweep. The rest are multi-issue renderer bulk commits (`a37fcba3c`, `b978bb5a1`) and three enhancements. The budget therefore went to:

- **Every HIGH closed since the baseline that no sibling cites (18)**: #4752 #4813 #4814 #4879 #4940 #5018 #5025 #5027 #5041 #5052 #5064 #5123 #5134 #5151 #5169 #5190 #5228 #5243. Also **#4941**, the HIGH at the head of the churn-top `a37fcba3c` bulk commit.
- **MEDIUM, spread across subsystems (17)**: #4815 #4816 #4819 (gameplay/save), #4883 #4884 #4886 (renderer unwind), #4656 #4657 (parsers), #4622 #4624 (NIF caps), #4782 (volumetrics NaN), #5038 (ECS dialogue), #5058 #5034 (save/load), #5045 (DIAL DATA), #5229 (TRNS radians), #5017 (Starts Unconscious).
- **Unlabelled bug closes (2)**: #5160, #5161.
- **Churn-top, never verified (2)**: #3865 (#2 by churn), #4413 (#1 by churn).

### Guard runs (rustc 1.96.0 toolchain, `TMPDIR=/mnt/data/tmp`)

| Command | Result |
|---|---|
| `cargo test -p byroredux --bin byroredux -- <45 filters>` | 62 passed, 0 failed, 2 ignored (data-gated) |
| `cargo test -p byroredux --bin byroredux -- <8 filters>` (batch 2: TRNS, debug gate, distant water, tex.dump) | 11 passed, 0 failed, 1 ignored (FNV data) |
| `BYROREDUX_REQUIRE_GAME_DATA=1 cargo test -p byroredux --bin byroredux -- --ignored --exact cell_loader::precombined::tests::exterior_precombine_instances_are_world_absolute` | 1 passed (541 instances, mean (−79990.7, 91610.3), which lies inside cell (−20,22)) |
| `cargo test -p byroredux-renderer --lib -- <26 filters>` | 26 passed |
| `cargo test -p byroredux-plugin --lib -- <12 filters>` | 12 passed |
| `cargo test -p byroredux-nif --lib -- <3 filters>` / `version_literal` | 4 passed / 3 passed |
| `cargo test -p byroredux-bsa --lib` (8 filters) · `-p byroredux-sfmaterial` (3) · `-p byroredux-physics` (10 filters) | 8 / 3 / 13 passed |
| `cargo test -p byroredux-scripting --lib -- <5 filters>` | 5 passed |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-scripting --lib --no-fail-fast` | 524 passed, 0 failed (the #5025 lane) |
| `cargo test -p byroredux-debug-server --lib -- rejects_absolute_and_parent_paths debug_cli_component_counts_match_the_registry` | 1 passed, **1 failed**: the #4756 doc-count guard ("must report 70"). Sibling finding, see above |
| `cargo test -p byroredux --bin byroredux -- groundcover_hasher` | 2 passed (#5119 / #4607 guard) |

## Summary table

| Issue | Title | Status | Fix Present | Guard |
|-------|-------|--------|-------------|-------|
| #4752 | Unauthenticated debug port: arbitrary local file read + write, on by default | PASS | Yes (`debug_server_allowed`, `main.rs:78`; screenshot confined, `system.rs:220-239`; `confine_to_roots` in `debug_load.rs`) | `release_debug_server_requires_explicit_opt_in`, `rejects_absolute_and_parent_paths`, `output_path_is_confined_to_the_dump_directory`, `cell_load_paths_are_confined_to_startup_roots`, `confinement_rejects_a_symlink_out_of_the_root` ✓ |
| #4813 | REFR/ACHR Initially Disabled (0x800) never decoded | PASS | Yes (`walkers.rs:1032`, `references/mod.rs:655-659`, `spawn.rs:493-504`) | 5 tests ✓ (plugin + bin + scripting) |
| #4814 | ACHR Starts Dead (0x200) never decoded | PASS | Yes | `starts_dead_is_decoded_only_for_tes5_family_achr`, `starts_dead_actor_is_a_queued_corpse_at_completion` ✓ |
| #4879 | Cancelled streaming apply orphans queued textures | PASS | Yes | 4 `texture_registry_tests` ✓ |
| #4940 | ReSTIR initial-candidate normalisation | PASS | Yes (`restirWSum *= restirM;` ordering) | `fresh_enumeration_is_weighted_by_candidate_count`, `fresh_estimator_expectation_is_the_sum_of_light_radiance` ✓ |
| #5018 | Translucency lobe self-shadowing | PASS | Yes (+ successors #5191/#5192/#5249) | `direct_shadow_rays_orient_their_origin_toward_the_light` ✓ |
| #5025 | Nested guards in `populate_candidates` / `running_quests_binding_entity` | PASS | Yes (`quest_alias.rs:972-1125`, `interaction.rs:1294-1330`; the #5293 cache drops each guard before the next) | Lock-order lane: scripting 524/0 (this run), bin 2705/0 (concurrency audit run) |
| #5027 | Dead player stays `Dead` after load | PASS | Yes | `dead_and_restraint_cleared_on_live_player_by_saved_absence` ✓ |
| #5041 | GetIsID compares placed ref, not base | PASS | Yes (`condition.rs:343-352`) | `get_is_id_matches_run_on_base_object_not_placed_reference` ✓ |
| #5052 | `ActorControlState` additive on process-lifetime player | PASS | Yes (same commit as #5027) | same test ✓ |
| #5064 | Caustic bindings 9/10 unwritten without geometry globals | PASS | Yes (+ #5188) | `caustic_dispatch_is_gated_on_geometry_bindings_written` ✓ |
| #5123 | Oblivion player body re-spawns `__max_default_light` | PASS | Superseded by #5189 (the artifact never spawns) | `spawn_nif_lights_never_spawns_known_exporter_artifact_lights`, `spawn_nif_lights_drops_known_exporter_artifact_by_name` ✓ (renamed from the #5123 names) |
| #5134 | Starfield WTHR fog distances in metres | PASS | Yes | `starfield_public_index_lifts_wthr_fog_distances`, `legacy_index_wthr_keeps_authored_units` ✓ |
| #5151 | Starfield WATR metric fields reach WATAL | PASS | Yes (+ `24cb577d2` real-data pin) | `starfield_public_index_lifts_watr_dnam_distances` ✓ |
| #5169 | FO76 WATR absorption / lane 3 | PASS | Yes | `fo76_index_watr_lifts_only_the_absorption_triplet`, `fo76_fourth_concentration_lane_is_not_promoted_to_oceanness` ✓ |
| #5190 | CDB `TextureReplacement` collapsed slot-agnostically | PASS | Yes | 7 `merge.rs` / `starfield_mat` tests ✓ |
| #5228 | FO4 exterior precombines at ≈2× world position | PASS | Yes (fix `f3e1bba62`, no `Fix #` keyword) | `exterior_apply_advances_precombine_at_zero_origin` ✓; data-gated `exterior_precombine_instances_are_world_absolute` ✓ (run) |
| #5243 | Distant LOD water is one default-height sheet | PASS | Yes (+ `d1ec1c57f`, #5244) | `distant_water_honors_the_xclw_tri_state` + 4 more ✓; FNV real-data half not run |
| #4941 | Specular-disabled BGSM hands metalness to keyword classifier | PASS | Yes | `bgsm_specular_disabled_overrides_a_keyword_metal_classification` ✓ |
| #4815 | Load discards saved AI-procedure state | PASS | Yes | `save_load_reseat_keeps_overlaid_procedure_state_across_clocks` ✓ |
| #4816 | Combat never disengages | PASS | Yes (+ #5046) | `ambient_combat_disengages_after_the_grace_period_out_of_contact`, `scripted_combat_never_disengages_on_lost_contact` ✓ |
| #4819 | `SpellList` not parked in `ReferenceState` | PASS | Yes | `spell_list_is_parked_and_restored_with_actor_values` ✓ |
| #4883 | `build_blas_batched` early exits abandon `prepared` | PASS | Yes | `pre_submit_exits_unwind_the_prepared_originals` ✓ |
| #4884 | BLAS scratch retired before replacement exists | PASS | Yes | 3 `blas_scratch_realloc_order_tests` ✓ |
| #4886 | `recreate_descriptor_sets` not transactional | PASS | Yes (+ #5207) | `recreate_descriptor_sets_allocates_the_replacement_before_the_old_pool_dies` ✓ |
| #4656 | BA2 DX10 header u32 multiply panics | PASS | Yes | `linear_size_saturates_instead_of_overflowing_at_u16_max_dimensions` ✓ |
| #4657 | sfmaterial readers recurse unbounded | PASS | Yes | 3 `reader.rs` depth tests ✓ |
| #4622 | BSTriShape derived stride has no upper bound | PASS | Yes (`bs_tri_shape.rs:538-545` block-end arm; fixed in mis-titled `b7491072f`) | `overstated_data_size_cannot_borrow_the_next_blocks_vertex_bytes` ✓ |
| #4624 | `parse_havok_packfile` pre-sizes from `num_sections` | PASS | Yes (`havok_packfile.rs:391`; same commit) | `section_count_is_bounded_before_allocation` ✓ |
| #4782 | Raw V-buffer history has no non-finite guard | PASS | Yes | `v_buffer_history_rejects_non_finite_and_the_store_clamps_to_fp16` ✓ |
| #5038 | Dialogue surface reads never-cleared per-NPC topic | PASS | Yes | `a_second_conversation_presents_the_second_npc` ✓ |
| #5058 | `QuestAliasInjectionState.factions` serde-skipped | PASS | Yes | `load_reset_strips_stale_alias_injected_player_factions` ✓ |
| #5034 | Worn meshes not reconciled after load | PASS | Yes (+ #5255, #5266) | `load_reconcile_diffs_roots_against_restored_slots` ✓ |
| #5045 | DIAL DATA byte 0 misread on Skyrim+ | PASS | Yes | `skyrim_plus_read_the_category_byte_not_the_flags_byte` + 2 parse tests ✓; real-data `fo4_dialogue_branches_and_categories` is plugin `--ignored`, so it was not run (OOM rule) |
| #5229 | FO4 TRNS rotation radians converted as degrees | PASS | Yes (`loading_screen.rs:510-521`) | `trns_transform_wins_over_inline_skyrim_defaults`, `stage_pose_converts_the_authored_triple_to_yup` ✓ |
| #5017 | FO4 Starts Unconscious never decoded | PASS | Yes | 7 tests ✓ |
| #5160 | Melee rays cannot hit NPC colliders | PASS | Yes | `corridor_cast_hits_a_bone_the_zero_width_ray_threads` ✓ |
| #5161 | Insane-but-finite keyframes reach Rapier | PASS | Yes (+ #5246 escalation ladder) | 12 tests ✓. `a_clean_substep_between_clamps_avoids_parking` was intentionally replaced by #5246's `a_second_burst_parks_even_after_a_clean_substep` ✓. The skeever probe is data-gated and was not run |
| #3865 | `SubRecord` fixture builder defined 32× | **PARTIAL** | Mostly | `no_module_redeclares_a_local_subrecord_builder` ✓, but it has a blind spot. See **REG-2026-10-08-01** |
| #4413 | EXAL ground cover Phase C, authored-model tier | PASS | Yes | 5 tests ✓ |

## Step 4: unconditional fragile-area guards (7/7 green)

| Area | Verify | Result |
|---|---|---|
| GPU struct sizes | `cargo test -p byroredux-renderer --lib -- gpu_` | 73 passed, 1 ignored (`gpu_filter_preserves_constant_radiance_and_broadens_a_lobe` needs a device) |
| NIFAL single boundary | `cargo test -p byroredux-core --features inspect --lib -- resolve_pbr`; one `fn translate_material` (`material_translate.rs:627`) | 7 passed; single definition |
| Typed particle emitters | `cargo test -p byroredux apply_emitter_params` | 3 passed |
| Collision coverage | `cargo test -p byroredux-nif --lib -- collision` | 155 passed |
| ReSTIR reservoir | grep `resRadiance` in `crates/renderer/shaders/` | only the two retirement comments (`lighting.glsl:173`, `triangle.frag:3160`) |
| Scheduler access | `system_access_declaration_tests` | 7 passed |
| Save shape | `serde_default_guard_tests` | 9 passed |

## Follow-up on the 09-29 sweep's findings

- **REG-2026-09-29-01 → #5117**: closed by `4ba1b069e`. The throughput half is now tracked as open **#5365**.
- **REG-2026-09-29-02 → #5118**: fixed in `e04fa1ef6`. Read statically, not executed. `scripts/check-playable-smoke-contracts.sh:24-61` now derives its neutralisation list from each fixture's `FIXTURE_DATA_ENV` (this covers `BYROREDUX_OBLIVION_DATA`). A self-check fails the script if any `BYROREDUX_*_DATA` the harness references is not neutralised.
- **REG-2026-09-29-03 → #5119**: fixed in `72769b95a` (+ `594b3b0f1`, #5258). `version_literal_tests` (3) and `groundcover_hasher_tests` (2) pass.

## Findings

### REG-2026-10-08-01: The #3865 consolidation missed a fourth-name `SubRecord` builder, and its guard cannot see it
- **Severity**: LOW
- **Dimension**: Regression guards / test hygiene
- **Location**: `crates/plugin/src/esm/records/misc/dialogue.rs:1856-1861` (`sub_of`); `crates/plugin/src/esm/records/test_support.rs:41` (private `zstring`), `:84-130` (the guard); `crates/plugin/src/esm/records/gras_tests.rs:11-15`; `crates/plugin/src/esm/records/soun.rs:126-130`
- **Status**: NEW. This is an incomplete fix of #3865 (closed 2026-09-12 by `cd5460e40`), not a regression: `sub_of` came in with `47d9e394d` (2026-08-30), before the fix.
- **Description**: #3865 folded the per-module `SubRecord` builders into `test_support::sub`. Its guard flags only definitions named `sub`, `mk_sub`, `make_sub`, `edid` or `modl`. `dialogue.rs`'s test module still defines `fn sub_of(code: &[u8; 4], data: &[u8]) -> SubRecord`, a verbatim copy of `sub` used at 4 sites, which is exactly what the issue set out to remove. Separately, `test_support::zstring` is private (`fn zstring`, not `pub(crate)`), so `gras_tests.rs` (`zstring`) and `soun.rs` (`zstring_sub`) each re-wrap the same null-terminated builder over `sub`.
- **Evidence**: `git grep -n "fn [a-z_0-9]*(.*-> SubRecord" -- crates/plugin` lists `sub_of` and the two `zstring` copies. The guard's needle list is `["sub", "mk_sub", "make_sub", "edid", "modl"]` (`test_support.rs:86`), so it passes.
- **Impact**: Cosmetic and maintenance only, which is the cost #3865 itself named. The guard reads as comprehensive but it is a name allowlist: any newly named generic copy passes it.
- **Related**: #3865.
- **Suggested Fix**: Replace `sub_of` with `test_support::sub`. Make `zstring` `pub(crate)` and drop the two local wrappers. Then make the guard match the body shape (`SubRecord { sub_type: *` inside a fn returning `SubRecord`) instead of a fixed name list.

## Notes for the orchestrator

- **Mis-titled fix commits (TD4 class, informational)**: #4622 and #4624 were fixed in `b7491072f`, titled "Refactor code structure and remove redundant changes". #4940 was fixed in `a37fcba3c`, which says "for issue 4940" with no keyword. #5228 and #5229 were fixed in `f3e1bba62` ("fix(cell_loader): …"), which cites no number. All five were closed by hand with a fix comment. `git log --grep="#N"` cannot find any of them.
- **Not run (rule-bound)**: plugin `--ignored` real-data tests (#5045 FO4 DIAL, #5151 depth pin), the FNV real-data distant-water test (it parses the whole master), and the skeever probe (needs `BYRO_SKEEVER_NIF`).

## Severity counts

| Severity | NEW | Already tracked / sibling-found |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 0 | 0 |
| LOW | 1 | 0 |

Suggested next step: `/audit-publish docs/audits/AUDIT_REGRESSION_2026-10-08.md`
