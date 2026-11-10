# #6686 — standalone source scans in `byroredux`, first pass

**Scope of this pass:** the seven files the issue names, 142 of its 301 sites.
The other 159 sites (about 130 tests across `app_step.rs`, `app_frame.rs`,
`save_io/live_reload_tests.rs`, `systems/*`, `cell_loader/*`, …) are **not yet
classified**. Counts come from a script over `byroredux/src`.

Classes: **device/data** (needs Vulkan, the winit event loop, or game data),
**runtime** (replaceable by running the code), **prose**, **syntax**, and
**input** (a read that feeds a check, not a scan itself).
"Read" means I read the test's body; "by doc" means classified from the test's
doc comment, which states its reason, without reading every assert.

| File | Sites | Test(s) | Class | Basis |
|---|---|---|---|---|
| `boot/schedule/mod.rs` | 32 + 19 concat rows | the `*_SRC` constants and concat lists under `system_access_declaration_tests` | **input** | They are the sources `acquisition_scan` parses (#6685); the check is now a syntax-tree walk, not a search. They stay as the data under test. |
| `boot/schedule/mod.rs` | 1 | `condition_src_covers_every_condition_file` | syntax | Read. Completeness of a concat list. |
| `boot/schedule/mod.rs` | 1 | `overlay_open_arms_the_tracker_lazily` | device/data | Read. The arm lives in the winit event handler, which needs an `ActiveEventLoop`. |
| `boot/mod.rs` | 15 concat rows | `SOURCES` | input | Out of scope: the concat list itself. |
| `boot/mod.rs` | 584, 698 | `sources_ordering_tests`, the sentinel-list test | syntax | Read. Machinery for the remaining text scans: once those retire, the sentinels and ordering rules can go. |
| `systems/navmesh_path.rs` | 21 | `no_navmesh_procedure_clones_its_waypoints_into_the_stepper`, `walk_movers_share_one_pass_two_write_back`, `cacheless_walk_movers_share_apply_walk_step` | syntax | Read the docs: "Neither shape is visible to a behavioural test, so pin it at the source" (#3269, #5580, #6364). |
| `cell_loader/object_lod.rs` | 14 | the LOD-resolve and clamp tests (5) | syntax | Read two, by doc for the rest. `spawn_object_lod_quad_threads_the_mesh_resolver` says it is "Not Vulkan/archive-reachable to exercise end-to-end"; the clamp tests pin which resolve function each loader calls. |
| `scheduler_access_tests.rs` | 3 | `MAIN_RS`, `APP_EVENTS_RS`, `APP_FRAME_RS` consts and the tests using them | device/data | By doc. Call order in `App::new` / `about_to_wait` / `render_one_frame`, none constructible without a window and a device. |
| `scheduler_access_tests.rs` | 11 | `per_frame_driver_functions_stay_within_their_size_pins` consts | syntax | Line-budget pins; #6680's line-budget item covers them. |
| `npc_spawn/tests.rs` | 7 | `resolve_inherited_call_sites_are_enumerated_and_pinned` | syntax | Read the doc: "the source-scan substitute" for a boundary the type system gives no signal on. |
| `npc_spawn/tests.rs` | 5 | `prepare_prebaked_state` purity scan | syntax | Read. Forbids three spellings in one function. |
| `app_events.rs` | 11 | startup order, input-quarantine pin release, scratch-coverage and group tests, `about_to_wait` and `window_event` shape, shutdown order | device/data (event-loop and `App` drop order); the `App` field-group and scratch tests are syntax | Read four (`authored_bench_origin…`, `about_to_wait_skips_the_scheduler_without_a_renderer`, `window_event_gates_consumed_through_the_predicate`, the shutdown pair by doc). The event-loop methods take an `ActiveEventLoop`; `Drop for App` needs a renderer. |

**Replaced in this pass:** none. Every test read in these seven files states why
no behavioural form exists, and I found none the doc was wrong about.

**Left for the next pass:** the 159 unclassified sites. Read first, from the
test names: `save_io/live_reload_tests.rs` (7), `systems/character.rs` (6),
`app_step.rs` (6), `app_frame.rs` (6), `systems/cinematic.rs` (4),
`cell_loader/load.rs` (4).

## Second pass — the six files the first pass listed as "read first" (2026-11-06)

**Scope:** `save_io/live_reload_tests.rs` (7), `systems/character.rs` (6), `app_step.rs` (6),
`app_frame.rs` (7, of which two have no doc), `systems/cinematic.rs` (4),
`cell_loader/load.rs` (4) = 34 sites. **Every row is classified "by doc"**: from the
test's name and doc comment, not verified assert by assert. Together with the first pass
that is 176 of 301 sites; about 125 remain unclassified.

| File | Sites | Class | Basis |
|---|---|---|---|
| `save_io/live_reload_tests.rs` | 7 | device/data | Each pins a step order inside `execute_pending_save_loads`, which needs a live `VulkanContext` and archives (#3789/#4135, #5060, #6081, #5435, #6083, #6238, #6239). The docs say so for #3789 and #5435. |
| `systems/character.rs` | `momentum_is_cleared_through_one_method`, `…_leaves_authored_water_damage_to_water_damage_system`, `the_swim_decisions_stay_in_pinnable_helpers`, `the_production_probe_gate_calls_the_pinned_predicate`, `the_ground_probe_screens_its_grounded_half_on_the_walkable_normal` | syntax | The docs state the reason: `character_controller_system` has no test caller, and the pins keep a pure predicate in the path. The predicate's own truth table already runs. |
| `systems/character.rs` | `player_water_state_samples_weather_once_before_water_queries` | syntax (lock order) | Pins where a resource guard is taken relative to water storage (#3265). A runtime check would be `BYRO_LOCK_ORDER_CHECK`, which is a graph over a whole run, not a unit test. |
| `app_step.rs` | `the_save_drain_publishes_…`, `texture_swaps_flush_the_uploads_they_queue`, `interior_transition_uses_the_shared_budgeted_apply_job`, `upscaler_switch_failure_exits_the_event_loop`, `static_blas_recovery_runs_between_frames_…` | device/data | `App` step methods that take a renderer or an event loop. |
| `app_step.rs` | `scene_centroid_distance_acquires_global_transform_before_mesh_handle` | syntax (lock order) | #3695, same reasoning as above. |
| `app_frame.rs` | `between_frames_is_captured_before_draw_frame_runs`, the three rollback tests, `render_one_frame_does_not_assert_on_material_overflow_count`, `a_loading_stage_frame_takes_the_groundcover_off_path` | device/data | `render_one_frame` order; needs a `VulkanContext`. `prepare_groundcover_models` (L1217) has no doc and was **not read**. |
| `systems/cinematic.rs` | `text_key_query_is_acquired_before_the_string_pool`, `the_scene_registry_guard_is_released_before_the_quest_acquisitions`, `scene_trigger_router_evaluates_no_conditions_under_the_stages_guard` | syntax (lock order) | #3446, #3937, #6008: a guard's scope against other acquisitions. #6085's `acquisition_scan` covers acquisition *sets*, not intra-function guard scope. |
| `systems/cinematic.rs` | `the_router_calls_the_gates_shared_eligibility_predicate` | syntax | #4333: "pin the call, not the copy". |
| `cell_loader/load.rs` | `every_animation_route_installs_the_combat_family_too`, `interior_apply_job_is_budgeted_and_reclaimable`, `interior_water_spawns_before_the_reference_load_flush`, `the_synchronous_interior_load_runs_the_apply_job` | device/data | The docs say each is "CI-reachable without a Vulkan device or shipped ESM archives" only as a source check. |

**Replaced in this pass:** none. No doc names a behavioural alternative, and the
renderer-or-archive dependency each one states is real.

**Not done:** the acceptance criterion "every scan classed replaceable has a behavioural
test" cannot be met until the remaining ~125 sites are classified, so #6686 stays open.

## Third pass — the next six files by site count (2026-11-06)

**Census:** outside the 13 files classified so far, `rg -c 'include_str!\(|read_to_string\('` finds **216 lines
in 87 files**. That counts every read, including `SOURCES` concat rows and reads that feed a check, so it is
an upper bound on scans, not a scan count. This pass takes the six biggest files (the counts below are
those lines). Classified **by doc comment only**; five reads had no doc and were not classified.

| File | Lines | Test(s) | Class | Basis |
|---|---|---|---|---|
| `material_translate.rs` | 11 | `anisotropic_rationale_matches_what_the_source_formats_carry` (#3613), `documented_texture_role_list_matches_the_struct` (#3465) | prose | The docs say they keep a rationale comment and two hand-written texture-role lists in `nifal.md` and the audit SKILL honest against the struct. A comment or document is the subject. |
| `material_translate.rs` | | `every_exterior_spawner_inserts_a_boundary_material` (#2444) | device/data | Doc: the spawners need a live `VulkanContext` (mesh upload, texture resolve). |
| `material_translate.rs` | | `every_source_derived_material_field_is_pinned_by_a_test` (#3462) | syntax | Meta-completeness: checks that each `source.X → material.X` line has a test that reverts it. The pinned object is the test file's text. |
| `workspace_hygiene_tests.rs` | 10 | 8 repo lints: self-described disposable examples, string literals with a 6+ space run, `classify_pbr` mentions framed as historic, README/ROADMAP line ceilings, example-target counts, real-data pin lanes, the live-schedule table | prose / syntax | These lint the repository tree (docs, comments, file layout), not engine behaviour. None has a runtime form. `every_registered_system_is_named_in_the_live_schedule_table` reads a document against the registered names: the document is the subject. |
| `extensions/mod.rs` | 9 | not classified | | Only `sdk_input_action` carries a read, and it has no doc. Not read. |
| `streaming_tests.rs` | 6 | `the_starved_arm_closes_the_boundary_and_arms_the_retry` (#6093), `every_lod_ring_gates_on_admission_and_never_caches_a_refusal` (#6092) | device/data (first), syntax (second) | The first pins a caller's handling that "no verdict test reaches": it runs inside the streaming step with a renderer. The second pins that each LOD ring classifies a no-block attempt before caching it. Both docs say a verdict test cannot see it. |
| `save_io/serde_default_guard_tests.rs` | 6 | not classified | | Not read. They fingerprint the serialized shape of saved types by file path; the subject is source text (see the #6715 baseline note in that file). |
| `npc_spawn/mod.rs` | 6 | not classified | | Only a module doc was found at the first read. Not read. |
| `cell_loader/spawn/mesh_instance.rs` | 6 | `morph_spawn_uses_mesh_handle_shared_delta_cache`, `morph_slot_creation_is_reachable_end_to_end` (#4399), `every_ligh_spawn_site_consumes_the_canonical_falloff_lane` (#4514, #6692) | device/data, syntax | The morph pair spans spawn sites that need a device; #6692's own note keeps the falloff scan on purpose, because the shared step attaches no light and each site stands alone. |

**Replaced in this pass:** none.

**Tally.** 176 of 301 sites were classified after the second pass. This pass classifies about 20 more by doc
and leaves about 28 lines in these six files unread. The remaining ~150 lines are in 81 files, mostly one to
five each. The issue's acceptance cannot be met by reading alone: each unread site needs its test body read
to confirm the doc, and any that turn out replaceable need a behavioural test and an injected fault.

## Fourth pass — every remaining file (2026-11-10)

**Census.** Outside the 20 files classified in passes 1-3, a read of any file (`include_str!` /
`read_to_string`) is **172 lines in 84 files**; **162 lines in 79 files** of those read Rust source, which is what the
issue counts. After this pass the Rust-source figure is **155 in 75** (the four files below lose their last scan).
**Method:** every site's enclosing test and its doc comment were listed by script; the doc states the reason for
nearly all of them. Bodies were **read** for the ones marked *read*; the rest are "by doc/name" and were not
verified assert by assert. A doc that says "needs a `VulkanContext`" was taken at its word unless the body showed
otherwise.

### Replaced (4 tests, 7 sites)

| Test | Was | Now | Injected fault that fails it |
|---|---|---|---|
| `render/static_meshes.rs::every_supplemental_texture_slot_is_written_exactly_once` (2 sites + a 60-line text parser) | parsed the lane writes out of the draw builder's source and compared names | `supplemental_texture_indices(&MaterialTextureSet<u32>)` is now a pure function; `every_supplemental_texture_slot_carries_its_own_role_once` gives every role its own non-zero value and checks each lane by hand-written table, the table's length against `slot::COUNT`, and that its lanes are a permutation of `0..COUNT` | `lanes[TINT] = inner_layer` fails lane 0; deleting the `WRINKLE` write fails lane 5 |
| `components.rs::name_index_and_subtree_cache_stay_fx_hash_map`, `build_subtree_name_map_stays_fx_hash_map` (2 sites) | searched `indices.rs` and `anim_convert.rs` for the type's spelling | function-pointer ascriptions to `FxHashMap<..>`: the hasher is part of the type | `NameIndex.map` as a std `HashMap` stops the test module compiling |
| `commands/env_health_tests.rs::weather_sky_walk_covers_every_float_field_of_the_structs` (2 sites + `declared_field_names`) | scanned the `WeatherSkyState` / `ImageSpace` declarations for field names | an exhaustive destructure of both structs (no `..`) plus a hand-written float-field table, compared with the gate's walked names in both directions | removing `wind_speed` from the walk fails "not walked"; a new struct field stops the build |
| `asset_provider/tests/material_path.rs::present_normal_derive_canonicalises_before_probing` (1 site) | searched `texture.rs` for `canonical_texture_key(&derived)` | the decision is `derive_present_normal_map_path_with(diffuse, probe)`; the test hands it a probe that answers only for the canonical key and records what it was asked, for a `Data\Textures\…` and a `textures\…` diffuse | probing the raw `derived` path returns `None` and fails |

All four changed one production file only to expose a pure function or to leave the production body unchanged.
The `static_meshes.rs` change is the one that touches a hot path (`build_static_draw_command`); the function is a
straight-line copy of the 16 writes it replaced.

### Classified, by file

Classes as before. **input** = the read feeds a check, it is not itself a scan.

| File (sites) | Class | Basis |
|---|---|---|
| `scene/mesh_entity.rs` (6) | device/data + syntax | #6692 / #6691 / #6817: both spawn paths need a device; the pins are "each path calls the step once", "caps are hidden before the first registry call" (a leaked handle), "the gate is handed the carried material". *read the docs.* |
| `cell_loader/lod_support.rs` (5) | syntax | Rings route eviction through `retain_resident` (#5820), abandon a refused group (#6332), release before an empty return: "which function a ring calls" has no behavioural form short of a device. |
| `commands_tests.rs` (5) | syntax (lock order), prose, runtime half already there | `lock_order_pin_tests` pin which guards are live across a call (doc: the detector is process-wide); the audit-skill claim (#3996) is prose; `form_id_arguments_parse_hex_through_one_spelling` already runs the parser, and only its tail scans two files for `from_str_radix`. |
| `main.rs` (5) | input + syntax + device | `stored_upscaler` reads a settings file; the three pins are `App` wiring (event loop and window); `every_bench_gpu_key_is_printed_on_the_bench_line` already runs `bench_gpu_tokens`, and two `APP_EVENTS_RS` needles are what is left. *read.* |
| `anim_convert.rs` (4) | syntax | #4426 (flip-frame resolve goes through the per-role resolver), spawn sites use the shared sink helpers. |
| `asset_provider/script.rs` (4) | syntax | #5998 / #6001 / #3939: order of gates inside the VM attach, which "needs a script archive". |
| `cell_loader/reference_enable_gate_tests.rs` (4), `references/source_pin_tests.rs` (3), `references/synth_child_tests.rs` (1), `scripted_lock_gate_tests.rs` (1) | device/data | Docs: "Those need Vulkan and game data to drive, so pin the wiring in source." |
| `cell_loader/spawn.rs` (4), `scene/world_setup.rs` (3), `scene.rs` (1), `scene/scene_runtime.rs` (1), `scene/nif_loader.rs` (2), `scene/nif_loader_light_tests.rs` (2), `player_body.rs` (1), `precombined.rs` (1), `precombined_clip_handle_tests.rs` (1), `sky_params_cleanup_tests.rs` (1), `navm_boundary_soak_tests.rs` (1) | device/data | The spawn / install / flush / unload step each pins runs inside a function that takes a `VulkanContext` or a loaded index. |
| `cell_loader/placement_lod.rs` (2), `terrain_lod.rs` (2), `terrain_lod_btr.rs` (2), `unload.rs` (2), `water.rs` (3) | device/data + syntax | Mesh-resolver threading (#2362), lod water render-only (#4500), rebuild reuses the projection (#5289): the spawners need a device and an archive. |
| `parsed_nif_cache.rs` (4) | syntax | #6300: a new `parse_nif` call without a `note_scene_health` beside it. |
| `render/skin_offsets_hasher_tests.rs` (4) | syntax | The spelling of one map type across four files. A type ascription per site would replace it; the sites are `App` and render-collection locals, so it needs a refactor first. Candidate, not done. |
| `save_io/registry_completeness_tests.rs` (4) | syntax | Enumerates every `impl Component` / `impl Resource` in three directories; Rust cannot enumerate impls at runtime. |
| `asset_provider/material/merge.rs` (3) | syntax + size pin | One exported fn; the private arms exist; `merge_bgsm_arm` line count (the line-budget item of #6680). |
| `asset_provider/tests/starfield_mat.rs` (3) | prose + syntax | #4429, #5284: the parked-kinds table in `nifal.md` against `MaterialTextureSet`; the CDB resolve tail has one home. |
| `bench.rs` (3), `bench_camera.rs` (1) | input + prose | The baselines TSV and `capture.sh` against each other (#4417, #5125); `bench_camera`'s round-trip already runs, the tail pins that the applier calls `pose.rotation()` and no `from_rotation_arc`. *read.* |
| `systems/debug.rs` (3), `systems/eat_sleep.rs` (3), `systems/walk_anim.rs` (2), `systems/locomotion.rs` (1), `armor.rs` (1) | syntax | Telemetry reaches every consumer; one definition of `entity_global_form_id`; no game read; AI-package defaults have one home; no game gate in the armor consumer. *"no copy" has no behavioural form.* |
| `systems/script_requests.rs` (2), `systems/vats.rs` (2), `systems/combat_ai.rs` (1), `combat.rs` (1), `ragdoll.rs` (1), `ragdoll_pose_tests.rs` (2), `npc_spawn/resumable/mod.rs` (1), `runtime.rs` (1), `studio_host.rs` (1), `systems/npc_dialogue.rs` (1) | syntax (order, lock order) | Gate before archive lookup, the cell-transition call before dispatch, `PhysicsWorld` acquired between storage passes, a stand-in ragdoll never replaces a decoded one. Lock-order pins are what `BYRO_LOCK_ORDER_CHECK` does not catch per unit test (see pass 2). |
| `systems/water.rs` (2), `systems/particle.rs` (1), `systems/sandbox.rs` (1), `ui_input.rs` (1), `scaleform_hud.rs` (1), `zup_yup.rs` (1) | syntax | "No scratch map is allocated", "every declared parameter is read" (an unused fn parameter draws no warning), the `.clear()` contract, "the cache path dispatches nothing", the push table's arms. `sandbox.rs` states that the missing piece is a full `World` fixture; `zup_yup.rs` that a value test cannot tell the dispatcher from the plain helper at the shipping default. Both are candidates if a fixture / a non-default mode becomes available. |
| `render/env_var_cache_tests.rs` (2), `render/skinned.rs` (1), `render/groundcover_hasher_tests.rs` (1) | syntax | `OnceLock`-cached env reads, LRU refresh before an early return, a hasher spelling in production text. |
| `render/directional_upload_tests.rs` (1), `scene/cloud_tile_scale_tests.rs` (1) | device/shader | Rust against GLSL (`lighting.glsl`, `sky.glsl`). |
| `fog.rs` (2), `loading_screen.rs` (2), `env_translate/climate.rs` (2), `streaming_helpers.rs` (2), `streaming/pre_parse.rs` (1), `npc_spawn/ai_package.rs` (2) | syntax | Where a floor is applied; teardown routes through the canonical path; one default-weather rule; salvage is queued; lock hoisting ("the result is identical either way, so no behavioural test can see it"); one `OnceLock` env read. |
| `cell_loader/exterior.rs` (4), `asset_provider/material/mod.rs` (4), `inventory/mod.rs` (4), `cell_loader/index.rs` (2) | input | `SOURCES` / `production_source` concat lists. |
| `commands/mod.rs` (1), `asset_provider/tests/bgsm_merge.rs` (1), `cell_loader/nif_import_registry.rs` (2) | prose | Command docs against the registry; a deferral marker at two sites; `nifal.md`'s converter list against the chain. |
| `commands/scene.rs` (1) | input | Reads the `Material` declaration to list its `f32` fields, then **runs** `mat.set` on each. Moving the list to serde would carry the NaN override sentinels as `null`; left. *read.* |
| `commands/terrain.rs` (1), `extensions/install.rs` (1) | runtime read | A file the code under test writes / reads; not a source scan. |
| `sf_smoke.rs` (2) | prose + syntax | Report text; `tally_pack_ins` already has a value test. *read.* |
| `systems/combat_anim.rs` (1), `asset_provider/texture.rs` (2), `asset_provider/tests/archive_precedence.rs` (1) | syntax | Fixture paths against the loader; swap acquires per mesh; a directory walk so "the provider nobody wrote down" is covered. |

**Still open.** Two candidates are named above (`skin_offsets_hasher_tests`, and the `sandbox` / `zup_yup` pair that
wait on a fixture). Nearly every file is classed by doc, not by reading; if a doc is wrong, the class is wrong.
