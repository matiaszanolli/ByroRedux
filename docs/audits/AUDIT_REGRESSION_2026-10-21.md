# Regression Verification Audit — 2026-10-21

**HEAD**: 4a3e89d56 · **Baseline**: `docs/audits/AUDIT_REGRESSION_2026-10-12.md` (HEAD `5d31fd637`) · **Audited**: 97 previously-closed issues (36 HIGH issues closed since the baseline, 17 older churn-ranked fixes no earlier sweep verified, 44 fixes closed since the baseline whose files churned most afterwards), the 4 findings of the previous sweep, the Step 4 fragile-area guards, the whole-workspace test and lock-order gates, local workspace clippy, and the CI state of `main` · **Unchanged since baseline (skimmed)**: n/a. This sweep is itself the delta pass: 784 commits, 423 distinct `Fix #N` issues, 824 churned source files since `5d31fd637`.

Run context: the `/audit-suite --preset comprehensive` run of 2026-10-21, solo and read-only. I launched no engine and ran no smoke script or `--ignored` corpus test. Every cargo command used the repo `target/` and the pinned toolchain (cargo 1.96.0, which matches CI). Scratch files are in `/tmp/audit/regression/`: `full_test.txt`, `lockorder_test.txt`, `clippy_warn.txt`, `ci_job.log`, `runs.txt`, `candidates.txt`, `later_churn.txt`, `group{A,C}_guards.txt`.

## Summary

