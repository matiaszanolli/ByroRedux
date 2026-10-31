# Regression Verification Audit — 2026-10-31

**HEAD**: 670f81917 · **Baseline**: `docs/audits/AUDIT_REGRESSION_2026-10-27.md` (HEAD `fa6be4a6b`; churn base `fa6be4a6b`) · **Audited**: ~70 previously-closed issues (all 7 HIGH issues closed since the baseline, ~25 MEDIUM and LOW bug fixes closed since, 20 churn-ranked older fixes no earlier sweep had verified), the baseline's findings and the gates they track, the Step 4 fragile-area guards, all closed-HIGH real-data pins, the whole-workspace test, lock-order and clippy gates, and the CI state of `main` · **Unchanged since baseline (skimmed)**: n/a. This sweep is the delta pass: 167 commits (`fa6be4a6b..HEAD`), 135 issues closed and 252 `.rs`/shader files changed since the baseline.

Run context: part of the `/audit-suite --preset comprehensive` run of 2026-10-31. I ran it read-only. I did not launch the engine or run any smoke script (this machine has game data, so the playable-smoke contract script was read, not run). I ran no `--ignored` tests on `byroredux-plugin` except through the one-process-per-pin runner. Every cargo command used the repo `target/` with `CARGO_INCREMENTAL=0` and cargo 1.96.0, which matches CI. HEAD `670f81917` touches only audit and docs files, so its source equals `8985a2e99`, the last `main` commit with a completed CI run (the run on `670f81917` was still in progress). Scratch files are in `/tmp/audit/regression/`: `full_test.txt`, `lock_bin.txt`, `lock_scr.txt`, `clippy_block.txt`, `clippy_all.txt`, `step4.txt`, `pins.txt`, `ci_job_*.log`, `runs.txt`, `cand_all.txt`, `unverified.txt`, `closed_since.json` and `dim_1.md` through `dim_5.md`.

## Summary

