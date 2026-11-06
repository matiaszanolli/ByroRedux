# #6687 — standalone source scans in `byroredux-renderer`, first pass

**Scope of this pass:** the five files the issue names (131 of 445 sites) plus
the three tests the issue samples. The other ~314 sites are **not yet
classified**. Counts come from a script over `crates/renderer/src`.

Classes: **device/shader** (GLSL-vs-Rust contract, barrier or submit order, a
Vulkan error path: no device-free alternative), **runtime**, **prose**,
**syntax**. "Read" = I read the body; "by doc/name" = classified from the test's
name and doc comment only, **not verified assert by assert**.

## Replaced (3)

| Site | Test | Replacement |
|---|---|---|
| `vulkan/texture.rs` 1805 | `dds_payload_check_is_an_error_not_a_release_assert` | The gate is now `check_dds_payload`, a pure function called with the region walk's total; `a_truncated_dds_payload_is_a_returned_error_not_a_panic` runs it on short, exact and long payloads. Call order is unchanged. The scan-ratchet baseline for `texture.rs` drops 2 → 1. |
| `vulkan/scene_buffer/gpu_instance_layout_tests.rs` | `gpu_instance_does_not_re_expand_with_per_material_fields` | An exhaustive destructure of `GpuInstance` (no `..`): adding, removing or renaming any field, including a per-material field written over a padding word, stops the test build. It replaces a search for 45 banned names. |
| `vulkan/scene_buffer/shader_contract_tests.rs` 3188 | `hash_gpu_material_fields_covers_every_gpu_material_field` | The trailing search for `h.write(mat.as_bytes());` is deleted. The byte-flip loop above it already proves every byte is hashed, and a hash that drifted back to a field list fails there. |

## Classified (read)

| Site | Test | Class | Why |
|---|---|---|---|
| `acceleration/tests/predicates_tests.rs` 1220 | `ensure_tlas_state_consumes_the_shrink_request_past_the_commit_point` | device/shader | Order of a flag clear against a fallible Vulkan sequence; a pure helper would not enforce where it is called. |
| `texture.rs` 1735 | `staging_overrun_is_a_release_error` | syntax (order half) | The `Result` half already runs; the scan pins that the check precedes the first recorded barrier. |
| `texture.rs` 1927 | `rgba_overwrite_validates_against_creation_extent` | syntax | The pure gate (`validate_rgba_upload`) is tested; this pins that `can_update_rgba` compares against the stored extent, which needs a `Texture` (a device). |
| `gpu_instance_layout_tests.rs` | `the_doc_names_the_file_that_holds_the_per_frame_eviction_call` | prose | A doc names a file. |

## Classified by doc/name only

| File (sites) | Tests | Class |
|---|---|---|
| `scene_buffer/shader_contract_tests.rs` (38, minus 1 replaced) | discriminant, probe, water, material, light, instance, camera, terrain GLSL-vs-Rust lockstep (about 24 sites); descriptor-table and pipeline docs (5); hand-typed-literal and byte-view hashing call-structure scans (about 7) | device/shader (the lockstep ones: the issue says they stay), prose, syntax |
| `sync.rs` (24) | `render_finished_is_sized_and_indexed_per_swapchain_image`, `frames_in_flight_contract_names_every_dependent_resource` (20 sites), `the_all_slots_wait_argument_is_pinned` | device/shader, prose (the contract list), device/shader |
| `svgf.rs` (24) | param-w doc, infallible-record and no-`?`-after-latch call-structure pins, `destroy_nulls_every_scalar_handle_after_destroying_it` (11), anchors | prose, syntax, device/shader |
| `texture.rs` (22 remaining) | upload-orchestrator failure classes, device-loss disposal, queue-guard and fence order, command-buffer free on every error path, DDS setup unwind, staging release label, the initialize-layouts submit family (7) and its in-flight check (5) | device/shader (Vulkan error and submit paths), syntax |
| `context/post_passes.rs` (22) | every record helper and frame recorder is documented (10), cloud-column caustic transmittance, skip-clear latch, TAA/SVGF failure latching, the retired rebind, the post-bloom tap, GPU-timer bracketing | prose, syntax |

**Candidates for pure-function extraction next** (decisions inside
`post_passes.rs`, by name): `caustic_sun_transmittance_applies_the_cloud_column_only_outdoors`,
`record_bloom_pass_skips_raw_correctness_views_before_dispatch`,
`record_volumetrics_pass_routes_skip_clears_through_the_shared_latch`. Each is
a decision that `acceleration/predicates.rs` shows can be a tested function;
moving them must not change call order, so they need the `BYRO_VALIDATION=1`
check before they land.
