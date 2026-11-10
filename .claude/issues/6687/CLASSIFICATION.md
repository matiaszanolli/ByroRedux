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
| `acceleration/tests/blas_static_tests.rs` 587 | `the_doc_names_the_file_that_holds_the_per_frame_eviction_call` | prose | Read. `memory-budget.md` must name the file holding the per-frame `evict_unused_blas(` call; the pointer is the whole claim. |

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

## Correction to "Candidates for pure-function extraction next" (2026-11-06)

I read the three named tests' bodies. **None is a candidate: each decision is already a pure function,
and the scan pins only that the recorder calls it.**

| Test | Pure function already in place | What the scan still pins | Class |
|---|---|---|---|
| `caustic_sun_transmittance_applies_the_cloud_column_only_outdoors` | `caustic_sun_transmittance(exterior, column)`, asserted by value in the same test | the exact `record_caustic_splat_pass(…, sky_params.is_exterior)` argument at the call site | device/shader |
| `record_bloom_pass_skips_raw_correctness_views_before_dispatch` | `render_debug_requires_raw_output(flags, mode)` | the call precedes `bloom.dispatch(` and a `return;` sits between | device/shader (order) |
| `record_volumetrics_pass_routes_skip_clears_through_the_shared_latch` | `skip_clear_decision(ran, cleared)` | the call is made with that latch field, and `record_neutral_frame` has one call site | device/shader (order) |

So the earlier pass's claim that these three can lose their scans by extraction was wrong. Their remaining
scans pin call wiring inside `&mut VulkanContext` recorders, which no device-free test reaches. #6687 stays
open: ~314 sites are still unclassified and the no-extraction finding above narrows where to look. The next
useful reads are the `svgf.rs` and `sync.rs` call-structure pins, in that order.

## Second pass — every remaining Rust-source scan (2026-11-10)

**Census.** Counting only reads of Rust files (`include_str!("….rs")` and `read_to_string`), outside the five files
the first pass took, there are **352 lines in 64 files** after this pass (354 before it: two sites are
removed by the replacements below). The issue's 445 counts every `include_str!` form and the five named files, so the two figures are
not comparable. **Method:** each site's enclosing test and its doc comment were listed by script; the doc states the
reason for almost all of them. Bodies were **read** only where marked *read*; everything else is classified by
doc/name and was not verified assert by assert, so a wrong doc means a wrong class.

The issue's own rule holds across the board: shader-contract, barrier-order, destroy-order and submit-order scans have
no device-free alternative and stay. This pass looked for the host-logic minority.

### Replaced (3 tests)

