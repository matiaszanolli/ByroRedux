# Regression Verification Audit — 2026-10-27

**HEAD**: fa6be4a6b · **Baseline**: `docs/audits/AUDIT_REGRESSION_2026-10-21.md` (HEAD `4a3e89d56`; churn base `36d1a2c2c`) · **Audited**: 54 previously-closed issues (all 18 HIGH issues closed since the baseline, 11 churn-ranked older fixes that no earlier sweep verified, and the 25 MEDIUM fixes closed since the baseline whose production files churned most afterwards), the baseline's findings and the gates they track, the Step 4 fragile-area guards, all 63 closed-HIGH real-data pins, the whole-workspace test and lock-order gates, and the CI state of `main` · **Unchanged since baseline (skimmed)**: n/a. This sweep is the delta pass: 331 commits, 326 issues closed and 522 source files changed since `36d1a2c2c`.

Run context: part of the `/audit-suite --preset comprehensive` run of 2026-10-27. I ran it alone and read-only. I did not launch the engine or run any smoke script, and I did not run `--ignored` on `byroredux-plugin` other than through the one-process-per-pin runner. Every cargo command used the repo `target/` with cargo 1.96.0, which matches CI. HEAD `fa6be4a6b` touches only `.claude/` files, so its source is identical to `eae8fd740`, the last `main` commit with a completed CI run. Scratch files are in `/tmp/audit/regression/`: `full_test.txt`, `pins.txt`, `ci_jobs.txt`, `ci_failed.log`, `ci_<sha>.log`, `ci_testjob.log`, `candidates.txt`, `medium_churn.txt`, `closed_since.json`, and `dim_1.md` through `dim_5.md`.

## Summary

