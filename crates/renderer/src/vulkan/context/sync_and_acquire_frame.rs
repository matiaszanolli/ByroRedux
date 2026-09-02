//! Frame-start synchronization + swapchain acquire — extracted from
//! `draw.rs` (#3282 / TD1-2026-08-24-01) to shrink `draw_frame`. Covers the
//! in-flight fence wait, the per-frame deferred-destroy tick, and the
//! swapchain image acquire; the recording order (fence wait → CPU-side
//! telemetry/readback drains → swapchain acquire → deferred-destroy tick →
//! RT geometry-descriptor re-point) is unchanged from the pre-split
//! `draw_frame`.

use super::{FrameTimings, VulkanContext};
use anyhow::{Context, Result};
use ash::vk;
use std::time::Instant;

impl VulkanContext {
    /// Wait for this frame-in-flight slot's fence, drain per-frame CPU-side
    /// telemetry / readbacks, acquire the next swapchain image, and tick
    /// deferred-destroy for resources whose countdown reached zero. Extracted
    /// verbatim from `draw_frame` — the recording/wait order is unchanged.
    ///
    /// Returns `Ok(None)` when the swapchain came back `ERROR_OUT_OF_DATE_KHR`
    /// — the original `draw_frame` returned `Ok(true)` directly from this
    /// point, so the caller must do the same on `None`. Returns
    /// `Ok(Some((frame, img, suboptimal)))` on the normal path.
    pub(super) fn sync_and_acquire_frame(
        &mut self,
        t: &mut FrameTimings,
    ) -> Result<Option<(usize, usize, bool)>> {
        let frame = self.current_frame;
        // #1197 / PERF-DIM7-03 — reset per-frame descriptor-writes
        // counters on both skin compute pipelines. The dispatch
        // bodies bump these only when they actually call
        // `vkUpdateDescriptorSets`; steady state stays at 0.
        if let Some(ref p) = self.skin_compute {
            p.reset_descriptor_writes_counter();
        }
        if let Some(ref p) = self.skin_palette {
            p.reset_descriptor_writes_counter();
        }

        // Wait for this frame-in-flight slot AND the previous slot to be
        // available. SVGF's temporal pass reads the previous slot's G-buffer
        // images (mesh_id, motion, raw_indirect) — without waiting on the
        // other slot's fence, a read-after-write hazard exists when the GPU
        // hasn't finished the other slot's render pass. See #282.
        //
        // Cost: zero in practice — the GPU is rarely more than 1 frame
        // behind the CPU, so the other fence is almost always signaled.
        let fence_t0 = Instant::now();
        // SAFETY: `in_flight[frame]` and `in_flight[prev]` are live fences; both were signal-targets of prior `queue_submit`s (or created pre-signaled), so the wait cannot deadlock. This frame's `cmd` is not re-recorded until this wait returns, so the GPU is done with the prior recording.
        unsafe {
            let prev = (frame + 1) % super::super::sync::MAX_FRAMES_IN_FLIGHT;
            self.device
                .wait_for_fences(
                    &[
                        self.frame_sync.in_flight[frame],
                        self.frame_sync.in_flight[prev],
                    ],
                    true,
                    u64::MAX,
                )
                .context("wait_for_fences")?;
        }
        t.fence_wait_ns = fence_t0.elapsed().as_nanos() as u64;

        self.flush_pending_morph_weights()?;

        // EX-05 / #2736 — harvest this slot's image-health counters from the
        // *prior* use of the slot, then zero them for the frame about to be
        // recorded. The fence wait above proves submission completed
        // (device-side access scope only) — it does NOT by itself prove the
        // GPU write is host-visible; that additionally requires the memory
        // to be host-coherent (or an explicit invalidate). See
        // `collect_image_health`'s doc comment for why that holds here.
        // #2740 (REN-D4-04).
        self.collect_image_health(frame);
        if let Some(ref mut cluster_cull) = self.cluster_cull {
            cluster_cull.collect_telemetry(&self.device, frame);
        }
        match self
            .scene_buffers
            .collect_selected_ray_probe(&self.device, frame)
        {
            Ok(Some(record)) => {
                self.selected_ray_probe_result =
                    Some(super::super::render_debug::SelectedRayProbeResult::from_gpu(record));
            }
            Ok(None) => {}
            Err(error) => log::warn!("selected-ray probe readback failed: {error}"),
        }
        if self.renderer_config.rt_test_lod_telemetry {
            match self
                .scene_buffers
                .collect_rt_lod_telemetry(&self.device, frame)
            {
                Ok(sample) if sample.fragments > 0 && self.frame_counter.is_multiple_of(60) => {
                    let scale = self
                        .renderer_config
                        .rt_test_lod_scale_bits
                        .map(f32::from_bits)
                        .unwrap_or(6.0);
                    log::info!(
                        "rt-lod-telemetry: scale={scale:.6} fragments={} bins={:?} \
                         reflection_traced={} reflection_lod_culled={} gi_traced={} \
                         gi_lod_culled={}",
                        sample.fragments,
                        sample.bins,
                        sample.reflection_traced,
                        sample.reflection_lod_culled,
                        sample.gi_traced,
                        sample.gi_lod_culled,
                    );
                }
                Ok(_) => {}
                Err(error) => log::warn!("RT-LOD telemetry readback failed: {error}"),
            }
        }

        // #1194 — read this slot's TIMESTAMP results (from the prior
        // cycle's use of this slot), then reset the pool for the
        // upcoming frame. The fence wait above proves the prior
        // submission for this slot is complete, so query results
        // are guaranteed available — no host stall here. First-cycle
        // reads return zero (active_bits never set yet); steady-state
        // reads are one MAX_FRAMES_IN_FLIGHT cycle behind, which is
        // fine for per-pass instrumentation.
        if let Some(ref mut timers) = self.gpu_timers {
            timers.read_and_reset(&self.device, frame);
        }

        // If a screenshot was captured last frame, the GPU is done — read it back.
        self.screenshot_finish_readback();
        // #3308 — same fence-proven timing as the screenshot readback above.
        self.depth_capture_finish_readback();

        // Acquire next swapchain image. Bracketed (Phase 9) so a
        // FIFO-present-mode block waiting for the next image is
        // surfaced in `CpuFrameTimings.acquire_ms` rather than
        // disappearing into the gap between fence_wait and
        // cmd_record. The acquire itself blocks until the image
        // is available; on most desktop drivers + Wayland/X11
        // compositors this is also where vsync ends up.
        let acquire_t0 = Instant::now();
        // SAFETY: swapchain + loader are live; `image_available[frame]` is an unsignaled binary semaphore (its prior signal was consumed by last cycle's submit wait on this slot) so acquiring into it is legal. The OUT_OF_DATE arm bails before the semaphore is depended on.
        let (image_index, suboptimal) = unsafe {
            match self.swapchain_state.swapchain_loader.acquire_next_image(
                self.swapchain_state.swapchain,
                u64::MAX,
                self.frame_sync.image_available[frame],
                vk::Fence::null(),
            ) {
                Ok((idx, suboptimal)) => (idx, suboptimal),
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(None),
                Err(e) => anyhow::bail!("acquire_next_image: {:?}", e),
            }
        };
        t.acquire_ns = acquire_t0.elapsed().as_nanos() as u64;

        let img = image_index as usize;

        // From here through `queue_submit` below, `image_available[frame]`
        // is signal-pending (set by the acquire above) and any `?`-
        // propagated error would leak the signal into the next acquire,
        // tripping VUID-vkAcquireNextImageKHR-semaphore-01779. Each
        // fallible call between this point and the submit recovers via
        // `recreate_image_available_for_frame` — sibling to the
        // `in_flight` fence recovery already wired through
        // `recreate_for_swapchain` (#908). See #910 / REN-D5-NEW-01.

        // If this swapchain image is still in use by a different frame, wait.
        let image_fence = self.frame_sync.images_in_flight[img];
        if image_fence != vk::Fence::null() && image_fence != self.frame_sync.in_flight[frame] {
            // SAFETY: `image_fence` is a live fence belonging to whichever frame last used this swapchain image; it was a `queue_submit` signal-target, so the wait terminates. Guarantees that image's prior frame finished before we reuse it. On error we clear the pending acquire signal before propagating.
            unsafe {
                if let Err(e) = self
                    .device
                    .wait_for_fences(&[image_fence], true, u64::MAX)
                    .context("wait for image fence")
                {
                    let _ = self
                        .frame_sync
                        .recreate_image_available_for_frame(&self.device, frame);
                    return Err(e);
                }
            }
        }
        self.frame_sync.images_in_flight[img] = self.frame_sync.in_flight[frame];

        // #952 / REN-D1-NEW-04 — `reset_fences` MOVED to immediately
        // before `queue_submit`. Pre-fix this ran here, then ~2200
        // lines of `?`-propagated fallible work followed before the
        // submit re-signaled the fence. Any error in that window left
        // the fence UNSIGNALED with no pending submit, and the next
        // frame's both-slots `wait_for_fences(..., u64::MAX)` at
        // lines 174-183 blocked forever — logical deadlock matching
        // the resize-path window closed by #908. Reorder narrows the
        // window to a single fallible call; the submit-failure error
        // arm below additionally recreates the fence to cover that
        // residual case.

        // Deferred-destroy tick. Runs AFTER `wait_for_fences` so every
        // resource whose countdown reaches zero this frame is
        // guaranteed unreferenced by any in-flight command buffer.
        // Pre-#418 this ran at the TOP of `draw_frame`, before the
        // fence wait — `AccelerationManager::tick_deferred_destroy`
        // (and the `mesh_registry` / `texture_registry` siblings, all
        // three destroy GPU resources) could free a BLAS / buffer /
        // image the previous frame's TLAS or blit was still reading.
        // Latent because `MAX_FRAMES_IN_FLIGHT`-conservative countdowns
        // kept the window from ever closing, but a policy change that
        // shortened the countdown would have turned this into a
        // sync2-validated use-after-free.
        //
        // `texture_registry.begin_frame` advances the internal frame
        // counter that the tick compares against — must run BEFORE the
        // tick so the counter reflects "this frame" during the
        // deferred-destroy decision.
        self.texture_registry.begin_frame(&self.device, frame);
        if let Some(ref alloc) = self.allocator {
            self.mesh_registry
                .tick_deferred_destroy(&self.device, alloc);
            self.texture_registry
                .tick_deferred_destroy(&self.device, alloc);
            if let Some(ref mut accel) = self.accel_manager {
                accel.tick_deferred_destroy(&self.device, alloc);
            }
        }

        // Re-point the RT-shading global-geometry descriptor (bindings 8/9)
        // to the CURRENT global SSBO for THIS frame-in-flight, every frame.
        // The global vertex/index SSBO is reallocated to a brand-new
        // `VkBuffer` whenever cell-stream growth marks geometry dirty
        // (`MeshRegistry::rebuild_geometry_ssbo`), but the binding was
        // written only ONCE at scene setup (`scene.rs::setup_scene`). Without
        // this per-frame refresh the descriptor keeps naming the OLD buffer,
        // which `rebuild_geometry_ssbo` defers to the destroy queue and
        // `tick_deferred_destroy` (just above) frees `MAX_FRAMES_IN_FLIGHT`
        // frames later — at which point the next RT hit-fetch
        // (`getHitUV` / `getHitTriNormal`, bindings 8/9, on the
        // reflection / refraction / GI paths) dereferences freed device
        // memory → GPU page fault → ~TDR → `VK_ERROR_DEVICE_LOST`. The
        // raster path never hit this because it re-fetches the buffer fresh
        // each frame (`cmd_bind_vertex_buffers` below); only the once-bound
        // RT descriptor dangled. Mirrors `write_tlas` (binding 2, re-pointed
        // every frame): safe because `in_flight[frame]` was just waited on,
        // so this frame's descriptor set is idle. See WATAL §0 device-loss
        // hunt. (bindings 8/9 are PARTIALLY_BOUND, so the None case — no
        // geometry yet / headless — leaves them validly unbound.)
        if let (Some(vb), Some(ib)) = (
            self.mesh_registry.global_vertex_buffer.as_ref(),
            self.mesh_registry.global_index_buffer.as_ref(),
        ) {
            self.scene_buffers.write_geometry_buffers(
                &self.device,
                frame,
                vb.buffer,
                vb.size,
                ib.buffer,
                ib.size,
            );
            if let Some(ref caustic) = self.caustic {
                caustic.write_geometry_buffers(
                    &self.device,
                    frame,
                    vb.buffer,
                    vb.size,
                    ib.buffer,
                    ib.size,
                );
            }
        }

        Ok(Some((frame, img, suboptimal)))
    }
}