| Test | Was | Now | Injected fault that fails it |
|---|---|---|---|
| `context/build_and_upload_instances.rs::ui_instance_idx_is_clamped_to_none_past_max_instances` | searched the function's text for `(idx < MAX_INSTANCES).then_some(idx as u32)` and for the absence of the old capture | `ui_instance_candidate(pushed_at)` is a pure function; the test calls it at `0`, `MAX-1`, `MAX`, `MAX+7` | `<` to `<=` fails the `MAX` case |
| `…::ui_instance_idx_is_reclamped_to_the_post_grow_slot_capacity` (value half) | searched for the `.filter(|idx| *idx < instance_capacity)` expression | `ui_instance_within_capacity(candidate, capacity)`; `9/10`, `10/10`, `11/10`, `None` | `<` to `<=` fails `10/10` |
| `acceleration/tests/blas_static_tests.rs::skinned_blas_stays_fx_hashed` | searched `acceleration/mod.rs` for two spellings | an accessor returning `&FxHashMap<EntityId, BlasEntry>`; the hasher is part of the type | `skinned_blas` as a std `HashMap` stops the test module (and the field's own users) compiling |

The order half of #4722 stays a scan (`ui_instance_idx_final_clamp_follows_the_grow_and_the_capacity_read`): a value
test cannot see where a function is called. The self-scan ratchet baseline for `build_and_upload_instances.rs` drops
7 to 6. No call order, barrier or timer bracket moved in this pass, so no `BYRO_VALIDATION=1` run was needed for it.

### Classified, by file

| File (sites) | Class | Basis |
|---|---|---|
| `context/build_and_upload_instances.rs` (22) | device/shader + syntax | Scratch is persistent, not reserved to draw count (#3675, #6485); flag assembly above the `skip_batch` arm (#3978); the rigid-history latch is order-independent (#4007); grow before upload, TLAS map after the grow and capped (#4833, #6494). All about the body of a `&mut VulkanContext` method. |
| `shader_constants.rs` (21) | device/shader + prose | The generated header against `shader_constants_data.rs` (#1860); dispatch grids and reductions derive from the generated workgroup size (#2768, #5620); temporal history indexing uses `(f + N - 1) % N` (#2771). Rust against GLSL. |
| `acceleration/tests/` (30: `blas_static_tests` 16, `scratch_tests` 9, `predicates_tests` 3, `tlas_tests` 2) | device/shader + prose | Budget checks in the compaction and admission loops, scratch-serialise barriers before the first build, the AS-build to ray-query barrier on both `build_tlas` arms; docstrings naming commands and files (#3997). |
| `context/skinned_blas_refit.rs` (14) | device/shader | Phase order of the recorder, barrier bracket, built-entity marking, eviction drains outside the `(skin_compute, accel_manager)` guard (#3374). This recorder is the subject of #6708. |
| `caustic.rs` (13) | device/shader | Upload scale against `composite.frag` (#2776); accumulator clears go through one helper and name TRANSFER on both sides; `destroy()` nulls handles. |
| `context/draw.rs` (13) | device/shader (submit order) | Latches promoted after submit, not at record time (#5956, #6811, #6291); guard before acquire (#4604); resets at the top of `draw_frame`; file-size pins. |
| `buffer.rs` (12), `image.rs` (2), `taa.rs` (2) | device/shader | Teardown arms null handles and release without a label; the staging pool releases its allocator (#4187); readback location (#2752). All need a device to construct. |
| `context/helpers.rs` (12) | prose + device/shader | Error pointers name the file that holds the `log::error!`; the G-buffer doc against the render pass's depth final layout; capture staging frees share one helper (#4039). |
| `skin_compute.rs` (11), `morph_compute.rs` (2), `context/dispatch_skin_and_cluster.rs` (8) | device/shader | BLAS stride, slot-stamp sentinel, descriptor-write cache, pipeline bound once per batch (a type token covers the other half, #4205), AS-write to AS-read barrier at frame scope (#4179), compute passes take the build-gated TLAS (#4779). |
| `frame_upscaler.rs` (10) | device/shader + prose | FSR teardown outside the allocator guard (#2158); `# Safety` sections on six `unsafe fn` (#2684); every native blit takes the actual source layout (#4592). |
| Pass files (71): `sky_cube` 10, `volumetrics` 8, `bloom` 7, `volumetrics/noise` 6, `composite` 5, `shadow_filter` 4, `water` 4, `groundcover_models` 4, `presentation` 4, `groundcover/frame` 3, and `local_grid`, `restir`, `sky_dome`, `gbuffer`, `egui_pass`, `cloud_noise` 2 each, `tonemap`, `exposure_meter`, `groundcover/construct`, `volumetrics/init` 1 each | device/shader | Each pins a GLSL structure, a push-constant or UBO field order against Rust, a barrier or stage pair, or a pass's failure latch. The Rust-mirror tests in this set (`tonemap`, `water`) already run their mirror; the scan is what checks the mirror still describes the shader. |
| `context/` recorders and teardown (40): `resources` 9, `function_size_pin_tests` 7, `resize` 6, `geometry_pass` 5, `shrink_frame_scratch` 4, `sync_and_acquire_frame` 3, `depth_capture` 3, `teardown` 2, `render_debug` 1 | device/shader + size pin | Static BLAS recovery bounded per frame and honouring the deadline; readback invalidation before reading; drain order in teardown; the line-budget pins. |
| Registries, descriptors, meshes (28): `descriptors` 9, `texture_registry/dynamic_rgba` 7, `texture_registry/mod` 4, `texture_registry_tests` 4, `mesh` 2, `pipeline` 1, `pool_growth_cap_tests` 1 | device/shader + prose | Flush skips released slots; descriptor sets are allocated before the old pool dies (#4886); pool sizes derive from layout bindings; the pool-growth size gate runs before the duplicate allocation. |
| Device, allocator, timers, buffers (25): `device` 8, `gpu_timers` 5, `allocator` 3, `destroy_coverage_tests` 3, `light_history` 2, and `gpu_instance_layout_tests`, `light_overflow_tests`, `scene_buffer/upload`, `source_scan` 1 each | syntax + prose | Bounded `CStr` readers (#5273); required features against unconditional enables (#4895); allocator-lock acquires confined to helpers; the timer doc table against `QUERIES_PER_FRAME` and the `TimerPass` rows; `light_history.rs` / `upload.rs` hold no std map or `DefaultHasher` (a negative text search); the ratchet itself. |
| `context/mod.rs` (8), `material_tests.rs` (8) | syntax + prose | Fx hashing of `pose_dirty` and ten `VulkanContext` fields; `fill_scratch_telemetry` covers every scratch field; no stale `GpuMaterial` size in prose. |

Rows sum to 352.

### Candidates that are not done

- **The remaining Fx-hash cluster in `context/mod.rs`** (`pose_dirty_crosses_the_crate_boundary_without_siphash`, the
  ten `VulkanContext` field spellings, `skin.slots` / `morph_slots`): each is an `FxHashMap` / `FxHashSet` in a declared
  type, so each could be an accessor ascription like the `skinned_blas` one above. The fields sit in several modules
  and `skin_slot_pool` is in `core`, so this is a dozen small edits, not one.
- **`shadow_filter.rs::bytes_per_pixel_matches_frame_images`** counts `: GpuImage,` fields in a struct; an exhaustive
  destructure would do it without constructing one.
- The three decisions the first pass named (`caustic_sun_transmittance…`, `record_bloom_pass_skips…`,
  `record_volumetrics_pass_routes…`) are already pure; nothing new there.

#6687 stays open for the Fx-hash cluster and the destructure; everything else in the inventory is classed, and none of
the 352 remaining scans has a device-free form I could find in the docs or in the bodies read.
