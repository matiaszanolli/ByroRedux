# #6688 — standalone source scans in `crates/scripting`, classified

Counted by script over `crates/scripting/src` (the `SOURCES` concat rows in
`fragment.rs`, `translate/effects.rs` and `obscript_effects/mod.rs` are left
alone, as the issue says). 38 sites read a Rust file outside a concat row; the
rest read docs or `.psc` fixtures, which are data, not scans of code.

Classes: **device/data** (needs Vulkan or game data), **runtime** (replaceable by
a check that runs the code), **prose** (pins what a doc or doc comment says),
**syntax** (pins the shape of source that has no run-time observable).

| Site(s) | Test | Class | Disposition |
|---|---|---|---|
| `cleanup.rs` 185-283 (21 rows) + 298 | `every_documented_pattern_b_marker_drains_in_its_named_consumer` | **runtime** | **Replaced.** Each scripting-crate consumer is run on a world holding its marker (`cleanup.rs`); `OnResetEvent`'s private consumer is run in `obscript_objects.rs`; the eight binary request systems are run in `byroredux/src/systems/request_drain_tests.rs`. The "drained by both" half is `pattern_b_markers_survive_event_cleanup`, which runs `event_cleanup_system`. The old scan was also weak: `vm_state`'s row matched a producer's `query_mut::<TwoStateTransitionBatch>` insert as well as the drain. |
| `cleanup.rs` 148 | `every_drained_marker_is_a_documented_pattern_a_marker` | prose | Kept. It compares the module doc's Pattern-A list to the drain list; there is nothing to run. |
| `condition.rs` 193 (+ two docs) | `catalog_count_doc_pin_tests` | prose | Kept. Pins the figures in `docs/` and the module doc table against `CATALOG.len()`. |
| `condition/tests.rs` 735 | `get_distance_variant_keeps_its_doc_comment` | prose | Kept. rustdoc attaches a doc to the next item; the only observable is the doc text. |
| `condition/tests.rs` 1307 | `get_actor_value_does_not_hold_actor_values_across_ruleset` | syntax | Kept. The lock-order graph is opt-in (`BYRO_LOCK_ORDER_CHECK`) and process-wide, so it cannot be switched on inside one unit test without tripping every parallel test; the test already runs the detector leg when the variable is set. |
| `events.rs` 361, 381 (+ three docs) | `every_event_type_the_engine_docs_name_still_exists` | prose | Kept. Docs ground truth (#3950). |
| `fragment.rs` 192 | `every_production_file_is_in_the_sources_concat` | syntax | Kept. Completeness of the `SOURCES` list itself, which is out of scope. |
| `translate/effects.rs` 133 | `every_production_file_is_in_the_sources_concat` | syntax | Kept, same reason. |
| `fragment/tests.rs` 4647, 4657 | `the_nested_lock_residual_list_names_every_type_apply_effect_acquires` | prose | Kept. It pins a doc inventory against `apply_effect`'s acquisitions. It finds those with a run-time-composed turbofish needle, the pattern #6685 replaced in the binary crate. A syntax-tree scan here needs `syn` as a dev-dependency of `byroredux-scripting`; not added without approval. |
| `obscript_objects.rs` 885 | `collect_due_does_not_rebuild_a_stub_set_or_use_siphash` | syntax | Kept. Pins hasher types and the absence of a per-frame set; neither is observable without an allocation probe. |
| `story_manager.rs` 1411 | `phase_b_never_takes_the_node_state_under_the_stages_guard` | syntax | Kept. A lock-shape pin the detector cannot see: the CI lane never installs the node state, so no enabled run drives the path (#6003). |
| `translate/mod.rs` 414, 461, 485 | the three panic-net placement tests | syntax | Kept. The test docs state why: no hand-built `.pex` drives the unaudited arms into a panic. |
| `translate/tables.rs` 113 | `the_recognizer_chain_routes_event_names_through_this_table` | syntax | Kept. Inline and table spellings behave identically, so only the source differs. |

Class totals for the 38 sites: runtime 22 (replaced), prose 7, syntax 9
(of which two are the out-of-scope `SOURCES` completeness guards). No device or
game-data scans exist in this crate.

No guard was removed without a replacement.
