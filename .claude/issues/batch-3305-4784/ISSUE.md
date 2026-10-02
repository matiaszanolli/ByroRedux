# Batch: 3305 3307 3429 3807 4599 4633 4722 4784 (fetched 2026-10-02)

## #3305 [OPEN] REN-2026-08-26-01: dynamic actors (creatures) show no ground-contact shadow despite correct light/instance visibility masks
labels: bug, renderer, medium, vulkan

FO4 Commonwealth screenshot: two feral hound creatures cast no ground shadow while
statics do. Light-level and instance-level visibility masks both check out
(`lights.rs:192` FULL mask; `predicates.rs:719-751` Actor → DYNAMIC_ACTOR bit).
Issue's own conclusion: needs RenderDoc capture (TLAS instance list + shadow-ray
hit/miss for the exact pixel), NOT more source-reading — project policy
`feedback_speculative_vulkan_fixes` forbids speculative render fixes.
Candidate directions listed: skinned-BLAS first-sight-frame refit timing
(`blas_skinned.rs:390-412`), creature race→RenderLayer resolution, creature vs
humanoid BLAS registration path.

## #3307 [OPEN] EX-10/11 item 8: active VWD full-model culling (decouple full-REFR spawn radius from radius_unload)
labels: enhancement, renderer, legacy-compat, terrain-exterior

Feature request. Detection half landed (`LodCoverageStats::vwd_full_model_overlaps`
= 0, commit 2a84ab97). Active cull needs per-REFR streaming radius beyond
`radius_unload` — whole-cell-only streaming today. Issue's own text: building it
disabled = real effort, no proof; enabling blind violates no-guessing policy;
needs live visual validation at the transition boundary.

## #3429 [OPEN] UI-D6-2026-08-27-04: animating menu allocates a fresh full-viewport VkImage and blocks on a fence every frame, ahead of draw_frame
labels: bug, renderer, medium, memory, performance, ui

`TextureRegistry::update_rgba` recreates the image (`Texture::from_rgba` →
one-time CB submit + fence wait + staging release), old image parked in
`pending_destroy` ring drained after 2 frames. Real HUDs (compass/bars) differ
every frame → per-frame full-viewport image create/destroy (8.3 MB @1080p, three
live copies) + blocking `vkWaitForFences` on main thread ahead of `draw_frame`.
Fix: genuine in-place path for dynamic RGBA entries — persistent image +
per-FIF staging ring + `cmd_copy_buffer_to_image` recorded into the frame's own
command buffer. Locations: `texture_registry.rs:1596-1645`, `vulkan/texture.rs:71-99`,
`:114-131`, `app_frame.rs:316-333`.

## #3807 [OPEN] EX-14/15 item A: ground-cover density streaming (GRAS/REGN scatter, chunking, LOD, RT proxy)
labels: enhancement, ecs, renderer, legacy-compat, terrain-exterior

Multi-phase feature (§11.1 measurement gate FIRST, then scatter → blades/wind →
LOD → RT proxy → GRAS decode → OwnershipTracker classes). Phase 0 (canonical
types, LTEX map, palette) landed 2026-08-12. Everything past unstarted; GRAS
still parses through `parse_minimal_esm_record`. Measurement gate requires
`--bench-hold` on real terrain before any code — the doc's own rule.

## #4599 [OPEN] SAFE-D3-2026-09-21-01: `.expect()` on a poisoned allocator lock is reachable from Drop / teardown (double panic → abort)
labels: bug, renderer, low, safety, concurrency

