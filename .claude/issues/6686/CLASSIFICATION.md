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
