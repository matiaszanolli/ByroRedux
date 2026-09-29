# Regression Verification Audit — 2026-09-29

**HEAD**: `9fcfdc3fc` · **Baseline**: `docs/audits/AUDIT_REGRESSION_2026-09-22.md` (HEAD `ee6d3fb39`) · **Audited**: 40 previously-closed issues (33 closed since the baseline, which none of today's 26 sibling reports cite, plus 7 churn-ranked older fixes not verified in the last three sweeps), the Step 4 unconditional fragile-area guards, and an open-but-fixed reconciliation against TD4-2026-09-29-01 · **Unchanged since baseline (skimmed)**: n/a. This sweep is itself the delta pass.

Run context: the `/audit-suite --preset comprehensive` run of 2026-09-29. Scratch files: `/tmp/audit/regression/{batch_0_selection,batch_1,batch_2,batch_3,step4,close_candidates}.md`.

## Summary

| Result | Count |
|---|---|
| Closed issues checked | **40** |
| PASS (fix present, guard present and run green) | 36 |
| PARTIAL (fix present, no guard, or a doc-only close) | 3 (#4606, #4607, #1042) |
| N/A (closed on a measurement, no code by design) | 1 (#4795) |
| **FAIL (regression)** | **0** |
| Step 4 fragile-area guards | 7/7 green |
| Open-but-fixed close candidates | **31** (of the 32 re-queried; #4864 stays open) |

**Findings**: 0 CRITICAL, 0 HIGH, 2 MEDIUM, 1 LOW. None of them is a `Regression of #N`.

- REG-2026-09-29-01: MEDIUM. #4606 was closed on its doc half. The GPU-idle throughput defect is still in the code and no open issue tracks it.
- REG-2026-09-29-02: MEDIUM. `check-playable-smoke-contracts.sh` does not neutralise `BYROREDUX_OBLIVION_DATA`. On a machine with Oblivion installed it runs the real P0 gate (release build plus engine launch) and then reports FAIL.
- REG-2026-09-29-03: LOW. The #4607 and #1042 fixes are present but have no guard.

### Regressions found today by sibling reports (cross-referenced, not re-derived)

These come from today's sibling reports. This sweep did not re-check them, and they are not counted in the 40.

| Issue | Reported by | ID |
|---|---|---|
| #3475 | performance | PERF-D1-01 |
| #4595 | safety | SAFE-D4-02 |
| #4655, #4664, #4673 | parsers | — |
| #4687 | physics | PHYS-D2-01 |
| #3504 | tech-debt | TD9-01 |
| #4767, #4568, #2256, #4342 | tech-debt | — |

The 40 issues below were chosen so they do not overlap with this list or with anything a sibling report cites.

## Selection method

```bash
D=2026-09-22; base=$(git rev-list -1 --before="$D 23:59" HEAD)     # cb44d99f6
git diff --name-only "$base"..HEAD -- '*.rs' '*.glsl' '*.comp' '*.frag' '*.vert'   # 477 files, 209 commits
# churn-ranked fix candidates (skill Step 1), then minus the 112 issues verified by the
# 09-22 / 09-11 / 08-30 sweeps
gh issue list --state closed --search "closed:>=2026-09-22" --limit 400   # 361 closed, all COMPLETED
# minus every #N cited by today's 26 sibling reports (537 numbers) -> 118 uncited
```

The churn top-40 was mostly issues the 09-22 sweep had already verified: #4055, #4054, #779, #4090, … all appear in its table. The budget therefore went to the following, sorted by severity label:

- **Crit/high closed since the baseline, uncited (8)**: #4779 #4576 #4773 #4827 #4828 #4829 #4830 #4983.
- **Medium closed since the baseline, uncited (25)**: #4248 #4429 #4571 #4577 #4578 #4601 #4602 #4606 #4607 #4703 #4704 #4728 #4730 #4766 #4792 #4793 #4794 #4795 #4831 #4832 #4833 #4834 #4835 #4838 #4839.
- **Churn-top, not verified in the last three sweeps (7)**: #3364 #3365 #3366 #3367 #1042 #3231 #2570.

Guard runs:

| Command | Result |
|---|---|
| `cargo test -p byroredux-renderer --lib -- <36 filters>` (two runs) | 44 + 23 passed, 0 failed |
| `cargo test -p byroredux --bin byroredux -- <18 filters>` (`TMPDIR=/mnt/data/tmp`) | 35 passed, 0 failed |
| `cargo test -p byroredux-nif --lib` (4 filters) | 5 passed |
| `cargo test -p byroredux-bsa --lib` | 3 passed |
| `cargo test -p byroredux-physics --lib` | 1 passed |

## Summary table

| Issue | Title | Status | Fix Present | Guard |
|-------|-------|--------|-------------|-------|
| #4779 | Volumetrics inject traces a stale TLAS after a failed build | PASS | Yes | `compute_ray_query_passes_take_the_build_gated_tlas` ✓ |
| #4576 | Removing blend→EFFECT divert makes legacy FX cards occlude | PASS | Yes | `tlas_tests` #4576 fixtures ✓ |
| #4773 | `fog_coverage` / `fog_scale_height_meters` transposed at `record_post_passes` | PASS | Yes | `record_post_passes_named_arguments_match_parameter_positions` ✓ |
| #4827 | `drawIndirectFirstInstance` never enabled | PASS | Yes | 2 caps tests ✓ |
| #4828 | LightShaft/SkyAperture evaluated as NUCLEAR | PASS | Yes | 2 shader-constant tests ✓ |
| #4829 | `skin_palette.comp` boneWorld range frozen | PASS | Yes | 2 skin_compute tests ✓ |
| #4830 | `dds.rs` `scale_channel` unchecked u32 arithmetic | PASS | Yes | 3 dds tests ✓ |
| #4983 | `pickup_loot`/`restore` hold `PickedUp` write guard across mesh walk | PASS | Yes | `picked_up_restore_walks_meshes_before_taking_the_marker`, `unload_captures_do_not_invert_production_lock_orders` ✓ |
| #4248 | Container LVLI entries never expanded | PASS | Yes | `container_loot_resolves_lists_using_live_player_level` ✓ |
| #4429 | No canonical role for Starfield rough/metal single-channel kinds | PASS | Yes | `starfield_single_channel_kinds_are_parked_by_name_or_canonical` ✓ |
| #4571 | `PickedUp` lands on root, render skips test meshes | PASS | Yes | `pickup_stamps_the_subtree_meshes_not_just_the_root` ✓ |
| #4577 | `composite.frag.spv` stale on debug constants | PASS | Yes | `composite_frag_spv_debug_mode_guard_matches_render_debug_mode_max` ✓ |
| #4578 | AgX clamps linear input before log2 | PASS | Yes | `agx_highlights_keep_increasing_and_match_the_reference_ramp` ✓ |
| #4601 | All-slots fence wait unpinned | PASS | Yes | `the_all_slots_wait_argument_is_pinned` ✓ |
| #4602 | Readbacks lack a device→host flush edge | PASS | Yes | `the_host_flush_edge_precedes_end_command_buffer_and_follows_every_writer` ✓ (de-vacuized by #4842) |
| #4606 | Top-of-frame all-fence wait idles the GPU | **PARTIAL** | Doc half only | n/a. See **REG-01** |
| #4607 | Ground-cover host collection uses SipHash + per-frame allocations | **PARTIAL** | Yes | none. See **REG-03** |
| #4703 | StartCombat does not suspend ambient package | PASS | Yes | 2 ai_package tests ✓ |
| #4704 | Alias packages never reach package-less actors | PASS | Yes | `alias_package_runs_on_an_actor_with_no_base_packages` ✓ |
| #4728 | Flat water moves against scroll | PASS | Yes | renderer + physics lockstep tests ✓ |
| #4730 | CI gate ran `m-exteriors.sh.sh` | PASS | Yes | contract script literal-`run_gate` scan ✓ (only with Oblivion data masked, see **REG-02**) |
| #4766 | `adaptation_alpha_is_a_saturating_ramp` dropped from registry | PASS | Yes | test ✓ + `no_test_attribute_is_stacked_on_another` ✓ |
| #4792 | Fog cluster widened 40 B → 784 B | PASS | Yes | 3 volumetrics packing tests ✓ |
| #4793 | Sealed-interior sun rim rays every frame | PASS | Yes (live part) | `sun_visibility_rays_are_gated_on_a_live_sun_term` ✓ |
| #4794 | Seam blending re-inflates per NPC | PASS | Yes | `repeated_hand_hooks_reuse_the_import_and_keep_actor_deformation_private` ✓ |
| #4795 | #3540 fit projection removed → rebuild churn | N/A | Closed on A/B measurement, no policy change | none by design |
| #4831 | `depthLinearize` fed fog near/far | PASS | Yes | `depth_decode_sites_read_no_fog_lanes` ✓ |
| #4832 | Window-portal grazing gate sign-inverted | PASS | Yes | 2 shader-contract tests ✓ |
| #4833 | TLAS `instance_custom_index` capped at `MAX_INSTANCES` | PASS | Yes | 3 tests ✓ |
| #4834 | kind-102 shadow mask ignores blend state | PASS | Yes | `non_blended_no_lighting_geometry_keeps_its_opaque_shadow_bucket` ✓ |
| #4835 | 16/24-bpp DDS expand makes length check vacuous | PASS | Yes | 6 tests ✓ |
| #4838 | `MorphSlot::destroy` never releases shared delta | PASS | Yes | `release_shared_destroys_on_last_strong_ref_despite_a_weak` ✓ |
| #4839 | Interior portal sky feeds zero sun illuminance | PASS | Yes | 3 tests ✓ |
| #3364 | `canonical_shader_type` FO76-only | PASS | Yes (+ #3900 successor) | 2 slot_role tests ✓; face-tint pin superseded by `starfield_face_tint_is_translated_and_never_binds_slot_three_as_height` ✓ |
| #3365 | `multi_pick` narrowing has no Skyrim real-data pin | PASS | Yes | `skyrim_leveled_item_multi_pick_semantics_are_pinned_on_the_shipped_master`: present, `#[ignore]` data-gated, **not run** (plugin `--ignored` OOM rule) |
| #3366 | `plugin_for_form_id` indexes by position not slot | PASS | Yes | 3 `plugin_for_form_id_*` tests ✓ |
| #3367 | BSA bit-31 embed-name semantics unsourced | PASS | Yes | 3 bsa tests ✓ |
| #1042 | NIF version / BSVER bare literals | **PARTIAL** | Yes | none. See **REG-03** |
| #3231 | No morph-target vertex deformation path | PASS | Yes | `morph_slot_creation_is_reachable_end_to_end` ✓; `morph_slot_shares_deltas_but_keeps_entity_weights` present |
| #2570 | `is_pbr` has no negative test for Oblivion | PASS | Yes | `legacy_is_pbr_tests` (2) ✓ |

## Per-issue notes (non-trivial entries only)

The Summary table is the per-issue record. Fix commits and fix sites for every entry are in `/tmp/audit/regression/batch_{1,2,3}.md`. Entries that needed judgement:

## #4779: After a TLAS build fails, the volumetrics inject keeps tracing the stale TLAS
- **Status**: PASS
- **Closed**: 2026-09-23
- **Fix commit**: `da0f5c7f9`
- **Fix site**: `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (`ray_query_tlas`, `.filter(|_| self.tlas_build_succeeded_last_frame)`)
- **Fix present**: Yes. The volumetrics pass (`post_passes.rs:573`) and the ground-cover scatter both take their handle through the helper.
- **Guard test**: `compute_ray_query_passes_take_the_build_gated_tlas` — passes
- **Notes**: `record_caustic_splat_pass` still reads `accel.tlas_handle(frame)` directly (`post_passes.rs:459`). The fix's doc names the reason: `caustic_splat.comp` early-outs on `sceneFlags.x`, which `build_and_upload_instances.rs:1215` drives from the same success flag. That is correct today. It is the one raw consumer left, so a future caustic change that drops the shader early-out would reopen #4779.

## #4606: The top-of-frame wait on every frame-in-flight fence idles the GPU
- **Status**: PARTIAL (doc-only close)
- **Closed**: 2026-09-22
- **Fix commit**: `492df1c7c`. The close comment says the comment "landed inside" it.
- **Fix site**: `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs` (comment at the `wait_for_fences` call)
- **Fix present**: The backwards "cost stays zero" comment is corrected. The throughput defect is unchanged.
- **Guard test**: `the_all_slots_wait_argument_is_pinned` pins the rider list, which is the precondition for narrowing. There is no throughput guard.
- **Notes**: See REG-01.

## #4795: #3540's fit projection was removed; over-budget scenes rebuild-churn
- **Status**: N/A (closed by measurement)
- **Closed**: 2026-09-28
- **Fix commit**: `69266fb82` (docs only; `PERFORMANCE_FIX_STATUS_2026-09-26.md` § #4795)
- **Fix present**: n/a. The A/B found no per-frame churn, so the policy did not change.
- **Guard test**: none. Nothing pins "no per-frame BLAS restore under a tight budget".
- **Notes**: The run was cut short by an unrelated Rapier MultiSAP panic after the first cell crossing. That is thin evidence for the close, but it was a deliberate decision, so it is not filed.

## #3365: `multi_pick` narrowing has no Skyrim real-data pin
- **Status**: PASS (guard present, not run)
- **Closed**: 2026-08-28
- **Fix commit**: `d9d2d16a6`
- **Fix site**: `crates/plugin/tests/parse_real_esm.rs:3657`
- **Guard test**: `skyrim_leveled_item_multi_pick_semantics_are_pinned_on_the_shipped_master` is `#[ignore = "needs Skyrim SE game data on disk"]`. It was not run because `cargo test -p byroredux-plugin -- --ignored` is banned on this machine (OOM).

## Step 4 — Unconditional fragile-area guards

| Area | Verify | Result |
|---|---|---|
| GPU struct sizes | `cargo test -p byroredux-renderer gpu_` | 72 passed, 1 ignored ✓ |
| NIFAL single boundary | `cargo test -p byroredux-core --features inspect resolve_pbr` | 7 passed ✓ |
| Typed particle emitters | `apply_emitter_params` (byroredux) | 3 passed ✓ |
| Collision coverage | `cargo test -p byroredux-nif collision` | 155 passed ✓ |
| ReSTIR reservoir | `git grep resRadiance crates/renderer/shaders/` | Only the two retirement comments (`include/lighting.glsl:167`, `triangle.frag:3163`). Still retired ✓ |
| Scheduler access | `system_access_declaration_tests` | 6 passed ✓ |
| Save shape | `serde_default_guard_tests` | 10 passed ✓ |

## Findings

### REG-2026-09-29-01: #4606 was closed on its doc half; the GPU-idle throughput defect is still in the code and has no open issue
- **Severity**: MEDIUM
- **Dimension**: Closed-issue verification (GPU pipeline throughput)
- **Location**: `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs:59-80`; `crates/renderer/src/vulkan/sync.rs` (rider list)
- **Status**: NEW. #4606 was closed without its fix; this is a candidate to reopen it. It is not a regression, because the fix never landed.
- **Description**: #4606 has two parts: a MEDIUM throughput defect (on a GPU-bound frame, the all-slots fence wait drains the queue before the CPU records anything) and a comment claiming the cost was zero. The close comment says "this issue's doc-rot half is closed" and "the wait-narrowing itself remains open perf work gated on that plan". The issue was closed as COMPLETED anyway. None of the 163 open issues tracks the narrowing. A search of `/tmp/audit/issues.json` for fence/wait titles finds only #3429, which is unrelated.
- **Evidence**: The corrected comment itself says the defect is live:
  > Narrowing to `in_flight[frame]` alone is the fix, but ONLY after every rider on the all-slots wait is migrated per-FIF or defer-destroyed … then validated with `BYRO_VALIDATION=1`.

  The bench numbers it cites give fence_ms / wall_ms of 10.59/13.08 (Prospector TAA), 7.53/10.74 (Whiterun) and 16.12/37.64 (MedTek). Today's performance report still carries a "fence-bound" bench regression (R6a-regress-22) in ROADMAP. Nothing in the issue tracker covers it.
- **Impact**: Most of each GPU-bound frame is a stalled GPU. This is the largest measured frame-time sink, and with no open issue it is invisible to `/audit-performance` dedup and to `/fix-issue` planning.
- **Related**: #4606, #4601 (rider pin), #282, #3442, R6a-regress-22 (ROADMAP)
- **Suggested Fix**: Reopen #4606, or file a successor, for the throughput half. Carry the documented preconditions as acceptance criteria: all riders migrated, the #282 in-buffer barrier, and a validation run on both upscaler modes.

### REG-2026-09-29-02: `check-playable-smoke-contracts.sh` does not neutralise `BYROREDUX_OBLIVION_DATA`; it runs the real P0 gate locally and then fails
- **Severity**: MEDIUM
- **Dimension**: Guard integrity (smoke-gate contract lane)
- **Location**: `scripts/check-playable-smoke-contracts.sh:37-40` (env override block); `docs/smoke-tests/fixtures/oblivion.env` (`FIXTURE_GATES=(p0-door-interaction)`, `FIXTURE_DATA_DEFAULT=/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data`)
- **Status**: NEW. It is described as unverified in `.claude/commands/audit-runtime/SKILL.md` Dim 1 ("Verify against the script before filing"). This sweep verified it, empirically. No open issue covers it; #4547, which is closed, made the Oblivion route dispatchable but did not touch this script.
- **Description**: The contract loop runs each gate with the Skyrim SE / FNV / FO3 / FO4 data variables pointed at an empty temp dir, and expects exit 77 (SKIP). The Oblivion fixture (`84bbc44ed`, 2026-09-17) declares `p0-door-interaction` and reads `BYROREDUX_OBLIVION_DATA`, which the script never overrides. On any machine with Oblivion at the default path, the "missing-data" probe runs the real gate: `cargo run --release`, the engine, and `byro-dbg`. The gate then passes (exit 0), and the contract reports `FAIL -- p0-door-interaction[oblivion] missing-data path exited 0 instead of SKIP=77`. `set -e` plus `fail` aborts the script there, so every later contract never runs. That includes the #4730 literal-`run_gate` scan, the W1 route checks and the m47 self-test.
- **Evidence**: While verifying #4730, this sweep ran the script as the audit-runtime skill describes it ("runs each gate with data neutralised"):
  - It exited 1 with the FAIL line above.
  - `target/release/byroredux` was rebuilt at 17:39:59, which proves the real gate ran.
  - A re-run with `BYROREDUX_OBLIVION_DATA=<empty dir>` exited 0, and every remaining contract passed, including #4730's.
  - CI is not affected, because the runner has no Oblivion data.
- **Impact**:
  - The script is documented and CI-labelled as data-free, but on a normal dev machine it launches a GPU process. That breaks the project's "don't launch the engine beside the user's instance" rule, and it breaks the no-engine constraint of audit runs such as this one.
  - Every local run reads red.
  - The early abort hides the contracts that come after it.
- **Related**: #3039 (per-fixture SKIP≠PASS contract), #4547, #4730
- **Suggested Fix**: Build the override list from the fixtures, not a hand-kept list. For each `fixtures/*.env`, export its `FIXTURE_DATA_ENV` pointing at `$MISSING_DATA`. Add a self-check that fails when a fixture declares a data variable the loop does not neutralise.

### REG-2026-09-29-03: Two verified fixes have no regression guard (#4607 hot-path hashing, #1042 bare version literals)
- **Severity**: LOW
- **Dimension**: Guard coverage (hardening gap)
- **Location**: `byroredux/src/render/groundcover.rs` (#4607); `crates/nif/src/blocks/` and `crates/nif/src/version.rs` `bsver` (#1042)
- **Status**: NEW (PARTIAL results for #4607 and #1042)
- **Description**:
  - **#4607**: `render/groundcover.rs` is FxHash end to end. The only std `HashSet` is at `:1013`, inside `#[cfg(test)]`. But unlike `skin_offsets`, `light_history`, `SkinSlotPool` and `rigid_motion_history`, no `*_not_siphash` / `does_not_use_siphash` source-scan test covers the file. The fix commit's only CI edit (`c010c9fb9`) was the unrelated #4603 lock-order lane.
  - **#1042**: the sweep holds. There are 0 bare `NifVersion(0x…)` in non-test `blocks/`, and all 37 bare `bsver <op> N` hits in `crates/nif/src` are comments or strings. No test enforces this.
- **Evidence**: `git grep -n "HashMap\|HashSet" byroredux/src/render/groundcover.rs` finds `:18/89/90/261` (Fx) and `:1013` (test). `git grep -nE "bsver(\(\))?\s*(>=|<=|>|<|==|!=)\s*[0-9]+" crates/nif/src ':!*test*'` returns 37 lines, all in comments or strings.
- **Impact**: A reintroduced std map in the per-frame ground-cover path would break the #2923 hot-path rule, or a new bare BSVER literal would appear, and nothing would fail.
- **Related**: #4607, #2923, #1042, #1336
- **Suggested Fix**: Add a `source_scan::production_text`-based pin for `render/groundcover.rs` next to `skin_offsets_hasher_tests.rs`. Add a NIF-crate source scan that rejects `NifVersion(0x` and bare `bsver` comparisons outside `version.rs`.

## Open-but-fixed reconciliation (TD4-2026-09-29-01)

This sweep re-queried all 32 numbers named in TD4-2026-09-29-01 (TD4's 30, plus the #4748 doc half and #4866) with `gh issue view`. **All 32 are still OPEN** at the time of this run. "Own" marks a spot-check this sweep made at HEAD `9fcfdc3fc`. Other rows rest on the named sibling report.

| Issue | Sev | Fixing commit | Evidence | Verified by |
|---|---|---|---|---|
| #4739 | low | `546366364` | `SoundCache` + `play_oneshot_cached` via `get_or_load` | audio |
| #4740 | low | `546366364` / `2f8538334` | tests use `headless()` | audio |
| #4741 | low | `2f8538334` | `late.rs:236-243` names `camera_follow_system` | audio |
| #4742 | medium | `2f8538334` | player swing removed; `player_swing_without_marked_draugr_does_not_queue_feedback` (`combat_anim.rs:821`) | audio + own (test present) |
| #4744 | low | `2f8538334` | tautological self-match removed | audio |
| #4745 | low | `2f8538334` | all fixtures carry `--sounds-bsa` (successor AUD-2026-09-29-D5) | audio |
| #4746 | low | `2f8538334` | `systems/audio.rs:1,226-229` | audio |
| #4748 | medium | `2f8538334` | resource-determinism disclaimer in `snapshot.rs` + `save-load-roundtrip.md:62-63` | save |
| #4749 | **high** | `2f8538334` | `pbr_classified_at_import` gate (`material_translate.rs:609`), placeholder `false` | speedtree + own |
| #4750 | **high** | `2f8538334` | `MAX_OBSCRIPT_VM_NESTING = 32` (`obscript_vm.rs:53`, checks at `:295`, `:576`) | scripting + own |
| #4751 | low | `2f8538334` | `ObScriptDiagnostics` link fixed | scripting |
| #4753 | medium | `63c0aee3b` | `encode` enforces `MAX_MESSAGE_SIZE`, `MAX_WALK_DEPTH` 64 | tooling |
| #4754 | low | `63c0aee3b` | 10 s timeout + 2 tests (successor TOOL-D1-01) | tooling |
| #4755 | medium | `63c0aee3b` | `requested_gameplay_components_are_registered` (`registration.rs:416`) | tooling + own |
| #4756 | low | `63c0aee3b` | `debug_cli_component_counts_match_the_registry` (`registration.rs:449`) | tooling + own |
| #4758 | medium | `63c0aee3b` | `atomic_write` (successor TOOL-D4-01) | tooling |
| #4759 | medium | `63c0aee3b` | `esm_opens` + `a_present_but_truncated_main_plugin_fails_validation` (`validate.rs:311`) (successor TOOL-D5-01) | tooling + own |
| #4760 | low | `63c0aee3b` | only `playable-smoke.yml:45` repo-var fallback remains | tooling + own |
| #3429 | medium | `e2f99ad55` | same-extent `update_rgba` queued; `record_pending_rgba_uploads` | UI |
| #4127 | low | `8fd66a2b6` | coordinate-system.md matches code | legacy-compat |
| #4778 | low | `e26441c34` | `composite_and_volumetrics_uniforms_match_rust_field_order` (`composite.rs:1620`) | own |
| #4783 | medium | `88c23887b` | `filter_fog_volumes_for_grid` / `fog_volume_within_grid_reach` (`volumetrics.rs:633-665`) | own |
| #4785 | medium | `90c779748` + `e2f99ad55` | sun block gated; local-light rays skipped for zero-scattering froxels (`volumetrics_inject.comp:3040`) — both halves | own |
| #4862 | low | `ab255cfd2` | `shaders_pin_the_mirror_constants` asserts log-space clamp order + `!contains("clamp(val, 0.0, 1.0)")` + THIRD_PARTY_NOTICES pin | own |
| #4863 | low | `ab255cfd2` | `exposure_meter.comp:68` skips NaN/Inf | own |
| #4865 | low | `ab255cfd2` | `static_assert`s in `byro_fsr3.h:82-85` | own |
| #4866 | low | `88c23887b` | `model_timer_encloses_all_phases_and_stats_copy` (`groundcover_models.rs:958`) | exterior + own |
| #4867 | low | `ab255cfd2` | `vizFlags = legacyDebugMode ? dbgFlags : 0u` (`triangle.frag:200`); `legacy_visualization_bits_are_disabled_in_named_debug_modes` | own |
| #4868 | low | `0e0d35b96` | `snapshot_from_bits_with_valid_bits` (`gpu_timers.rs:471`) | renderer + own |
| #4872 | low | `0e0d35b96` | memory-budget.md "0, 14, or 28 MiB per slot" (`:184`), noise pair 294,912 B (`:491`) | own |
| #4880 | **high** | `e26441c34` | `record_dds_upload` destroys image on allocate failure (`texture.rs:468`), frees on view failure (`:529-535`), cleanup before recording (`:501`) | own |

**Close candidates: 31.**

**Keep open: #4864.** The docs are aligned, but its Suggested Fix requires an `upscaler_quality` A/B or a RenderDoc mask capture to pick the reactive-mask policy, and that was never recorded.

For #4745, #4754, #4758 and #4759, close with a pointer to the successor finding each fix introduced.

## Disclosure

The REG-02 evidence came from an accidental violation of this run's constraints. Verifying #4730's guard with `bash scripts/check-playable-smoke-contracts.sh`, which the audit-runtime skill describes as data-neutralised, ran the real Oblivion P0 smoke gate: a release build and one engine plus `byro-dbg` session, which exited 0 on its own. No engine process was left running (`pgrep` was clean at 17:40). The re-run used `BYROREDUX_OBLIVION_DATA` pointed at an empty scratch dir and launched nothing. No source, skill or issue was modified.

## Next step

`/audit-publish docs/audits/AUDIT_REGRESSION_2026-09-29.md` publishes REG-01…03. The 31 close candidates are a manual close. That is not part of publishing.