`StagingGuard::cleanup` (buffer.rs ~:637-658), `GpuBuffer::destroy`
(~:1471-1475), `StagingPool::trim_to` (~:343-347), `Texture::destroy`
(texture.rs ~:631-635), `VulkanContext::drop` (teardown.rs ~:432) all
`.lock().expect("allocator lock poisoned")`. Poisoned mutex + drop during unwind
= double panic → abort; later hits skip save_pipeline_cache/destroy_device/
destroy_instance. #4089 moved only `GpuImage` to `into_inner()` recovery.
Fix: route free/Drop-side locks through the GpuImage recovery
(`unwrap_or_else(PoisonError::into_inner)` + #2398 rationale comment); log
`free()` errors inside Drop instead of expect. Allocation-side expects may stay.
Checks: SIBLING (every poisoned-lock expect on free/destroy/Drop paths in
crates/renderer converted), DROP (poisoned allocator still reaches full reverse
order), LOCK_ORDER (no new lock scope), TESTS (helper unit-tested vs poisoned
mutex or source pin).

## #4633 [OPEN] NIFAL-D2-2026-09-21b-01: #4549's finite gate is inputs-only — compose_transforms' own arithmetic can still manufacture inf before the TLAS instance build
labels: bug, renderer, medium, nifal

#4549 gates NIF readers on `is_finite()`. But
`compose_transforms` (`crates/nif/src/import/transform.rs:13-25`) computes
`parent.scale * child.scale` etc. with no check on the *products*: two finite
extreme scales (1e20 × 1e20) → +inf f32. Convergence point:
`tlas_instance_transform` (`crates/renderer/src/vulkan/acceleration/predicates.rs:115-121`)
passes `draw_cmd.model_matrix` straight to `VkTransformMatrixKHR` with no check.
~5% of random float corruptions are finite-but-overflowing (vs 0.4% direct NaN).
Fix: gate compose_transforms' output and/or GlobalTransform propagation on
finiteness, or check at the convergence point (drop instance + rate-limited
warning). SIBLING: #4621 BSSkin::BoneData bind bypass (different producer, same
class). TESTS: regression pin for finite-but-extreme inputs.

## #4722 [OPEN] UI-D5-2026-09-21-01: #4199 moved the instance-upload clamp to the slot's grown capacity, but the UI overlay's firstInstance guard still compares against MAX_INSTANCES, before the grow
labels: bug, renderer, low, vulkan, ui

`build_and_upload_instances.rs:597-617`: `ui_instance_idx` gated only on
`MAX_INSTANCES`, computed BEFORE `ensure_instance_capacity` runs at `:683`. If
len is between slot capacity and MAX_INSTANCES AND the grow fails,
`upload_instances` clamps (`count = instances.len().min(capacity)`) but the
overlay still submits the stale idx as `firstInstance` → OOB SSBO read in
`ui.vert` (`robust_buffer_access` off, device.rs:652) feeding garbage bindless
textureIndex — the #3601 consequence. Fix: compute `ui_instance_idx` AFTER the
grow, against slot's actual `instance_capacity[frame]` (expose accessor beside
`instance_buffer_size(frame)`). Update/replace
`ui_instance_idx_is_clamped_to_none_past_max_instances` pin. SIBLING of #4726
(DrawBatch clamp, fixed 475240e1e).

## #4784 [OPEN] PERF-D5-2026-09-23-02: every froxel runs RK2 backtrace + 6-neighbour gather + curl forcing when any transport emitter is active
labels: bug, renderer, medium, performance, shaders

`volumetrics_inject.comp`: `simulationDt > 0` makes empty froxels (majority) run
27 trilinear fetches + 9 reprojections each (~+22 M fetches/frame @720p; est.
0.3–0.6 ms @1080p). Fix: coarse occupancy mask on the 16³ fog-cluster grid
(atomicOr from inject pass, 1 frame behind), CPU OR of transported source
clusters, dilate 1 cell, skip whole `hadHistory && dt > 0` block for empty
cells, hoist curl/wind under `activity > 0`, measure with PERF-D8-01 split
bracket. **BATCH STATE (2026-10-02, prior session)**: scene-level gates + curl
hoist already landed; occupancy-mask core deliberately left open pending live
A/B on real hardware (No-Guessing). Do NOT re-attempt without that gate.
