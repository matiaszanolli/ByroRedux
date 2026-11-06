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
