**HEAD**: `9409e7b20` · **Baseline**: `docs/audits/AUDIT_RENDERER_2026-10-31.md` (HEAD `670f81917`; 375 commits since, 97 under the renderer-adjacent paths, 156 files / +18 045 / −8 210 under `crates/renderer`, `byroredux/src/render`, `crates/core/src/lighting.rs`, `crates/fsr3-sys`) · **Audited**: all 12 dimensions, delta-first (Dims 1–8, 10, 6+12 by one agent each; Dims 9 and 11 by me, inline, because their delta is rename-only and one config struct)

# Renderer Audit — 2026-11-07 (delta)

## Executive Summary

| Severity | NEW | Existing | Findings |
|---|---:|---:|---|
| CRITICAL | 0 | 0 | — |
| HIGH | 0 | 0 | — |
| MEDIUM | 2 | 0 | REN-D5-01, REN-D5-02 (both M39 texture streaming, default OFF) |
| LOW | 17 | 0 | 12 doc-rot, 3 test-gap, 1 telemetry, 1 hot-path allocation |

- **The baseline's open findings are gone.** Verified in code, not from commit titles: #6493 (one overflow warn per episode), #6494 (order pins scoped to function bodies, with non-vacuity asserts), #6495 and #6496 (focus lattices tile with `RENDER_ORIGIN_SNAP`; every `depthTex` read in `composite.frag` is a `texelFetch`), #6506 / #6521 (`SceneBuffers::destroy` walks `moved_surface_buffers`, pinned by `destroy_walks_every_per_frame_buffer_family`). #6486 is half-fixed (the per-pixel `inverse()` is gone; the report's other half stays open).
- **The two MEDIUMs share one root: M39 residency code was written against well-formed input and a single change per handle.** REN-D5-01 is a panic on a truncated BC texture; I re-traced it (see the finding). REN-D5-02 is a same-batch double change whose GPU consequence is unconfirmed.
- **Nothing in the unsafe-to-ship class.** No struct drifted from its shader mirror (`GpuCamera` 368 B, `GpuMaterial` 432 B, `GpuInstance`/`GpuLight`/`GpuTerrainTile` unchanged, `CompositeParams` gained `view_proj` with its field-order test green). No barrier, layout or teardown regression was found in the three large refactors (#6708 / #6709 / #6710 splits, #6697–#6700 sub-struct grouping, #6371 `destroy_pass_objects`).
- **Guards**: renderer lib **1616 passed, 0 failed, 1 ignored** (the device-only `sky_cube` filter test, outside any dimension); every named Guard in each dimension exists, is not `#[ignore]`d and is not vacuous (spot-read by the dimension agents). `scripts/check-shader-artifacts.sh`: 40 shaders + the early-test variant match glslang byte for byte. Dim 9/11 filters I ran myself: `palette` 16, `skin` 93, `exposure` 14, `tonemap` 16, `upscal` 30, `morph` 20, `fsr3-sys` 8, bin `renderer_config_defaults_to_fsr_quality` 1, all green.
- **Scope note.** No engine launch and no `BYRO_VALIDATION=1` run (read-only audit); everything GPU-only is under *Needs RenderDoc*.

## RT Pipeline Assessment

- **AS / SSBO indexing**: clean. The #6709 `build_blas_batched` and #6708 `record_skinned_blas_refit` splits keep statement order and every error / cleanup path (`unwind_prepared`, the #5201 may-be-in-flight leak-vs-destroy rule, `alloc_compact` rollback, scratch allocated before the old one retires, the #4177 scratch barrier). The #6485 moved-surface scratch is balanced (no early return between take and hand-back).
- **Ray queries / ReSTIR / GI (M-LIGHT)**: the GLSL matches the checklist. Ray-origin floor and its sign (#6607), RLT-SHD-012 cosine weight (cap of 4 never binds), penumbra width packed into `directUnshadowed.y`, `pathHitEmitterBsdf` π scaling at both `pathHitRadiance` sites, `GI_SAMPLE_LUMINANCE_CLAMP` = π, SSAO multiplying only the untraced share, the GI gate at its Phase 1b form (RLT-GI-012 deferred), `renderDebug.z` bits 1–2 and `MAT_FLAG_LAMBERTIAN_REFERENCE` (bit 24) inert unless set, ReSTIR substrate/plane gates untouched. Only comment rot (REN-D2-01/02).
- **Denoiser / composite**: both `LOCAL_FIRE_SUNLIT` composite variants are valid and `composite_frag_spv_matches_recompiled_branch_count` (161) passes; the "temporarily disable sun radiance" commit was followed by its restore. Shadow-filter reach follows the penumbra (2/2/3/4 iterations by tier, 9 of 9 tests). Two CPU-decides/GPU-consumes agreements are unpinned (REN-D7-02).
- **Frame recording**: the #6710 `record_geometry_pass` split was compared phase by phase with `d66db4358^`: record order, dynamic state, the state carried across batches and the render-pass begin/end bracket with its timers are unchanged. #6608's loading-stage path takes the ground-cover "off" route; the temporary GPU checkpoint code from `0ee0090a2` is fully removed.

## GPU-Struct & Memory Assessment

- **Layout**: no field or size change to any size-pinned struct. New hand-mirrored block `GpuLocalGridParams` (144 B) matches its GLSL by hand but has no guard (REN-D3-01); the local grid's set-1 layouts are checked against SPIR-V only at device boot (REN-D8-04).
- **Teardown**: orderings intact after #6697–#6700 (`skin.slots` before `skin.compute`; `frame_upscaler::destroy_allocations` after `destroy_device_objects`; `exposure` before the allocator `Arc::try_unwrap`). `destroy_pass_objects` keeps the destroy order in all four migrated passes and adds a null guard to SSAO. The local combustion grid, scene-linear capture staging, #6624 cube fallback (engine-owned, ref-pinned `u32::MAX`) and deferred descriptor replay against the live entry (`23f4aeee3`) are all sound.
- **M39 residency**: the new-image / GPU-copy-of-shared-levels / old-image-through-deferred-destroy path, the budget derivation and the eviction / decline rules match the skill text and are guarded; the two hazards found are REN-D5-01 (untrusted input) and REN-D5-02 (batching).
- **Ledger**: `memory-budget.md` understates the local grid (REN-D8-01) and has no row for capture staging (REN-D5-03).

## Findings

### REN-D5-01: a truncated BC DDS panics the floor-first upload path when streaming is on; the size check comes after the slice
- **Severity**: MEDIUM
- **Dimension**: Memory/Lifecycle
- **Location**: `TextureRegistry::flush_upload_batch` (`crates/renderer/src/texture_registry/upload.rs`), `dds::upload_byte_range` / `dds::level_byte_range_in` / `dds::payload_first_level` (`crates/renderer/src/vulkan/dds.rs`)
- **Status**: NEW (CONFIRMED by code trace; not run on a device)
- **Description**: With `render.texture_streaming` on, a 2D chain above `FLOOR_EDGE_PX` takes the floor-first branch. `payload_first_level` returns 0 for any payload that is shorter than the chain and not an exact level suffix, so `floor_first` is `Some(floor)` with `payload_first == 0`. `upload_byte_range(&meta, first_mip > 0, 0, len)` then returns `level_byte_range_in(..)`, whose end is the whole chain's byte total, not clamped to `pixel_data.len()`. `flush_upload_batch` slices `&pixel_data[levels.clone()]` (for `average_rgb_levels`) and `&pixel_data[levels]` (for `record_dds_upload`) with it. The residency-change arm slices `&pixel_data[level_byte_range_in(..)]` the same way. Both slices run before `check_dds_payload` can return the #4511 error.
- **Evidence**: `parse_dds` does not validate payload length for BC formats (only the expand path refuses short data), and the existing test `a_truncated_dds_payload_is_a_returned_error_not_a_panic` (`vulkan/texture.rs`) exercises `check_dds_payload`, not the registry slice. The `payload_first_level` doc itself says a truncated file "still fails the upload's size check", which holds only if nothing slices first.
- **Impact**: A corrupt or mod-authored BC1/BC3/BC5 texture (>256 px, multi-mip, short payload) panics the render thread inside the one-time-commands path (holding the transfer-fence mutex) instead of falling back to the checkerboard. Streaming is OFF by default today, but Phase 4 plans default-on (#6631), and `dds.rs` parses untrusted archive data.
- **Suggested Fix**: Reject (log + `continue`, as the parse-failure arm does) when `levels.end > pixel_data.len()`, in both the floor-first and the residency-change arms, or have `upload_byte_range` / `level_byte_range_in` return `Option`. Add a device-free test: a 512×512 DXT1 header with a few-byte payload must not panic the slicing step (factor the range check into a pure fn).
- **Completeness**: SIBLING (other `pixel_data[..]` slices in `texture_registry/`), TESTS.

### REN-D5-02: two residency changes for one handle can land in one flush batch; the `Residency::apply` refusal is ignored
- **Severity**: MEDIUM (GPU consequence unconfirmed; accounting skew is certain on the described path)
- **Dimension**: Memory/Lifecycle
- **Location**: `flush_upload_batch` install loop (`residency.apply(change);` result dropped, then `entry.texture.replace(texture)`), `queue_residency_change`, `Residency::apply` (`texture_registry/`)
- **Status**: NEW
- **Description**: `queue_residency_change` plans against the entry's current residency, not against changes already in `pending_promotions`. `apply_wanted` avoids it for its own candidates, but a promotion whose host read was already in flight (`TextureStreamer::in_flight`) lands through `queue_promotion_bytes` regardless of a demotion queued for that handle meanwhile. A budget drop queues many demotions (`spend_budget` sweep 2 demotes any texture above its floor that is not `locked`; `locked` only means "change already queued"), so the overlap is reachable.
- **Evidence**: no dedupe in `queue_residency_change`; the stale guard (`current.first_resident == change.old_first`) is in the record loop only, so a second change in the same batch passes it; `residency.apply(change)` is a statement and `landed_changes.push` runs unconditionally. Across batches the stale check works. Test `a_stale_change_is_refused` pins `apply` itself, not the install loop.
- **Impact**: wrong budget / telemetry accounting at minimum. A later change is recorded with `old_levels` computed from a residency that no longer matches the image, so a barrier / copy subresource can be out of range (a spec violation `BYRO_VALIDATION=1` would flag).
- **Suggested Fix**: in the record loop keep a small set of handles already staged this batch and treat a second change for the same handle as stale; in the install loop, if `apply` refuses, retire the new texture through `pending_destroy` instead of installing it. Optionally refuse a handle with a queued change in `queue_residency_change`.
- **Needs**: a `BYRO_VALIDATION=1` run with streaming on and a shrinking `render.texture_budget_mib` while promotions are in flight.

### REN-D1-01: stale line-number and phase-number references in `blas_static.rs` comments after the #6709 split
- **Severity**: LOW (doc-rot)
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs`, `build_blas_batched` (alloc_compact rollback comment) and `prepare_static_blas_batch` (geometry SAFETY comment)
- **Status**: NEW
- **Description**: The rollback comment says "Mirrors the build/copy-phase cleanup pattern at lines 733-745 / 815-832"; the SAFETY comment cites "`PreparedBlas::geometry` (line ~610)" and "the Phase 2 batched build submission" (the build submission is Phase 4, `record_batch_builds`; Phase 2 is `ensure_batch_scratch`).
- **Evidence**: `grep -n "lines 733\|line ~610\|Phase 2 batched" blas_static.rs` hits all three (re-run by me).
- **Impact**: none at runtime; it misleads a reader auditing the `'static` geometry claim (SAFE-21), the one invariant that leans on that comment.
- **Suggested Fix**: name the symbols (`the copy_result failure arm below`, `PreparedBlas::geometry`) and say Phase 4.

### REN-D2-01: "only sample 0 runs the detailed trace" is stale after PAF-001
- **Severity**: LOW (doc-rot)
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/shaders/triangle.frag` (ReSTIR finalize "Mover taint" comment block) and the same sentence in `.claude/commands/audit-renderer/SKILL.md` Dim 2 ("Documented hole")
- **Status**: NEW
- **Description**: Samples `sr > 0` now also call `traceLightTransmittanceDetailed` (for `sampleHitDistance`, the penumbra estimate) but discard `sampleMask` / `sampleInstance`, so the mover-taint hole is unchanged; the stated reason is no longer true.
- **Evidence**: `uint sampleMask; uint sampleInstance; sampleTransmission = traceLightTransmittanceDetailed(i, rayOrigin, rayDir, rayTMax, sampleMask, sampleInstance, sampleHitDistance);` in the `else` arm of `if (sr == 0)`.
- **Impact**: none on rendering; it suggests widening the taint needs a new trace when it only needs the instance output kept.
- **Suggested Fix**: reword to "only sample 0's blocker instance feeds the taint; samples > 0 run the detailed trace but drop their instance"; update the SKILL.md sentence (recompile `triangle.frag.spv` only if shader text is touched — `scripts/check-shader-artifacts.sh` covers it).

### REN-D2-02: `phase2_gi_energy_contracts` doc still describes the reverted RLT-GI-012 gate
- **Severity**: LOW (doc-rot)
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`, doc comment on `phase2_gi_energy_contracts` (third bullet)
- **Status**: NEW
- **Description**: The doc says the GI gate reads the isotropic footprint with the full-range ceiling; the body (after `a89ca367e`) asserts the opposite (`RT_LOD_GI = 2.0`, `giTransportEligible && rtLOD < RT_LOD_GI`, no `rtLodGi`).
- **Evidence**: the `"RLT-GI-012 deferred"` assertion messages in the same function.
- **Impact**: a maintainer may "restore" the widening the test blocks (+3..11 ms measured).
- **Suggested Fix**: "RLT-GI-012 is deferred to the Q5 radiance cache; the Phase 1b gate is pinned below."

### REN-D3-01: `GpuLocalGridParams` (the `LocalGridParams` UBO) has no layout or lane guard
- **Severity**: LOW (test-gap)
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/volumetrics/local_grid.rs` (`GpuLocalGridParams`), `crates/renderer/shaders/include/local_grid_params.glsl` (`uniform LocalGridParams`)
- **Status**: NEW
- **Description**: The local fire grid added a hand-mirrored std140 block (`boxes: [[f32;4]; MAX_LOCAL_BOXES]`, `info: [u32;4]`, `sun_dir`, `sun_radiance`, `phase`, `render_origin`; 144 B), bound by the inject pass (binding 26) and composite (binding 12). `every_remaining_uniform_block_size_matches_its_host_struct` (`reflect.rs`) has 8 cases and omits it; `every_shader_struct_is_classified` scans only `struct` and push-constant blocks, so a `uniform` block is outside it.
- **Evidence**: by hand the layout matches today (4×16 + 5×16 = 144 B; `MAX_LOCAL_BOXES = LOCAL_GRID_MAX_BOXES` is single-sourced). No `src` test mentions `LocalGridParams`. All members are vec4-class, the lane-transposition class `reflect.rs` guards for `VolumetricsParams`.
- **Impact**: none now; a lane edit on one side silently misroutes box origins, masks or sun phase in both consumers.
- **Suggested Fix**: add `LocalGridParams` to the `reflect.rs` size table and a `uniform_block_member_offsets_by_name` pin against `offset_of!(GpuLocalGridParams, ..)`, as the `VolumetricsParams` pin does.

### REN-D3-02: lane and struct-sync comments contradict live code in six places
- **Severity**: LOW (doc-rot)
- **Dimension**: GPU-Struct Layout
- **Location**: (1) `include/camera_ubo_members.glsl`, the `renderDebug` comment ("z = LOD telemetry enable"; live bits 0 = RT-LOD telemetry, 1 = `--rt-test-no-gi-clamp`, 2 = `--rt-test-no-reservoir-clamp`); (2) `GpuCamera.render_debug` rustdoc in `scene_buffer/gpu_types.rs` (bits 0–1 only); (3) `VulkanContext.render_debug_flags` rustdoc in `vulkan/context/mod.rs` (catalogue stops at `0x200` and calls it "reserved"; it is `DBG_VIZ_AO`); (4) `include/volumetrics_bindings.glsl` `vec4 wind_gust` ("Remaining lanes are reserved"; `.z` = `BYRO_SUN_ALPHA`, `.w` = `BYRO_SUN_DUST` strength are live); (5) `GpuLocalGridParams.info` rustdoc ("w unused"; it carries `LOCAL_GRID_DEBUG_*` bits); (6) `docs/engine/renderer.md` "Shader Struct Sync (CRITICAL)" (still "six GLSL sites"; since #6681/#6682 `GpuInstance` and `CameraUBO` members are declared once)
- **Status**: NEW
- **Description / Evidence**: each read at the symbol; item 6 is `grep -n "six GLSL" docs/engine/renderer.md`.
- **Impact**: doc rot of exactly the "semantic lane change" kind the layout tests cannot see; a reader could repurpose `renderDebug.z` bit 2, `wind_gust.z/.w` or `info.w` as free.
- **Suggested Fix**: update the six sites (item 1 once, in `camera_ubo_members.glsl`; the `dof_w` history-mode substring test matches a different member and is unaffected).

### REN-D4-01: `shader-pipeline.md` omits the linear-capture copies and the local-grid sim, and still says the shadow filter runs ×2
- **Severity**: LOW (doc-rot)
- **Dimension**: Sync/Barriers
- **Location**: `docs/engine/shader-pipeline.md` (step 11b `shadow_filter.comp x2`, step 13 volumetrics, the composite → bloom gap); guard `every_frame_recorder_is_documented` (`context/post_passes.rs`)
- **Status**: NEW
- **Description**: No row for `linear_capture_record_copy` (composite → bloom, two layout round-trip barriers) or `linear_capture_record_copy_post_taa` (after the TAA dispatch), nor for the volumetrics stage B2 local-grid sim (`cmd_fill_buffer` of the brick buffer, TRANSFER→COMPUTE barrier, three history barriers, timed dispatch, publish barrier, plus composite-side `record_publish_bricks` / `composite_barriers`). Step 11b says ×2; `ShadowFilterPipeline::dispatch` runs `iterations` = 2/2/3/4 by ray-quality tier (`SHADOW_FILTER_MAX_ITERATIONS` = 4).
- **Evidence**: `grep -n -i "linear_capture\|linear capture\|local.grid\|local_sim" docs/engine/shader-pipeline.md` is empty (re-run: no `linear_capture`, no `local_grid` hit); `every_frame_recorder_is_documented` collects only `record_*` method names from nine named files, so `linear_capture_record_copy*` and `grid.record_frame` (in `volumetrics.rs`) are invisible to it.
- **Impact**: a reader debugging a sync-validation report on the capture or local-grid barriers has no row to check; the guard gives false assurance that the recorder list is complete.
- **Suggested Fix**: add the rows (with the restore barrier's stage masks) and change 11b to "× iterations (2..4 by tier)"; optionally widen the scan to `*_record_*` names and add `volumetrics.rs`.

### REN-D5-03: the capture staging has no memory-budget row
- **Severity**: LOW (doc-rot)
- **Dimension**: Memory/Lifecycle
- **Location**: `docs/engine/memory-budget.md`; `LinearCaptureState.staging` (`ensure_linear_capture_staging`, GpuToCpu, render extent × 8 B, ≈16.6 MB at 1080p)
- **Status**: NEW (the local-grid half of the original observation is REN-D8-01)
- **Description**: The scene-linear capture staging is allocated on first capture and kept until teardown or a larger request; like its screenshot and depth twins it has no ledger row. Teardown is correct (`destroy_linear_capture_staging` is in `Drop`).
- **Evidence**: `grep -i "capture staging" docs/engine/memory-budget.md` is empty.
- **Impact**: documentation only; debug-CLI paths.
- **Suggested Fix**: one "diagnostic capture staging (host, on demand)" row covering screenshot, depth and linear capture.

### REN-D6-01: doc rot left by the #6712 constant move
- **Severity**: LOW (doc-rot)
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/asset_provider/material/mod.rs` (the `/// #951 / SAFE-26 — bounded-cache caps` block now above a `use` item); `byroredux/src/material_translate.rs` (`NEAR_MIRROR_NEUTRAL_ROUGHNESS` doc)
- **Status**: NEW
- **Description**: `df17d9dba` left the SAFE-26 cache-cap paragraph attached to a `use`; the caps it describes live in `provider.rs`. Separately the moved doc says the constant "Matches the neutral `classify_pbr_keyword`'s arms already use", but the classifier's no-signal terminal is `PbrMaterial::NO_SIGNAL_NEUTRAL.roughness == 0.85` (since #5197) and the constant is 0.5 (it equals only the skin/body arm).
- **Evidence**: `grep -n 'SAFE-26' byroredux/src/asset_provider/material/mod.rs`; the constant is `pub(crate) const NEAR_MIRROR_NEUTRAL_ROUGHNESS: f32 = 0.5;` (re-read). The move itself was verbatim (`material_flags.rs` is a rename with 0 content diff).
- **Impact**: documentation only. Whether 0.5 vs 0.85 is intended is NIFAL-owned (`/audit-nifal`); this report claims only that the sentence is false as written.
- **Suggested Fix**: delete the orphaned lines; reword the doc to say it is a chosen mid-roughness (or decide it should be `NO_SIGNAL_NEUTRAL.roughness`, an owner call).

### REN-D7-01: `shadow_filter::Push` SAFETY comment still says "all-`u32` fields … 16-byte block"
- **Severity**: LOW (doc-rot on an `unsafe impl`)
- **Dimension**: Soft Shadows
- **Location**: `crates/renderer/src/vulkan/shadow_filter.rs`, `unsafe impl super::buffer::NoUninit for Push`
- **Status**: NEW
- **Description**: `a2180dd18` added `pixel_angle: f32` and `reach_max: f32` (now 24 B, pinned by `push_constants_match_the_shader_block`). The SAFETY text was not updated. The impl remains sound (no padding, all plain scalars).
- **Evidence**: the comment reads "`#[repr(C)]`, all-`u32` fields tiling the 16-byte block with no padding"; the struct is `[u32;2], u32, u32, f32, f32`.
- **Impact**: none at runtime; audit hygiene.
- **Suggested Fix**: "`[u32;2]`, two `u32` and two `f32` scalars tiling the 24-byte block".

### REN-D7-02: two CPU-decides / GPU-consumes agreements in the resolve chain have no pin
- **Severity**: LOW (test-gap)
- **Dimension**: Denoiser/Composite
- **Location**: `VulkanContext::shadow_filter_active` (`post_passes.rs`) vs the `shadow_filter_active` composite input (`build_and_upload_instances.rs`, lands in `caustic_flags.y`); `CompositePipeline::dispatch(.., local_fire_sunlit)` (`composite.rs`) vs `LOCAL_FIRE_SUNLIT` in `composite.frag`
- **Status**: NEW (the first is already flagged "unpinned" in the audit skill)
- **Description**: (a) the dispatch (`record_shadow_filter`) and composite's flag both call `shadow_filter_active()`, and `r.shadowfilter` flips `shadow_filter_enabled` only at a frame boundary, so they agree today; no test fixes it. (b) Pipeline creation hard-codes `constant_id(0)` data while the GLSL declares `layout(constant_id = 0) const bool LOCAL_FIRE_SUNLIT`; `composite_frag_spv_matches_recompiled_branch_count` sees only the unspecialised SPIR-V.
- **Evidence**: `grep shadow_filter_active` shows exactly two consumers; `grep LOCAL_FIRE_SUNLIT|constant_id` in `composite.rs` shows no scanning test.
- **Impact**: a refactor that gates the dispatch on something the composite flag does not see, or renumbers the spec constant, would silently read a stale / fallback correction view or drop sun scatter from the close fire march.
- **Suggested Fix**: two `source_scan::production_text` tests: both consumers use `shadow_filter_active()` and no other predicate; `constant_id(0)` in `composite.rs` pairs with `constant_id = 0` / `LOCAL_FIRE_SUNLIT` in `composite.frag`, and `pipeline_sunless` is created and destroyed.

### REN-D8-01: the memory-budget ledger understates the local-grid irradiance field by 2× and omits the brick buffers
- **Severity**: LOW (doc-rot)
- **Dimension**: Volumetrics
- **Location**: `docs/engine/memory-budget.md` ("World-space combustion local grid" paragraph); `irradiance_extent` / `irradiance_resident_bytes` in `volumetrics/local_grid.rs`
- **Status**: NEW
- **Description**: The ledger says "160×90×64 × 8 B × 2 FIF ≈ 14 MiB at 1280×720". The image is created from `irradiance_extent(froxel_extent)`, whose width is doubled (radiance and mean light direction side by side), so ≈28 MiB at 720p and ≈112 MiB at a 2560×1440 render (default FSR Quality at 4K). The per-slot 8 KiB `BRICK_BUFFER_BYTES` buffers and the 8 KiB `combustion_phase_table` image are not listed.
- **Evidence**: `irradiance_extent` is `width: froxel.width * 2` (re-read); `irradiance_resident_bytes` (which applies the doubling) is called only from the creation `log::info!`; `memory_budget_doc_states_the_resident_cost` greps only `**48 MiB resident**`; the doc line 566 still reads "160×90×64 × 8 B × 2 FIF ≈ 14 MiB".
- **Impact**: the VRAM ledger (RT minimum 6 GB, ~4 GB budget) is off by up to ~56 MiB at 1440p render. No functional effect.
- **Suggested Fix**: correct the paragraph and add the brick / phase-table rows; extend the doc test to pin `irradiance_resident_bytes` for a stated extent, as `volumetrics_host_buffer_ledger_matches_the_code` does for host buffers.

### REN-D8-02: stage-1b "simulated, not drawn" text and "bindings 12..=15" survive stage 1c
- **Severity**: LOW (doc-rot)
- **Dimension**: Volumetrics
- **Location**: module doc of `volumetrics/local_grid.rs`; header of `shaders/volumetrics_local_sim.comp`; field doc `VolumetricsPipeline::local_grid`; `memory-budget.md` heading "(#6533 stage 1b)"; docs of `local_fire_views` and the errors in `context/resize.rs` / `context/init.rs::local_fire_views_required`; the first paragraph of `LocalBox::overlaps` (describes `contains_source`)
- **Status**: NEW
- **Description**: `d3e3acc7b` (stage 1c) made composite draw the grid, but four places still say "Nothing reads the result yet" / "not yet drawn". Composite's local-fire bindings are 12..=17 (`LocalFireViews` documents it) while `local_fire_views`, the resize error and the init error say 12..=15.
- **Evidence**: `grep -n "not yet drawn\|Nothing reads the result\|12..=15"` over `crates/renderer`.
- **Impact**: misleads the reader about whether the grid is live; no runtime effect.
- **Suggested Fix**: update the docs and the two error strings; move the misplaced paragraph to `contains_source`.

### REN-D8-03: nothing pins the local grid's skip and completion wiring (the #4780 class for new state)
- **Severity**: LOW (test-gap)
- **Dimension**: Volumetrics
- **Location**: `VolumetricsPipeline::record_skipped_frame`, `VolumetricsPipeline::mark_frame_completed`, `record_volumetrics_pass` (the `if !ran { vol.clear_local_grid_params(..) }` arm)
- **Status**: NEW
- **Description**: A skipped or failed dispatch calls `grid.invalidate_history()` (through `record_skipped_frame`, which `record_neutral_frame` also runs) and rewrites the slot's table with `clear_local_grid_params`; `mark_frame_completed` promotes box history only after submit. Correct today; no test names the three lines. `every_skipped_frame_drops_the_temporal_history_not_just_the_first` lists seven lanes and not the grid.
- **Evidence**: `grep -rn "clear_local_grid_params\|mark_frame_completed" crates/renderer/src` finds no assertion.
- **Impact**: deleting either line brings back a stale-history or stale-atlas march (flicker, or fire frozen from a skipped frame) with every test green.
- **Suggested Fix**: add the three needles to the #4780 source-shape test (`production_text`).

### REN-D8-04: the local grid's descriptor layouts are reflection-checked only at device boot
- **Severity**: LOW (test-gap)
- **Dimension**: Volumetrics
- **Location**: `local_grid_layout_bindings` (set 1 of `volumetrics_local_sim.comp` and `combustion_phase_bake.comp`) in `volumetrics/local_grid.rs`; the `assert_layout!` list in `crates/renderer/src/vulkan/reflect.rs`
- **Status**: NEW
- **Description**: The static test lists `volumetrics`, `volumetrics_integrate`, `caustic` and `composite`, not the local grid. `inject_local_grid_bindings` (26..=31) is covered through the inject layout. Set 1 of the sim and bake shaders is validated only by `validate_set_layout(1, ..)` inside `LocalGrid::build`, which needs a Vulkan device.
- **Evidence**: `grep -rn "local_grid_layout_bindings" crates/renderer/src` returns only the definition and `build` (re-run: no `local_grid` in `reflect.rs`).
- **Impact**: a binding or type drift passes `cargo test` and fails at engine start; a missing grid is fatal because composite requires `local_fire_views`.
- **Suggested Fix**: add an `assert_layout!` for the local grid (set 1, both shaders). `LocalSimPush` is a second hand-built block pinned only by string / offset tests, not by a size comparison with the compiled push block.
- **Related**: REN-D3-01.

### REN-D8-05: per-frame `Vec` allocations on two new volumetric host paths
- **Severity**: LOW (hot-path allocation)
- **Dimension**: Volumetrics
- **Location**: `LocalBoxSet::update` (`sources: Vec<Source>` collected every frame with an emitter in reach); `build_composite_params` (`candidates` vector) and `sky_apertures::nearest_apertures` (a fresh output `Vec`)
- **Status**: NEW
- **Description**: Neither uses a caller-owned scratch; the repo keeps such paths as persistent scratch (#6485).
- **Evidence**: `let mut sources: Vec<Source> = volumes.iter()...collect()` in `LocalBoxSet::update`; `candidates.into_iter().map(...).collect()` in `nearest_apertures`.
- **Impact**: small and bounded (a handful of elements), allocator traffic on fire scenes and portal-sky interiors; not measured.
- **Suggested Fix**: keep both as scratch fields cleared and reused per frame.

### REN-D12-01: the overlay's "GPU passes Σ" double counts `volumetrics_local_sim`
- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/debug-ui/src/panels/studio.rs` (`NESTED_GPU_BRACKETS`, `gpu_pass_upper_bound_ms`)
- **Status**: NEW
- **Description**: The sim bracket is recorded inside `volumetrics` (`vol.dispatch` → `grid.record_frame(..)` between the `Volumetrics` start and end), but the table lists only `volumetrics_inject`, `volumetrics_integrate`, `shadow_filter`, `cloud_shadow`. The metrics system publishes a `"volumetrics_local_sim"` row, so the Σ adds the sim's ms a second time whenever `volumetrics` and the sim both ran. `GpuTimerSnapshot.volumetrics_local_sim_ms`'s own doc says never to sum them.
- **Evidence**: `NESTED_GPU_BRACKETS` has four entries (re-read); `volumetrics_local_sim` appears in `main.rs` `BENCH_GPU_KEYS` and `s.gpu_volumetrics_local_sim_ms`; `gpu_pass_sum_excludes_nested_child_brackets` covers only inject / integrate.
- **Impact**: the overlay's headline GPU total is overstated by the sim's cost in fire scenes. Bench-line and per-row figures are unaffected.
- **Suggested Fix**: add `("volumetrics_local_sim", "volumetrics")`; extend the test and the hover text; better, a test that every bracket documented as nested in `GpuTimerSnapshot` appears in the table.

### REN-D12-02: stale comments in the local-sim timer / telemetry code
- **Severity**: LOW (doc-rot)
- **Dimension**: Debug/Telemetry
- **Location**: `gpu_timers.rs` (`Q_VOLUMETRICS_LOCAL_SIM_*` doc and the `TimerPass::row` comment); `volumetrics/local_grid.rs` (`record_frame` SAFETY comment); `context/telemetry.rs` (`fill_texture_residency_telemetry` / `fill_upscaler_telemetry` docs); `vulkan/taa.rs` (`TAA_VARIANCE_GAMMA` doc)
- **Status**: NEW
- **Description**: (a) The sim is recorded BEFORE the inject bracket ("Stage B2" precedes "Stage C" in `VolumetricsPipeline::dispatch`), but the timer docs say "recorded right after it" and the SAFETY text says "the froxel inject dispatch recorded just before". (b) The M39 telemetry fns were inserted between `fill_upscaler_telemetry`'s doc paragraph and its fn, so the paragraph documents the wrong item. (c) `TAA_VARIANCE_GAMMA`'s doc cites `rt_test_taa_gamma`; the field is `rt_test_taa_gamma_bits`.
- **Evidence**: call order in `VolumetricsPipeline::dispatch`; the two doc blocks in `telemetry.rs` run together with no item between them.
- **Impact**: misleads about barrier / bracket order; no runtime effect (the bracket is correctly nested inside `volumetrics`).
- **Suggested Fix**: correct the three statements; move the paragraph back onto `fill_upscaler_telemetry`; fix the field name.

## Prioritized Fix Order

1. **Correctness / safety**: REN-D5-01 (panic on untrusted DDS, before streaming goes default-on, #6631), then REN-D5-02 (needs a validation run to size it).
2. **Pins that protect fresh code**: REN-D3-01, REN-D8-03, REN-D8-04, REN-D7-02 (test-only, all `cargo test`-visible, no Vulkan change).
3. **Telemetry**: REN-D12-01 (one table row).
4. **Hot path**: REN-D8-05.
5. **Doc rot** (batch into one commit; none touches shader text except possibly REN-D2-01's comment, which `check-shader-artifacts.sh` would then require recompiling): REN-D1-01, D2-01, D2-02, D3-02, D4-01, D5-03, D6-01, D7-01, D8-01, D8-02, D12-02.

## Needs-RenderDoc (observations only; nothing here is a finding)

- Same-batch double residency change (REN-D5-02): `BYRO_VALIDATION=1`, streaming on, shrinking `render.texture_budget_mib` with promotions in flight; also sync-validation of the promotion barrier on `old_image` (ALL_COMMANDS → TRANSFER) while a previous frame still samples it.
- Post-TAA linear-capture tap and the PreBloom restore barrier into bloom's in-place write (`--linear-capture`).
- Local-grid sim, composite bindings 12..=17 and the sunless composite pipeline; the phase-bake pass ends with no trailing barrier (its table write is published to composite only by the init submit's fence wait); the sim binds inject set 0, whose bindings 27..=29 sample `state[frame]` while the sim writes the same image through set 1 (should be valid since the sim shader does not use them statically); cost of the inject pass's unconditional irradiance zero-write.
- Shadow filter at tiers 2 and 3, and at the tier 0/1 two-iteration reach (6 px) next to pixels whose blocker search disagrees; possible grain from the RLT-SHD-012 cosine weight at tier 0; the far-from-camera size of the ray-origin floor.
- Local fire march caps (`LOCAL_FIRE_MAX_BRICK_JUMPS`, `LOCAL_FIRE_MAX_STEPS_PER_BOX`) under a fast camera (owned by #6531 / #6533, stage 1c not accepted).
- Look checks for RLT-GI-011 (bounce vs direct brightness) and the 0.0047 rad exterior sun penumbra with the cosine weight, as A/B captures.
- The `record_geometry_pass` split under `BYRO_VALIDATION=1` (already verified identical bench tokens by the #6710 commit; a fresh run would confirm).

## Stale skill premises (for the next `/audit-sync`)

- Dim 10 **Paths** / **First step** list `byroredux/src/systems/light_anim.rs` but not `byroredux/src/light_translate.rs`; since #6717 a path-scoped `git log` on `light_anim.rs` no longer sees canonicalizer changes (`_audit-owners.md` and `audit-oblivion` already name it).
- Dim 12: `RenderDebugMode::USER_MODES` is now 20 and `RENDER_DEBUG_MODE_MAX` 19 (`volumetric_radiance`, #6727); the skill says 19 / 18. `QUERIES_PER_FRAME` 62 / 26 `TimerPass` variants + 5 geometry phases match the skill. The skill's `NESTED_GPU_BRACKETS` hedge ("verify and file") is now REN-D12-01.
- Dim 2: the "Documented hole" sentence repeats the stale claim (REN-D2-01).
- Dim 4: the `construction_and_recording_drivers_stay_within_their_size_pins` Guard text says "#6143 stays open as the tracker"; #6143 was closed this session against its own proposed fix (all eleven functions pinned, the six extractions landed). Dim 3's `renderDebug.z` sentence ("its `.z` a bit field (bit 0 … bit 1 … bit 2 …)") is already right; the contradicting comments are in code (REN-D3-02).
- Dim 5: the Guard line cites no test for the M39 truncated-payload case; add the REN-D5-01 test once it exists.
- Dim 8: add the local-grid reflect / skip-wiring pins (REN-D8-03/04) to the Guard line once they exist.

## Existing / carried (not re-filed)

#6341, #6316, #6315, #5365, #6371 (the remaining hand-written pass teardowns), #6106 (geometry-pool refusal, partial), #6248, #6486 (half-fixed, see above), #6531 / #6533 (combustion local grid tracking), #6631 (M39 default-on).

## Checked, no finding

- **Dim 1**: #6709 / #6708 splits (statement order, error paths, `built_this_frame` gating, both eviction sites outside their guards); `acceleration` 147/147; TLAS barrier, static-BLAS recovery and bin `app_step` guards pass.
- **Dim 9**: the `skin_vertices.comp` hunk only replaces two `buffer_reference` blocks with `#include "include/buffer_references.glsl"`; `skin_compute.rs` / `morph_compute.rs` changes are field-path renames and a doc retarget; `SkinSlotPool` change is a doc comment. Guards green (see Executive Summary).
- **Dim 10**: lighting is clean. `byroredux/src/light_translate.rs` bodies are verbatim copies of the removed block with no call site dropped; `VisibilityMask::for_legacy_local_light()` unchanged; `shadowableLightRadiance` is the only `disneyDiffuseSplit` site; the Lambertian flag is pinned at all three shading sites; exterior sun radius 0.0047 vs interior 0.020; dome and fog colours decoded exactly once.
- **Dim 11**: the only change is six `rt_test_*` fields on `RendererConfig` (`upscaling.rs`), all defaulting to off / `0` / `None`; `renderer_config_defaults_to_fsr_quality` passes.
- **Dim 6**: `every_exterior_spawner_inserts_a_boundary_material` still sees every spawner (through `scene/mesh_entity.rs`); no per-game branch between `Material` and `MaterialTable::intern`. `cross_game_translation_completeness` (`#[ignore]`d, needs vanilla data) was not run.
- **Dim 12**: scene-linear capture reads back after the all-slots fence wait, staging is freed in teardown, the decode layout is pinned (`linear_capture_texel_layout_matches_hdr_format`); readers honour `_active`; every post pass keeps its raw-output gate (`volumetric_radiance` included); the `--rt-test-*` switches are inert at default.
