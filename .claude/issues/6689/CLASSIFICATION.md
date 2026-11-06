# #6689 — standalone source scans in `nif`, `plugin`, `physics`, classified

Counted by script over each crate's `src/`: 64 `.rs` reads, none in a `SOURCES`
concat row. Two are function-size pins (`nif/src/anim/function_size_pin_tests.rs`,
`physics/src/function_size_pin_tests.rs`), which the issue excludes, leaving 62.
Every test was read; those that state their reason for being a source scan are
quoted by it.

Classes: **device/data**, **runtime** (replaceable by running the code),
**prose** (pins what a doc or doc comment says), **syntax** (pins source shape
with no run-time observable). Totals: runtime 2 (replaced, below), prose 13,
syntax 47, device/data 0.

## nif (23 sites)

| Site(s) | Test | Class | Why it stays |
|---|---|---|---|
| `blocks/collision/constraints.rs` 1137 | `physal_seam_doc_tests` | prose | `physal.md` and the file's docstrings must name the same constraint inventory. |
| `blocks/properties_tests.rs` 594 | `apply_mode_gate_is_its_own_constant_…` | syntax | The test doc: the two constants share a value, so reuse is "behaviorally invisible". |
| `blocks/properties_tests.rs` 1367 | `apply_mode_doc_records_the_unconsumed_value_measurement` | prose | The measured histogram in a field doc. |
| `blocks/shader_tests/starfield.rs` 475 (+6) | `every_tail_capturing_block_reports_it_and_parse_nif_records_it` | syntax | Declarations vs overrides must balance per file; the alternative, a hard-coded type list, "is precisely the shape that goes stale". |
| `import/collision/mod.rs` 801, 825 | `dispatched_shape_structs` / `resolved_shape_structs` | syntax | Dispatch-to-resolve parity; has caught real gaps. A behavioural form needs a binary fixture per block type, or a data-driven dispatch table. |
| `import/walk/lights.rs` 257, 300 | `dispatched_light_structs` / `resolved_light_structs` | syntax | Same parity, for lights. |
| `import/collision/ragdoll.rs` 1661 | `every_ragdoll_drop_site_still_logs_its_reason` | syntax | The test doc: asserting the log line needs a log-capture crate, a new dependency. |
| `import/mesh/sse_skin_index_space_tests.rs` 107 | `no_remap_contract_docs_decode_fn_not_payload_alias` | prose | Which item a doc comment attaches to. |
| `import/tests/app_culled_visibility.rs` 159 (+3) | `the_shape_cull_gate_is_spelled_once`, `the_satellite_walkers_use_the_one_node_gate` | syntax | Counts how many places spell the cull test; the property is call structure. |
| `import/types/material.rs` 268 (+1) | `dark_role_has_one_writer_and_carries_its_census_doc` | prose (doc half), syntax (one-writer half) | Census text; one producer site. |
| `import/walk/mod.rs` 1092 | `the_scene_graph_walkers_never_call_a_satellite_walker` | syntax | Its doc: "a behavioural test would pass just as happily with the call present". |

## plugin (20 sites)

| Site(s) | Test | Class | Why it stays |
|---|---|---|---|
| `equip.rs` 1269 | `fo76_and_starfield_arms_are_marked_provisional` | prose | Pins the PROVISIONAL marking of an unsourced arm; its behaviour is pinned by the sibling `fo76_and_starfield_currently_follow_fo4`. |
| `esm/cell/mod.rs` 77, 156, 182 | `plugin_loading_doc_pin_tests` (three) | prose | Docs name the real struct fields and the real dispatch split. |
| `esm/cell/tests/txst.rs` 575 | `decal_data_doc_does_not_claim_a_nonexistent_m28_decal_milestone` | prose | A stale claim in a doc. |
| `esm/records/index.rs` 2114 | `every_index_map_is_a_category_or_a_recorded_exclusion` | syntax | Reads the `EsmIndex` field list against `categories()` rows; both are declarations. |
| `esm/records/misc/quest.rs` 2160 | `alias_flag_roster_covers_every_declared_constant` | syntax | Counts `pub const ALIAS_FLAG_` lines against a hand-kept roster that also carries independent value pins; a macro making the roster would lose them. |
| `esm/records/tests.rs` 3277 (+11) | `remap_fid_has_exactly_one_definition` | syntax | Searches twelve files for a second `fn remap_fid`. |
| `esm/records/tests.rs` 3313 | `dispatch_handled_fourccs_matches_the_live_dispatch_arms` | syntax | Parity of a tool's const with the dispatch `match` arms. |

## physics (19 sites)

| Site(s) | Test | Class | Disposition |
|---|---|---|---|
| `sync.rs` 2621, 2652 | `profile_flags_are_read_once_per_process_not_per_tick`, `profile_accessors_read_their_own_variables` | **runtime** | **Replaced.** Both flags now go through `env_flag_once`, and `an_env_flag_is_read_once_and_then_cached` runs it: first lookup reads, a later change is not seen, a fresh cell reads again. The `OnceLock`-in-source half and the variable-name half are gone. What is left of the first test is the call-structure pin (the tick system never calls `env::var`). |
| `sync.rs` 2562, 2600 | module-doc tests | prose | The module doc names the buoyancy phase and its ordering; the doc's wording about the NIF opt-out. |
| `sync.rs` 2671, 2716 | `register_newcomers_parallel_section_holds_no_world_guard`, `physics_diagnostics_resolve_forms_after_storage_guards_drop` | syntax | Lock shape. The lock-order graph is opt-in and process-wide, and "a pool thread blocking on a lock the caller holds is invisible to it" (the test's own doc). |
| `water.rs` 1635 | `cpu_wave_phase_a_travels_along_dir_a_in_lockstep_with_the_shader` | syntax | The CPU crest sign must match `water.vert`; the shader cannot be run here, and the CPU height mixes wave B in. |
| `water.rs` 4420 | `wet_link_angular_drag_uses_the_shared_inertia_torque` | syntax | The dry-link decay is run by the test above it; the two wet arms need a surface fixture and a marker fixture each. |
| `water.rs` 4571 (+1) | `water_scan_latches_are_not_siphash` | syntax | Hasher types and the absence of an O(n) search. |
| `world/mod.rs` 1842 | capsule-sharing test | syntax | The behavioural half already runs; the scan covers three files for a per-call `SharedShape::capsule_y`. |
| `world/mod.rs` 3714, 3825 | step-cost and kinematic-count rationale tests | prose | Doc consistency; the measured behaviour has its own test. |
| `world/mod.rs` 3758 (+1) | `step` scan guards | syntax | "neither claim is observable from behaviour" (test doc). |
| `world/mod.rs` 4042 | `buoyancy_applies_forces_through_the_public_wrappers` | syntax | Routing through `pw.add_force`; the raw-body path has the same visible effect. |
| `world/mod.rs` 5657 (+2) | `wake_contract_tests` | syntax | Test doc: "Neither is observable from behaviour". |

No guard was removed without a replacement.
