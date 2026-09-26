**HEAD**: `078f650ec` · **Baseline**: `AUDIT_RENDERER_2026-09-21.md` (HEAD `f97775ca8`; Dim 5 was last audited in full there) · **Audited**: Dim 5 only (`--focus 5`; first request `--focus 15` named no dimension, so it was re-issued as 5) · **Unchanged since baseline (skimmed)**: `instance.rs`, `allocator.rs`, `image.rs`, `swapchain.rs`, `placeholder.rs`, `deferred_destroy.rs`, `acceleration/blas_skinned.rs`, `acceleration/constants.rs`, `scene_buffer/gpu_types.rs`, `mesh/geometry_ssbo.rs` (census hook only), `exposure.rs` / `exposure_meter.rs` (tests only) · **Out of scope this run**: Dims 1–4, 6–12.

# Renderer Audit — 2026-09-26 (Dimension 5: GPU memory, resource lifecycle & teardown)

Delta window: `f97775ca8..HEAD`. About 35 commits touched Dim 5 paths (`git log --since=2026-09-21` over the Dim 5 path list). Read-only audit: no source was edited and no engine was launched. The work ran as three parallel sub-scopes:

- **A**: teardown, init, resize, device selection, allocator.
- **B**: deferred destroy, buffers, scene buffer, AS memory, morph.
- **C**: global geometry SSBO, texture registry, DDS and texture upload.

I re-read the code behind 13 of the 21 findings myself, listed under Process notes. The rest are sub-agent findings checked by reading.

**Totals: 0 CRITICAL · 2 HIGH · 9 MEDIUM · 10 LOW** (21 findings).
- 20 are NEW or a regression. Finding 03 is a Regression of closed #1921, and finding 02 folds in Existing #4854 (its leak half) alongside a NEW submit-after-free half.
- One (19) is an Existing status update for open #4599.
- No finding duplicates an open issue otherwise. Item 15.6 cross-references Existing #4870, and item 15.12 lists #4872, #4869 and #4871 as already tracked.

## Executive Summary

The lifecycle machinery is in good shape. The failures are in the *error paths* and in *registry bookkeeping*, not in the steady-state frame loop.

**Passed (no finding).**
- **Teardown coverage.** Every allocator-owned object in `VulkanContext` and its sub-structs is destroyed exactly once, before `allocator.take()`.
  - This covers the new `dynamic_rgba` arena, `groundcover_models`, the exposure meter and GPU timers.
  - Every holder of its own `Arc<Allocator>` is drained before the `try_unwrap`.
  - The three load-bearing orderings are still exactly three, pinned by `ordering_count_doc_sites_agree_tests`.
