//! `VulkanContext` teardown — split out of `mod.rs` (#1749 / TD1-004),
//! the mirror image of `init.rs`. Contents moved verbatim: the
//! allocator-owned-resource destroy helper (already split out of the
//! destructor under #2406 / TD1-003) plus the destructor `impl` itself.
//! Destruction order inside is load-bearing and unchanged.

use super::*;

/// Teardown helper split out of `Drop` (#2406 / TD1-003).
impl VulkanContext {
    /// Destroy every subsystem whose resources are owned by the GPU
    /// allocator.
    ///
    /// **Not reverse-creation order** (corrected 2026-08-30,
    /// CONC-D6-2026-08-30-02) — an earlier revision of this doc claimed it
    /// was; the sequence actually starts with `texture_registry`, the
    /// *first*-created subsystem. After the `device_wait_idle` this
    /// function's contract requires, Vulkan imposes no cross-subsystem
    /// destroy ordering (a descriptor set may name a destroyed image view
    /// as long as it is never used again, and every parent/child pair is
    /// contained inside one subsystem's own `destroy`) — only four
    /// orderings below are load-bearing, each commented at its own site:
    /// `skin_slots` before `skin_compute`; placeholders after the passes
    /// whose descriptors name them; `frame_upscaler::destroy_allocations`
    /// after `destroy_device_objects`; and `exposure` before the
    /// `Arc::try_unwrap`. **Do not "restore" reverse-creation order** — that
    /// would reshuffle those four local constraints, and moving
    /// `skin_compute`'s pipeline/pool destroy ahead of its per-slot
    /// `free_descriptor_sets` is a real
    /// `VUID-vkFreeDescriptorSets-descriptorPool-parameter` violation.
    ///
    /// Extracted verbatim from `Drop` as one contiguous block — the densest
    /// branch cluster in the teardown (~25 `Option` arms, each paired with
    /// the subsystem's own `destroy`). The ordering inside is load-bearing
    /// and unchanged: this is a *move*, not a reorganisation. A flat,
    /// explicitly-ordered destroy sequence is the correct shape for Vulkan
    /// teardown, so no attempt is made to abstract the steps further — the
    /// rest of `Drop` still reads in order at the call site.
    ///
    /// # Safety
    /// Caller must have waited for the device to go idle (`Drop` does so as
    /// its first action) so no in-flight command buffer still references any
    /// of these objects, and `alloc` must be the allocator these resources
    /// were allocated from.
    unsafe fn destroy_allocator_owned_resources(&mut self, alloc: &SharedAllocator) {
        self.texture_registry.destroy(&self.device, alloc);
        self.scene_buffers.destroy(&self.device, alloc);
        // EX-05 / #2736 — the image-health counter buffers. `GpuBuffer::Drop`
        // is a safety net only; the canonical path destroys explicitly here so
        // the allocation is returned while the allocator is still live.
        // SAFETY: device idle by this fn's contract, and `alloc` is the
        // allocator these buffers were created from.
        for mut buffer in self.image_health_buffers.drain(..) {
            buffer.destroy(&self.device, alloc);
        }
        // M29 — destroy SkinSlots BEFORE the SkinComputePipeline
        // because slots own descriptor sets allocated from the
        // pipeline's descriptor pool. Pool destruction implicitly
        // frees the sets but the FREE_DESCRIPTOR_SET flag means
        // we should explicitly free them through the pipeline
        // first to keep the validation layer quiet. The ordering
        // also matches the static `accel_manager` teardown
        // pattern (skinned_blas before pipeline scratch buffers).
        if let Some(ref skin) = self.skin_compute {
            let slots = std::mem::take(&mut self.skin_slots);
            for (_eid, slot) in slots {
                skin.destroy_slot(&self.device, alloc, slot);
            }
        }
        // #3231 — MorphSlot owns its private weight buffer and an Arc to a
        // mesh-shared delta. Destroying every slot releases the final delta
        // reference as well; the weak cache is only an index and is cleared
        // after its owners are gone.
        for (_eid, mut slot) in std::mem::take(&mut self.morph_slots) {
            slot.destroy(&self.device, alloc);
        }
        self.morph_delta_cache.clear();
        if let Some(ref mut accel) = self.accel_manager {
            // Pre-drain per-skinned-entity BLAS via the
            // `pending_destroy_blas` queue so the
            // `MAX_FRAMES_IN_FLIGHT` countdown lets any in-flight
            // refit settle before destruction. Post-#1138 /
            // CONC-D3-NEW-01 `manager.destroy()` also drains
            // `skinned_blas` directly, so this pre-drain is now
            // an optimization (countdown-aware destruction)
            // rather than a correctness requirement — the
            // `device_wait_idle` above already covers any
            // in-flight reference.
            for eid in accel.skinned_blas_entities() {
                accel.drop_skinned_blas(eid);
            }
            // `destroy()` calls `drain_pending_destroys`
            // internally (#732) so we do NOT need a separate
            // `tick_deferred_destroy` here even though
            // `draw_frame` won't run another tick after
            // shutdown. REN-D7-NEW-05 (audit 2026-05-09)
            // flagged the missing tick; the structural fix
            // already landed via #732's factor-out of the
            // drain into `destroy()`.
            accel.destroy(&self.device, alloc);
        }
        if let Some(ref mut cc) = self.cluster_cull {
            cc.destroy(&self.device, alloc);
        }
        if let Some(ref mut sc) = self.skin_compute {
            sc.destroy(&self.device);
        }
        // NOTE: `skin_palette` + `gpu_timers` teardown was
        // hoisted to the allocator-independent block near the top
        // of Drop (#1483) — they need no allocator and must run on
        // the allocator-`None` path too. `skin_compute` above
        // stays here: its descriptor pool must outlive the
        // allocator-dependent per-slot teardown earlier in this
        // guard.
        if let Some(ref mut ssao) = self.ssao {
            ssao.destroy(&self.device, alloc);
        }
        // #2141 / #2142 — the 1×1 placeholders. Torn down here,
        // alongside the passes whose descriptors may still name
        // them: both are allocator-backed, so they must go before
        // the `self.allocator.take()` + `Arc::try_unwrap` below,
        // and after the descriptor sets that reference them have
        // stopped being used (the device_wait_idle at the top of
        // Drop already guarantees nothing is in flight).
        // SAFETY (both `destroy` calls): `device_wait_idle` ran at
        // the top of Drop, so no in-flight command buffer still
        // references these handles.
        if let Some(ref mut p) = self.placeholder_ao {
            p.destroy(&self.device, alloc);
        }
        if let Some(ref mut p) = self.placeholder_caustic_sink {
            p.destroy(&self.device, alloc);
        }
        // The exposure resource owns its own device + allocator (an
        // `Arc` clone of the shared allocator), so it self-frees via
        // stored handles rather than the `alloc` local. It MUST be
        // destroyed here — before the `self.allocator.take()` +
        // `Arc::try_unwrap` below — or its lingering allocator clone
        // trips the outstanding-reference leak guard (#665). `destroy`
        // is idempotent, so the field's own `Drop` later is a no-op.
        if let Some(ref mut exposure) = self.exposure {
            exposure.destroy();
        }
        // The output views must be retired after presentation
        // descriptors and before composed-scene inputs. The SDK
        // context half already ran in the allocator-independent block
        // above (#2158) — do not move it back down here.
        if let Some(ref mut upscaler) = self.frame_upscaler {
            upscaler.destroy_allocations(&self.device, alloc);
        }
        if let Some(ref mut composite) = self.composite {
            composite.destroy(&self.device, alloc);
        }
        if let Some(ref mut caustic) = self.caustic {
            caustic.destroy(&self.device, alloc);
        }
        if let Some(ref mut vol) = self.volumetrics {
            vol.destroy(&self.device, alloc);
        }
        if let Some(ref mut b) = self.bloom {
            b.destroy(&self.device, alloc);
        }
        // `self.water` teardown is hoisted above because WaterPipeline owns
        // the SharedAllocator clone needed to free its parameter SSBOs. Its
        // destroy must stay before the Arc::try_unwrap below; the per-FIF
        // accumulator images still use the context allocator here (#3140).
        if let Some(ref mut wca) = self.water_caustic_accum {
            // SAFETY: parent Drop runs after `device_wait_idle`
            // earlier in the teardown sequence; no in-flight
            // command buffer references the per-FIF accumulator
            // images. #1255 / Phase C of #1210.
            wca.destroy(&self.device, alloc);
        }
        if let Some(ref mut svgf) = self.svgf {
            svgf.destroy(&self.device, alloc);
        }
        // SAFETY: Drop runs after device_wait_idle; no in-flight
        // command references the reservoir buffers. (Already inside an
        // `unsafe` block, so no inner `unsafe` wrap needed.)
        self.reservoir_buffers.destroy(&self.device, alloc);
        if let Some(ref mut taa) = self.taa {
            taa.destroy(&self.device, alloc);
        }
        if let Some(ref mut gbuffer) = self.gbuffer {
            gbuffer.destroy(&self.device, alloc);
        }
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        // SAFETY: device_wait_idle ensures all GPU work is complete before
        // destroying resources. Destruction does NOT follow reverse-creation
        // order (see `destroy_allocator_owned_resources`'s doc, corrected
        // 2026-08-30) — Vulkan imposes no cross-subsystem ordering once the
        // device is idle; only four local orderings (documented at their own
        // sites) are load-bearing.
        unsafe {
            let _ = self.device.device_wait_idle();

            // Egui pass destroys its render pass + framebuffers
            // here; its `Renderer` field's own Drop tears down the
            // pipeline + descriptor pool + per-frame buffer pools
            // when the `Option<EguiPass>` itself drops below.
            if let Some(mut pass) = self.egui_pass.take() {
                pass.destroy(&self.device);
            }
            if let Some(mut presentation) = self.presentation.take() {
                presentation.destroy(&self.device);
            }

            // ── Allocator-independent teardown (#1483 / REN-D23-NEW-02
            // + sibling scan) ─────────────────────────────────────────
            // These subsystems own only device-level handles (query
            // pools, compute/graphics pipelines, descriptor pools +
            // layouts) — no gpu-allocator memory. They were previously
            // nested inside the `Some(allocator)` guard further down, so
            // on an allocator-`None` Drop path (#1426 early-return, or
            // any future allocator-taken-early path — **hypothetical at
            // HEAD**: `VulkanContext::allocator` is never set to `None`
            // except this function's own final `take()`, verified
            // 2026-08-30) their handles would leak and the validation
            // layer would flag "destroyed device with live objects".
            // Hoisting them here is still worth keeping as
            // defence-in-depth — alongside
            // `egui_pass.destroy()` above — runs them on EVERY Drop path,
            // and still before the `VkDevice` is destroyed at the bottom.
            // The pipelines reference `self.render_pass`, destroyed far
            // below, so pipeline-before-render-pass ordering is preserved.
            //
            // NOTE: `skin_compute`'s pipeline destroy is deliberately NOT
            // hoisted — it must run AFTER the allocator-dependent per-slot
            // teardown (slots own descriptor sets from its pool; see the
            // ordering comment in the guard below), so it stays inside the
            // guard where that ordering holds.
            if let Some(ref mut timers) = self.gpu_timers {
                // INVARIANT (REG-06 / #1638, #1483): this query-pool destroy
                // lives in the allocator-INDEPENDENT block (above), NOT inside
                // the `Some(allocator)` guard below — query pools own no
                // gpu-allocator memory, so they must be torn down on the
                // allocator-`None` Drop path too or they leak. Do not move it
                // back under the allocator guard.
                // #1194 — per-pass GPU timer query pools. Queue idle is
                // guaranteed by the `device_wait_idle()` at the top.
                timers.destroy(&self.device);
            }
            if let Some(ref mut sp) = self.skin_palette {
                // M29.5 — palette compute pipeline. No per-slot
                // allocations to drain (single dispatch per frame, not
                // per-skinned-entity), so destroy is unconditional.
                sp.destroy(&self.device);
            }
            if let Some(ref mut w) = self.water {
                w.destroy(&self.device);
            }
            // #2158 — the FSR SDK context is allocator-independent: it owns
            // SDK-side pipelines, descriptor pools, and `VkDeviceMemory`
            // allocated outside gpu-allocator's view. Left inside the
            // `Some(allocator)` guard below it would be skipped entirely on an
            // allocator-`None` Drop path (hypothetical at HEAD — see #1483's
            // note above), dropping (or never dropping) SDK
            // objects relative to `vkDestroyDevice` — exactly the #1483 failure
            // mode this hoist defends against. Its per-FIF output images DO need the allocator and stay in
            // the guard, run after this so the context has already let go of
            // them. Ordered after `presentation.destroy()` above, which is what
            // the guard-side comment required.
            if let Some(ref mut upscaler) = self.frame_upscaler {
                upscaler.destroy_device_objects(&self.device);
            }

            self.destroy_screenshot_staging();
            self.destroy_depth_capture_staging();

            self.frame_sync.destroy(&self.device);
            // Destroy persistent transfer fence (#302). device_wait_idle
            // above ensures it's not signaled in-flight.
            {
                let fence = *self
                    .transfer_fence
                    .lock()
                    .expect("transfer fence lock poisoned");
                self.device.destroy_fence(fence, None);
            }
            self.device.destroy_command_pool(self.transfer_pool, None);
            self.device
                .free_command_buffers(self.command_pool, &self.command_buffers);
            self.device.destroy_command_pool(self.command_pool, None);
            destroy_main_framebuffers(&self.device, &mut self.framebuffers);
            // Destroy texture registry, scene buffers, and acceleration structures.
            // Allocator-owned subsystems (#2406) — NOT reverse-creation order,
            // see `destroy_allocator_owned_resources`'s doc.
            // `alloc` is cloned rather than borrowed out of `self` so the
            // helper can take `&mut self`; `SharedAllocator` is an `Arc`,
            // so this is a refcount bump, not a copy of the allocator.
            if let Some(alloc) = self.allocator.clone() {
                self.destroy_allocator_owned_resources(&alloc);
            }

            // Destroy depth resources before the allocator.
            // Helper enforces order: view → image → free allocation. The
            // image must be destroyed while its bound memory is still
            // valid (Vulkan spec VUID-vkFreeMemory-memory-00677). Same
            // helper used by recreate_swapchain — see #33 / R-10.
            if let Some(ref allocator) = self.allocator {
                destroy_depth_resources(
                    &self.device,
                    allocator,
                    &mut self.depth_image_view,
                    &mut self.depth_image,
                    &mut self.depth_allocation,
                );
                // Soft-particle depth-history image + its sampler.
                self.device
                    .destroy_sampler(self.depth_history_sampler, None);
                self.depth_history_sampler = vk::Sampler::null();
                destroy_depth_resources(
                    &self.device,
                    allocator,
                    &mut self.depth_history_view,
                    &mut self.depth_history_image,
                    &mut self.depth_history_allocation,
                );
            }

            // `destroy_render_pass_pipelines` destroys `self.pipeline`
            // (the opaque raster path), the wireframe variant, and every
            // entry in `blend_pipeline_cache`. They all share the single
            // `self.pipeline_layout` destroyed immediately below, so one
            // layout destroy covers every pipeline. Pre-fix the sharing was
            // load-bearing but undocumented; a future pipeline variant with
            // its own layout needs a matching second
            // `destroy_pipeline_layout` call here. See REN-D7-NEW-01 (audit
            // 2026-05-09).
            //
            // The UI overlay pipeline shares that same layout but is owned by
            // `PresentationPipeline` since #3426, and is destroyed with it
            // far above (`presentation.destroy()`) — i.e. still before this
            // layout destroy, which is the ordering that matters.
            destroy_render_pass_pipelines(
                &self.device,
                &mut self.pipeline,
                &mut self.pipeline_wireframe,
                &mut self.blend_pipeline_cache,
            );
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            // Meshes after pipelines: pipelines consume meshes at draw time,
            // so meshes should outlive the pipelines that reference them.
            if let Some(ref alloc) = self.allocator {
                self.mesh_registry.destroy_all(&self.device, alloc);
            }
            // Save pipeline cache to disk while every subsystem's
            // pipeline-create activity is fresh in the cache. The
            // cache survives all the subsystem destroys above (the
            // file payload is the cache *contents*, not a handle to
            // the device-side blob), so saving here vs earlier in
            // the teardown is structurally equivalent. The previous
            // ordering (save then destroy) is preserved — the
            // REN-D7-NEW-02 concern was that subsystem destroy
            // panicking would lose the save; the actual `destroy_*`
            // calls here can't panic (every fallible op is masked
            // by the surrounding `unsafe` block) so the ordering is
            // also safe under abnormal teardown. Documented for the
            // next reader. See audit 2026-05-09.
            save_pipeline_cache(&self.device, self.pipeline_cache);
            self.device
                .destroy_pipeline_cache(self.pipeline_cache, None);
            self.device.destroy_render_pass(self.render_pass, None);
            self.swapchain_state.destroy(&self.device);
            // Drop the allocator before destroying the device.
            // take() extracts from Option, then try_unwrap gets the inner
            // Mutex if we hold the last Arc, then into_inner gives us the
            // Allocator which we drop — running its cleanup while the device
            // is still alive.
            if let Some(alloc_arc) = self.allocator.take() {
                match std::sync::Arc::try_unwrap(alloc_arc) {
                    Ok(mutex) => drop(mutex.into_inner().expect("allocator lock poisoned")),
                    Err(arc) => {
                        // #665 / LIFE-L1 — the strong-count clones live
                        // inside `GpuBuffer` / `Texture` / `StagingPool`
                        // fields that haven't naturally dropped yet.
                        // Pre-fix the code logged a warning, hit
                        // `debug_assert!(false, …)` (silent in release
                        // builds), and FELL THROUGH to
                        // `device.destroy_device` below. The natural-
                        // Drop pass that runs once this method returns
                        // would then release those Arc clones; when the
                        // last one drops, the inner `Allocator` runs
                        // its destructor, which calls `vkFreeMemory`
                        // on whatever sub-allocations are still tracked
                        // — against a destroyed `VkDevice`. Driver-
                        // level use-after-free.
                        //
                        // Safer in release: leak the device + surface +
                        // instance + debug messenger handles entirely.
                        // The natural-Drop pass below now happens with
                        // a still-valid device, the late `vkFreeMemory`
                        // calls succeed against alive memory, and the
                        // OS reaps the leaked Vulkan handles at process
                        // exit. Debug builds still hit the assertion
                        // so the leak source is investigatable in CI.
                        log::error!(
                            "GPU allocator has {} outstanding references — \
                             leaking allocator + device + surface + instance to avoid \
                             use-after-free on driver-side `vkFreeMemory` of late \
                             natural-Drop allocations. Process must terminate to reclaim.",
                            std::sync::Arc::strong_count(&arc),
                        );
                        debug_assert!(false, "GPU allocator leaked: outstanding Arc references");
                        return;
                    }
                }
            }
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            if let Some((ref utils, messenger)) = self.debug_messenger {
                utils.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
        log::info!("Vulkan context destroyed cleanly");
    }
}