| Result | Count |
|---|---|
| Closed issues checked by this sweep | **97** (+4 follow-ups from the previous sweep) |
| PASS (fix present, and a default-lane guard exists and ran green) | **94** |
| FAIL, new to this suite | **1**: #5849, the blocking clippy gate (REG-2026-10-21-01) |
| FAIL / PARTIAL already tracked by an open issue | **2**: #5115 → open #6193; #4659 → open #6182 |
| Step 4 fragile-area guards | **7/7 green** |
| `cargo test --workspace --no-fail-fast` (1.96.0) | **11,039 passed / 0 failed / 361 ignored** |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test --workspace --no-fail-fast --exclude byroredux-ui` (CI's own command) | **10,913 passed / 8 failed**. All 8 are the two cycles already reported as ECS-2026-10-21-01/-02. No other cycle |
| `cargo clippy --workspace --keep-going` | **10 lints**: 4 in `byroredux-plugin` (the 4 CI shows), 6 in the `byroredux` bin that CI never reaches |

**New findings from this sweep: 0 CRITICAL, 0 HIGH, 1 MEDIUM, 1 LOW.**

- REG-2026-10-21-01 (MEDIUM, Regression of #5849): the blocking `cargo clippy --workspace -- -D warnings` step has been red on all 17 `main` pushes since `3ab1b5557` (2026-10-20 14:44 UTC). Four plugin lints stop the step, so the `byroredux` bin, `byroredux-scripting`, `byroredux-game-detect`, `byro-detect` and `byro-launcher` are never linted. The bin carries 6 more lints that CI cannot see.
- REG-2026-10-21-02 (LOW, NEW): the release workflow added on 2026-10-18 has failed on every tag push (`v0.101` twice, `v0.102` twice). Each time it ran before the commit's CI had finished. Nobody re-ran it, so neither tag has a GitHub release. CI on both tagged commits did go green afterwards.

### CI state of `main`: other lanes, cited only

| Lane | State | Suite ID |
|---|---|---|
| ABBA lock-order detector | red on every one of the last 40 runs | ECS-2026-10-21-01 / -02. My local run reproduces exactly their two edges: `AnimationPlayer → AnimationClipRegistry` (1 test, `systems::cinematic::tests::scripted_one_shot_idle_marks_the_actor_until_it_finishes`) and `ParticleEmitter → TextureHandle` (7 tests in `render::particles::quantize_fade_tests` and `render::water_wave_params_tests`) |
| ECS cached-pointer aliasing (Miri) | red on the last 3 runs (since `21b9743e0`) | ECS-2026-10-21-03 |
| Test + Check + Clippy | red on 17 consecutive runs; only the `cargo clippy` step fails, and `cargo test` is green | **REG-2026-10-21-01 (this report)** |
| real-data-gates, rt-correctness | every run since 10-13 `cancelled` (self-hosted runners never pick up the jobs) | Existing #6182 |
| release (tags) | 4/4 failed | **REG-2026-10-21-02 (this report)** |
| Shader parity, text-source integrity, machete, smoke-gate contracts, dhat-heap, traceability | green | — |

## Findings

### REG-2026-10-21-01: The blocking clippy gate is red again, and it hides the bin crate's lints
- **Severity**: MEDIUM
- **Dimension**: CI gates / closed-fix regression
- **Location**: `crates/plugin/src/esm/records/weather.rs:808`, `crates/plugin/src/esm/reader.rs:621`, `crates/plugin/src/esm/cell/index.rs:714-716` (2 lints). The hidden ones are at `byroredux/src/asset_provider/object_graph.rs:260`, `byroredux/src/cell_loader/load.rs:676`, `byroredux/src/cell_loader/spawn.rs:403`, `byroredux/src/commands/terrain.rs:17`, `byroredux/src/scene/nif_loader.rs:519` and `byroredux/src/terrain_height.rs:47`. Gate: `.github/workflows/ci.yml` step `cargo clippy` (`cargo clippy --workspace --keep-going -- -D warnings`).
- **Status**: Regression of #5849. #5849 ("The blocking clippy gate is red again … leaves … the `byroredux` bin unlinted") was closed by `251d491ef` on 10-15. The gate went red again on 10-19, and `12677d718` re-zeroed it on 10-20 without an issue. It was green for three runs, then `3ab1b5557` turned it red again. The open #6193 covers only the non-blocking `--all-targets` step, and #5622 covers allow-reason comments. Neither covers this step.
- **Description**: In CI run 39645304183 (HEAD `4a3e89d56`), the plugin lib fails with 4 deny-level lints:
  - `nonminimal_bool` ×2 at `index.rs:714`/`:715`, from `765e387a0` (Fix #6084)
  - `redundant_closure` at `reader.rs:621` (`.filter(|slot| is_kind(slot))`), from `0a2955d33` (Fix #6118)
  - `manual_is_multiple_of` at `weather.rs:808`, from `67433ee3a` (Fix #6038)

  `--keep-going` keeps independent crates going, but nothing that depends on `byroredux-plugin` is linted. The CI log's `Checking` list is missing `byroredux`, `byroredux-scripting`, `byroredux-game-detect`, `byro-detect` and `byro-launcher`. Run locally on the same toolchain without `-D`, clippy reports 6 more lints in the bin, all from 10-20/10-21 commits:
  - `nonminimal_bool` at `object_graph.rs:260` (Fix #6126 `9a1f24b3e`)
  - `needless_borrow` at `load.rs:676` (`&absorbed` after Fix #6127 `67e724c3e` made it a `&mut`)
  - `too_many_arguments` 8/7 at `spawn.rs:403` (`c53d87679`, part of #6048, 10-21)
  - `unnecessary_lazy_evaluations` at `commands/terrain.rs:17` and `type_complexity` at `terrain_height.rs:47` (both `5017a64c2`)
  - `type_complexity` at `nif_loader.rs:519` (Fix #6124 `8879b67b4`)

  `byroredux-scripting` is clean.
- **Evidence**: The failing step is the only failure in the `Test + Check + Clippy` job of all 17 runs from `3ab1b5557` to `4a3e89d56`, and `cargo test` passes in each. The last green run is `79477b4a3`, and the first red one is `3ab1b5557`, whose range `79477b4a3..3ab1b5557` contains `67433ee3a`. Local `cargo clippy --workspace --keep-going --message-format=short` reports `byroredux-plugin (lib) generated 4 warnings` and `byroredux (bin "byroredux") generated 6 warnings` (`/tmp/audit/regression/clippy_warn.txt`).
- **Impact**: For 27 hours `main` has had no working lint gate on the crate that holds most of the gameplay code. Because the whole `Test + Check + Clippy` job is red, a real `cargo test` failure would show up as the same red job. This is the fifth blocking-gate regression this month (#5308, #5477, #5621, #5849, and now this), and each one went in through fix commits that nobody ran clippy on.
- **Related**: #5849, #5621, #5308, #6193 (open, `--all-targets`), `12677d718` (the 10-20 re-zero)
- **Suggested Fix**: Apply the 10 mechanical fixes. Clippy prints the replacement for each, and the `too_many_arguments` site needs either a parameter struct or a reasoned `#[allow]`, per #5622. Consider running `cargo clippy -p <touched crates> -- -D warnings` in the fix-issue flow before commit. The gate only fails after the push.