| Result | Count |
|---|---|
| Closed issues checked | **54**, plus 4 follow-ups from the previous sweep |
| PASS (fix present and guarded; default-lane guards ran green, real-data guards are pinned and green) | **54** |
| FAIL among the 54 | **0** |
| Follow-ups from the previous sweep | #6222 PASS, #6331 PASS, #6182 PASS (local pin runner), **#6193 FAIL** (REG-2026-10-27-05) |
| Step 4 fragile-area guards | **8/8 green** |
| `scripts/run-real-data-pins.sh` (installed masters) | **all 63 closed-HIGH pins green** |
| `cargo test --workspace --no-fail-fast` | **11,503 passed / 1 failed / 371 ignored**. The 1 failure is REG-2026-10-27-01 |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test`, run on `-p byroredux --bin byroredux` and on `-p byroredux-scripting --lib` | **13 failed**: the size pin plus three lock-order cycles. This exactly matches CI |

**New findings: 0 CRITICAL, 3 HIGH, 1 MEDIUM, 1 LOW**, plus 1 duplicate cited from another audit.

### CI state of `main`

`main` has been red on every push since `c7463c04d` (2026-10-25 22:16 UTC): 28 consecutive CI runs. The last fully green commit is `79c8322f4` (10-24 23:46 UTC). The push that turned it red carried 21 commits, among them the #6350 VATS series, `92173d61f` (#4128) and `bed162262` (#6250). Three lanes are red in every run since then:

| Lane | Cause at HEAD | ID |
|---|---|---|
| Test + Check + Clippy → step `cargo test` | `per_frame_driver_functions_stay_within_their_size_pins`. The failing tests changed over the 2.5 days: 10-25/26 had four other failures, all fixed since; the size pin has failed since `22d6e1a80` (10-27) | **REG-2026-10-27-01** |
| ABBA lock-order detector | three new cycles | **REG-2026-10-27-02 / -03 / -04** |
| Vulkan validation layers (lavapipe) | teardown panic `GPU allocator leaked: outstanding Arc references` (3 refs) | **Dup of AUDIT_SAFETY_2026-10-27 SAFE-2026-10-27-01**. I confirmed it independently: `SceneBuffers::destroy` (`crates/renderer/src/vulkan/scene_buffer/descriptors.rs:361-426`) has no loop over `moved_surface_buffers`, the field `bed162262` (#6250) added. With `MAX_FRAMES_IN_FLIGHT = 2`, that leaves the allocator at a strong count of 3. |
| Blocking `cargo clippy` | green again since `c1b2b1d12`. #6222 (REG-2026-10-21-01) closed 10-27 | — |
| `cargo clippy --all-targets` (non-blocking) | red | **REG-2026-10-27-05** |
| Miri, shader parity, text integrity, machete, smoke contracts, dhat, traceability | green | — |

Because the whole `Test + Check + Clippy` job and the ABBA job have been red for 2.5 days, a new real test failure would show up as the same red jobs. The #6331 release gate refused tag `v0.104` (commit `57a9272be`) because CI on that commit was red. The gate worked as designed; no v0.104 release exists.

## Findings

### REG-2026-10-27-01: `render_one_frame` outgrew its size pin, and the `cargo test` step has been red since 10-27 12:46 UTC
- **Severity**: MEDIUM
- **Dimension**: CI gates / closed-fix regression
- **Location**: `byroredux/src/app_frame.rs` (`fn render_one_frame`, 728 lines). The pin is in `byroredux/src/scheduler_access_tests.rs`, `per_frame_driver_functions_stay_within_their_size_pins` (`(APP_FRAME_RS, "fn render_one_frame(", 716)`).
- **Status**: Regression of #5096. That fix pinned the per-frame drivers' sizes (`a4ef215d4`). #6143 is open, but its remaining scope is the skinned-BLAS refit and `build_blas_batched`, not this pin.
- **Description**: The pin held at 712 lines through `296d21912`. `e71451d08` (10-26, terrain distance softening) took it to 715. `22d6e1a80` (10-27, "feat(renderer): implement geometry admission tiers and pool reserve management", `app_frame.rs` +14/−1) took it to 728. The test fails with `` `fn render_one_frame(` carries 728 lines (pin 716) — move the new work into a helper or module and record the decision, rather than raising the pin (#5096 / #6143) ``.
- **Evidence**: It fails locally in the full workspace run and in the CI `cargo test` step of every run from `22d6e1a80` to `eae8fd740`. A brace-count measurement of `render_one_frame` at each commit gives 79c8322f4 = 712, 296d21912 = 712, e71451d08 = 715, 22d6e1a80 = 728, HEAD = 728.
- **Impact**: `cargo test` on `main` is red. That masks any other test regression, and it also keeps the release gate (#6331) shut. The growth itself is what the pin exists to stop: `render_one_frame` grew by 16 lines in two days.
- **Related**: #5096, #4342, #6143 (open), #6359 (open; same class, different function)
- **Suggested Fix**: Move the geometry-admission tier wiring from `22d6e1a80` into a `streaming_helpers` (or `App`) method that `render_one_frame` calls. Do not raise the pin.

### REG-2026-10-27-02: `weather_system` reads `SkyParamsRes` while holding the `CloudSimState` write guard; `build_sky_params` takes them the other way round
- **Severity**: HIGH (floor for ECS deadlock potential; same class as #6208/#6209)
- **Dimension**: CI gates / lock order
- **Location**: `byroredux/src/systems/weather.rs:1264-1281` (`try_resource_mut::<CloudSimState>()`, then `try_resource::<SkyParamsRes>()` for `cap_cloud_layer_drift`) versus `byroredux/src/render/sky.rs:160-165` (`outdoor_sky_params(world, sky_res: &SkyParamsRes, …)` → `world.try_resource::<CloudSimState>()`)
- **Status**: Dup of AUDIT_ECS_2026-10-27 ECS-2026-10-27-03 (suite reconciliation — publish the ECS entry, not this one). It was introduced by `4f2596bf6` (10-26, "textured cloud layers are capped at 20 m/s of ground drift").
- **Description**: The detector reports the cycle `CloudSimState → SkyParamsRes → CloudSimState`. `weather_system` is an Early exclusive and `build_render_data` runs on the main thread after the scheduler, so the two do not overlap today. Even so, the detector rejects the pair, and the ABBA lane has been red on every push since `b269744f2`.
- **Evidence**: Under `BYRO_LOCK_ORDER_CHECK=1`, `render::sky::tests::{interior_only_session_bakes_procedural_outdoor_sky, interior_only_boot_uses_live_clock_for_portal_sun, the_portal_sun_is_the_sun_above_the_clouds}` fail at `crates/core/src/ecs/lock_tracker.rs:476`. The backtrace runs `outdoor_sky_params` (sky.rs:165) ← `build_sky_params` (sky.rs:98).
- **Impact**: The ABBA gate is red, so a real cross-thread cycle added now would go unnoticed (lock-order lane blind spots: this is the 7th time the lane has gone red). The pair becomes a live deadlock if either side ever moves off the main thread or out of exclusivity.
- **Related**: #6208, #6209, #5535, #313
- **Suggested Fix**: In `weather_system`, copy the four `cloud_tile_scale*` values out of `SkyParamsRes` before it takes the `CloudSimState` write guard. That is one short-lived read, dropped before the write.

### REG-2026-10-27-03: `vats_scan` takes `ActorVitals` under the `GlobalTransform` query guard
- **Severity**: HIGH (floor for ECS deadlock potential)
- **Dimension**: CI gates / lock order
- **Location**: `byroredux/src/systems/vats.rs:339-346`. The code holds `world.query::<GlobalTransform>()` and calls `world.get::<ActorVitals>(entity)` per row, then `FormIdComponent`, `Dead`, `CreatureActor` and the others.
- **Status**: Dup of AUDIT_ECS_2026-10-27 ECS-2026-10-27-02 (suite reconciliation — publish the ECS entry, not this one). It was introduced by `7eb4de261` (10-25, "Refs #6350: the targeting scan…"). Open #6429 covers `vats_mode_system`'s understated access declaration, not this lock-order cycle.
- **Description**: The detector reports `ActorVitals → ActorValues → GlobalTransform → ActorVitals`, and `vats_scan` closes the cycle. The code comment cites "Single-component queries + `world.get` per the #4104 house pattern", but the pattern only holds when the outer guard is dropped before the `get`s. `vats_mode_system` is an Early exclusive, so no deadlock is reachable today.
- **Evidence**: 7 `systems::vats::scan_tests::*` tests fail under `BYRO_LOCK_ORDER_CHECK=1`, both locally and in CI. The backtrace runs `World::get` ← `vats_scan` (vats.rs:346).
- **Impact**: This is the same gate blindness as REG-02. VATS is the live #6350 track and is still growing, so more lookups will land in this loop.
- **Related**: #6350, #6429 (open), #4104
- **Suggested Fix**: Collect `(entity, translation)` from the `GlobalTransform` query into a `Vec` and drop the guard before the per-entity `get`s. Alternatively, iterate `ActorVitals` first and `get` the transform.

### REG-2026-10-27-04: `apply_pending_group_swaps` re-acquires the `AnimationPlayer` storage while holding it and `PendingGroupSwap`
- **Severity**: HIGH (floor for ECS deadlock potential)
- **Dimension**: CI gates / lock order
- **Location**: `crates/scripting/src/obscript_anim.rs:186-193`. The code holds `query::<AnimationPlayer>()` and `query::<PendingGroupSwap>()`, then calls `sequence_player(world, object)`, which takes `world.query::<AnimationPlayer>()` again at line 86.
- **Status**: Dup of AUDIT_ECS_2026-10-27 ECS-2026-10-27-01 (suite reconciliation — publish the ECS entry, not this one). The outer guards come from `92173d61f` (10-24, Fix #4128), and the inner query from `4d36470b5` (10-21, #6253).
- **Description**: The detector reports `AnimationPlayer → PendingGroupSwap → AnimationPlayer`. Beyond the ordering, this is a recursive read of one `RwLock` on one thread. Under a writer-preferring lock, that read self-deadlocks if a writer queues between the two reads. The function's own comment (lines 180-184) warns about this hazard for the write path. The caller `pending_group_swap_dispatch` (`byroredux/src/boot/schedule/update.rs:972`) is an exclusive, so no writer can be queued today.
- **Evidence**: Under `BYRO_LOCK_ORDER_CHECK=1`, `obscript_anim::tests::{a_held_request_applies_at_a_loop_wrap_and_the_latest_wins, flag_zero_waits_for_the_cycle_end_before_the_swap}` fail. The backtrace runs `World::query` ← `sequence_player` (obscript_anim.rs:86) ← `apply_pending_group_swaps` (:193).
- **Impact**: This is the same gate blindness as REG-02, and the recursive read is a latent self-deadlock if the system is ever made parallel.
- **Related**: #4128, #6253, #6208
- **Suggested Fix**: Resolve `sequence_player` for each pending object before the `players`/`pendings` guards are taken. Alternatively, give `sequence_player` a variant that takes the already-held `players` guard.

### REG-2026-10-27-05: The non-blocking `cargo clippy --all-targets` step is red again
- **Severity**: LOW
- **Dimension**: CI gates / closed-fix regression
- **Location**: `crates/plugin/examples/shore_probe.rs:28` (dead_code `CELL`) and `:89` (type_complexity), both from `ceaf85a54` (10-26). `byroredux/src/cell_loader/spawn/synthesize_trimesh_tests.rs:599-608` has 3× `field_reassign_with_default`, from `1efe39722` (10-26). `byroredux/src/scene/world_setup.rs:2218-2219` has `field_reassign_with_default` in the #6406 fix's own test (`8e20ced6b`). The step is `.github/workflows/ci.yml` "cargo clippy (--all-targets, non-blocking)".
- **Status**: Regression of #6193 (closed 10-22). That issue was itself a regression of #5850.
- **Description**: Because the step sets `continue-on-error: true`, GitHub reports it as `success`. Its log on `eae8fd740` ends with `could not compile byroredux-plugin (example "shore_probe") due to 2 previous errors`, `could not compile byroredux (bin "byroredux" test) due to 4 previous errors`, and exit 101.
- **Evidence**: Job log of CI run 40596734340 (`/tmp/audit/regression/ci_testjob.log`, step "cargo clippy (--all-targets, non-blocking)").
- **Impact**: Lints build up in test and example targets without anyone seeing them. This is the third time the step has gone red after a cleanup.
- **Related**: #6193, #5850, #5621, #6222
- **Suggested Fix**: Apply the six mechanical fixes. Making the step's failure visible (for example, a job summary line) would stop the silent green.

### Dup: lavapipe validation lane, teardown panic
- Dup of AUDIT_SAFETY_2026-10-27 SAFE-2026-10-27-01 (also renderer REN-D5-01). `SceneBuffers::destroy` never frees `moved_surface_buffers` (#6250, `bed162262`). This is the cause of the red `Vulkan validation layers (lavapipe)` lane on all 28 runs since `c7463c04d`. The CI log shows `GPU allocator has 3 outstanding references` → `debug_assert!` at `crates/renderer/src/vulkan/context/teardown.rs:496`.

## Per-issue entries

### Dim 1: the HIGH issues closed since the baseline (18/18 PASS)

| Issue | Title (short) | Status | Fix commit | Fix site | Guard |
|---|---|---|---|---|---|
| #5755 | MSWP source compare spelling | PASS | f81e6e722 | `byroredux/src/cell_loader/refr.rs` `mswp_source_matches`; `spawn/mesh_instance.rs:134` | `mswp_source_matches_canonicalizes_and_honours_wildcards`, `build_overlay_xmsp_matches_a_materials_prefixed_xato_path` (ok); real-data pin `fo4_mswp_canvas_swap_matches_the_picture_frame_nif` (green) |
| #5932 | root-hosted collider pairing | PASS | e19982b8f | `cell_loader/graph_nodes.rs` `pair_collisions` | `a_root_hosted_body_stays_on_the_flat_spawn` (ok) |
| #6100 | Oblivion idle clip path | PASS | abd3d5001 | `npc_spawn.rs` `humanoid_default_idle_kf_path` | `installed_default_idle_clips_exist_in_the_meshes_archive` (pinned `bin`, green) |
| #6101 | guard-free getter lock order | PASS | 824c620a5 | `crates/scripting/src/fragment/effects/evaluate.rs` `evaluate_guard_guard_free` → `try_resource_2` | `try_resource_2_does_not_deadlock_against_resource_2_mut` (ok) |
| #6118 | Starfield 0xFE/0xFD FormID spaces | PASS | 0a2955d33 | `crates/plugin/src/esm/reader.rs` `remap_starfield`; `load_order.rs:429` | `starfield_small_master_forms_decode_by_small_ordinal` and siblings (ok); pinned |
| #6120 | Starfield 16-byte ACBS | PASS | 78facba89 | `actor/npc.rs` `ACBS_STARFIELD_LEN` arm :871 | `starfield_16byte_acbs_parses_gender_level_and_template_flags` (ok); pinned (#6354) |
| #6132 | swept pickup colliders detach | PASS | dc829fc83 | `reference_state.rs:611` → `unload.rs` `detach_victims_from_surviving_parents` | assertions in `save_reload_window_tombstone_is_applied_by_restore_resident` (ok) |
| #6202 | terrain height-blend continuity | PASS | 5f1a88929 | `bindings.glsl` `byroTerrainBlendCore` | `glsl_blend_fades_each_lane_with_its_share` and mirror tests (ok) |
| #6207 | equal attenuation → kira zero span | PASS | b0586f285 | moved by the #6140 split to `crates/audio/src/components.rs:53` `distance_range` | `equal_attenuation_never_hands_kira_a_zero_span` (ok) |
| #6208 | idle playback registry under player guard | PASS | 1e37a7536 | `systems/cinematic.rs:65` | ABBA lane: `scripted_one_shot_idle_marks_the_actor_until_it_finishes` absent from every red run |
| #6209 | ParticleEmitter under seven guards | PASS | a49073cc9 | `cell_loader/unload.rs` (unchanged since) | ABBA lane: particle/water tests absent from every red run |
| #6210 | PSDT duration is minutes | PASS | 7e2f21482 | `misc/pack.rs` `duration_minutes`/`start_minute` | `skyrim_plus_psdt_duration_is_minutes_so_the_window_closes` (ok) |
| #6211 | HKX step budget charges leaf tables | PASS | fbccdcbff | `crates/hkx/src/behavior.rs` | `a_fan_out_onto_a_clip_with_full_tables_is_refused_by_the_step_budget` (ok) |
| #6212 | shared inlining budget | PASS | 5696d0947 | `behavior.rs` `InlineBudget` | `inlined_volume_is_bounded_by_one_shared_weight_budget`, `a_cycle_through_two_blenders_is_refused` (ok) |
| #6213 | EyeEnvmap relative space | PASS | c6c0c5c90 | `triangle.frag` | `eye_envmap_measures_its_relative_centres_against_the_relative_position` (ok) |
| #6214 | EyeEnvmap centres Y-up | PASS | 52e48bd4c | `crates/nif/src/import/material/shader_data.rs` | `eye_envmap_writes_scale_and_both_reflection_centers_y_up` (ok) |
| #6343 | failed layout walk fails closed | PASS | 9be0666fd | `context/resize.rs`, `context/init_stages.rs` (split from `init.rs`), `svgf.rs`, `caustic.rs` | `a_failed_layout_walk_fails_closed_instead_of_warning_and_continuing` (ok) |
| #6406 | interior forced-weather keeps room lighting | PASS | 8e20ced6b | `scene/world_setup.rs` `apply_forced_weather_in_interior` | `a_forced_weather_reapply_leaves_the_rooms_own_lighting`, `the_interior_reapply_brackets_both_installs_with_the_rooms_state` (ok) |

### Dim 2: follow-ups from the previous sweep

| Issue | Status | Notes |
|---|---|---|
| #6222 (REG-2026-10-21-01, blocking clippy) | PASS | Closed 10-27. The blocking step is green on `c1b2b1d12` and `eae8fd740`. It had been red again for 12 runs (10-26 19:55 → 10-27 22:14) before the close |
| #6331 (REG-2026-10-21-02, release workflow) | PASS | v0.103 was released. v0.104 was refused by the green-CI gate because `57a9272be` had a red ABBA lane |
| #6182 (real-data lanes) | PASS | The schedule is disabled by design; the local `scripts/run-real-data-pins.sh` runs all 63 pins green |
| #6193 (`--all-targets` clippy) | **FAIL** | REG-2026-10-27-05 |

### Dim 3: churn-ranked older fixes (11/11 PASS) and MEDIUM fixes closed since the baseline (25/25 PASS)

| Issue | Status | Fix present | Guard |
|---|---|---|---|
| #5851 / #5852 / #5853 | PASS | `GC_FRAME_SERIAL` has 0 hits; the machete dev-deps note is at `ci.yml:80`; the flat-import doc is at `crates/nif/src/import/mod.rs:423` | CI machete lane green; scans |
| #5743 / #5844 / #5845 / #5846 / #5847 / #5848 | PASS | `FOOTSTEP_SURFACE_PROBE_REACH`, `MAX_LINK_CHAIN_HOPS`, `MATERIAL_DECAL_SLOTS`, `MAX_SKELETON_BONES`, `MAX_SPLINE_CONTROL_POINTS` are named and used | shader-constant and contract tests (ok) |
| #5593 | PASS | The only production raw `DescriptorSetLayoutBinding::default()` sites left are the builder (`descriptors.rs:144`) and the bindless array builder | — |
| #4906 | PASS | `cover_masks_for_cell`, `authored_cover_is_localised` | `cover_masks_put_each_lists_records_in_that_lanes_bits`, `the_model_tier_drops_the_affinity_only_for_a_localised_cover` (ok) |
| #5778, #6241, #5921, #6070, #5757, #6027, #6251, #6237, #6093, #6092, #5495, #6246, #6128, #6227, #6123, #6257, #6231, #6094, #5997, #6253, #6102, #6122, #6113, #6247, #6085 | PASS | All present; guard names are listed in `/tmp/audit/regression/dim_3.md` | ≥1 default-lane guard each, all ok. The #5778, #6241, #6070 and #6122 guards also have real-data variants |

#6128's guard `a_restored_oblivion_body_part_keeps_the_race_skin` failed in CI on 10-25 and 10-26 and passes at HEAD; it was fixed within the window.

### Dim 4: Step 4 fragile-area guards (8/8 green)

| Area | Result |
|---|---|
| GPU struct sizes | 59 `gpu_` tests ok |
| NIFAL single boundary | 8 `resolve_pbr` ok |
| Typed particle emitters | 3 `apply_emitter_params` ok |
| Collision coverage | 209 `collision` ok |
| ReSTIR `resRadiance` retired | only the retirement comments remain (`lighting.glsl:173`, `triangle.frag:3340`) |
| Scheduler access | 9 `system_access_declaration_tests` ok |
| Save shape | 13 `serde_default_guard_tests` ok |
| Closed-HIGH real-data pins | `every_real_data_pin_names_an_ignored_test_in_its_lane` ok; runner: **all 63 pins green** |

## Summary table

| Issue | Title | Status | Fix Present | Guard |
|-------|-------|--------|-------------|-------|
| #5755 | MSWP spelling compare | PASS | Yes | ok + pin |
| #5932 | root-hosted collider | PASS | Yes | ok |
| #6100 | Oblivion idle clip | PASS | Yes | pin green |
| #6101 | quest pair lock order | PASS | Yes | ok |
| #6118 | Starfield FormID spaces | PASS | Yes | ok + pin |
| #6120 | Starfield ACBS | PASS | Yes | ok + pin |
| #6132 | swept collider detach | PASS | Yes | ok |
| #6202 | terrain blend continuity | PASS | Yes | ok |
| #6207 | kira zero span | PASS | Yes | ok |
| #6208 | registry under player guard | PASS | Yes | ABBA lane (cycle gone) |
| #6209 | ParticleEmitter guards | PASS | Yes | ABBA lane (cycle gone) |
| #6210 | PSDT minutes | PASS | Yes | ok |
| #6211 | HKX leaf-table budget | PASS | Yes | ok |
| #6212 | HKX inline budget | PASS | Yes | ok |
| #6213 | EyeEnvmap relative | PASS | Yes | ok |
| #6214 | EyeEnvmap Y-up | PASS | Yes | ok |
| #6343 | layout walk fails closed | PASS | Yes | ok |
| #6406 | interior forced weather | PASS | Yes | ok |
| #5096 | per-frame driver size pins | **FAIL** | Pin present, exceeded | fails → REG-2026-10-27-01 |
| #6193 | `--all-targets` clippy | **FAIL** | — | step red → REG-2026-10-27-05 |
| 36 others (Dim 3) | — | PASS | Yes | ok |

Next step: `/audit-publish docs/audits/AUDIT_REGRESSION_2026-10-27.md`