| Result | Count |
|---|---|
| Closed issues checked | **~70**, plus 6 follow-ups from the previous sweep |
| PASS (fix present and guarded; default-lane guards ran green, real-data guards are pinned and green) | **all but the items below** |
| FAIL among the issues checked | **0** (the four regressions below are in gates and in code added after a fix, not in a verified fix site) |
| Follow-ups from the previous sweep | REG-01 (#6491) **fixed**, REG-02/03/04 (#6445/#6444/#6443) **fixed and closed**, REG-05 (#6492) **still red**, SAFE-01 (#6506) **still present** |
| Step 4 fragile-area guards | **8/8 green** |
| `scripts/run-real-data-pins.sh` (installed masters) | **all 66 closed-HIGH pins green** (63 on 10-27; #6459, #6460 and #6463 added three) |
| `cargo test --workspace --no-fail-fast` | **11,646 passed / 1 failed / 380 ignored**. The 1 failure is REG-2026-10-31-03 |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test`, on `-p byroredux --bin byroredux` and `-p byroredux-scripting --lib` | **2 failed**: the ROADMAP ceiling (REG-03) and one new lock-order cycle (REG-01). The scripting lib is green. This matches CI exactly |
| `cargo clippy --workspace -- -D warnings` (blocking) | **red, 3 errors** (REG-02) |

**New findings: 0 CRITICAL, 1 HIGH, 3 MEDIUM, 0 LOW** (the HIGH is a duplicate of AUDIT_ECS_2026-10-31 ECS-2026-10-31-01). Two existing open issues were re-confirmed (#6506, #6492) and are cited, not re-filed.

### CI state of `main`

`main` has been red on every completed push for six days. Between 2026-10-27 18:00 UTC and now there are 42 completed CI runs on `main`, and all 42 failed. The last fully green commit is still `79c8322f4` (10-24). The lanes, on `8985a2e99`:

| Lane | Cause at HEAD | ID |
|---|---|---|
| Test + Check + Clippy → `cargo test` | `readme_and_roadmap_hold_their_line_ceilings`: ROADMAP.md is 808 lines against the 800 ceiling | **REG-2026-10-31-03** |
| Test + Check + Clippy → `cargo clippy` (blocking) | three lints in `byroredux-scripting` | **REG-2026-10-31-02** |
| ABBA lock-order detector | the ROADMAP test again, plus `crafting::tests::improving_raises_a_held_item_in_place_and_never_hands_out_a_copy`: a new `Inventory`/`ItemInstancePool` cycle | **REG-2026-10-31-01** (dup of ECS-2026-10-31-01) |
| Playable smoke gate contracts | `docs/smoke-tests/m43-armor.sh` is in no contract loop. The checker exits on the first miss, and eight more M43 scripts are in the same state | **REG-2026-10-31-04** |
| Vulkan validation layers (lavapipe) | teardown panic `GPU allocator leaked: outstanding Arc references` | Existing: #6506 (SAFE-2026-10-27-01). `SceneBuffers::destroy` still has no loop over `moved_surface_buffers` (`grep` finds the field only in `buffers.rs` and `upload.rs`) |
| `cargo clippy --all-targets` (non-blocking) | red | Existing: #6492 (REG-2026-10-27-05). The site list has changed, see the note under REG-02 |
| Miri, shader parity, text integrity, machete, dhat, traceability | green | — |

Lanes that went red and green again inside the window, so they need no finding: the `per_frame_driver_functions_stay_within_their_size_pins` pin (red 10-27 → 10-29, fixed by `7eff680ac`), the three 10-27 lock-order cycles (fixed 10-28 by `d7da276d3`, `44cad6523`, `db3b32e47`), `example_targets_hold_their_per_crate_ceilings` (red for one run on `4e6132a73`, fixed by `39c04fe90`) and the blocking clippy errors in `bench_capture.rs`, `npc_spawn/ai_package.rs` and `systems/vats.rs` (red on 10-30, fixed since).

## Findings

### REG-2026-10-31-01: `improve_plan` reads `ItemInstancePool` while holding the `Inventory` guard; the save validator takes them the other way round
- **Severity**: HIGH (floor for ECS deadlock potential; the same class as #4982 and #6443-#6445)
- **Dimension**: CI gates / lock order
- **Location**: `byroredux/src/crafting.rs:225-232` (`fn improve_plan`: `let inventory = world.get::<Inventory>(player)?;`, then `.map(|(stack, row)| (stack, stack_quality(world, row)))`) and `crafting.rs:205-209` (`fn stack_quality`: `world.try_resource::<ItemInstancePool>()`). The reverse edge is `crates/save/src/validate.rs:488-499` (`validate_inventory_instances`: pool first, then `query::<Inventory>()`).
- **Status**: Dup of AUDIT_ECS_2026-10-31 ECS-2026-10-31-01 (suite reconciliation: publish the ECS entry, not this one). Regression of #4982 (closed 2026-09-28; it removed this exact `Inventory → ItemInstancePool` shape from `reference_state.rs`, see the comment at `cell_loader/reference_state.rs:225-235`). Introduced by `2c55546f4` (10-30 23:10, "improving items at the grindstone and armor workbench"). New; no open issue.
- **Description**: `improve_plan` keeps the `Inventory` read guard alive across the whole row iterator, and `stack_quality` takes the `ItemInstancePool` read guard inside it. That records `Inventory → ItemInstancePool`. `validate_inventory_instances` records `ItemInstancePool → Inventory`, and its own comment says it took the pool first on purpose, to keep this pair in one order. `improve_item` calls `improve_plan` twice per craft, so this is production code, not only a test helper. The test helper `quality_of` (`crafting.rs:768-771`) has the same shape.
- **Evidence**: Under `BYRO_LOCK_ORDER_CHECK=1` the test `crafting::tests::improving_raises_a_held_item_in_place_and_never_hands_out_a_copy` fails at `crates/core/src/ecs/lock_tracker.rs:489` with `attempted acquisition of ItemInstancePool while holding Inventory ... cycle ItemInstancePool → Inventory → ItemInstancePool`. The backtrace runs `World::try_resource` ← `crafting::stack_quality` (crafting.rs:208) ← `improve_plan`/`quality_of`. I reproduced it locally (`/tmp/audit/regression/lock_bin.txt`) and it matches CI run 41215450288.
- **Impact**: The ABBA gate is red again, which hides any real cycle added next. `improve_item` runs from the crafting page on the main thread, so no deadlock is reachable today. The pair becomes a live deadlock if either side ever runs off the main thread or outside an exclusive system.
- **Related**: #4982, #4616, #3649, #6443, #6444, #6445, #313
- **Suggested Fix**: Copy the matching rows (`(row index, stack)`) out of the `Inventory` guard into a `Vec` and drop the guard before pricing them, the way `armor.rs::totals` clones the inventory first. Do it once, in a single `stack_quality`-style accessor: the "stack's instance → pool quality" lookup is spelled three times (`crafting.rs:205-209`, `armor.rs:147-151`, `inventory/actions.rs:599-602`), and only the crafting copy holds a guard around it.

### REG-2026-10-31-02: The blocking `cargo clippy --workspace -- -D warnings` step is red again
- **Severity**: MEDIUM
- **Dimension**: CI gates / closed-fix regression
- **Location**: `crates/scripting/src/fragment/vm/natives/ownership_crime.rs:108` (`unnecessary_map_or`: `map_or(true, Value::truthy)`), and `crates/scripting/src/jail.rs:305` and `:389` (`useless_conversion`: `i32::try_from(sentence_gold)` where `sentence_gold` is already `i32`). Both come from the 10-30 JAIL work (`9c6c37cd0`, `36d7bfbd6`).
- **Status**: Regression of #6222 (closed 10-27, "the blocking clippy gate is green again, on every crate"). #6222 was itself a regression of #5849.
- **Description**: `cargo clippy --workspace --keep-going -- -D warnings` stops with `could not compile byroredux-scripting (lib) due to 3 previous errors`. Because the failing crate is a dependency of the binary, the step reports nothing about the crates downstream of it.
- **Evidence**: `/tmp/audit/regression/clippy_block.txt` (local, rustc/clippy 1.96.0) and CI run 41215450288, step "cargo clippy". The step has been red on every CI run since 10-30 (first cause `bench_capture.rs:98` from `f7194719f`, then the JAIL lints).
- **Impact**: The blocking gate has been red for a day and the same class has recurred three times (#5849, #6222, now). Any further lint in the other crates is masked.
- **Related**: #6222, #5849, #6492 (the non-blocking `--all-targets` step)
- **Suggested Fix**: Use `is_none_or(Value::truthy)` at `ownership_crime.rs:108` and drop the two `i32::try_from(...).unwrap_or(i32::MAX)` wrappers in `jail.rs`. Note on the existing #6492: the `--all-targets` step now also lists `crates/scripting/src/condition/tests.rs:134` (unused imports `Oblivion`, `Starfield`), `jail.rs:658` (`GUARD` never used) and `obscript_more/tests.rs:684-685` (`field_reassign_with_default`), besides the `shore_probe.rs` sites it already tracks.

### REG-2026-10-31-03: ROADMAP.md is 808 lines against the 800-line ceiling, and `cargo test` is red on it
- **Severity**: MEDIUM
- **Dimension**: CI gates / closed-fix regression
- **Location**: `ROADMAP.md` (808 lines). The ceiling is enforced by `byroredux/src/workspace_hygiene_tests.rs:871` (`readme_and_roadmap_hold_their_line_ceilings`).
- **Status**: Regression of #5332 (closed 10-12, "ROADMAP back under 800 with the ceiling enforced per commit"), which was itself a regression of #5111. `3b668d713` (10-26) and `43252c859` (10-29, "Refs #6375 #6378") each trimmed it back. Introduced by `8985a2e99` (10-31 12:19, "docs(render): fire lighting visual A/B result"), which took the file from 798 to 808 lines.
- **Description**: The ROADMAP sat at 798 lines for the whole M43 series and the next docs commit pushed it 8 lines over.
- **Evidence**: `ROADMAP.md is 808 lines against its 800-line ceiling (#5111) — trim before committing (#5332)`. It is the only failure in the local `cargo test --workspace` run (11,646 passed) and it fails in both the `cargo test` job and the ABBA job in CI run 41215450288.
- **Impact**: `cargo test` on `main` is red, which masks any other test regression and keeps the #6331 release gate shut.
- **Related**: #5111, #5332, #6375, #6378, #6331
- **Suggested Fix**: Move the 10 added lines (or older superseded ones) to `HISTORY.md`. The rule is "trim before committing", so the `/session-close` flow, not the individual docs commit, is where this keeps slipping in.

### REG-2026-10-31-04: Nine new `docs/smoke-tests/m43-*.sh` scripts are in no contract loop, and the playable-smoke contract lane is red
- **Severity**: MEDIUM
- **Dimension**: CI gates / closed-fix regression
- **Location**: `scripts/check-playable-smoke-contracts.sh:123` (the `m43-adia-conversations m43-item-drop m43-crafting` loop) and `:328-332` (`UNCHECKED_SMOKE_GATES`). The uncovered scripts are `m43-armor`, `m43-attraction`, `m43-attraction-reaction`, `m43-fo4-actions`, `m43-hello`, `m43-mines`, `m43-node-window`, `m43-skyrim-actions` and `m43-tempering`.
- **Status**: Regression of #6461 (closed 10-30, "the smoke contract checker covers m42-package-procedures, vats-targeting and w2-dynamic-water, and cannot lose the next gate"). That issue's own comment says the class recurred three times already (#5427, #6057, #6461). The lane first went red on `59ae0b846` (10-31 02:43 UTC).
- **Description**: The checker at the end of the file fails when a script in `docs/smoke-tests/` is neither named earlier in the checker nor listed in `UNCHECKED_SMOKE_GATES`. The M43 live-gate work (`4dd6197dc`, `7c9414845`, `93428cba1`, `834e83822`, `31aca901d`, `542f3de7b`, ...) added nine scripts and touched neither list. `fail()` calls `exit 1`, so CI shows only the first (`m43-armor.sh`).
- **Evidence**: CI run 41215450288, job "Playable smoke gate contracts": `FAIL -- docs/smoke-tests/m43-armor.sh is in no contract loop and not in UNCHECKED_SMOKE_GATES`. I reproduced the miss list by running the checker's name test over `docs/smoke-tests/*.sh` without executing any script (the gate scripts launch the engine on a machine that has game data).
- **Impact**: The lane is red, and a gate that exits 0 on missing game data would be counted as a pass (the `SKIP != PASS` contract the checker protects). Two of the nine scripts, `m43-hello` and `m43-attraction-reaction`, are staged-mover live gates whose missing-data behaviour nobody has verified.
- **Related**: #6461, #6057, #5427, #6331
- **Suggested Fix**: Add the nine scripts to the `m43-adia-conversations m43-item-drop m43-crafting` loop after confirming each exits 77 on missing data, or list them in `UNCHECKED_SMOKE_GATES` with a reason. The checker prints one miss per run; make `fail` collect all misses so one CI run shows the full list.

## Per-issue entries

### Dim 1: the HIGH issues closed since the baseline (7/7 PASS)

| Issue | Title (short) | Status | Fix commit | Fix site | Guard |
|---|---|---|---|---|---|
| #6443 | `apply_pending_group_swaps` re-enters `AnimationPlayer` under the `PendingGroupSwap` guard | PASS | d7da276d3 | `crates/scripting/src/obscript_anim.rs` `apply_pending_group_swaps` (snapshots the pending pairs, drops the guard, then resolves each player with one guard held) | ABBA lane: `a_held_request_applies_at_a_loop_wrap_and_the_latest_wins`, `flag_zero_waits_for_the_cycle_end_before_the_swap` pass under `BYRO_LOCK_ORDER_CHECK=1` |
| #6444 | `vats_scan` under the `GlobalTransform` guard | PASS | 44cad6523 | `byroredux/src/systems/vats.rs` `vats_scan` (gathers `(entity, translation)`, drops the guard, builds `target_bodies_by_owner` once) | ABBA lane: all `systems::vats::scan_tests::*` pass |
| #6445 | `weather_system` reads `SkyParamsRes` under the `CloudSimState` write guard | PASS | db3b32e47 | `byroredux/src/systems/weather.rs:1265-1277` (`cloud_tile_scales` copied out first) | ABBA lane: `render::sky::tests::*` pass |
| #6459 | FNV light-beam and glow VFX sheets get solid colliders | PASS | 4385d5fe8 | `byroredux/src/cell_loader/spawn.rs` `render_geometry_can_collide` (`unlit_blended_sheet`) | `unlit_blended_sheets_do_not_synthesize_collision` (ok); real-data pin `fnv_light_vfx_sheets_synthesize_no_collider` in `real-data-pins.txt` |
| #6460 | 406 FNV INFOs and 22 objects declined on unmodelled script getters | PASS | 102e4f5cd | `crates/scripting/src/condition/catalog.rs`, `eval.rs`, `obscript_cmd.rs` | `script_getter_functions_are_modeled_on_fallout_only_and_read_the_vms_body` (ok); pins in `real-data-pins.txt:58` |
| #6471 | Spawn plays a model's first `NiControllerSequence` instead of its idle | PASS | 69af7947a | `crates/core/src/animation/named_sequences.rs` `NamedSequences::spawn_handle`; `cell_loader/spawn.rs` | `spawn_starts_the_idle_never_an_activation_clip`, `spawn_plays_the_idle_sequence_beside_the_standalone_clip` (ok) |
| #6479 | `inline_references` weighs a sub-behavior by node count | PASS | ec8f8af6f | `crates/hkx/src/behavior.rs` `clip_weight`, `modifier_weight`, `machine_weight` | `a_modifier_list_of_expression_tables_is_weighed_by_what_it_holds` (ok) |

### Dim 2: follow-ups from the previous sweep

| Item | Status | Notes |
|---|---|---|
| REG-2026-10-27-01 / #6491 (`render_one_frame` size pin) | **Fixed**, issue still open | `7eff680ac` (10-29) took the function back inside the pin (pin 716, measured ~702 by brace count). `851636a13` (10-30, crafting) added to `app_frame.rs` afterwards and the pin test still passes. Close #6491; #6143 stays open for the skinned-BLAS refit and `build_blas_batched` |
| REG-2026-10-27-02 / #6445 | PASS (closed) | See Dim 1 |
| REG-2026-10-27-03 / #6444 | PASS (closed) | See Dim 1 |
| REG-2026-10-27-04 / #6443 | PASS (closed) | See Dim 1 |
| REG-2026-10-27-05 / #6492 (non-blocking `--all-targets` clippy) | **Still red** | Existing: #6492. The `shore_probe.rs` and `synthesize_trimesh_tests.rs` sites are still there and new ones landed; see the note under REG-02 |
| SAFE-2026-10-27-01 / #6506 (lavapipe teardown panic) | **Still present** | Existing: #6506. `SceneBuffers::destroy` still never frees `moved_surface_buffers` |
| #6222, #6331, #6182 | #6222 **regressed** (REG-02); #6331 PASS (the release gate still holds `main` red); #6182 PASS (pin runner below) | |

### Dim 3: MEDIUM and LOW fixes closed since the baseline, and churn-ranked older fixes (PASS)

| Issue | Status | Fix present | Guard |
|---|---|---|---|
| #4119, #6221, #6226, #6053 | PASS | `queue_increment_own_i64` entity check; ObScript untracked-state decline; non-locational `PlayMusic` at the listener; LOD-coverage selection | `owned_state_increment_on_an_unprojected_entity_is_refused`, `untracked_state_declines_instead_of_answering_a_constant`, `a_non_locational_play_is_heard_at_the_listener`, `authored_selection_partitions_across_skipped_levels` (ok) |
| #6096, #6099 | PASS | FaceGen path uses the plugin-local id; facetint is Skyrim-only | `only_skyrim_probes_a_facetint_path`, `plugin_local_object_id_strips_the_slot_bits_per_slot_kind` (ok) |
| #6114, #6133 | PASS | NIF import registry defers eviction during a cell load; held corpse ragdoll joins the queue on release | `deferred_eviction_keeps_the_batch_resident_then_trims_to_the_cap`, `deferred_eviction_is_bounded_by_the_headroom`, `a_held_corpse_joins_the_queue_only_on_release` (ok) |
| #6448, #6449, #6450, #6465 | PASS | BPND decoded at xEdit offsets; WEAP base VATS to-hit at DNAM@40; FO4-sized BPND left undecoded | `parse_bptd_leaves_an_fo4_sized_bpnd_undecoded` (ok); the DNAM@40 assertion is in the `#[ignore]`d `parse_real_esm.rs` (medium, so no pin required) |
| #6464 | PASS | FO4 "Use Land Data" children inherit the parent landscape | `use_land_data_children_inherit_on_oblivion_fo3_fnv_and_fo4_only` (ok); real-data `fo4_goodneighbor_inherits_the_commonwealth_landscape` |
| #6466, #6467, #6468, #6469 | PASS | Find activation queues through `PendingFragmentActivations`; incapacitated shooter ends a volley; Flee/Ambush decline fail-open conditions; `vats.enter` shares the key gates | `an_activator_target_is_activated_through_the_next_frame_queue`, `an_incapacitated_shooter_does_not_keep_its_volley`, `flee_and_ambush_are_declined_on_a_fail_open_condition_pass`, `the_entry_refusal_covers_every_gate_the_key_applies` (ok) |
| #6470, #6446, #6447 | PASS | `count_of_any` sums a form list, saturating; failed gear sets of despawned wearers are reclaimed; `PendingScriptStoryEvents` write declared | `count_of_any_sums_the_listed_forms_and_saturates`, `a_despawned_wearers_failed_set_is_reclaimed`, `system_access_declaration_tests` (ok) |
| #6476, #6477, #6478 | PASS | Skin-ness comes from the import; Oblivion underwear sub-meshes keep their own skin; Fallout-only opcodes no longer shadow Oblivion's | `the_skin_family_excludes_oblivions_underwear_sub_meshes`, `fallout_only_opcodes_do_not_shadow_oblivions` (ok) |
| #6411, #6452, #6453, #6458 | PASS | Localised cover keeps the default LTEX class; coarse LOD margin (`coarse_refine_radius`); seam exemption keyed on the cell pair | `a_localised_cover_keeps_the_default_land_textures_classification`, `terrain_seam_stats_*` (ok); #6452 has no test named for it, only the `terrain_lod.rs` selection tests that call `coarse_refine_radius` (PARTIAL-leaning, kept PASS because those tests exercise the function) |
| #6415, #6418, #6419, #6420 | PASS | Underwater god rays take cloud-shadow transmittance; IOR and shore tuning owned in core; CPU shore direction follows the mesh rule | `the_shaft_lane_packs_a_clamped_finite_transmittance`, `the_physics_bed_field_agrees_with_the_baked_shore_direction` (ok) |
| #6427, #6428, #6433 | PASS | Trespass/pickpocket fines are authored settings; affliction pool reads the composed value | `the_trespass_and_pickpocket_fines_are_authored_settings`, `pool_value_reads_the_composed_value_not_the_damage_layer` (ok) |
| #6515, #6516, #6517, #6472, #6360, #6223 | PASS | Use Keywords template bit; FACT CRVA cut points; Skyrim LE BSA sweep via `data_dir`; Z-up swap ratchet; `mq101_conformance` split; one Creation-era WTHR decoder | `a_use_keywords_shell_takes_the_leveled_picks_keywords`, `every_game_data_dir_helper_routes_through_data_dir`, the `workspace_hygiene_tests` ratchet, `asset_classification_is_case_insensitive_and_scoped`, the `weather.rs` and `spatial_units_tests` suites (ok) |
| #6502 | PASS | Recursive-read warning names the user's call site | `recursive_read_warning_names_the_users_call_site` (ok) |
| #6498, #6499, #6501 | NOT RUN | Baseline refresh and `p5-f5-f9-quicksave.sh` change; these are runtime gates (`runtime audit` owns them). I did not launch the engine | — |
| #6362, #6363, #6372, #6190, #5979, #5750 | PASS | `PlayerEntity::get` accessor; `Inventory::count_of`; `ItemKind::weapon_stub`; `ObScriptLedger` v117 drop; `entity_global_form_id`; `GetIsVoiceType`/`IsCommandedActor` getters | `workspace_hygiene_tests` scans, `condition/tests.rs`, `condition_catalog_real_data.rs` (ok) |
| #4996, #4997, #4817, #4822, #4823 | PASS | `container_loot_system` declares the physics write; `register_newcomers` rayon section pinned; `CombatDisposition` armed at finalize; `AddSpell` gives an actor a spell list; save-classification list | `physics/src/sync.rs` #4997 test, `system_access_declaration_tests`, `round_trip_tests.rs` (ok) |
| #3159, #3112, #3034, #2400, #3460, #2491, #5113, #5582, #4470 | PASS | `SetLocked`/`SetLockLevel` effects; weapon equip slot kept out of the biped bit space; loop text-key scan; animation-system lock order; and the doc/constant fixes | `fragment/tests.rs` #3159 tests, `text_events.rs` #3034 tests, `lock_tracker`/`validate.rs` order tests (ok); doc and constant fixes confirmed by grep |

The 7 HIGH, ~45 MEDIUM and LOW rows above are covered by the 11,646-test workspace run, which had a single failure (REG-03). Guard names for every row are in `/tmp/audit/regression/dim_3.md` and the commit subjects above.

### Dim 4: Step 4 fragile-area guards (8/8 green)

| Area | Result |
|---|---|
| GPU struct sizes | 76 `gpu_` tests ok (was 59 on 10-27) |
| NIFAL single boundary | 7 `resolve_pbr` ok (was 8; the filter now matches 7 tests in `byroredux-core`, none removed in the diff of that file) |
| Typed particle emitters | 3 `apply_emitter_params` ok |
| Collision coverage | 192 `byroredux-nif collision` ok (was 209 on 10-27; `git diff fa6be4a6b..HEAD -- crates/nif` removes no `#[test]`, so the count change is filter scope, not lost guards) |
| ReSTIR `resRadiance` retired | only the retirement comments remain (`lighting.glsl:173`, `triangle.frag:3341`) |
| Scheduler access | 9 `system_access_declaration_tests` ok |
| Save shape | 13 `serde_default_guard_tests` ok |
| Closed-HIGH real-data pins | `every_real_data_pin_names_an_ignored_test_in_its_lane` ok; runner: **all 66 pins green** |

## Summary table

| Issue | Title | Status | Fix Present | Guard |
|-------|-------|--------|-------------|-------|
| #6443 | group-swap re-entry | PASS | Yes | ABBA lane |
| #6444 | `vats_scan` guard | PASS | Yes | ABBA lane |
| #6445 | weather/sky lock order | PASS | Yes | ABBA lane |
| #6459 | FNV VFX colliders | PASS | Yes | ok + pin |
| #6460 | FNV script getters | PASS | Yes | ok + pin |
| #6471 | spawn idle sequence | PASS | Yes | ok |
| #6479 | HKX inline weight | PASS | Yes | ok |
| #6491 / #5096 | per-frame driver size pins | PASS (fixed since 10-27) | Yes | ok (issue still open) |
| #6222 | blocking clippy gate | **REGRESSED** | Fix held 3 days | step red → REG-2026-10-31-02 |
| #5332 | ROADMAP line ceiling | **REGRESSED** | Trims reverted | test red → REG-2026-10-31-03 |
| #6461 | smoke contract coverage | **REGRESSED** | Nine scripts uncovered | lane red → REG-2026-10-31-04 |
| #4982 | `Inventory`/`ItemInstancePool` order | **REGRESSED** (new site) | Old site fine | ABBA lane red → REG-2026-10-31-01 |
| #6492 / #6193 | `--all-targets` clippy | still red (existing) | — | step red |
| #6506 | `moved_surface_buffers` teardown | still present (existing) | No | lavapipe lane red |
| ~45 others (Dim 3) | — | PASS | Yes | ok |

Next step: `/audit-publish docs/audits/AUDIT_REGRESSION_2026-10-31.md`