### REG-2026-10-21-02: The release workflow has failed on every tag pushed since it was added, so no release exists
- **Severity**: LOW
- **Dimension**: CI gates
- **Location**: `.github/workflows/release.yml` (the CI-success gate step, `gh run list --workflow ci.yml --commit "$GITHUB_SHA"`), `docs/releasing.md:28`
- **Status**: NEW. I searched open and closed issues for "release workflow" and found nothing. The pipeline was added by `13877d3ce` (2026-10-18).
- **Description**: The gate reads the commit's CI conclusion once and fails if it is not `success`. In all four tag-push runs, CI was still queued or running:

  | Run | Time (UTC) | Tag | Commit |
  |---|---|---|---|
  | 39140525596 | 10-18 15:26 | `v0.101` | `d4b095576` |
  | 39140689177 | 10-18 15:29 | `v0.101` | `d4b095576` |
  | 39355513091 | 10-20 01:49 | `v0.102` | `12677d718` |
  | 39418367607 | 10-20 11:54 | `v0.102` | `28e1f413e` |

  In each case the log reads "CI has not succeeded on … (none)". CI on `d4b095576` finished green at 15:44 and on `28e1f413e` at 12:10. The workflow was never re-run, and `gh release view v0.101` / `v0.102` both return "release not found".
- **Evidence**: `gh run list --workflow release.yml` and `gh run view <id> --log-failed`; the CI run timestamps come from `gh run list --workflow ci.yml --commit <sha>`.
- **Impact**: Both tagged alphas exist as tags with no published binaries. The fail-fast gate turns "tag right after push", the normal workflow, into a silent no-release unless someone remembers to re-run it.
- **Related**: TOOL-D4-2026-10-21-02 (release-doc rot, same pipeline, different defect)
- **Suggested Fix**: Make the gate wait for CI: poll `gh run watch`, or loop on `gh run list` until the run completes, with a timeout. Or trigger the release from a `workflow_run` of CI on the tag's commit. Then re-run release for `v0.102`.

## Sibling-reported regressions and residuals: cited, not re-reported

I did not re-derive these, apart from the lock-order and Miri lanes, which my runs and the CI data confirm. Each is already on `SUITE_FINDINGS.md`: #5974, #5994 (SCR-04), #6102 (SCR-05), #5997 (SCR-06), #6199 (EXT-D6-02), #6033 (SAFE-01), #6031 (CONC-03), #6028 (EXT-D1-05), #5936 (PHYS-D2-01), #5943 (PERF-D1-01), #5871 (GAME-D1-01), #5891/#5893 (PAR-D1-01/-02), #5966 (UI-D4-01), #6070 (PHYS-D3-03), #5961 (REN-H-01), `c231f5b5f` (EXT-D4-01), #6115 (NIF-D1-01), #5792 (NIF-D4-01), #5757 (SF-D4-02), #6048 (FO3-02), #6200 (SF-D2-01).

