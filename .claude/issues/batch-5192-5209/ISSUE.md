# Batch 5192 + 5200–5209 (renderer-audit leftovers, 2026-10-03 report `AUDIT_RENDERER_2026-10-03.md` @ f002763b4)

## #5192 [OPEN] REN-D10-2026-10-03-02: soft-light wrap / back-light / thick-translucency lobes self-occlude on closed meshes
labels: bug, renderer, medium, game:fo4, game:skyrim, shaders

**Severity**: MEDIUM (visual only, flagged materials: Skyrim soft lighting/tint, FO4 BGSM translucency_thick_object skin).
**Location**: `crates/renderer/shaders/include/lighting.glsl` (`shadowableLightRadiance`: `bethesdaDiffuseLightFactor` wrap, `bethesdaBackFactor`, `MAT_FLAG_TRANSLUCENCY_THICK_OBJECT` arm); `crates/renderer/shaders/triangle.frag` ReSTIR finalize (`offsetRayOriginForDirection` from #5018, `visibility = mix(vec3(1.0), transmissionFrame, shadowFade)`); `acceleration/predicates.rs shadow_mask_for_instance`.

**Description**: The transmission-family lobes are non-zero only where `rawNdotL < 0`. On a watertight mesh `Ng·L < 0` there too, so #5018's direction-aware origin starts the shadow ray just inside the body; the far wall commits as opaque (no facing-cull flag, #4580), `traceShadowTransmittanceDetailed` returns 0, and the lobe is zeroed under every tracing light inside SHADOW_FADE_START (8k BU), fading in unshadowed 8k–12k. ReSTIR pHat also includes these lobes → reservoir samples spent on guaranteed-dark candidates (noise on characters).

**Suggested Fix**: Split `shadowableLightRadiance` into a reflection part and a transmission part (back-light, translucency, wrap's `rawNdotL < 0` excess). Trace the transmission part with the fragment's own instance skipped, or leave it unshadowed with existing `shadowFade` semantics. Pin with a `shader_contract` test. Accept via before/after capture of a Skyrim soft-lit head + FO4 thick-translucency behind-lit surface (RenderDoc). Pairs with REN-D2-2026-10-03-01.

## #5200 [OPEN] REN-D1-2026-10-03-01: #4633's non-finite TLAS drops counted nowhere in TlasIntegritySnapshot → FAIL with all cause counters 0
labels: bug, renderer, low, vulkan

**Location**: `acceleration/tlas.rs` (`build_tlas_instances` non_finite_transform arm + snapshot assignment); `acceleration/mod.rs` (`TlasIntegritySnapshot`); `crates/core/src/ecs/resources/mod.rs` (`RtIntegrityStats`, `verdict`, `machine_line`); `context/telemetry.rs` (`fill_rt_integrity_stats`).

**Description**: #4633 added the 4th drop cause (non-finite model matrix → `tlas_instance_transform` returns None). The counter only reaches a rate-limited warn. `eligible_instances` is incremented before the arm; snapshot has fields only for the 3 older causes → `emitted < eligible` with all cause counters 0. The warn's `"; ..."` overflow marker also excludes the non-finite count (`missing_blas_total > missing_samples.len()`).

**Suggested Fix**: Add `non_finite_transform: u32` to snapshot + `RtIntegrityStats` + `machine_line`; fold into `verdict`; count in the overflow marker. Extend `every_blas_residency_accessor_reaches_the_rt_integrity_snapshot` or add a sibling test pinning the whole chain.

## #5201 [OPEN] REN-D1-2026-10-03-02: build_blas_batched destroys prepared/compacted BLAS on MaybeInFlight failures, against #4891 policy
labels: bug, renderer, low, vulkan, memory

**Location**: `acceleration/blas_static.rs` `build_blas_batched`: the `if let Err(e) = build_result` arm (calls `unwind_prepared(.., Some(query_pool))`) and the `if let Err(e) = copy_result` arm (destroys `prepared` originals + `compact_accels`).

**Description**: #4891 split one-time-submit failures into `NotSubmitted` / `MaybeInFlight`; rule = destroy when not in flight, leak when maybe in flight. `buffer.rs` + `texture_registry/upload.rs` follow it; both BLAS submissions use the same helper but never consult it; their SAFETY comments assert the contradicted premise. Fence-wait failure ⇒ build/copy cmd buffer may still execute while the arm destroys every AS it writes + its query pool (UB on an already-failing device).

**Suggested Fix**: Branch both arms on `OneTimeCommandError::may_be_in_flight(&e)`: unwind on NotSubmitted; `mem::forget` on MaybeInFlight. Correct both SAFETY comments. Source-pin via a `one_time_failure_class_tests`-style needle scan over `blas_static.rs`.

## #5204 [OPEN] REN-D3-2026-10-03-02: light ↔ identity parallel invariant enforced at 3 sites, only collect_lights tested
labels: bug, renderer, low, test-gap

**Location**: `context/assemble_camera_and_lights.rs` (post-combustion decorate re-sort via `light_resort_scratch`; `frame_light_ids.resize` only inside `if let Some(ref mut volumetrics)`); `byroredux/src/app_frame.rs` (loading-stage `keeps_light` compaction); `scene_buffer/upload.rs upload_lights` (only a `debug_assert_eq!` on length).

**Description**: #5055's `previousLightToCurrent[]` header requires every light's identity to take the same permutation/filter its GpuLight took. Site 2 (hand-written compaction) and site 3 (hand-copied re-sort inside a device-owning method, untestable) are untested. Release-mode length mismatch panics at `identities[..count]`.

**Suggested Fix**: Extract one pure helper `sort_lights_by_priority_with_ids(&mut [GpuLight], &mut [[u32;4]], directional_count, scratch)`, unit-test once (identities follow lights; directional prefix untouched), call from both sort sites. Hoist the `resize` out of the `if let`.

## #5205 [OPEN] REN-D4-2026-10-03-02: shader-pipeline.md early-test pipeline says "material kind 0" (stale since #5057) and names context/draw.rs as renderOrigin.w uploader
labels: documentation, renderer, low, pipeline, doc-rot

**Location**: `docs/engine/shader-pipeline.md` §Per-Frame Submission Order step 6; §Render-origin-relative.

**Description**: Since 3c197ed8c (#5057) `allows_early_fragment_tests` admits `material_kind <= MATERIAL_KIND_MAX_LIGHTING_SHADER` (16). The `renderOrigin.w` FSR-reset upload lives in `assemble_camera_and_lights.rs` (comment predates the #3282 split).

**Suggested Fix**: Step 6 → "material kind 0..=MATERIAL_KIND_MAX_LIGHTING_SHADER (16, reviewed BSLightingShaderProperty types; pinned by `early_fragment_kinds_have_no_discard_or_depth_write_path`)". Pointer → `context/assemble_camera_and_lights.rs`. Doc-pin test where one fits.

## #5206 [OPEN] REN-D5-2026-10-03-02: image→model LSCR switch overwrites retained Artwork::Image without drop_texture
labels: bug, renderer, low, memory

**Location**: `byroredux/src/loading_screen.rs` `spawn_model_stage` (the `self.artwork = Some(Artwork::Stage(..))` assignment).

**Description**: `Artwork::Image` is deliberately retained across transitions; `present_image_artwork` releases the previous image on replace; `spawn_model_stage` only drains `retired_stage` then overwrites — the retained image's texture handle is dropped on the floor, refcount never hits zero, bindless slot leaks until shutdown. One leak per image→model switch (needs a load order mixing image-only and model LSCRs).

**Suggested Fix**: Before assigning the stage, `if let Some(Artwork::Image { texture, .. }) = self.artwork.take() { ctx.texture_registry.drop_texture(&ctx.device, texture) }`, mirroring `present_image_artwork`.

## #5207 [OPEN] REN-D5-2026-10-03-03: recreate_descriptor_sets leaks the replacement samplers when pool creation or set allocation fails
labels: bug, renderer, low, vulkan, memory

**Location**: `texture_registry/mod.rs` `recreate_descriptor_sets`.

**Description**: On mip-bias change, 4 `VkSampler`s are created first; the following `create_descriptor_pool` returns via bare `?`; the `allocate_descriptor_sets` error arm destroys only `new_pool`. Local `Option<[vk::Sampler; 4]>` has no Drop → 4 sampler handles leak per failed recreate; #2156's `set_upscaler_mode` rollback re-enters → 4 more per retry.

**Suggested Fix**: Create replacement samplers after pool+sets succeed, or destroy them in both error arms. Extend the #4886 pin (`recreate_descriptor_sets_allocates_the_replacement_before_the_old_pool_dies`) to assert this.

## #5208 [OPEN] REN-D5-2026-10-03-04: volumetrics' per-slot host-visible buffers (incl. #4784's occupancy masks) have no memory-budget.md row
labels: documentation, renderer, low, memory, doc-rot

**Location**: `vulkan/volumetrics/init.rs` (`fog_volume_buffers`, `fog_cluster_buffers`, `fog_cluster_index_buffers`, `combustion_light_moment_buffers`, `combustion_occupancy_buffers`); ledger `docs/engine/memory-budget.md` §Volumetrics (M55).

**Description**: Unledgered: `fog_cluster_index_buffers` 4096 × (64+128) × 4 B = 3 MiB/slot (CpuToGpu, BAR-eligible), `fog_cluster_buffers` 4096 × 16 B = 64 KiB/slot, fog-volume upload buffer, 8 KiB/slot moment readback, `combustion_occupancy_buffers` 4096 × 4 B = 16 KiB/slot (32 KiB total, #4784). ~6.2 MiB host-visible total invisible to BAR budgeting (cf. #4889).

**Suggested Fix**: Add a per-slot host-buffer sub-table with sizes derived from named constants; optionally pin like `memory_budget_ledgers_the_sky_and_ground_cover_owners`.

## #5209 [OPEN] REN-D5-2026-10-03-05: VulkanContext::drop still expects the transfer-fence lock — #4599 poison policy covers allocator locks only
labels: bug, renderer, low, safety

**Location**: `vulkan/context/teardown.rs` (`.lock().expect("transfer fence lock poisoned")`); lock holder `with_one_time_commands_inner` (`vulkan/texture.rs`, `fence_guard`).

**Description**: #4599 routed allocator locks through `lock_recovering`/`into_inner_recovering`; `VulkanContext::drop` still `.expect`s `transfer_fence`. That mutex is held across reset/submit/wait; the panicking call inside the window is `queue.lock().expect("graphics queue lock poisoned")`. The #4599 pin counts only allocator spellings, so it can't see this site.

**Suggested Fix**: Use `allocator::lock_recovering(&self.transfer_fence)` in Drop; also for the queue lock inside the fence window. Extend the #4599 allowlist scan to the `expect("… lock poisoned")` family within `teardown.rs`.