- **Deferred destroy.** Every eviction, unload and grow site is deferred or proven idle. No immediate-while-in-flight destroy was found.
  - This includes the new instance-SSBO grow (#4833) and the TLAS refit work in `5eb07a4f3`.
- **Two-phase geometry compaction.** It holds: only `apply_compaction_plan` publishes offsets.
- **DDS untrusted-input hardening.** #4511, #4512, #4515, #4830 and #4835 hold.
- **Ledger numbers.** The scene-buffer, TLAS and ground-cover rows re-derive from the code constants.

**HIGH.**
1. **REN-D5-2026-09-26-01.** A cancelled streaming cell apply orphans every texture it had queued but not yet flushed. Each one is uploaded into a slot whose refcount is already 0 and stays resident until process exit.
2. **REN-D5-2026-09-26-02.** In `record_dds_upload` on the *batched* path, a failing `create_image_view` (or allocate) leaks the image. It also lets `flush_upload_batch` submit a command buffer that copies from an already-destroyed staging buffer. This is the "use-after-free" half that #4854 (leak only) never described.

**MEDIUM highlights.**
- `StagingPool` capacity labels decay on reuse, so the 128 MiB retained-memory budget is enforced against a shrinking ledger. This is a regression of #1921, reintroduced by the #4512/#4593/#4790 remedy.
- The core `buffer.rs` constructors and `build_blas_batched` Phases 1–3 do not unwind on allocator failure.
- BLAS scratch is retired before its replacement exists.
- `recreate_descriptor_sets` neither rewrites reserved-but-unflushed slots nor survives its own failure.
- `parse_dds` accepts non-square cubemaps.
- The ground-cover blade draw issues a multi-draw indirect on devices that may lack `multiDrawIndirect`.
- A failed HUD staging step is now fatal to `draw_frame`.

## RT Pipeline Assessment (AS memory lifetime only — build/shading correctness is Dims 1–2)

- **TLAS.**
  - `ensure_tlas_state` is allocate-then-swap with a defensive `device_wait_idle` before any destroy (#2673/#1390).
  - `shrink_tlas_to_fit` only records `tlas_shrink_pending` (#2929), and `shrink_tlas_scratch_to_fit` allocates before retiring (#2915).
  - The `5eb07a4f3` host caches (`last_entity_ids`, `tlas_entity_ids_scratch`) shrink, are ledgered (3,145,728 B at 262,144 instances) and have `ctx.scratch` rows.
  - `invalidate_tlas_recording` is wired into `rollback_skin_frame_state`.
- **Static and skinned BLAS.**
  - Eviction goes through `pending_destroy_blas` and skinned BLAS are pinned against LRU while in flight.
  - Residency accounting (`pending_destroy_static_bytes`) credits on drop and releases on tick, and every decrement is `saturating_sub`.
  - The constants (`BLAS_REBUILD_SLACK_BYTES`, `TLAS_SCRATCH_SLACK_BYTES`, `TLAS_REBUILD_SLACK_BYTES`, `MIN_TLAS_INSTANCE_RESERVE`, `MIN/MAX_BLAS_BUDGET_BYTES`, the `(heap − reservation)/3` formula) match memory-budget.md.
  - Findings: **05** (batched-build error exits) and **06** (scratch retired before replacement).
- **Instance SSBO grow (#4833/#4199).** `ensure_instance_capacity` is allocate-both-then-swap, rewrites scene bindings 4 and 18 in place and retires both old buffers through the countdown.
  - Only the caustic set (binding 5) needs an explicit rebind. Volumetrics binding 19 and the ground-cover model tier bindings 10/11 are rewritten every frame they are used.
  - The TLAS instance map is capped at the slot's real capacity (`instance_map_cap`).

## GPU-Struct & Memory Assessment

- **Teardown.** Zero ownership gaps. The enumeration is every `GpuBuffer` / `GpuImage` / `Texture` / `StagingPool` / `Allocation` field across 43 files.
  - It maps onto exactly `destroy_allocator_owned_resources` plus `Drop`.
  - Every `Some(..) → None` transition in `context/*.rs` destroys first.
  - No non-renderer crate holds a `SharedAllocator` or `GpuBuffer`.
- **Resize.** `recompute_blas_budget_for_current_state` runs last, after `upscaler.recreate` (`blas_budget_is_recomputed_after_the_upscaler_catches_up`).
  - `screen_scaled_reservation_bytes` bills the froxel grid, every render-extent pass via `render_extent_bytes_per_pixel_x1024`, the upscaler output and the FSR SDK bytes via `FrameUpscaler::sdk_memory_bytes`.
  - Every scene-set writer is rewritten on resize.
  - Findings: **12** (`recreate_swapchain_core` `?` windows) and **08** (`recreate_descriptor_sets`).
- **Staging.** #4593/#4790/#4512 hold for every acquire/release pair. The accounting consequence is **03**.
- **Ledger.** The one new persistent owner with no row is the `dynamic_rgba` staging arena (**14**). #4872 remains open and still current (model-tier instance tail 0/14/28 MiB, not 7; dangling volumetrics noise-volume row).
- **New owners in the window.** `GroundCoverModelTier`, the exposure meter, the `dynamic_rgba` arena and the fog cluster/medium buffers each have teardown coverage. Only the arena lacks a ledger row.

## Findings

### HIGH

#### REN-D5-2026-09-26-01: A cancelled streaming apply orphans its queued-but-unflushed textures — `flush_upload_batch` installs a texture into a slot whose last reference was already released
- **Severity**: HIGH (unbounded, session-long GPU memory leak on a normal-operation path; provisional. Downgrade to MEDIUM only if telemetry shows cancels with a non-empty upload queue are rare.)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/texture_registry/upload.rs` (`flush_upload_batch` install block, `queue_or_hit_for_view`), `crates/renderer/src/texture_registry/release.rs` (`drop_released_texture`, `release_ref`, `release_refs_batch`, `decrement_ref`), `crates/renderer/src/texture_registry/mod.rs` (`pending_dds_uploads`, the `TextureEntry` invariant "`texture.is_some()` iff `ref_count > 0`"), caller chain `byroredux/src/cell_loader/exterior.rs` (`ExteriorCellApplyJob::cancel`) → `unload_cell` → `drop_textures`
- **Status**: NEW
- **Description / Evidence**:
  - `enqueue_*` reserves a slot (`texture: None`, `ref_count: 1`, a `path_map` entry) and pushes a `PendingDdsUpload`.
  - `drop_released_texture` on such an entry does `entry.texture.take()` → `None` and returns, so the queued upload survives. `pending_dds_uploads` is only ever pushed to (`upload.rs` push) and `mem::take`n (`flush_pending_uploads`). There is no `retain` and no purge anywhere.
  - At the next flush, the record loop and the install block in `flush_upload_batch` never read `ref_count`. The install sets `entry.texture = Some(texture)` on a slot with `ref_count == 0` and no `path_map` key.
  - Nothing frees it before `TextureRegistry::destroy`: `tick_deferred_destroy` walks only `pending_destroy`, and `decrement_ref` on that slot returns "already-released".
  - Reachability, read in code: `ExteriorCellApplyJob::cancel` calls `references.cancel(world)` and then `unload_cell` directly, with no texture flush.
    - `flush_pending_cell_textures_on_yield` flushes only at ≥ 64 uploads or ≥ `MAX_UPLOAD_BATCH_BYTES` (`YIELDED_TEXTURE_UPLOAD_BATCH_MIN`, `should_flush_pending_cell_textures`).
    - So any cell with fewer than 64 fresh textures holds its whole reservation set unflushed until completion.
    - Failure arms that release just-resolved unflushed handles (`terrain.rs` `release_splat_layer_textures`, the `terrain_lod_btr.rs` failed-upload release) reach the same state.
  - Sibling of the same broken invariant: `update_rgba`'s extent-change arm quietly revives a `ref_count == 0` entry.
- **Impact**: Each such event uploads the queued textures and orphans them for the process lifetime. Re-entering the cell re-reserves fresh slots and uploads them a second time. `live_slot_count()` counts the orphans as dead, so the leak is invisible in telemetry. memory-budget.md's claim that "GPU image memory itself IS correctly reclaimed" is false for this path. Per-event size is bounded by the unflushed queue (< 64 textures or < 128 MiB). Frequency is unmeasured. The orphans also accelerate slot exhaustion (#2030).
- **Related**: #1922 (dead handle after a failed flush — a different mechanism), #2030, #524.
- **Suggested Fix**:
  1. In `flush_upload_batch`, skip any upload whose `textures[handle].ref_count == 0` before parsing or staging.
  2. Retain-filter `pending_dds_uploads` for freed handles once per release batch.
  3. Make `update_rgba` refuse `ref_count == 0` rather than revive.
  4. Pin it device-free: `queue_or_hit` → `release_ref` → assert the queue no longer names the handle. Those functions are already exercised without a device by the existing registry tests.

#### REN-D5-2026-09-26-02: `record_dds_upload` — a failing `create_image_view` or `allocate` leaks the image, and on the batched flush path a post-record failure submits a command buffer that reads a destroyed staging buffer
- **Severity**: HIGH (a Vulkan spec violation is at least HIGH. It is failure-path only, but the failure is the VRAM-OOM the engine is most likely to meet.)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/texture.rs` (`Texture::record_dds_upload`: `create_image(..)?`, `.allocate(..).context("Failed to allocate DDS texture image memory")?`, the bind arm, the tail `create_image_view(..).context("Failed to create DDS texture image view")?`), consumer `crates/renderer/src/texture_registry/upload.rs` (`flush_upload_batch` record closure)
- **Status**: **Existing #4854** (the view-arm leak; verified still current) **+ NEW** (the submit-after-free half and the allocate-arm image leak)
- **Description / Evidence**:
  - (a) #4854 holds. After `bind_image_memory` succeeds, a `create_image_view` failure returns via `?` with the `VkImage` and its allocation un-owned.
  - (b) NEW. In the function body the order is: bind → `cmd_pipeline_barrier` → `cmd_copy_buffer_to_image` → `cmd_pipeline_barrier` → `create_image_view(..)?`.
    - The early return drops the local `StagingGuard`, which destroys the staging buffer and frees its allocation.
    - In `flush_upload_batch` the record closure logs "dropping queued upload" and `continue`s, then returns `Ok(())`, so `with_one_time_commands_reuse_fence` submits the command buffer.
    - That buffer still contains a `vkCmdCopyBufferToImage` from the destroyed buffer (VUID-vkQueueSubmit-pCommandBuffers-00070 class). Once the sub-allocation is freed, the GPU can read released device memory.
    - The synchronous `from_dds_with_mip_chain` path is safe: `with_one_time_commands` frees a command buffer whose closure returned `Err` without submitting. Only the batched path reaches the hazard.
  - (c) NEW. The `allocate(..)?` failure (VRAM OOM, the most plausible failure in this function) leaks the just-created `VkImage`. The #2178 fix covered only the bind arm.
  - `image::tests::no_file_outside_this_module_rolls_its_own_image_chain` allow-lists `texture.rs` as a documented specialisation, and no test covers its error arms.
- **Impact**: One leaked `VkImage` handle per failed upload, plus a GPU read of freed memory on the batched path. Trigger is OOM-class, so likelihood is low and impact is not.
- **Related**: #2178, #4854, #2164 (the staging-side analog, fixed in `create_staging_buffer`).
- **Suggested Fix**: Fix by reordering rather than adding cleanup. Create the image view (CPU-only) immediately after bind and before recording, with one unwind (view → image → allocation) for every arm, so nothing is recorded until every fallible step has succeeded. Better still, route image creation through `GpuImage::create` in `crates/renderer/src/vulkan/image.rs`, which already implements the create → allocate → bind → view chain with the #2178/#4089 unwinding. Pin it with a source-scan asserting no `?` after the first `cmd_pipeline_barrier`. **Needs a `BYRO_VALIDATION=1` fault-injection run** (forced `vkCreateImageView` failure) to observe the submit-with-destroyed-buffer error.

### MEDIUM

#### REN-D5-2026-09-26-03: `StagingPool` capacity labels decay on reuse, so retained host-visible memory is not bounded by `DEFAULT_STAGING_BUDGET_BYTES`
- **Severity**: MEDIUM (impact ceiling HIGH if a soak confirms; live measurement owed)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/buffer.rs` (`StagingPool::acquire` / `release` / `total_capacity` / `trim_to`, `select_evictions`, `StagingGuard::release_to`, `GpuBuffer::create_device_local_buffer`, `create_device_local_buffers_batched`, `copy_bytes_range`), `crates/renderer/src/vulkan/texture.rs` (`record_dds_upload`, `overwrite_rgba_pixels`), `crates/renderer/src/texture_registry/upload.rs` (`flush_upload_batch`), `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`upload_terrain_tiles`)
- **Status**: **Regression of #1921** (closed 2026-07-11, `a0b5539c4`; text in `.claude/issues/1921/ISSUE.md`). Knowingly reintroduced by #4512 (`80813026a`, its own comment says "#1921's ledger concern returns") and extended to the mesh and terrain sites by #4593 (`59c4a01d4`) and #4790 (`78a98ed87`). No open issue.
- **Description / Evidence** (mechanism confirmed by reading `acquire` / `release` / `release_to`):
  - `acquire(size)` returns `(vk::Buffer, Allocation)` only, so the entry's capacity is lost. Every call site releases with the *current request's* `size`.
  - Best-fit hands an entry only to a request ≤ its label, and release re-labels it to that request. An entry's label is therefore monotonically non-increasing while its `VkBuffer` create size is fixed.
  - `total_capacity()` under-counts retained bytes, `release`'s over-budget `trim_to` never fires, and `select_evictions` evicts the largest *label* first, so honest entries go while decayed large buffers stay. There is no production `trim_to(0)` caller and no production `total_capacity()` reader.
  - Deterministic sequence from the code: a 64 MiB request creates B1 (create 64, label 64). A 3 MiB request reuses B1 and relabels it 3. A later 64 MiB request finds no label ≥ 64 and creates B2. A 4 MiB request takes B2 (label 4). Real bytes grow 64 MiB per cycle while labels stay a few MiB.
  - A Python model of exactly these rules (sub-scope B; not a measurement of the live engine) gave declared 125–128 MiB versus real 0.88–1.45 GiB (discrete BC1/BC3 sizes) and 3.19 GiB (log-uniform 4 KiB–22 MiB). The same model with the label set to the true create size gave real == declared ≈ 124 MiB.
  - Three pools carry the same rule (`TextureRegistry::staging_pool`, `MeshRegistry::geometry_staging_pool`, `SceneBuffers::terrain_tile_staging_pool`). It is more acute now because `geometry_staging_pool` holds the two 64 MiB `advance_geometry_rebuild` chunk buffers *and* is fed by the small per-group precombine uploads (`upload_scene_meshes_batched`) that relabel those entries small.
- **Impact**: CpuToGpu (BAR / VRAM under ReBAR) memory retained above the documented bound during texture-heavy or mesh-heavy streaming. Growth is per upload, not per frame. The magnitude depends on the real request-size mix and is not measured live.
- **Related**: #4512, #4593, #4790, #1921. memory-budget.md "Not yet ledgered: StagingPool retained capacity" (the "retention bound" it implies is false under this defect).
- **Suggested Fix**: Record the `VkBuffer` create size per entry inside `StagingPool` (return it from `acquire` and carry it in `StagingGuard`) and release at that. This satisfies both #4512 (label ≤ create size, never the allocation footprint) and #1921 (label == real). Add a `real_bytes()` gauge to the `rt.integrity` / scratch telemetry and a pure test over the extracted best-fit and relabel logic. The three tests that pin the request-size rule (`staging_release_capacity_tests::pooled_staging_releases_the_requested_size_not_the_allocation_footprint`, `::terrain_ring_releases_staging_at_the_requested_size`, `dds_upload_guard_tests::staging_release_capacity_is_requested_size_not_allocation_size`) must move with the fix. **Confirm first with a grid-soak** comparing the sum of real create sizes to `total_capacity()` per pool.

#### REN-D5-2026-09-26-04: The core `buffer.rs` constructors do not unwind on allocator failure; the geometry rebuild retries them every frame
- **Severity**: MEDIUM (one-shot per failure, amplified to per-frame by the retry loop; escalate to HIGH if fault injection confirms the bind and submit arms)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/buffer.rs` — `StagingPool::acquire` (`allocate(..)?` leaks the VkBuffer; `bind_buffer_memory(..)?` leaks buffer and allocation), `GpuBuffer::create_host_visible`, `create_host_readback`, `create_device_local_uninit`, `create_device_local_buffer` (allocate, bind, and `with_one_time_commands(..)?` arms). The unwinding siblings are `create_staging_buffer` (#2164) and `create_empty_device_local_buffer` (#3298).
- **Status**: NEW (sibling gap of closed #2164; #4854/finding 02 is the same class in `texture.rs`)
- **Description / Evidence**:
  - Each constructor calls `create_buffer`, then `allocator.allocate(..)?`, then `bind_buffer_memory(..)?` with a bare `vk::Buffer` local (a `Copy` handle with no `Drop`). I confirmed this in `create_host_visible`: there is no destroy on the allocate or bind error arms. `GpuBuffer::Drop` cannot help because `Self` is never built.
  - `staging_guard_coverage_tests` exempts `buffer.rs` as the legitimate creator, so nothing pins the unwind.
  - Amplification: `SceneBuffers::ensure_instance_capacity` documents "on allocation failure the slot keeps its current buffers", and `grow_instance_ssbos` runs twice per frame. `byroredux/src/app_frame.rs` calls `mesh_registry.rebuild_geometry_ssbo` every frame while a rebuild is in progress or dirty, and only `warn!`s on `Err`. A persistent failure therefore leaks at least one VkBuffer per attempt per frame, and the bind or submit arms leak real allocations, including the full global SSBO in `build_geometry_ssbo`.
- **Impact**: Handle leak per failed allocate. Real device or host memory per failed bind or submit.
- **Related**: #2164, #3298, #4854.
- **Suggested Fix**: Factor one `create_bound_buffer` helper that owns the unwind (destroy on allocate error; destroy and free on bind error) and route the five sites through it. Give `create_device_local_buffer` a guard that also covers the copy-submit failure arm. Pin it with a source-scan like `staging_guard_coverage_tests`. The bind and submit arms need a `BYRO_VALIDATION=1` fault-injection run.

#### REN-D5-2026-09-26-05: `build_blas_batched` Phases 1–3 early `?` exits abandon `prepared` (raw AS handles and result buffers)
- **Severity**: MEDIUM (error path under BLAS/VRAM pressure)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs` — `AccelerationManager::build_blas_batched`: Phase 1 `GpuBuffer::create_device_local_uninit(..)?` (result buffer for mesh k), Phase 2 `create_device_local_uninit(scratch)?`, Phase 3 `create_query_pool(..).context("Failed to create compaction query pool")?`
- **Status**: NEW (sibling gap of closed #1097 / #316 / #2926)
- **Description / Evidence**:
  - I confirmed the shape by reading. The AS-create failure arm right below Phase 1 unwinds `prepared` (`for mut p in prepared { destroy_acceleration_structure; p.buffer.destroy }`, the #1097 fix), but the `create_device_local_uninit(..)?` immediately above it does not.
  - On a Phase-1 failure at mesh k > 0, `prepared[0..k]` is dropped. Each `GpuBuffer` hits the #656 Drop net (WARN plus `debug_assert!(false)`, so a panic in debug builds), and each `vk::AccelerationStructureKHR` (no `Drop`) leaks, with the AS left alive over a freed buffer.
  - Phase 2 and Phase 3 do the same for the whole batch.
- **Impact**: AS handle leak (and a debug-build panic) exactly when the admission gate or budget is being stressed. The caller treats `Err` as "RT loses the tail", so the leak repeats on every retried batch.
- **Suggested Fix**: Wrap the three exits in the same unwind the later phases use (a small `unwind_prepared(prepared, query_pool)` helper).

#### REN-D5-2026-09-26-06: BLAS scratch is retired before its replacement exists — a failed reallocation leaves `blas_scratch_buffer == None` while live skinned BLAS need it
- **Severity**: MEDIUM (error path; degraded RT on animated actors until another build path allocates scratch)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/memory.rs` — `shrink_blas_scratch_to_fit` (`pending_destroy_scratch.push(old)`, then `create_device_local_uninit`, whose `Err` arm says "leave `blas_scratch_buffer` as `None` … degraded but correct"); `crates/renderer/src/vulkan/acceleration/blas_static.rs` — `build_blas_batched` Phase 2 (same order, then `?`); `crates/renderer/src/vulkan/acceleration/blas_skinned.rs` — `build_skinned_blas_batched_on_cmd` (immediate old destroy, then allocate). Consumer: `refit_skinned_blas` (`blas_scratch_buffer absent`).
- **Status**: NEW
- **Description / Evidence**:
  - I confirmed the ordering in both `shrink_blas_scratch_to_fit` and Phase 2. The old buffer is moved into `pending_destroy_scratch` first, then the allocation is attempted.
  - "Degraded but correct" holds only when no BLAS survives. The shrink deliberately sizes to the union peak over static and skinned BLAS, so it is reachable with skinned BLAS live. After a failed realloc, `record_skinned_blas_refit` gets `Err` per dirty entity per frame, and `refit_count` (bumped only on success) never reaches the forced-rebuild limit.
  - The TLAS sibling was fixed with allocate-then-swap (#2915). Deferral (#1782) fixes lifetime, not the `None` state.
- **Impact**: Raster is unaffected. RT shadows, reflections and GI of animated NPCs freeze at their last BLAS pose, with per-frame WARN spam, in the VRAM-pressure regime. Recovery needs some other path to allocate scratch.
- **Suggested Fix**: Allocate the replacement first and retire the old buffer only on success, as `shrink_tlas_scratch_to_fit` does. In `build_skinned_blas_batched_on_cmd` keep the old scratch on `Err`. The immediate free stays valid there because of `draw_frame`'s all-slots wait.

#### REN-D5-2026-09-26-07: `recreate_descriptor_sets` rewrites only live textures — reserved-but-unflushed and dead handles sample an unwritten PARTIALLY_BOUND descriptor after any resize
- **Severity**: MEDIUM (undefined shader read of an unwritten descriptor; needs `BYRO_VALIDATION=1` / GPU-assisted validation to observe)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/texture_registry/mod.rs` — `TextureRegistry::recreate_descriptor_sets` (the `rewrites` collection uses `entry.texture.as_ref()`; the per-slot queue is discarded by `pending_set_writes[..].clear()`); redirect contract in `crates/renderer/src/texture_registry/upload.rs` (`enqueue_dds_for_view`) and `release.rs` (`drop_released_texture`)
- **Status**: NEW
- **Description / Evidence**:
  - The pool and sets are recreated, so every element starts unwritten. Only entries with `texture: Some` are rewritten (confirmed in code). The loop comment says dropped slots "will be redirected to the fallback on their next update", but the redirect in `drop_released_texture` is a one-shot at drop time and never re-runs. The queued redirect writes are discarded by the same `clear()`.
  - `texture: None` entries are: (i) reserved and unflushed slots, for which `enqueue_dds_for_view` explicitly writes the fallback "so a draw before the flush samples the checkerboard"; (ii) dead handles from a failed flush (#1922), which stay cache-hit targets and keep being handed to materials; (iii) dropped slots.
  - After a resize, (i) and (ii) are sampled by live draws while unwritten. (i) needs a resize during a multi-frame streaming apply. (ii) needs only a truncated or corrupt archive texture plus any later resize.
- **Impact**: An undefined read of a never-written descriptor: garbage or black sampling, and on some drivers a fault. This is the exact hazard the fallback redirect exists to prevent.
- **Suggested Fix**: In the rewrite pass, write the D2 or cube fallback descriptor for every `texture: None` entry. `TextureEntry` needs to keep its `view_kind`, or derive it from the queue. The `rewrites` plan is pure and can be pinned with a device-free test.

#### REN-D5-2026-09-26-08: `recreate_descriptor_sets` is not transactional — a failure after `destroy_descriptor_pool` leaves a destroyed pool handle that the rollback and `destroy()` destroy again
- **Severity**: MEDIUM (failure path only; needs fault injection)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/texture_registry/mod.rs` — `TextureRegistry::recreate_descriptor_sets`; callers `crates/renderer/src/vulkan/context/resize.rs` (`recreate_texture_ssao_bindings`, the `set_upscaler_mode` rollback), `TextureRegistry::destroy`
- **Status**: NEW
- **Description / Evidence**:
  - I confirmed the order in code: `destroy_descriptor_pool(self.descriptor_pool)`, then later `self.descriptor_pool = create_descriptor_pool(..).context(..)?` and `allocate_descriptor_sets(..).context(..)?`.
  - On `Err`, `self.descriptor_pool` still names the destroyed pool and `bindless_sets` names freed sets. The #2156 rollback in `set_upscaler_mode` re-enters `recreate_swapchain` → `recreate_descriptor_sets` and destroys the same handle again. A fatal resize failure then reaches `TextureRegistry::destroy` for a third destroy.
  - The sampler-replacement half is fine: `create_material_samplers` runs before any destroy.
- **Suggested Fix**: Create the new pool and sets before destroying the old ones, or null `self.descriptor_pool` and clear `bindless_sets` immediately after the destroy. Destroying a null handle is a no-op, which `destroy()` already tolerates from the partial-init path.

#### REN-D5-2026-09-26-09: `parse_dds` never validates that cubemap faces are square before `CUBE_COMPATIBLE` image creation
- **Severity**: MEDIUM (untrusted-input reader passes an invalid image description to `vkCreateImage`; defence in depth)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/dds.rs` — `parse_dds` (legacy `DDSCAPS2_CUBEMAP` path and the DX10 `D3D10_RESOURCE_MISC_TEXTURECUBE` path); consumer `crates/renderer/src/vulkan/texture.rs` (`record_dds_upload`, `ImageCreateFlags::CUBE_COMPATIBLE`, six array layers)
- **Status**: NEW
- **Description / Evidence**: The #4511 caps check `0 < w,h <= 8192` and clamp `mip_count`. A grep of `dds.rs` finds the six-face caps check and the cubemap flags but no `width == height` check. Vulkan's valid-usage rules for `CUBE_COMPATIBLE` images require equal width and height. Archive-supplied DDS (BSA, and BA2 via `build_dds_header`, which passes u16 dimensions straight through) can be mod-authored.
- **Impact**: Likely a driver error routed through the finding-02 path, but the behaviour is undefined rather than guaranteed.
- **Suggested Fix**: `ensure!(width == height)` when `legacy_cubemap || is_cubemap`, with a test on the existing `legacy_cubemap_exposes_six_faces` / `dx10_cubemap_exposes_six_faces` fixtures using unequal dimensions.

#### REN-D5-2026-09-26-10: The ground-cover blade draw issues a multi-draw indirect on a device that may lack `multiDrawIndirect`; #4827's "every indirect consumer is gated" pin does not cover it
- **Severity**: MEDIUM (a Vulkan spec violation where reachable, downgraded from HIGH only because no RT-capable device in the supported class lacks `multiDrawIndirect`)
- **Dimension**: Memory/Lifecycle (device-feature degradation)
- **Location**: `crates/renderer/src/vulkan/groundcover.rs` (`GroundCoverPipeline::record_draw`: `cmd_draw_indirect(.., self.frame_chunk_count, 16)` per LOD stream), `crates/renderer/src/vulkan/context/init.rs` (`groundcover` creation gated on `device_caps.ray_query_supported` only), `crates/renderer/src/vulkan/device.rs` (`caps_tests::first_instance_feature_is_enabled_and_every_indirect_consumer_is_gated`)
- **Status**: NEW (sibling of closed #4827)
- **Description / Evidence**:
  - I confirmed both halves in code. `create_logical_device` enables `multiDrawIndirect` only `if caps.multi_draw_indirect_supported`. `#4827` routed the main-batch path and the model tier through `DeviceCapabilities::indirect_draws_supported()`. The blade draw is a third indirect consumer with `drawCount = frame_chunk_count` (up to `GROUNDCOVER_MAX_CHUNKS` = 256 per stream).
  - With the feature off this violates VUID-vkCmdDrawIndirect-drawCount-02718 (drawCount must be 0 or 1). Its `firstInstance` is always 0, so only the multi-draw bit is at issue.
  - The pin test's name claims "every indirect consumer" but scans only `geometry_pass.rs`, `build_and_upload_instances.rs` and the model-tier creation site in `init.rs`. It passes today.
- **Impact**: A per-frame spec violation on any exterior with ground cover, on a device without `multiDrawIndirect`.
- **Suggested Fix**: Either create `groundcover` only when `multi_draw_indirect_supported`, or fall back to one `cmd_draw_indirect` per chunk. Or make `multiDrawIndirect` a hard requirement in `is_device_suitable` and delete the `multi_draw_indirect_supported` branches. Extend the pin to `groundcover.rs`. **Needs a `BYRO_VALIDATION=gpuav` run on a feature-limited device**; do not change the draw ordering blind.

#### REN-D5-2026-09-26-11: A failed dynamic-RGBA staging step is now fatal to `draw_frame`; before `e2f99ad55` the same failure was a logged, skipped HUD frame
- **Severity**: MEDIUM (a transient allocation failure in a non-essential overlay path terminates the process)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/texture_registry/dynamic_rgba.rs` (`TextureRegistry::record_pending_rgba_uploads`: `GpuBuffer::create_host_visible`, mapped write, the two `anyhow::ensure!`), `crates/renderer/src/vulkan/context/begin_frame_recording.rs` (`return Err(e)` after replacing the acquire semaphore), `byroredux/src/app_frame.rs` (`Err(e) => { log::error!("Draw failed: …"); event_loop.exit(); }`), `byroredux/src/hud.rs` (`MenuXmlHud::upload_frame`)
- **Status**: NEW
- **Description / Evidence**:
  - `update_rgba` / `write_rgba_inplace` now only queue pixels. The copy is recorded at the top of the next frame. Growing the per-slot arena (`create_host_visible`) or a mapped-write or flush error returns `Err` from `record_pending_rgba_uploads`. I confirmed that `begin_frame_recording` propagates it and that the `draw_frame` caller ends with `event_loop.exit()`.
  - The HUD and Scaleform drivers were written for the opposite contract: their `Err` arms only log.
  - The failure is handled cleanly (the semaphore is replaced, the command buffer is freed later) but fatally.
- **Impact**: Under host-visible / BAR pressure (the arena needs 8.3 MB at 1080p and 33 MB at 4K per overlay frame), a cosmetic HUD update kills the session instead of dropping one HUD frame.
- **Related**: #4608, #3429.
- **Suggested Fix**: On `Err`, log once, drop or retain the dirty updates for the next frame, and continue recording without the copy. Keep the fence-idle `ensure!` fatal only if it indicates a sequencing bug.

### LOW

#### REN-D5-2026-09-26-12: `recreate_swapchain_core` has two `?` windows between creating the new swapchain and retiring the old one
- **Severity**: LOW (every caller treats a resize failure as fatal and exits; the leak is on an exit path only)
- **Dimension**: Memory/Lifecycle (resize error paths)
- **Location**: `crates/renderer/src/vulkan/context/resize.rs` — `recreate_swapchain_core`
- **Status**: NEW
- **Description / Evidence**: I confirmed the order in code. `old_image_views` and `old_swapchain` are held in locals, then `swapchain::create_swapchain(..)?`, `FrameExtentSet::for_output(..)?` and `FsrTemporalState::new(..).context(..)?` all run *before* the old-view destroy loop and `destroy_swapchain(old_swapchain)`. (A) If `create_swapchain` fails, `old_image_views` is dropped (handles leaked) and `Drop` destroys the still-recorded old swapchain. (B) If either later `?` fails, `self.swapchain.state` is already the new swapchain, so both `old_image_views` and the retired `old_swapchain` leak. `Drop` never destroys the retired swapchain, which violates VUID-vkDestroySurfaceKHR-surface-01266 at exit.
- **Suggested Fix**: Move the old-view destroy loop and `destroy_swapchain(old_swapchain)` to immediately after `create_swapchain` succeeds and before the extent and FSR computations. It is a host-only reorder that keeps the LIFE-M1 constraint. Extend `old_image_views_destroyed_between_new_swapchain_creation_and_old_destroy`.

#### REN-D5-2026-09-26-13: Failure-path policy is inconsistent across the upload orchestrators (forget vs destroy vs no rollback)
- **Severity**: LOW (defence in depth)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/buffer.rs` (`GpuBuffer::create_device_local_buffers_batched`: `std::mem::forget(staging); std::mem::forget(buffers)`), `crates/renderer/src/texture_registry/upload.rs` (`flush_upload_batch` error arm, the `flush_pending_uploads` doc comment), `crates/renderer/src/mesh.rs` (`upload_scene_mesh`, `register_scene_mesh_keyed`, `upload_scene_mesh_global_only`)
- **Status**: NEW
- **Description / Evidence**: (a) The batched buffer path forgets the whole staging arena and every destination buffer on *any* `with_one_time_commands_reuse_fence` error, including the pre-submit failures where nothing was submitted. That strands their `Arc<Mutex<Allocator>>` clones and defeats `Arc::try_unwrap` at shutdown. (b) `flush_upload_batch` destroys immediately on the same error class, while the `flush_pending_uploads` doc says the staging buffers "leak into the pool". (c) `upload_scene_mesh` and `register_scene_mesh_keyed` call `accumulate_global_geometry` before `self.upload(..)?` and do not roll back on failure, unlike `upload_scene_meshes_batched`.
- **Suggested Fix**: Have `with_one_time_commands_inner` return a typed error separating "nothing submitted" (safe to destroy) from "submit or wait failed" (ambiguous), and branch on it in both orchestrators. Extract the batched path's pool rollback and use it in the single-mesh paths. Fix the doc.

#### REN-D5-2026-09-26-14: memory-budget.md has no row for the new `dynamic_rgba` staging arenas, and its Scaleform and MenuXml sections describe a retired mechanism
- **Severity**: LOW (a resource owner with no ledger row; close to MEDIUM at 4K, where the page's own native total already exceeds its < 4 GB target)
- **Dimension**: Memory/Lifecycle
- **Location**: `docs/engine/memory-budget.md` ("Scaleform UI (Ruffle / wgpu)" table and prose, "MenuXml HUD overlay textures", "Not yet ledgered"), `crates/renderer/src/texture_registry/dynamic_rgba.rs` (`DynamicRgbaUploads::staging`), `crates/renderer/src/vulkan/texture.rs` (`Texture::overwrite_rgba_pixels`)
- **Status**: NEW (window commit `e2f99ad55`; the closed #4526 row still exists but the mechanism under it changed; #4872 covers other rows)
- **Description / Evidence**:
  1. Arena. `DynamicRgbaUploads.staging` is `[Option<GpuBuffer>; MAX_FRAMES_IN_FLIGHT]`, `CpuToGpu`, grow-only, sized to the sum of dirty updates per frame. One overlay frame is 8,294,400 B at 1080p (×2 slots = 16.6 MB) and 33,177,600 B at 4K (×2 = 66.4 MB), doubling if both overlays are dirty. It is not a `*_scratch` field, so the #4610 guard cannot see it.
  2. Scaleform section. It says an animating HUD "cycles a fresh full-viewport `VkImage` every frame (#3429)". `update_rgba` now returns `dynamic_rgba.queue(..)` whenever `can_update_rgba(w, h)`, so only an extent or format change recreates the image. `docs/engine/ui.md` was updated in `e2f99ad55`, memory-budget.md was not.
  3. MenuXml section. It still cites the 3-buffer rotation as the hazard contract of `overwrite_rgba_pixels`. `write_rgba_inplace` no longer reaches it, and it has no production caller. Synchronisation is now barriers in the frame command buffer. Two of the three swapchain-extent textures (16.6 MB at 1080p, 66 MB at 4K) are avoidable residency that the ledger still justifies by a contract that is gone.
- **Impact**: The ledger understates resident memory by 16–66 MB (more with both overlays) and misdescribes two sections.
- **Related**: #4526, #4516, #4515 (closed), #4872 (open), #4608.
- **Suggested Fix**: Add a "dynamic RGBA staging arenas + pending pixel copies" row (formula above), rewrite the two sections to the queued-copy model, decide whether the 3-texture rotation collapses to 1, and delete or `#[cfg(test)]`-gate `overwrite_rgba_pixels`. Fix the stale `update_rgba` sentence in `crates/ui/src/player.rs` (`SwfPlayer::render`).

#### REN-D5-2026-09-26-15: Doc and comment drift in Dim 5 owners (bundle)
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle (doc rot)
- **Status**: NEW, except where marked
- **Items** (doc claim → code fact):
  1. `docs/engine/memory-budget.md` "Deferred-Destroy Queue": describes `VecDeque<(frame_id, T)>` freed at `current_frame - frame_id >= countdown` and omits the BLAS-scratch queue and the retired instance-buffer pair. The shared primitive is `DeferredDestroyQueue<T>` = `Vec<(T, u32)>` with a per-tick countdown, destroyed on the (`DEFAULT_COUNTDOWN`+1)th tick (`default_countdown_survives_max_frames_in_flight_ticks`). Only the texture registry uses `VecDeque` plus frame ids. `crates/renderer/src/deferred_destroy.rs` says "Three production users today"; `SceneBuffers::retired_instance_buffers` (#4199) is a fourth.
  2. memory-budget.md "Texture Registry": "Bindless array ceiling `min(maxPerStageDescriptorUpdateAfterBindSampledImages, 65 535)`". `device.rs` computes that, but `init.rs` passes `max_bindless_sampled_images / 2` per binding, so the 2D and cube arrays are each 32,767 (one shared index space). Real slot capacity is half the documented ceiling, which halves the #2030 exhaustion horizon. The section also omits `MAX_UPLOAD_BATCH_BYTES` (128 MiB per submit, #4197) and its claim that "GPU image memory itself IS correctly reclaimed" is false for the finding-01 orphans.
  3. `docs/engine/exterior-grid-streaming.md`: "Queued texture uploads are flushed per yielded reference slice". The code flushes only at ≥ 64 uploads or ≥ 128 MiB (`should_flush_pending_cell_textures`). This is what makes finding 01 reachable.
  4. `docs/engine/archives.md` (DDS header reconstruction): "caps1 = TEXTURE | MIPMAP | COMPLEX" unconditional. `build_dds_header` sets MIPMAP|COMPLEX only when `num_mips > 1` (COMPLEX also for cubemaps). The stale `ba2.rs` comment "renderer's dds.rs is lenient (ignores arraySize)" is false (arraySize must be 1 or 6), and "valid for our DDS parser" omits the 8192 cap.
  5. `crates/renderer/src/texture_registry/upload.rs` `flush_pending_uploads` doc: "the staging buffers leak into the pool" → the code destroys them (finding 13).
  6. `docs/engine/shader-pipeline.md`: never says the instance and previous-model SSBO pair starts at `INITIAL_INSTANCE_CAPACITY` (65,536) and grows per slot, nor that a grow rewrites Set 1 bindings 4 and 18 in place (and the caustic set's binding 5). Its `MAX_TERRAIN_TILES … 32 B each` row (actual 160 B) is **Existing #4870 item (5)**, not re-filed.
  7. `crates/renderer/src/vulkan/acceleration/predicates.rs` (`screen_scaled_reservation_bytes` doc) links `FrameUpscaler::resident_bytes`, which no longer exists; the cached figure is `FrameUpscaler::sdk_memory_bytes`.
  8. `crates/renderer/src/vulkan/device.rs` `RT_EXTENSIONS` doc says "Optional RT extensions (enabled when available)"; they are mandatory since #3759.
  9. `crates/renderer/src/vulkan/gpu_timers.rs` test doc: "`QUERIES_PER_FRAME` (40) … 20 start/end brackets" vs `QUERIES_PER_FRAME = 56` (28 brackets). The module header is right.
  10. `crates/renderer/src/vulkan/context/mod.rs`: the `pending_skin_unload_victims` doc cites a bare `mod.rs` line number for a loop that now lives in `teardown.rs::destroy_allocator_owned_resources`, and the struct comment "later fields are destroyed first" is inverted (Rust drops in declaration order).
  11. Pre-existing, out of window: per-entity `SkinSlot::output_buffer` (DEVICE_LOCAL, `vertex_count × 12 B`, up to `SKIN_MAX_SLOTS`) and `WaterPipeline::param_buffers` have no memory-budget.md row.
  12. **Existing #4872** (still current on both counts: the 7 vs 0/14/28 MiB model-tier tail, and the dangling volumetrics noise-volume row) and **#4869 / #4871** are not re-filed.
- **Suggested Fix**: One doc-sweep commit. Each item is a text change with the code fact quoted above.

#### REN-D5-2026-09-26-16: About a dozen `ray_query_supported == false` branches are still maintained although #3759 made them unreachable
- **Severity**: LOW (dead code with a real cost: it presents fallbacks that no longer exist)
- **Dimension**: Memory/Lifecycle (device init)
- **Location**: `crates/renderer/src/vulkan/context/init.rs` (`create_allocator` arg, `SceneBuffers::new` arg, `accel_manager`, `skin_compute`, `skin_palette`, `groundcover`, `water`), `context/geometry_pass.rs`, `context/assemble_camera_and_lights.rs`, `context/dispatch_skin_and_cluster.rs`, `context/telemetry.rs`, `crates/renderer/src/vulkan/device.rs` (`create_logical_device`, `RT_EXTENSIONS` doc)
- **Status**: NEW (the follow-up #3759's comment promised was never filed)
- **Description / Evidence**: `is_device_suitable` returns `None` when `!ray_query_supported`, so `caps.ray_query_supported` is a constant `true` at every reachable site. The gates keep the rt-disabled descriptor-layout permutation and the `buffer_device_address` allocator flag alive. They also mask finding 10: the *real* optional cap is the one left un-gated.
- **Suggested Fix**: Delete the branches or collapse them behind one `debug_assert!`, and correct the `RT_EXTENSIONS` doc.

#### REN-D5-2026-09-26-17: Device selection probes extension presence, not the feature bits `create_logical_device` force-enables; `textureCompressionBC` is neither required nor consulted
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle (device init)
- **Location**: `crates/renderer/src/vulkan/device.rs` — `is_device_suitable`, `create_logical_device`, `DeviceCapabilities::texture_compression_bc`
- **Status**: NEW
- **Description / Evidence**: `is_device_suitable` requires the three RT extensions, `shaderInt64` and `synchronization2`. `create_logical_device` additionally force-enables `independentBlend`, `fragmentStoresAndAtomics`, Vulkan 1.2 `runtimeDescriptorArray`, `descriptorBindingPartiallyBound`, `descriptorBindingSampledImageUpdateAfterBind`, `shaderSampledImageArrayNonUniformIndexing` and `bufferDeviceAddress` without probing any bit. A device lacking one fails at `vkCreateDevice` instead of being skipped for the next candidate. `texture_compression_bc` is probed and enabled-if-supported but no consumer checks it, although every shipped Bethesda texture is BC1/3/5/7.
- **Impact**: On the RT-capable hardware class every bit is present, so this is asymmetry, not a live failure. It means no fallback to a second GPU, and BC is a misleading "optional" cap that is really mandatory.
- **Suggested Fix**: Make the force-enabled bits and BC hard requirements in `is_device_suitable` (one `get_physical_device_features2` chain already exists), so failure is a clean "No suitable GPU".

#### REN-D5-2026-09-26-18: Nothing pins that `AllocatorResource` is removed from the `World` before `VulkanContext` drops
- **Severity**: LOW (test gap; the code is correct today)
- **Dimension**: Memory/Lifecycle
- **Location**: `byroredux/src/main.rs` (`impl Drop for App`: `remove_resource::<AllocatorResource>()` then `self.renderer.take()`), `byroredux/src/app_events.rs` (`App::shutdown`)
- **Status**: NEW
- **Description / Evidence**: I confirmed by reading that `shutdown` removes the resource, drains streamed cells, flushes pending destroys and then takes the renderer, and that `Drop for App` repeats it idempotently, so the panic-unwind and non-`CloseRequested` exits are covered. The only protection is the comment "INVARIANT (REG-08 / #1640, #1477)". No test scans either site. Reversing the two lines re-arms the #665 leak-guard branch (device, surface and instance leaked; a `debug_assert!` panic in debug).
- **Suggested Fix**: A source-scan test (runtime-composed needles over production text) asserting the `AllocatorResource` removal precedes `renderer.take()` in both `Drop for App` and `shutdown`.

#### REN-D5-2026-09-26-19: #4599 (poisoned-allocator `.expect()` reachable from teardown) is still open and unchanged in substance; its site list is incomplete
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs` (`destroy_depth_resources`: `.lock().expect("allocator lock poisoned")` and `.free(..).expect("Failed to free depth allocation")`; `destroy_staging_buffer`: `alloc.lock().unwrap()`), `crates/renderer/src/vulkan/context/teardown.rs` (`Drop`: `transfer_fence.lock().expect(..)` and `mutex.into_inner().expect("allocator lock poisoned")`)
- **Status**: **Existing #4599** (OPEN)
- **Description / Evidence**: No fix landed; `GpuImage` is still the only type that recovers poison via `into_inner()`. Sites not in the issue text: `destroy_depth_resources` (called from both `Drop` and the resize path), `destroy_staging_buffer` (`Drop`), and the `transfer_fence` mutex `.expect` in `Drop` (a second poisoned-lock sink on a different mutex). None is new in the window; the new `dynamic_rgba` arena reaches teardown through the existing `GpuBuffer::destroy`.
- **Suggested Fix**: Add these three sites to #4599's sibling checklist when it is worked.

#### REN-D5-2026-09-26-20: `instance_map_scratch` (#4193) is the only per-frame scratch `Vec` with no shrink
- **Severity**: LOW (host RAM only, ≤ 2 MiB at 262,144 draws)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/mod.rs` (`ScratchBuffers::instance_map_scratch`), `crates/renderer/src/vulkan/acceleration/predicates.rs` (`build_instance_map`: `out.clear(); out.reserve(len)`), `crates/renderer/src/vulkan/context/shrink_frame_scratch.rs` (`shrink_frame_scratch`, four `Vec`s only)
- **Status**: NEW (sibling of the #2486 shrink-policy class; #4610 gave it a telemetry row but no shrink)
- **Description / Evidence**: `Vec<Option<u32>>` (8 B/entry) reserves to the draw count each frame and is never passed to `shrink_scratch_if_oversized`, unlike `gpu_instances_scratch`, `frame_lights_scratch`, `previous_models_scratch` and `batches_scratch`. One large exterior frame pins its capacity for the session.
- **Suggested Fix**: Add it to `shrink_frame_scratch` with the standard `2 × max(working, 512)` band.

#### REN-D5-2026-09-26-21: The `sync.rs` frames-in-flight rider list omits the ground-cover model tier and the skin/morph slot destroys, and rider 3 still says "blocking"
- **Severity**: LOW (doc and tripwire gap; the all-slots wait itself holds and is pinned by `the_all_slots_wait_argument_is_pinned`)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/sync.rs` (the #870/#3643/#4601 rider list and `frames_in_flight_contract_names_every_dependent_resource`), `crates/renderer/src/vulkan/groundcover_models.rs` (`GroundCoverModelTier::prepare` / `harvest`), `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (skin/morph slot eviction)
- **Status**: NEW (sibling of closed #4601)
- **Description / Evidence**: `GroundCoverModelTier::prepare` host-writes the per-slot record, table and shape buffers and `harvest` reads `stats_readback[frame]` from `app_frame.rs` *before* `draw_frame`'s own wait. That is the identical dependency rider 8 records for `groundcover.rs::prepare`, but it is neither listed nor test-pinned. The immediate `SkinSlot` / `MorphSlot` destroys are named in `sync_and_acquire_frame.rs`'s #3442 comment but not in the `sync.rs` list. Rider 3 says `terrain_tile_buffer` is "overwritten by a blocking staged copy"; since #3664 it is a recorded frame copy (`upload_terrain_tiles_records_nonblocking_frame_copy`), so the hazard is unchanged but the wording is stale.
- **Suggested Fix**: Add the three entries and extend the `(resource, owner)` table in the test, so a future narrowing of the all-slots wait (#4606's plan) audits a complete list.

## Prioritized Fix Order

Correctness first, then safety, then cost.

1. **01** — skip `ref_count == 0` in `flush_upload_batch` and purge the queue on release. It is device-free testable, and it is the only finding here that leaks on a normal-operation path.
2. **02** (with **#4854**) — reorder `create_image_view` ahead of recording, or route through `GpuImage::create`. This removes a submit-after-free.
3. **07 / 08** — make `recreate_descriptor_sets` write fallbacks for `texture: None` entries and be transactional.
4. **04 / 05 / 06** — one `create_bound_buffer` unwind helper, the `build_blas_batched` early exits, and allocate-before-retire for BLAS scratch.
5. **10 / 09** — gate or replace the ground-cover blade multi-draw, and reject non-square cubemaps at parse.
6. **03** — record the create size per `StagingPool` entry and add the real-bytes gauge. Measure with a grid-soak first.
7. **11 / 12 / 13** — HUD failure policy, resize `?` windows, orchestrator failure-path policy.
8. **14 / 15 / 20 / 21 / 16 / 17 / 18 / 19** — ledger, docs, tripwires and dead-code cleanup (one doc-sweep commit plus small tests).

## Needs-RenderDoc / live validation

- **02** and **04** (bind and submit arms) need a `BYRO_VALIDATION=1` fault-injection run (forced `vkCreateImageView` / bind / submit failure) to observe the submit-with-destroyed-buffer and leaked-handle reports. The allocate arms are visible in code.
- **07** needs GPU-assisted validation, or a resize during a streaming apply, to observe the unwritten-descriptor read.
- **10** needs a `BYRO_VALIDATION=gpuav` run on a device without `multiDrawIndirect`. Do not change the draw ordering blind.
- **03** needs a grid-soak logging the sum of per-entry `VkBuffer` create sizes against `StagingPool::total_capacity()` for the three pools.
- **01** needs telemetry counting cancels with a non-empty `pending_dds_upload_count()`, to settle HIGH versus MEDIUM.
- **Morph slot stamps after resize** (not filed): after a swapchain resize all morph LRU stamps are rebased to the "never dispatched" sentinel, which `should_evict_skin_slot` skips, so a dead entity's slot inside its 3-frame aging window at that instant is no longer reaped by the idle sweep. It is reaped only if `unload_cell` queued it. It needs a repro before it can be filed.

## Stale skill premises (for the next `/audit-renderer` sync)

1. **Dim 5 baseline list.** A 2026-09-24 full renderer audit filed REN-D5-2026-09-24-01/02/03/05 (#4830 and #4835 closed; #4872 and #4854 open) from a report that is not committed to `docs/audits/`. There is no D5-04 or D5-06 issue; D5-06 was merged into #4872.
2. **`FrameUpscaler::resident_bytes`** is now `sdk_memory_bytes`. `screen_scaled_reservation_bytes` is `pub(super)` in `acceleration/predicates.rs`, and the per-pass enumeration is `render_extent_bytes_per_pixel_x1024`.
3. **Resize checklist bullet.** The exposure meter has no size-derived resource (its dispatch rewrites its scene-view descriptor every time), and ground cover rebuilds only its render-pass-bound pipelines on a surface-format change, never on a plain resize. The real resize rebuilds are volumetrics, composite, reservoirs, SSAO, caustics and water caustic.
4. **`AllocatorResource` bullet.** `app_events.rs::shutdown` is only the normal-exit half. The panic-unwind guarantee lives in `main.rs` `impl Drop for App`.
5. **`drawIndirectFirstInstance` "degrade explicitly".** True for the main batches and the model tier, not for the ground-cover blade multi-draw (finding 10).
6. **Deferred-destroy bullet.** `build_skinned_blas_batched_on_cmd`'s immediate grow-destroy is sound because of the *both-slots* `wait_for_fences` in `sync_and_acquire_frame` (#3643/#870 rider 1), not "its own fence wait". The slot-local argument alone would be unsound at N ≥ 3.
7. **Shrink wiring bullet.** The two TLAS shrinks run only at the end of `draw_frame`, on the *next* slot (rider 7). Cell unload and swapchain recreate run `shrink_blas_scratch_to_fit` only, and `shrink_tlas_to_fit` no longer destroys anything (it records `tlas_shrink_pending`).
8. **Instance-buffer rebind bullet.** Only the caustic set (binding 5) needs `rebind_instance_buffer` after a grow. Volumetrics and the ground-cover model tier rewrite theirs every frame they are used, and cluster, UI and water sets do not name it.
9. **Retired instance-buffer pairs** are destroyed on the (`DEFAULT_COUNTDOWN`+1)th tick (3 ticks at N = 2), not "`MAX_FRAMES_IN_FLIGHT` frames".
10. **Morph bullet.** "An evicted slot must be *recreated* when the entity is next seen" is stale. `MorphSlot` is created once at spawn (`try_spawn_morph_slot`) and never recreated; only `SkinSlot` is lazy.
11. **Texture-registry bullet.** "Refcount/handle-slot recycling can't hand a still-in-flight slot to a new texture" describes a hazard class that does not exist: slots are never recycled (#372/#2030). The real risks are slot exhaustion (32,767 handles) and the orphan slots of finding 01.
12. **Placeholder bullet.** `placeholder.rs` holds exactly two `PlaceholderImage`s (1×1 R8 white AO and a 1×1 caustic storage sink). The 256×256 checker and 1×1 neutral-white fallbacks are `TextureRegistry` textures at handles 0 and 1, and there is no normal placeholder.
13. **`078f650ec` "split precombine uploads".** No renderer-side upload code changed; the split is cell-loader side and reuses `upload_scene_meshes_batched`. The renderer-side new upload path is `e2f99ad55`'s `texture_registry/dynamic_rgba.rs`, which the skill lists only as a file. `write_rgba_inplace` and `update_rgba` now queue through it, and `Texture::overwrite_rgba_pixels` is dead in production.
14. **Add to the Dim 5 checklist.** (a) A sibling-unwinding sweep for `buffer.rs` and `texture.rs` constructors (findings 02/04/05). (b) `StagingPool` label versus create size (finding 03). (c) Reserved-slot lifecycle: queued uploads versus `ref_count == 0` (finding 01). (d) Transactionality of `recreate_descriptor_sets` and the fallback rewrite (findings 07/08). (e) The `staging_guard_coverage_tests` and `no_file_outside_this_module_rolls_its_own_image_chain` allow-lists are exemptions, not coverage.

## Guard posture

- **Named guards.** `planning_compaction_does_not_publish_offsets`, `chunked_rebuild_consults_the_idle_threshold_before_duplicating` and `skin_slot_drain_sits_outside_the_skin_compute_guard` all exist, are not `#[ignore]`d and pass.
- **Filtered run.** `cargo test -p byroredux-renderer --lib` over the filters `teardown deferred_destroy dds staging texture_registry geometry_ssbo geometry_rebuild memory_budget screen_scaled scratch_row placeholder allocator load_bearing` plus the three named tests: 172 passed, 0 failed, 0 ignored.
  - That covers 54 `vulkan::dds::tests`, 7 `dds_upload_guard_tests`, the `geometry_ssbo` step, deferred-compaction and compaction-gate tests, 7 `allocator` tests and the `texture_registry` set.
- **Sub-scope A** also ran 13 narrow guards (ordering-count, FSR teardown order, `release_shared_destroys_on_last_strong_ref_despite_a_weak`, `no_file_outside_this_module_rolls_its_own_image_chain`, the #4827 pair, `blas_budget_is_recomputed_after_the_upscaler_catches_up`, and others), all green.
- **What the guards cannot see.** All 21 findings are failure-path, accounting or lifecycle-ordering faults with no source-shape signature.
  - `staging_guard_coverage_tests` exempts `buffer.rs` and `no_file_outside_this_module_rolls_its_own_image_chain` allow-lists `texture.rs`.
  - `first_instance_feature_is_enabled_and_every_indirect_consumer_is_gated` scans three files, not "every consumer".
  - Three staging tests pin the request-size rule that finding 03 is the accounting cost of.

## Process notes

- Dim 5 ran as three read-only sub-agents in parallel, split by sub-scope (A teardown/init/resize, B deferred-destroy/buffers/AS/morph, C geometry SSBO/textures/DDS). Findings were merged and de-duplicated across scopes:
  - Finding 02 merges A-05 and C's `record_dds_upload` finding.
  - Finding 03 merges B-11 and C-23.
  - Finding 04 merges B-12 and C-24.
  - Finding 14 merges A-03, B-15 and C-29.
- **Re-read by the orchestrator against current code**: findings 01, 02 (call order and closure behaviour), 03 (`acquire` / `release` / `release_to` mechanism), 04 (`create_host_visible`), 05, 06, 07, 08, 09 (absence of a square check), 10, 11, 12 and the #4870 overlap. The rest were confirmed by the sub-agents reading code.
- **Not independently reproduced**:
  - The 0.88 GiB–3.19 GiB magnitudes in finding 03 come from a Python model of the pool rules, not from a live measurement.
  - The claim in finding 02 that the vendored gpu-allocator 0.28.0 frees emptied blocks with `vkFreeMemory` was checked by the sub-agent only.
  - Finding 09's driver outcome is unverified.
- No engine was launched, no GPU test was run beyond the unit-test guards, and no source was modified.
- Dedup used the open-issue snapshot and a 600-issue open+closed snapshot. Closed #1921 is older than that snapshot, so its text was read from `.claude/issues/1921/ISSUE.md`.
- Scratch notes are `/tmp/audit/renderer/dim_5_{A,B,C}.md`, removed in Phase 4.