One extra site for an existing finding: PHYS-D3-2026-10-21-04 flags the stale "FO4 ragdolls do not activate yet" comment in `combat.rs`. The same sentence appears in `byroredux/src/asset_provider/actor_graph.rs:161-162` (#5861's corpse-graph skip) and in `combat.rs:1051-1056`. Fix them together.

## Per-issue results

A guard marked "ok" ran green in this sweep's `cargo test --workspace` (`full_test.txt`). "disk"/"master" marks a real-data `#[ignore]` companion. Each such issue also has a default-lane guard, listed with it.

### A. HIGH issues closed since the baseline (36)

| Issue | Title (short) | Status | Fix commit | Guard (default lane) |
|---|---|---|---|---|
| #5628 | StoryManagerRng re-entered by GetRandomPercent | PASS | faa00de2a | `get_random_percent_gate_dispatches_with_the_rng_installed` ok |
| #5630 | tag-`r` Set treated as ref assignment | PASS | aac6b25b3, superseded by 72fd96dfe | the original decline guard was replaced on purpose when remote variables shipped: `a_remote_variable_set_reaches_the_host_and_does_not_shadow_the_object` ok, plus `reference_indices_address_the_interleaved_scro_scrv_list` ok |
| #5631 | early Return in If lowers | PASS | ed6998c51 | `return_lowers_only_as_the_final_statement` ok |
| #5632 | branch-local rebinding lost | PASS | ed6998c51 | `rebinding_a_pre_branch_local_inside_a_branch_declines` ok |
| #5633 | SetMotionType literal table | PASS | 92a1782fe | `literal_motion_types_lower_only_where_every_game_agrees` ok |
| #5634 | GetOwningQuest on any receiver | PASS | 221befd61 | `get_owning_quest_lowers_only_on_self` ok |
| #5658 | MenuRenderer texture cache unbounded | PASS | 5dce486b6 | `decoded_textures_stop_at_the_renderer_budget`, `spellings_of_one_menu_texture_share_one_decode` ok |
| #5659 | HKX mask_size padding | PASS | eca0238e7 | `decode_spline_animation_accepts_a_padded_mask_table` (+ reject twin) ok |
| #5687 | WTHS DIFF unbounded recursion | PASS | 02187c7f9 | `hostile_self_referential_diffs_decline_at_the_depth_cap` ok |
| #5734 | quest-level dialogue conditions ignored | PASS | 6709eb65e (+8e4c6e40a lock release) | `a_catalogued_quest_gate_evaluates_running_state`, `an_unreadable_quest_gate_declines_the_hijacker` ok |
| #5755 | MSWP bare-BNAM vs `Materials\` compare | PASS | f81e6e722 | `mswp_source_matches_canonicalizes_and_honours_wildcards`, `build_overlay_xmsp_matches_a_materials_prefixed_xato_path`, `mswp_swap_fires_regardless_of_the_records_tree_folder` ok; corpus twin `fo4_mswp_canvas_swap_matches_the_picture_frame_nif` (disk) |
| #5765 | `reservoirsCurr` read through writeonly binding | PASS | a03cf9174 (refactor, no `Fix #` subject) | `restir_only_stores_visible_opaque_surface_reservoirs` ok: it pins the `atomicMin(reservoirClaim[pixelIdx], claimKey)` gate and a single store. Binding 16 is still `writeonly` (`bindings.glsl:768`) |
| #5791 | VM array/string allocation uncapped | PASS | 5c3c5758d | `vm_array_new/add_count/insert_is_capped`, `vm_string_concat_is_capped` ok |
| #5858 | infinite Bleedout timer → JSON null | PASS | 11ede64ec | `bleedout_with_and_without_a_recovery_time_survives_save_load_round_trip` ok |
| #5877 | boolean pass O(2^nesting) | PASS | 32352592f | `deeply_nested_ifs_are_walked_once_not_exponentially` ok |
| #5892 | looping trigger hangs the graph step | PASS | 08e4d2ec5 | `a_huge_playback_speed_cannot_hang_the_step` + 3 ok |
| #5932 | root-hosted collider paired with root | PASS | e19982b8f | `a_root_hosted_body_stays_on_the_flat_spawn` ok |
| #5969 | set-stage trigger bool defaults | PASS | f417c3f64 | `quest_stage_gate.rs` fixture `specific_actor_source_with_shipped_defaults` drives the `:1114` assertion; module ok |
| #5970 | inlining has no total-effect cap | PASS | 07f2a6561 | `multiplicative_inlining_declines_past_the_statement_budget` ok |
| #5981 | Array.Remove overflow panic | PASS | d6aeab474 | `vm_array_remove_huge_count_clamps` ok |
| #5982 | float `%` i32::MIN % -1 panic | PASS | 2144383b5 | `vm_float_modulo_overflow_wraps` ok |
| #5983 | cyclic struct recursion | PASS | 9018a5523 | `vm_cyclic_struct_is_not_stored` ok |
| #5984 | deferred runs bypass continuation cap | PASS | 73fddb4b8 | `zero_delay_deferrals_are_capped_at_the_push` ok |
| #5989 | GetActorValue enum code read as FormID | PASS | d1dd5e030 | `authored_get_actor_value_index_translates_to_the_loaded_avif` ok (+ disk twin) |
| #5990 | CTDA catalog game-agnostic on colliding indices | PASS | 9f418b4fa | `colliding_indices_are_unmodeled_on_the_games_that_reuse_them` ok |
| #6011 | printf precision unbounded | PASS | 266436c91 | `printf_precision_is_clamped` ok |
| #6017 | provider OnInit re-runs on respawn | PASS | 0b1f1545f | `provider_on_init_runs_once_per_reference_across_respawns` ok |
| #6050 | shape guard red on main | PASS | 5f8bbf454 | `serde_default_guard_tests` (13) ok |
| #6062 | head_look / magic_shader invert Children order | PASS | e4d86a29f | Lock-order lane: my run shows no cycle over `Children`/`Name`/`Material`. Both sites collect the subtree first (`head_look.rs:41`, `magic_shader.rs:29`) |
| #6075 | model-less templated CREA shells | PASS | 011ad0c07 | `a_model_less_spawner_shell_takes_the_leveled_picks_model` ok (+ master twin) |
| #6076 | persistent refs beyond the entry window | PASS | ca12e5638 | `a_tile_beyond_the_entry_window_owns_the_persistent_refs_standing_in_it`, `a_persistent_actor_is_promoted_from_its_stub_and_demoted_back` ok |
| #6100 | Oblivion idle path nonexistent | PASS | abd3d5001 | `oblivion_and_fallout_idle_paths_are_not_shared` ok (+ disk twin). No other Oblivion arm in `npc_spawn.rs` points at `locomotion\` |
| #6118 | Starfield 0xFE/0xFD FormIDs | PASS | 0a2955d33 | 5 `starfield_*` remap tests ok (+ disk twin). Its own `reader.rs:621` closure is one of REG-01's lints |
| #6120 | Starfield 16-byte ACBS | PASS | 78facba89 | `starfield_16byte_acbs_parses_gender_level_and_template_flags` ok. FO76's 20-byte ACBS is ESM-2026-10-21-02, a different layout |
| #6132 | pickup-tombstone sweep leaves dangling Children | PASS | dc829fc83 | it extended `save_reload_window_tombstone_is_applied_by_restore_resident` (`reference_state.rs:2469-2497`) ok |
| #6202 | terrain height-blend discontinuity | PASS | 5f1a88929 | `a_vanishing_layer_fades_out_continuously` + 6 ok, incl. `glsl_blend_fades_each_lane_with_its_share` |

### B. Older churn-ranked fixes no earlier sweep verified (17)

| Issue | Title (short) | Status | Fix commit | Guard |
|---|---|---|---|---|
| #4952 | Light SSBO doc row | PASS | a37fcba3c | `light_ssbo_docs_state_the_real_header_and_entry_size` ok |
| #4953 | GpuCamera NoUninit SAFETY | PASS | a37fcba3c | `gpu_camera_is_368_bytes`, `camera_ubo_size_matches_gpu_camera_in_every_shader` ok |
| #4974 | CLI upscaler persisted | PASS | b978bb5a1, refined by #5030 4144f2019 | `launch_override_stays_off_disk_until_an_in_session_choice_lands`, `pinned_launch_override_is_not_persisted_by_an_unrelated_save`, `non_chosen_record_leaves_settings_toml_untouched` ok. `rolled_back_switch_persists_the_running_mode_not_the_request` was removed on purpose by #5030 |
| #4975 | settings registry vs live upscaler | PASS | b978bb5a1 | `setting_change_loop_stages_the_upscaler_without_writing_the_registry` ok |
| #3710 | two PlayerEntity resources | PASS | ce7cc5dfb | rename holds (`PapyrusPlayerEntity`, `papyrus_demo/mod.rs:71`; `PlayerEntity`, `systems/character.rs:44`); registry rows pinned by `registry_completeness_tests` ok |
| #2508 | composite binding-8 caustic fallback | PASS | 33c39c871 | `water_caustic_inactive_maps_to_zero_caustic_flag`, `caustic_radiance_combines_glass_rgb_and_water_in_float_fixed_point` ok |
| #2513 | GPU `*_active` flags unconsumed | PASS | 33c39c871 | all 23 `gpu_pass_ms.insert` arms in `systems/metrics.rs` go through `gpu_bracket_ms`, incl. the shadow-filter bracket `fc83608d3` added at HEAD~2; `inactive_bracket_gates_to_none_not_zero` ok |
| #2514 | Disney lobes hard-zeroed | PASS | 33c39c871, d38d3a122 | `disney_lobe_arms_write_their_material_fields`, `pbr_bsdf_lobes_material_drives_all_four_disney_scalars` ok. Only particles still author zeros, which is correct |
| #5484 | XCRI/XPRI inverted at precombine gate | PASS | 777351ec1 | `parse_cell_xcri_pairs_remap_and_keep_their_bake_hash`, `effective_absorbed_refs_un_skips_only_the_missing_bakes_refs` ok |
| #5115 | `--all-targets` clippy debt | FAIL, **Existing #6193** (open, regression of #5850) | 3f0852e08 | non-blocking step; tracked |
| #4659 | no CI lane runs parser real-data suites | PARTIAL, **Existing #6182** | ec63d2636 | the `parsers` job exists in `real-data-gates.yml` but has never run (self-hosted, cancelled) |
| #4660 | strict-lane holes in real-data harnesses | PASS | ec63d2636 / 3970c8d19 | harness tests in `crates/bsa/tests/*`, `crates/bgsm/tests/parse_all.rs`; default-lane unit twins ok |
| #4919 | model candidate grid restarts per chunk | PASS | 4dfe97f3e | `model_candidates_tile_one_world_lattice_across_chunk_borders` ok |
| #4920 | model-tier overflow drops records | PASS | 4dfe97f3e, 226f2dd4c, da9405d09 | `over_budget_layout_grants_every_record_whole_plants` ok |
| #4921 | model tier rigid, no wind | PASS | 4dfe97f3e | `speedtree_gust_crest_travels_downwind` ok |
| #4922 | model-tier host re-derives per frame | PASS | 4dfe97f3e, 7814feea8 | `model_records_rederive_only_when_the_cover_changes` ok |
| #4923 | model-tier guard gaps | PASS | 4dfe97f3e | `water_rule_codes_match_the_translated_rule` ok |

### C. Closed since the baseline, ranked by churn on their files after the fix (44)

Ranked by `git rev-list --count <fix>..HEAD -- <fix files>` (`later_churn.txt`). Every guard below ran green.

| Issue | Status | Fix | Guard / evidence |
|---|---|---|---|
| #5763 VM per-frame budget | PASS | 195e3b4ef | its added tests ok (133 later commits on its files) |
| #5638 non-local assignment lowers | PASS | ed6998c51 | `assignment_to_a_script_variable_declines` ok |
| #5861 FO4 corpse re-animated by graph | PASS | 001de8741 | `combat.rs:1057-1059` removes `ObjectAnimationGraph`/`GraphLocomotion`/`ConsoleWalk`; `actor_graph.rs:163` skips `Dead`; tests ok |
| #5794 Finalize tail duplication | PASS | 2ba5c8c12 | refactor; spawn tests ok |
| #5636 GetQuestRunning for stopped quests | PASS | aac92c6ca | tests ok |
| #5637 CTDA remap fn 56 | PASS | 722c06a76 | tests ok |
| #5662 NaN prop-bone channel | PASS | 9707cb367 | tests ok |
| #5691 #5662 policy bypass | PASS | c3b578ae5 | tests ok |
| #5675 clutter-impact sound position | PASS | 0d8ea5d6e | tests ok |
| #5916 Skyrim level roll banks no perk | PASS | 612bd179e | tests ok |
| #6078 creature clips per game | PASS | 578d8432a | `creature_clip_candidates_are_per_game` ok (+ master twin) |
| #5862 script Health damage has no death | PASS | bae823643 | tests ok. GAME-D4-2026-10-21-03 is the separate magic-effect path |
| #5859 Bleedout not in eviction row | PASS | 33cc39bfc | tests ok |
| #6080 runtime-FaceGen gender from shell | PASS | 415c35cff | tests ok |
| #6077 XESP enable-parent initial state | PASS | 22ff0e12e | `an_enable_parent_child_follows_its_parents_resolved_state` ok (+ master twin) |
| #5954 MeshHandle guard across collect_subtree | PASS | 7a4f6c1a2 | `runtime.rs:1449` collects first; lock-order run shows no `MeshHandle` cycle |
| #5918 grant_experience truncation spin | PASS | 118edb1e6 | tests ok |
| #5897 lossless-compressed FO4 clips | PASS | 2649febc7 | tests ok (PAR-D5-01 / PAR-D1-03 are residual-quality findings, cited) |
| #6128 restored body part skips hooks | PASS | 1a8c05139 | tests ok |
| #6071–#6074 eviction-row gaps | PASS | 611769b16 | tests ok |
| #5920 player Perks from Player NPC_ | PASS | b14a1a7ec | `inventory.rs:3733/3773` test assertions ok |
| #5917 level-up reads base INT | PASS | 9c087f15e | `commands/gameplay.rs:807` test ok |
| #5683 has_user_flag rescans | PASS | d8b1d333c | tests ok |
| #6086 merge_cell_override resurrects fields | PASS | 2fa227d72 | `cell/tests/merge.rs:649,669` assertions ok. The same commit's sibling #6084 (`765e387a0`) added two of REG-01's lints |
| #6054 radius-12 door exit saturates vertices | PASS | a5958adf3 | tests ok |
| #6036 LTEX→TXST per plugin | PASS | 65add6a2e | tests ok |
| #5991 OnHit ordering | PASS | e4810c125 | `fragment/vm/tests.rs:2360` ok |
| #5973 negative Damage/RestoreValue | PASS | 3f32efc04 | tests ok |
| #5894 FO4 ref-frame decode drops root motion | PASS | 74733c655 | tests ok |
| #5860 Hit producers overwrite | PASS | ea28a35b8 | tests ok |
| #6084 REFR moved by override keeps base copy | PASS | 765e387a0 | tests ok (the commit also adds REG-01's `index.rs` lints) |
| #6037 WTHR PNAM layer stride | PASS | 7f4c25e3e | tests ok |
| #5898 object-graph failure cached | PASS | 2295db1e7 | tests ok |
| #6123 door transition re-parses ESM | PASS | 588c87bc4 | tests ok |
| #5651, #5652, #5653 Papyrus control/objective/lock | PASS | 5e2fc3c8e, 9777c41a2, 2ce808553 | tests ok |
| #5681 continuation queue uncapped | PASS | ef130cd6c | tests ok |
| #5649 npc_dialogue_selection Access row | PASS | 197a9e245 | `boot/schedule/mod.rs:1212` guard in `system_access_declaration_tests` ok |
| #5650 half-vacuous stub-spawner pins | PASS | a7e05dd7b | scans `production_text` (`exterior.rs:1370`, `script.rs:1681`) |
| #5813 PapyrusPlayerEntity reads | PASS | 106f636be | refactor; tests ok |
| #5686 RigidBodyData live-load overlay | PASS | 4e0c962c4 | tests ok. SAVE-2026-10-21-01 is a different residual (`PhysicsSourceForm` matching), cited |
| #5872 ProcessTrapHit from dialogue | PASS | 275fc103f | tests ok |
| #5866 script ownership → theft rule | PASS | ac0abfbae | `inventory.rs:2908` test ok |

### D. The previous sweep's findings

| ID | Issue | State | Verified |
|---|---|---|---|
| REG-2026-10-12-01 | #5759 | closed | `index.rs:300-302` now names `equip::resolve_racial_spells` and #4415 |
| REG-2026-10-12-02 | #5760 | closed | `merge.rs:540,676` say `.mat` decodes through `loose_mat` |
| REG-2026-10-12-03 | #5761 | closed | `08b09df56` aligned the BLAS-budget gate (hardware gate; not run here) |
| REG-2026-10-12-04 | #5762 | closed as already fixed | `set_permanent` now has a production caller (`crates/scripting/src/condition/eval.rs:484`) |

## Step 4: unconditional fragile-area guards

| Area | Result |
|---|---|
| GPU struct sizes | `gpu_instance_layout_tests` 18/18 ok, incl. `gpu_camera_is_368_bytes` |
| NIFAL single boundary | `resolve_pbr` 8/8 ok |
| Typed particle emitters | `apply_emitter_params` 3/3 ok |
| Collision coverage | 204 `collision` tests ok |
| ReSTIR `resRadiance` retired | only the retirement comments remain (`lighting.glsl:173`, `triangle.frag:3264`) |
| Scheduler access | `system_access_declaration_tests` 9/9 ok |
| Save shape | `serde_default_guard_tests` 13/13 ok |

## What was looked for and not found

- **Lock-order cycles beyond the two reported ones.** The full detector run fails 8 tests, and every failure is one of ECS-01's or ECS-02's two edges. In particular no `Children`/`Name`/`Material` edge (#6062) and no `MeshHandle` edge (#5954) came back.
- **Functional test regressions.** The whole workspace on 1.96.0 is green, 11,039/0.
- **Deleted guards.** Two guards added by a fix are gone: `#5630`'s decline test and `#4974`'s rollback-persist test. Both were removed on purpose by a later, more complete fix (`72fd96dfe` remote variables, `4144f2019` #5030), and that fix's replacement guard is present and green.
- **Vacuous guards on `#5765`.** The claim guard asserts the `atomicMin` gate and the single store, not just a needle that its own literals could satisfy. It reads the shader file, which has no test text in it.

## Not verifiable here

- Hardware or real-data gates (`#[ignore]`d, disk-only): the corpus twins listed above with "disk"/"master". I did not run them, per the suite rules.
- The release fix (REG-02) needs a tag push to confirm.

Suggested next step: `/audit-publish docs/audits/AUDIT_REGRESSION_2026-10-21.md`
