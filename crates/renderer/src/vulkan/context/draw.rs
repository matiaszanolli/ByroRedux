//! Frame recording and submission — the per-frame hot path.

use super::super::descriptors::memory_barrier;
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::assemble_camera_and_lights::CameraAssemblyOutput;
use super::begin_frame_recording::BeginFrameOutput;
use super::build_and_upload_instances::BuildInstancesOutput;
use super::frame_params::{FrameInputs, has_radiating_local_emitter};
use super::{FrameTimings, VulkanContext};
use anyhow::{Context, Result};
use ash::vk;
use byroredux_core::ecs::storage::EntityId;
use std::time::Instant;


impl VulkanContext {
    /// Publish CPU-staged morph weights after the dual-fence wait has proven
    /// no prior submission can still read their mapped buffers (#3244).
    pub(super) fn flush_pending_morph_weights(&mut self) -> Result<()> {
        for (&entity, slot) in &mut self.morph_slots {
            slot.flush_pending_weights(&self.device)
                .with_context(|| format!("flush MorphSlot weights for entity {entity}"))?;
        }
        Ok(())
    }

    /// #4294 — stamp the LRU of every `MorphSlot` whose entity `is_live`
    /// reports alive, ahead of this frame's eviction sweep. The app calls
    /// this once per frame before `draw_frame`; see
    /// [`crate::vulkan::skin_compute::refresh_live_slot_stamps`] for why a
    /// `MorphSlot` cannot take its liveness from the skin dispatch loop.
    pub fn refresh_morph_slot_lru(&mut self, is_live: impl Fn(EntityId) -> bool) {
        crate::vulkan::skin_compute::refresh_live_slot_stamps(
            self.morph_slots
                .iter_mut()
                .map(|(&entity, slot)| (entity, &mut slot.last_used_frame)),
            crate::vulkan::skin_compute::skin_lru_stamp(self.frame_counter),
            is_live,
        );
    }

    /// Whether FSR is not merely the *selected* upscaler mode but is
    /// actually dispatching this frame (#2518).
    ///
    /// `self.post.fsr_temporal.is_some()` answers a different question: it stays
    /// `Some` for the whole of `UpscalerMode::Fsr3(..)`, including when the
    /// FSR context never got created or `dispatch_failure` has latched. In
    /// those states the frame falls back to an unjittered native blit, so
    /// every "is FSR's projection jitter in play?" decision — the camera
    /// jitter itself and the DOF gate that exists to avoid conflicting with
    /// it — must key on this, not on mode selection. Sharing one accessor
    /// is what keeps the two from drifting apart again.
    ///
    /// #3632 — also `false` whenever `record_upscale_pass` is about to pass
    /// `force_native_blit: true` into `FrameUpscaler::record` (any render-
    /// debug view that requires raw, unreconstructed output). That path
    /// bridges straight to a native blit exactly like a missing context or a
    /// latched `dispatch_failure` does — the frame is never reconstructed —
    /// so it must fall out of this predicate the same way, or the jitter and
    /// DOF gates above apply FSR's sub-pixel offset to a frame nothing ever
    /// resolves it back out of. `self.render_debug_flags` / `render_debug_
    /// mode` are frame-stable (only a console command changes them, never
    /// mid-frame), so evaluating the same predicate again in
    /// `record_upscale_pass` cannot disagree with this one.
    pub(super) fn is_fsr_dispatch_active(&self) -> bool {
        self.post
            .frame_upscaler
            .as_ref()
            .is_some_and(|upscaler| upscaler.is_fsr_dispatch_active())
            && !crate::shader_constants::render_debug_requires_raw_output(
                self.render_debug_flags,
                self.render_debug_mode.shader_value(),
            )
    }

    pub fn draw_frame(&mut self, inputs: FrameInputs) -> Result<bool> {
        let FrameInputs {
            clear_color,
            view_proj,
            draw_commands,
            lights,
            fog_volumes,
            bone_world,
            skin_offsets,
            bind_inverse_pending_uploads,
            materials,
            has_effect_soft_material,
            camera_pos,
            render_origin: input_render_origin,
            ambient_color,
            fog_color,
            fog_near,
            fog_far,
            fog_extinction_per_meter,
            fog_single_scatter_albedo,
            fog_coverage,
            fog_scale_height_meters,
            fog_clip,
            fog_power,
            fog_height_reference,
            wind_params,
            wind_gust,
            ui_texture_handle,
            sky_params,
            dof,
            frame_time_delta_ms,
            timings,
            water_commands,
            underwater,
            image_space_modifier,
            pose_dirty,
        } = inputs;
        // #1796 / D6-02 — reset before either early-return guard below so
        // a bailed frame reads `false`; see the field doc on `skin_dispatch_ran`.
        self.skin_dispatch_ran = false;
        // #3991 — the submit-time counterpart. Reset here for the same reason,
        // and set only once `queue_submit` has returned `Ok`. Everything the
        // skin chain commits is latched between these two points.
        self.skin_state_submitted = false;
        self.skin_pending_populated.clear();
        // #3569 / D9-01 — reset alongside `skin_dispatch_ran`: this frame's
        // upload hasn't happened yet, so any stale `true` from a previous
        // frame's failure must not leak into this frame's rollback check.
        self.bind_inverse_upload_failed = false;
        // #2112 / D6-01 — same reasoning as `skin_dispatch_ran` above: reset
        // before the early-return guard so a bailed frame reads zero
        // instead of retaining the previous frame's counters. Frame without
        // a skinned section (no RT, no bone buffer) also reads zero.
        // Section-local increments below populate `last_skin_coverage_frame`;
        // `fill_skin_coverage_stats` snapshots it after `Scheduler::run`.
        self.last_skin_coverage_frame = super::super::skin_compute::SkinCoverageFrame::default();
        // Reset per-frame draw-call counts. Populated after the batch
        // merge (`batch_count`) and inside the indirect-grouping draw
        // loop below (`indirect_call_count`). Read by the app's stats
        // wiring after `draw_frame` returns to populate `DebugStats`.
        // #1258 / PERF-D3-NEW-03.
        self.last_draw_call_stats = super::DrawCallStats::default();
        // #1211 / REN-SAFETY — skip the frame when the main framebuffers
        // Vec is empty. `recreate_swapchain` destroys framebuffers up
        // front and only rebuilds them at the end (`resize.rs:564`);
        // any `?`-propagated failure between those two points leaves
        // the Vec at `len == 0`. The app-level caller logs the recreate
        // error and queues `event_loop.exit()`, but exit is queued —
        // the next `RedrawRequested` already in flight would index
        // `framebuffers[frame]` and panic.
        //
        // Return BEFORE `acquire_next_image` so `image_available[frame]`
        // is not left signal-pending without a paired wait. `Ok(false)`
        // (not `Ok(true)`) avoids a recreate-retry loop when the
        // underlying surface is still invalid — recovery rides the
        // next `Resized` / focus event instead.
        if self.swapchain.framebuffers.is_empty() {
            return Ok(false);
        }

        let mut armed_selected_ray_probe_generation = None;
        let volumetric_time_seconds = self.volumetric_time_seconds;
        // Use a local to avoid borrow complexity; copy out at end.
        let mut t = FrameTimings::default();

        // #3282 / TD1-2026-08-24-01 — `draw_frame` was split into 5
        // sibling-file phases (mirroring the existing `record_geometry_pass`
        // / `record_post_passes` extraction pattern) to shrink this
        // function. Pure code motion: every barrier, dispatch, and
        // recording order below is unchanged from the pre-split function —
        // see each phase's own module doc for its exact scope.
        let Some((frame, img, suboptimal)) = self.sync_and_acquire_frame(&mut t)? else {
            return Ok(true);
        };

        let BeginFrameOutput {
            cmd,
            clear_values,
            instance_map,
            tlas_t0,
        } = self.begin_frame_recording(frame, draw_commands, clear_color, sky_params)?;

        // Main framebuffer is now per-frame-in-flight (not per-swapchain-image).
        // Each frame slot has its own HDR color image, so no read-after-write
        // hazard across overlapping frames.
        let render_pass_begin = vk::RenderPassBeginInfo::default()
            .render_pass(self.swapchain.render_pass)
            .framebuffer(self.swapchain.framebuffers[frame])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.frame_extents.render,
            })
            .clear_values(&clear_values);

        let CameraAssemblyOutput {
            frame_lights,
            camera_cut,
            camera_static,
            effective_vp,
            pvp,
            inv_vp_arr,
            render_origin,
            previous_camera_position,
            fsr_frame,
        } = self.assemble_camera_and_lights(
            frame,
            lights,
            fog_volumes,
            view_proj,
            camera_pos,
            input_render_origin,
            ambient_color,
            fog_color,
            fog_near,
            fog_far,
            fog_extinction_per_meter,
            sky_params,
            dof,
            frame_time_delta_ms,
        )?;
        let vp = &effective_vp;
        // #4007 — the `reset` flag this frame actually hands FSR, captured
        // before `fsr_frame` is consumed by `record_post_passes`. It is
        // already final here: `assemble_camera_and_lights` applies the
        // camera-cut override before returning. Only a reset FSR was told
        // about may be cleared at the tail of this function; one raised
        // later in the frame must survive to the next one.
        let fsr_reset_delivered = fsr_frame.is_some_and(|params| params.reset);

        self.dispatch_skin_and_cluster(
            cmd,
            frame,
            draw_commands,
            bone_world,
            skin_offsets,
            bind_inverse_pending_uploads,
            pose_dirty,
            &instance_map,
            tlas_t0,
            &mut t,
        );

        let lights = frame_lights.as_slice();
        // Capture this before returning the scratch Vec below: post passes
        // run after geometry recording, when `frame_lights` has deliberately
        // been handed back to its persistent allocation.
        let local_emitters_present = has_radiating_local_emitter(lights);
        let BuildInstancesOutput {
            gpu_instances,
            previous_models,
            mut current_rigid_models,
            batches,
            ui_instance_idx,
            caustic_history_valid,
        } = self.build_and_upload_instances(
            cmd,
            frame,
            draw_commands,
            render_origin,
            camera_cut,
            camera_static,
            pose_dirty,
            lights,
            &instance_map,
            ui_texture_handle,
            materials,
            fog_color,
            fog_near,
            fog_far,
            fog_extinction_per_meter,
            fog_single_scatter_albedo,
            fog_scale_height_meters,
            fog_clip,
            fog_power,
            fog_height_reference,
            sky_params,
            camera_pos,
            inv_vp_arr,
            fog_volumes,
            underwater,
            water_commands,
            &mut armed_selected_ray_probe_generation,
            &mut t,
            // Stage 1 — seconds form of `frame_time_delta_ms` for the
            // exposure meter's adaptation factor.
            frame_time_delta_ms / 1000.0,
        );

        // #3837 — hand the lights Vec back to its field as soon as the borrow
        // above (`lights`) is dead, rather than ~400 lines later at the tail.
        // `assemble_camera_and_lights` vacated the field with `mem::take`, and
        // three `return Err` sites (`end_command_buffer`, `reset_fences`,
        // submit — the same recovery paths #910 hardened, so reachable in
        // practice on swapchain churn) sat between there and the old restore
        // point. On any of them the taken Vec dropped and next frame regrew
        // from zero. Restoring here removes the window structurally instead of
        // adding a restore to each site, so a future early return added below
        // cannot reintroduce it.
        //
        // The tail still owns the shrink policy; it now reads the length from
        // the field.
        //
        // SIBLING (#3837): `gpu_instances_scratch` and `previous_models_scratch`
        // are taken by `build_and_upload_instances` and restored at the same
        // tail, so they had the identical window. Neither is read between the
        // destructure above and that tail, so both come back here too.
        // `batches_scratch` is the same case but has one more use
        // (`record_geometry_pass`), so it is restored just after it.
        // #4413 — the ground-cover model tier writes its instances after the
        // list just uploaded, so it records here: after the upload fixed the
        // list's length, before the geometry pass draws them.
        self.record_groundcover_models(cmd, frame, gpu_instances.len() as u32);

        self.scratch.frame_lights_scratch = frame_lights;
        self.scratch.gpu_instances_scratch = gpu_instances;
        self.scratch.previous_models_scratch = previous_models;

        let cmd_t0 = Instant::now();
        self.record_geometry_pass(
            cmd,
            frame,
            &render_pass_begin,
            &batches,
            draw_commands,
            water_commands,
            &instance_map,
        );
        // #3837 — last use of `batches`; hand it back before the three
        // `return Err` sites below (see the sibling note above).
        self.scratch.batches_scratch = batches;
        // #4193 — `instance_map`'s last reader is the water pass inside the
        // geometry pass (it resolves each plane's SSBO slot through it), so
        // it returns to its scratch alongside `batches`.
        self.scratch.instance_map_scratch = instance_map;
        // #3991 — the three tail `Err` sites each need `&mut self` for the
        // skin-state rollback, which cannot be taken inside their `unsafe`
        // blocks while the sync-object recovery holds a disjoint field borrow.
        // Latch the error and act on it in the outer scope, the shape the
        // `end_command_buffer` site's own comment already asked for.
        let mut end_command_buffer_failed: Option<anyhow::Error> = None;
        let mut reset_fences_failed: Option<anyhow::Error> = None;
        let mut submit_failed: Option<anyhow::Error> = None;

        // SAFETY: tail of the per-frame command buffer — depth-history
        // snapshot, post/denoise/composite chain, egui overlay, screenshot
        // copy, and `end_command_buffer`. Each call documents its own
        // recording-order contract; this is the same single `unsafe` scope
        // `draw_frame` opened before the geometry pass was extracted (#1748).
        unsafe {
            // Publish the bounded fragment-shader probe record to the host.
            // The matching CPU read occurs only after this slot's fence wait
            // on its next use and performs a non-coherent invalidate first.
            memory_barrier(
                &self.device,
                cmd,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::HOST,
                vk::AccessFlags::HOST_READ,
            );

            // Soft-particle depth fade: when the scene-level material bit is
            // set, snapshot this frame's opaque depth into the sampleable
            // history image so effect-shader FX can feather their alpha
            // against the geometry behind them. The transparent FX wrote no
            // depth (z_write off), so the depth buffer here holds opaque-only
            // depth. The helper restores depth to READ_ONLY afterwards so
            // SSAO / SVGF / composite read it unchanged. When the bit is
            // clear, both images remain in their normal read-only layouts and
            // the full-resolution copy plus its barriers are omitted. See
            // `crates/renderer/shaders/triangle.frag` (soft-fade block).
            if has_effect_soft_material {
                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_depth_history_copy_start(&self.device, cmd, frame);
                }
                self.copy_depth_to_history(cmd);
                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_depth_history_copy_end(&self.device, cmd, frame);
                }
            }
            // #3308 — the render pass leaves the depth image in
            // DEPTH_STENCIL_READ_ONLY_OPTIMAL. A history copy, when enabled
            // above, restores that same layout before this helper; when the
            // copy is skipped, the layout is already the precondition
            // `depth_capture_record_copy` requires and restores.
            // SAFETY: `cmd` is recording outside any render pass here (same
            // contract `copy_depth_to_history` on the line above relies on),
            // and the depth image is in DEPTH_STENCIL_READ_ONLY_OPTIMAL.
            // Already inside this function's enclosing `unsafe` block.
            self.depth_capture_record_copy(cmd);

            // #1255 / Phase C of #1210 — sequence water.frag's
            // imageAtomicAdd writes (FRAGMENT_SHADER WRITE during the
            // main pass) so composite's FRAGMENT_SHADER READ in the
            // composite pass sees them. Render-pass-end is implicit
            // sync for color-attachment writes; descriptor-image
            // atomic writes need an explicit barrier. Skipped when
            // the accumulator failed init.
            self.record_post_passes(
                cmd,
                frame,
                img,
                caustic_history_valid,
                camera_pos,
                render_origin,
                vp,
                &pvp,
                inv_vp_arr,
                previous_camera_position,
                self.frame_counter,
                volumetric_time_seconds,
                sky_params,
                fog_color,
                fog_far,
                fog_extinction_per_meter,
                local_emitters_present,
                fog_single_scatter_albedo,
                fog_scale_height_meters,
                fog_coverage,
                fog_height_reference,
                wind_params,
                wind_gust,
                fog_volumes,
                fsr_frame,
                underwater,
                image_space_modifier,
                ui_instance_idx,
                &mut t.fog_cluster_ns,
            );

            // Debug-UI overlay (Phase 4 of the debug-UI plan).
            // The presentation pass (FSR 3.1 tail, `presentation.rs`) —
            // not composite — already wrote the swapchain image and left
            // it in PRESENT_SRC_KHR; the egui RP keeps that layout
            // via loadOp=LOAD + matching initial/final layouts, so
            // the only thing this needs is a fresh begin/end inside
            // the same command buffer. Skipped unless both
            // `init_egui` ran AND a frame was submitted via
            // `submit_egui_frame` this iteration.
            if let Some(pass) = self.overlay.egui_pass.as_mut() {
                if let Some((egui_ctx, output)) = self.overlay.egui_pending_output.take() {
                    // Pass the queue Mutex by reference: `dispatch` locks it
                    // only around the internal `set_textures` upload, not
                    // across tessellate + cmd_draw (which just record into
                    // `cmd`). CONC-D1-01 (#1713) — the pre-fix code held this
                    // guard across the entire dispatch call.
                    if let Err(e) = pass.dispatch(
                        crate::vulkan::egui_pass::EguiDispatchCtx {
                            device: &self.device,
                            cmd,
                            queue: &self.graphics_queue,
                            upload_command_pool: self.transfer_pool,
                        },
                        img as u32,
                        &egui_ctx,
                        output,
                    ) {
                        log::error!("egui overlay dispatch failed: {e:#}");
                    }
                }
            }

            // Screenshot capture: copy swapchain image to staging buffer
            // if requested. Must happen after composite (image has content)
            // and before end_command_buffer (still recording).
            let swapchain_image = self.swapchain.state.images[img];
            self.screenshot_record_copy(cmd, swapchain_image);

            // #4602 — the device→host flush edge, as the LAST command
            // before end_command_buffer. A fence's memory dependency
            // covers only device access: making this frame's device
            // writes visible to the host needs a memory dependency with
            // HOST_READ in its destination scope (the device→host domain
            // operation), paired with the host-side
            // `invalidate_if_needed` each readback already performs
            // (#2752). One global edge covers every host read of this
            // submission: the ground-cover counter copy (`record_scatter`),
            // the screenshot copy just above, the depth-capture copy, the
            // bounded fragment-shader probe record's atomics, and the
            // presentation pass's image-health atomicAdds (recorded after
            // the probe's own FRAGMENT→HOST barrier below, which therefore
            // could not cover them). Mirrors the in-tree precedents
            // (`compute.rs` cluster telemetry, `volumetrics.rs`
            // combustion, the probe barrier) — the four sites this
            // backfills simply predated the rule. TRANSFER covers every
            // `cmd_copy_buffer`/`cmd_copy_image_to_buffer`; FRAGMENT_SHADER
            // covers the presentation atomics; SHADER_WRITE brings the
            // compute-written counters' availability forward from their
            // publish edge.
            memory_barrier(
                &self.device,
                cmd,
                vk::PipelineStageFlags::TRANSFER | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::TRANSFER_WRITE | vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::HOST,
                vk::AccessFlags::HOST_READ,
            );

            if let Err(e) = self
                .device
                .end_command_buffer(cmd)
                .context("end_command_buffer")
            {
                // Drop out of the inner `unsafe { ... }` block — we
                // can't call `&mut self` recovery while a closure-style
                // recovery is held; do it in the outer scope below.
                // The `?`-replacement here mirrors the other 5 sites:
                // see #910 / REN-D5-NEW-01 (acquire-signal leak).
                let _ = self
                    .frame_sync
                    .recreate_image_available_for_frame(&self.device, frame);
                end_command_buffer_failed = Some(e);
            }
        }
        // #3991 — outside the `unsafe` block, where `&mut self` is free: the
        // command buffer was discarded, so every piece of skin state this
        // frame's recording latched describes work that will never run.
        if let Some(e) = end_command_buffer_failed {
            self.rollback_skin_frame_state(frame);
            return Err(e);
        }
        t.cmd_record_ns = (cmd_t0.elapsed().as_nanos() as u64)
            .saturating_sub(t.fog_cluster_ns);

        // Submit.
        let submit_t0 = Instant::now();
        let wait_semaphores = [self.frame_sync.image_available[frame]];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        // render_finished is PER SWAPCHAIN IMAGE. Re-using the same
        // semaphore on a per-frame-in-flight cycle (the pre-revert #906
        // pattern) trips VUID-vkQueueSubmit-pSignalSemaphores-00067
        // whenever swapchain_image_count > MAX_FRAMES_IN_FLIGHT: the
        // slot's submit re-signals `render_finished[slot]` while a
        // prior present on a different image is still tracking the
        // same handle. Per-image keys off the acquire boundary —
        // `acquire_next_image` returning `image_index` guarantees the
        // prior present of that image (and its semaphore consumption)
        // has completed. See `sync::FrameSync` doc for the full
        // rationale + the Khronos issue 2007 MAILBOX-discard
        // clarification that made this safe again.
        let signal_semaphores = [self.frame_sync.render_finished[img]];
        let command_buffers_to_submit = [cmd];

        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers_to_submit)
            .signal_semaphores(&signal_semaphores);

        // #952 / REN-D1-NEW-04 — `reset_fences` lands HERE, immediately
        // before `queue_submit`. The Vulkan spec only requires the
        // fence to be unsignaled at the moment of submit; resetting
        // any earlier opens a deadlock window if a `?`-propagated
        // error fires between the reset and the submit (was ~2200
        // lines pre-fix, see the moved-from comment higher up).
        // SAFETY: `in_flight[frame]` is live and (per the spec) need only be unsignaled at submit time; resetting it here, immediately before `queue_submit` re-signals it, leaves no deadlock window. On reset failure the fence stays SIGNALED (so next frame's wait won't hang) and we clear the pending acquire signal.
        unsafe {
            if let Err(e) = self
                .device
                .reset_fences(&[self.frame_sync.in_flight[frame]])
                .context("reset_fences")
            {
                // Pre-submit failure: the fence is still in its prior
                // SIGNALED state (the reset is what would have moved it
                // — and just errored), so the next frame's wait won't
                // hang. The acquired `image_available[frame]` slot
                // stays signal-pending though, so mirror the submit-
                // failure recovery to clear it.
                let _ = self
                    .frame_sync
                    .recreate_image_available_for_frame(&self.device, frame);
                reset_fences_failed = Some(e);
            }
        }
        // #3991 — as above: nothing recorded this frame will execute.
        if let Some(e) = reset_fences_failed {
            self.rollback_skin_frame_state(frame);
            return Err(e);
        }

        // SAFETY: queue access is serialized by `graphics_queue`'s Mutex held across the call (VUID-vkQueueSubmit-queue-00893); `cmd` was just closed by `end_command_buffer`, `image_available[frame]` is the wait semaphore and `in_flight[frame]` (just reset) is the signal fence. `cmd` is not re-recorded until that fence is next waited on. On failure both the acquire signal and the fence are recreated before propagating.
        unsafe {
            // Bind the MutexGuard, deref inside the call — `*self
            // .graphics_queue.lock()` would release the guard end-of-
            // statement (vk::Queue is Copy) before `queue_submit` ran,
            // defeating VUID-vkQueueSubmit-queue-00893 the Mutex was
            // added to enforce. Mirrors the present-queue site below.
            // See CONC-D2-NEW-01 (audit 2026-05-16).
            let queue = self
                .graphics_queue
                .lock()
                .expect("graphics queue lock poisoned");
            if let Err(e) = self
                .device
                .queue_submit(*queue, &[submit_info], self.frame_sync.in_flight[frame])
                .context("queue_submit")
            {
                // Submit failed — `image_available[frame]` was never
                // consumed by the (would-be) wait, so it stays signal-
                // pending. Recover before propagating so the next
                // acquire on this slot doesn't trip
                // VUID-vkAcquireNextImageKHR-semaphore-01779.
                // #910 / REN-D5-NEW-01.
                drop(queue);
                let _ = self
                    .frame_sync
                    .recreate_image_available_for_frame(&self.device, frame);
                // #952 / REN-D1-NEW-04 — the reset_fences just above
                // succeeded, so `in_flight[frame]` is UNSIGNALED with
                // no pending submit (this one just failed). Recreate
                // it as SIGNALED so the next frame's
                // `wait_for_fences(..., u64::MAX)` doesn't block forever.
                let _ = self
                    .frame_sync
                    .recreate_in_flight_for_frame(&self.device, frame);
                submit_failed = Some(e);
            } else {
                drop(queue);
            }
        }
        // #3991 — the site that gives this frame's whole recording its meaning.
        // `skin_state_submitted` stays `false`, which is what the caller's
        // rollback of the CPU-side pose commits reads.
        if let Some(e) = submit_failed {
            self.rollback_skin_frame_state(frame);
            return Err(e);
        }

        // #2715 (CONC-D7-UI-01) — `queue_submit` above just created a new
        // pending submission against `bindless_sets[frame]`, so
        // `TextureRegistry::apply_descriptor_write`'s immediate-write fast
        // path may no longer target this slot until the next `begin_frame`
        // (post fence-wait) call re-confirms it idle.
        self.texture_registry.note_frame_submitted(frame);
        if self
            .pending_selected_ray_probe
            .is_some_and(|request| Some(request.generation) == armed_selected_ray_probe_generation)
        {
            self.pending_selected_ray_probe = None;
        }

        // #917 / REN-D10-NEW-03 — advance SVGF + TAA `frames_since_
        // creation` counters now that `queue_submit` returned success.
        // Each pipeline self-gates on its `dispatched_this_frame` flag
        // set during recording, so a skipped dispatch (svgf_failed
        // latch, missing pipeline, upload_params failure) is a no-op
        // here. Pre-fix the counters advanced at record time, meaning a
        // record-time / submit-time failure between them and submit
        // success would leave the counter advanced without the
        // corresponding GPU write — the next frame would assume valid
        // history that wasn't actually written.
        // #3991 — the skin / skinned-BLAS chain joins its #917 siblings here.
        // Four pieces of state describing GPU work were previously committed at
        // RECORD time, above the three tail `Err` sites; they are latched
        // during recording now and promoted here, on the one line that means
        // "this frame's commands are on the queue".
        self.promote_skin_frame_state(frame);
        if let Some(ref mut svgf) = self.post.svgf {
            svgf.mark_frame_completed();
        }
        if let Some(ref mut taa) = self.post.taa {
            taa.mark_frame_completed();
        }
        if let Some(ref mut volumetrics) = self.post.volumetrics {
            volumetrics.mark_frame_completed();
        }
        self.volumetric_time_seconds += frame_time_delta_ms.max(0.0) * 0.001;
        if self
            .post
            .frame_upscaler
            .as_mut()
            .is_some_and(|upscaler| upscaler.take_submitted_dispatch())
        {
            self.post
                .fsr_temporal
                .as_mut()
                .expect("submitted FSR dispatch requires temporal state")
                .mark_dispatch_completed(fsr_reset_delivered);
        }
        // Object-transform history follows successful GPU submission, not
        // command recording or presentation. This mirrors TAA/SVGF history:
        // a failed submit cannot advance the source frame motion reprojects.
        std::mem::swap(
            &mut self.history.previous_rigid_models,
            &mut current_rigid_models,
        );
        current_rigid_models.clear();
        self.scratch.current_rigid_models_scratch = current_rigid_models;
        // #2486 / D5-01 — same shrink policy the two scratch Vecs get at the
        // bottom of this function. Both maps are `clear()`-then-`reserve(
        // draw_commands.len())`, so without this their capacity is the session
        // high-water mark rather than the working set, and one large-exterior
        // peak stays resident through the walk back into a small interior.
        // `previous_rigid_models` post-swap holds this frame's entries, which
        // is the working set for both.
        let working_rigid = self.history.previous_rigid_models.len();
        super::super::acceleration::shrink_map_scratch_if_oversized(
            &mut self.history.previous_rigid_models,
            working_rigid,
            512,
        );
        super::super::acceleration::shrink_map_scratch_if_oversized(
            &mut self.scratch.current_rigid_models_scratch,
            working_rigid,
            512,
        );

        // Present.
        let swapchains = [self.swapchain.state.swapchain];
        let image_indices = [img as u32];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        // SAFETY: present-queue access is serialized by `present_queue`'s Mutex held across the call; `render_finished[img]` (signaled by the submit above) is the present wait semaphore, and `swapchain` + `image_index` are the live acquired image. The OUT_OF_DATE arm degrades to `suboptimal=true` instead of touching stale state.
        let present_suboptimal = unsafe {
            let pq = self
                .present_queue
                .lock()
                .expect("present queue lock poisoned");
            match self
                .swapchain
                .state
                .swapchain_loader
                .queue_present(*pq, &present_info)
            {
                Ok(suboptimal) => suboptimal,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => true,
                Err(e) => anyhow::bail!("queue_present: {:?}", e),
            }
        };

        t.submit_present_ns = submit_t0.elapsed().as_nanos() as u64;
        let post_present_t0 = Instant::now();

        self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
        self.frame_counter = self.frame_counter.wrapping_add(1);

        let working_instances = self.shrink_frame_scratch();

        // #645 / MEM-2-3 — TLAS instance buffer mirrored shrink. The
        // slot we just incremented to (`current_frame` after the line
        // above) is the one whose previous frame work signalled at
        // the start of this frame, so its instance / staging /
        // device-local buffers are GPU-idle at this point and safe to
        // tear down. The slot we just SUBMITTED on (the one before
        // the increment) stays in flight and is left alone.
        //
        // SAFETY: see precondition on
        // `AccelerationManager::shrink_tlas_to_fit` — caller must
        // ensure no in-flight command buffer references the target
        // slot. The `current_frame_after_increment` slot's fence was
        // waited on at the start of this frame's recording (the
        // standard MAX_FRAMES_IN_FLIGHT alternation), so by the time
        // we reach this line its previous use has completed by
        // construction. Same justification used by `#504` for the
        // CPU-side scratch shrink above.
        if let Some(accel) = self.accel_manager.as_mut() {
            if let Some(allocator) = self.allocator.as_ref() {
                let slot_to_shrink = self.current_frame;
                unsafe {
                    // SAFETY: `accel`, `device` and `allocator` are live; the
                    // shrink runs on this frame slot after its prior GPU use
                    // completed (the caller's fence wait), so the freed TLAS
                    // scratch/buffers are not referenced by an in-flight build.
                    accel.shrink_tlas_to_fit(
                        slot_to_shrink,
                        working_instances as u32,
                        &self.device,
                        allocator,
                    );
                    // #682 / MEM-2-7 — TLAS build scratch shrink. Same
                    // safety justification as `shrink_tlas_to_fit`
                    // above (the slot's previous use completed before
                    // this frame's recording began). Since #2929,
                    // `shrink_tlas_to_fit` no longer destroys the slot (it
                    // sets a pending-shrink flag folded in by
                    // `ensure_tlas_state`), so this call's ordering relative
                    // to it is no longer load-bearing — the "tlas[slot] is
                    // None" arm this used to chase in one tick is reached
                    // only by a fresh/never-rebuilt slot now, independent of
                    // call order.
                    accel.shrink_tlas_scratch_to_fit(slot_to_shrink, &self.device, allocator);
                }
            }
        }

        t.post_present_ns = post_present_t0.elapsed().as_nanos() as u64;
        if let Some(out) = timings {
            *out = t;
        }
        Ok(suboptimal || present_suboptimal)
    }
}

// #3632 — `VulkanContext::is_fsr_dispatch_active` needs a live Vulkan device
// to exercise end-to-end (its inputs are `self.post.frame_upscaler` and the
// render-debug fields), so — matching this file's `composite_params_tests`
// / this crate's `fsr_startup_failure_promotes_to_taa_tests` pattern — this
// pins the fix at the source level: the raw-output predicate must be AND-ed
// into the upscaler check, never OR-ed or left independent, so it can only
// narrow `true` to `false` and never manufacture a `true` the upscaler
// check didn't already produce.
#[cfg(test)]
mod is_fsr_dispatch_active_tests {
    fn production_src() -> &'static str {
        include_str!("draw.rs")
    }

    #[test]
    fn folds_the_raw_output_debug_predicate_into_the_fsr_dispatch_check() {
        let src = production_src();
        let fn_start = src
            .find("pub(super) fn is_fsr_dispatch_active(&self) -> bool {")
            .expect("is_fsr_dispatch_active must still exist with this signature");
        let fn_end = src[fn_start..]
            .find("\n    pub fn draw_frame(")
            .map(|offset| fn_start + offset)
            .expect("is_fsr_dispatch_active must be immediately followed by draw_frame");
        let body = &src[fn_start..fn_end];

        let upscaler_check_pos = body
            .find("is_some_and(|upscaler| upscaler.is_fsr_dispatch_active())")
            .expect("must still start from FrameUpscaler's own dispatch-active check");
        let raw_output_pos = body.find("render_debug_requires_raw_output(").expect(
            "#3632 — the accessor must also gate on force_native_debug's raw-output \
             predicate, or a debug view that bridges straight to a native blit is left \
             jittered by a jitter gate that thinks FSR is still reconstructing",
        );
        assert!(
            upscaler_check_pos < raw_output_pos,
            "the upscaler dispatch check must come first, matching the doc comment's framing"
        );
        assert!(
            body[upscaler_check_pos..raw_output_pos].contains("&&")
                && body[upscaler_check_pos..raw_output_pos].contains('!'),
            "the raw-output predicate must be AND-ed in as a negated (suppressing) \
             condition, never OR-ed — it can only turn `true` into `false`, never \
             manufacture a `true` the upscaler check didn't already produce"
        );
    }
}


#[cfg(test)]
mod rigid_motion_contract_tests {
    use super::super::super::upscaling::engine_motion_to_fsr_pixels;
    use ash::vk;
    use byroredux_core::math::{Mat4, Vec3, Vec4};

    fn uv(clip: Vec4) -> [f32; 2] {
        let ndc = clip.truncate() / clip.w;
        [ndc.x * 0.5 + 0.5, ndc.y * 0.5 + 0.5]
    }

    #[test]
    fn stationary_rigid_vertex_has_zero_engine_and_fsr_motion() {
        let point = Vec4::new(0.25, -0.5, 0.0, 1.0);
        let model = Mat4::from_translation(Vec3::new(0.1, 0.2, 0.0));
        let current_uv = uv(model * point);
        let previous_uv = uv(model * point);
        let engine = [
            current_uv[0] - previous_uv[0],
            current_uv[1] - previous_uv[1],
        ];
        assert_eq!(engine, [0.0, 0.0]);
        assert_eq!(
            engine_motion_to_fsr_pixels(
                engine,
                vk::Extent2D {
                    width: 1920,
                    height: 1080,
                },
            ),
            [0.0, 0.0]
        );
    }

    #[test]
    fn moving_rigid_vertex_converts_to_previous_minus_current_pixels() {
        let point = Vec4::new(0.0, 0.0, 0.0, 1.0);
        let previous_uv = uv(Mat4::IDENTITY * point);
        let current_uv = uv(Mat4::from_translation(Vec3::new(0.02, -0.04, 0.0)) * point);
        let engine = [
            current_uv[0] - previous_uv[0],
            current_uv[1] - previous_uv[1],
        ];
        let fsr = engine_motion_to_fsr_pixels(
            engine,
            vk::Extent2D {
                width: 1000,
                height: 500,
            },
        );
        assert!((engine[0] - 0.01).abs() < 1.0e-6);
        assert!((engine[1] + 0.02).abs() < 1.0e-6);
        assert!((fsr[0] + 10.0).abs() < 1.0e-4);
        assert!((fsr[1] - 10.0).abs() < 1.0e-4);
    }
}

/// `draw_frame`'s body — production text only. `draw.rs`'s test modules are
/// interleaved with production code, so a scan cannot simply cut the file at its
/// first test module the way [`crate::source_scan::production_text`] does; and
/// every needle a source-scan test searches for is also spelled out in its own
/// literals, so a search over the whole file can be satisfied by the test itself
/// (#4604's defect class, again in #4842). Slicing to the method's own extent
/// excludes every test module.
#[cfg(test)]
fn draw_frame_body() -> &'static str {
    let src = include_str!("draw.rs");
    let start = src
        .find("\n    pub fn draw_frame(")
        .expect("draw_frame must still exist under this signature");
    let end = start
        + src[start..]
            .find("\n    }\n")
            .expect("draw_frame's closing brace at impl indentation");
    &src[start..end]
}

#[cfg(test)]
mod host_readback_flush_edge_tests {
    use super::draw_frame_body;

    /// #4602 — the global device→host edge must be the LAST command before
    /// `end_command_buffer`, so it covers every readback writer recorded
    /// this frame (the ground-cover counter copy, the screenshot copy, the
    /// depth-capture copy, the probe atomics, and the presentation pass's
    /// image-health atomics, which the probe's own earlier FRAGMENT→HOST
    /// barrier cannot cover). Needles composed at runtime (#3442) so this
    /// test cannot match its own literals.
    #[test]
    fn the_host_flush_edge_precedes_end_command_buffer_and_follows_every_writer() {
        let body = draw_frame_body();
        let edge = ("memory_barrier(\n                &self.device,\n                cmd,\n                vk::PipelineStageFlags::TRANSFER | vk::PipelineStageFlags::FRAGMENT_SHADER,").to_string();
        let edge_pos = body
            .find(&edge)
            .expect("the #4602 device→host flush edge must stay in draw_frame's tail — a fence's dependency covers only device access; the host readbacks need HOST_READ in a memory dependency's destination scope");
        // Every host-readback writer recorded in this tail must precede the
        // edge. The presentation pass records inside `record_post_passes`
        // (a bare `presentation` search would be satisfied by any comment
        // that names it); the screenshot copy is the last writer and the
        // depth-capture copy records earlier in the buffer.
        let production = &body[..edge_pos];
        for writer in [
            "self.depth_capture_record_copy(cmd)",
            "self.record_post_passes(",
            "self.screenshot_record_copy(cmd, swapchain_image)",
        ] {
            assert!(
                production.contains(writer),
                "`{writer}` must be recorded BEFORE the host flush edge (#4602)"
            );
        }
        // And the edge must precede end_command_buffer, with nothing recorded
        // in between — the edge covers only the writers before it, so a
        // command recorded after it reopens the stale-host-read window.
        let end = [".end_command_buffer(", "cmd)"].concat();
        let end_pos = body[edge_pos..]
            .find(&end)
            .map(|rel| edge_pos + rel)
            .expect("end_command_buffer must follow the host flush edge (#4602)");
        // Skip the edge call itself: its arguments are the only `barrier`
        // text the gap may hold.
        let edge_end = edge_pos
            + body[edge_pos..]
                .find(");")
                .expect("the flush edge is a call");
        let gap: String = body[edge_end..end_pos]
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for recorded in ["cmd", "record_", "barrier"] {
            assert!(
                !gap.contains(recorded),
                "`{recorded}` between the host flush edge and end_command_buffer: the \
                 edge must be the last recorded command or its writers are not covered \
                 (#4602). Gap:\n{gap}"
            );
        }
    }
}

/// Regression for #1211 / REN-SAFETY. `draw_frame` must early-return
/// when `self.swapchain.framebuffers` is empty (the state left behind when
/// `recreate_swapchain` fails partway). Without the guard the first
/// indexing access at the `RenderPassBeginInfo::framebuffer(...)` site
/// panics with `index out of bounds`, taking the process down on
/// surface-lost events that are normal Vulkan (window minimize,
/// monitor disconnect, compositor restart, NVIDIA driver mismatch
/// falling back to RADV).
///
/// Live unit test against a mocked `VulkanContext` is impractical —
/// 70+ Vulkan-loader fields with no safe defaults. Static source
/// assertion mirrors the precedent set by
/// `resize.rs::old_image_views_destroyed_between_new_swapchain_creation_and_old_destroy`
/// (#654 ordering check).
#[cfg(test)]
mod framebuffers_empty_guard_tests {
    /// #4604 — this test was vacuous since #3282 moved the wait and the
    /// acquire into `sync_and_acquire_frame.rs`: both needles then matched
    /// only this test's own literals, so `guard_pos < wait_pos` compared
    /// the test's strings to themselves and deleting the production guard
    /// passed. The repair follows #3991's sibling fix and #3442's
    /// compose-needles-at-runtime technique: anchor the ordering on
    /// `draw_frame`'s call to the split-out function, and pin the wait and
    /// acquire against the file that actually contains them.
    #[test]
    fn draw_frame_guards_on_empty_framebuffers_before_acquire() {
        let src = super::draw_frame_body();

        // The production guard — take the FIRST occurrence that is not this
        // test's own literal by searching from the top (the guard at the
        // top of draw_frame precedes every test module in the file).
        let guard_pos = src
            .find("if self.swapchain.framebuffers.is_empty() {")
            .expect("draw_frame must guard on empty framebuffers (#1211)");

        // The split-out acquire path: `draw_frame` delegates to it, and the
        // guard must precede that delegation.
        let sync_pos = src
            .find("self.sync_and_acquire_frame(&mut t)")
            .expect("draw_frame must delegate to sync_and_acquire_frame (#3282)");
        assert!(
            guard_pos < sync_pos,
            "framebuffers.is_empty() guard must come BEFORE \
             sync_and_acquire_frame — no point waiting for a frame we're \
             about to skip. (#1211)"
        );

        // The wait and the acquire live in the split-out file since #3282;
        // pin them THERE so this test cannot match its own literals.
        let sync_src = include_str!("sync_and_acquire_frame.rs");
        let wait_pos = sync_src
            .find(".wait_for_fences(")
            .expect("sync_and_acquire_frame should call wait_for_fences");
        let acquire_pos = sync_src
            .find(".acquire_next_image(")
            .expect("sync_and_acquire_frame should call acquire_next_image");
        assert!(
            wait_pos < acquire_pos,
            "inside sync_and_acquire_frame, the fence wait precedes the \
             acquire (the wait retires the slot this acquire will reuse)"
        );
    }
}

/// Regression for #1796 / D6-02. `skin_dispatch_ran` must be reset
/// `false` before both of `draw_frame`'s early-return guards (empty
/// framebuffers, `ERROR_OUT_OF_DATE_KHR`) and only flipped `true` once
/// `record_skinned_blas_refit` — the function that actually reads
/// `pose_dirty` and gates the skin compute dispatch — runs. A live
/// mocked `VulkanContext` test is impractical for the same reason as
/// `framebuffers_empty_guard_tests` above (70+ Vulkan-loader fields, no
/// safe defaults); a static source assertion pins the ordering instead.
#[cfg(test)]
mod skin_dispatch_ran_ordering_tests {
    /// #3991 / REN-2026-09-06-D4-01. `record_skinned_blas_refit` sets
    /// `skin_dispatch_ran` at its top, and the three sites below sit *under*
    /// that call: `end_command_buffer`, `reset_fences` and `queue_submit`. On
    /// any of them the command buffer is discarded and nothing recorded this
    /// frame executes, so every commit the skin chain made during recording
    /// describes GPU work that will never run.
    ///
    /// #917 established the correct shape ~30 lines below the submit for SVGF,
    /// TAA, volumetrics, FSR and the rigid-model history swap; the skin chain
    /// never adopted it. This pins that it now has: a promotion after the
    /// submit succeeds, and a rollback at each of the three sites.
    #[test]
    fn every_tail_err_site_rolls_back_the_recorded_skin_state() {
        let src = super::draw_frame_body();

        let call_site = src
            .find("self.dispatch_skin_and_cluster(")
            .expect("draw_frame must call dispatch_skin_and_cluster, which reaches the refit");
        let promote = src
            .find("self.promote_skin_frame_state(frame);")
            .expect("draw_frame must promote the skin state after a successful submit (#3991)");
        let submit_ok = src
            .find("svgf.mark_frame_completed();")
            .expect("the #917 post-submit block must still exist");

        // Each tail site latches its error, then rolls back in the outer scope
        // where `&mut self` is free.
        for (name, latch) in [
            ("end_command_buffer", "end_command_buffer_failed"),
            ("reset_fences", "reset_fences_failed"),
            ("queue_submit", "submit_failed"),
        ] {
            let guard = format!(
                "if let Some(e) = {latch} {{\n            self.rollback_skin_frame_state(frame);"
            );
            assert!(
                src.contains(&guard),
                "the {name} failure path must roll the recorded skin state back \
                 before propagating — otherwise a discarded command buffer \
                 leaves a stale bone palette, an unpopulated skin output marked \
                 populated, and a BLAS that is UPDATE-refit and ray-traced from \
                 memory that was never built (#3991)"
            );
            let latch_pos = src.find(&guard).expect("checked by the assertion above");
            assert!(
                call_site < latch_pos,
                "the {name} site must sit below the skin dispatch — if \
                 it did not, there would be no recorded skin state to roll back \
                 and this whole finding would not exist"
            );
            assert!(
                latch_pos < promote,
                "the {name} rollback must precede the promotion: they are the \
                 two exclusive outcomes of the same recording"
            );
        }

        // The promotion belongs with its #917 siblings, on the far side of the
        // submit rather than anywhere a later `Err` could still bypass it.
        assert!(
            promote < submit_ok,
            "promote_skin_frame_state must sit in the post-submit block \
             alongside svgf/taa/volumetrics mark_frame_completed (#917/#3991)"
        );
    }

    /// The record-time latch and the submit-time flag must both be reset at the
    /// top of `draw_frame`, or a bailed frame reports the previous frame's
    /// outcome (#1796 for the first, #3991 for the second).
    #[test]
    fn the_submit_time_flag_is_reset_alongside_the_record_time_latch() {
        let src = super::draw_frame_body();
        let record_reset = src
            .find("self.skin_dispatch_ran = false;")
            .expect("draw_frame must reset skin_dispatch_ran (#1796)");
        let submit_reset = src
            .find("self.skin_state_submitted = false;")
            .expect("draw_frame must reset skin_state_submitted (#3991)");
        let fb_guard = src
            .find("if self.swapchain.framebuffers.is_empty() {")
            .expect("draw_frame must guard on empty framebuffers (#1211)");
        assert!(record_reset < fb_guard && submit_reset < fb_guard);
        // And the flag is only ever SET from the record-time latch, inside the
        // promotion — never at recording time, which is the bug this fixes.
        let refit = crate::source_scan::production_text(include_str!("skinned_blas_refit.rs"));
        assert!(
            refit.contains("self.skin_state_submitted = self.skin_dispatch_ran;"),
            "the submit-time flag must be derived from the record-time latch \
             inside promote_skin_frame_state, and nowhere else (#3991)"
        );
    }

    #[test]
    fn skin_dispatch_ran_is_reset_before_both_early_return_guards() {
        let src = super::draw_frame_body();

        let reset_pos = src
            .find("self.skin_dispatch_ran = false;")
            .expect("draw_frame must reset skin_dispatch_ran to false (#1796)");
        let fb_guard_pos = src
            .find("if self.swapchain.framebuffers.is_empty() {")
            .expect("draw_frame must guard on empty framebuffers (#1211)");
        // #3991 — the second stale needle. The `ERROR_OUT_OF_DATE_KHR` match
        // moved into `sync_and_acquire_frame` when #3282 split `draw_frame`
        // into five, and this `find` had been matching its own literal ever
        // since. `draw_frame`'s guard is now the `?` + `else` on that call.
        let oode_guard_pos = src
            .find(
                "let Some((frame, img, suboptimal)) = self.sync_and_acquire_frame(&mut t)? else {",
            )
            .expect(
                "draw_frame must early-return on a failed / out-of-date acquire \
                 via sync_and_acquire_frame",
            );
        // #3991 — this used to anchor on `"self.record_skinned_blas_refit("`,
        // which does not appear in `draw.rs` at all: the call moved into
        // `dispatch_skin_and_cluster` and the `find` had been matching the
        // test's own literal ever since, comparing two needles inside this
        // module and pinning nothing. (The module doc above warns about
        // exactly this trap, in this exact file.) Anchor on the real chain:
        // `draw_frame` -> `dispatch_skin_and_cluster` -> the refit.
        let call_site_pos = src
            .find("self.dispatch_skin_and_cluster(")
            .expect("draw_frame must call dispatch_skin_and_cluster (#1796)");
        assert!(
            crate::source_scan::production_text(include_str!("dispatch_skin_and_cluster.rs"))
                .contains("record_skinned_blas_refit("),
            "dispatch_skin_and_cluster must still be what reaches \
             record_skinned_blas_refit, or this test's anchor is measuring the \
             wrong call (#1796 / #3991)"
        );

        assert!(
            reset_pos < fb_guard_pos,
            "skin_dispatch_ran reset must come BEFORE the empty-framebuffers \
             guard, or that early return would leave the flag from the \
             previous frame's outcome instead of reporting its own. (#1796)"
        );
        assert!(
            reset_pos < oode_guard_pos,
            "skin_dispatch_ran reset must come BEFORE the \
             ERROR_OUT_OF_DATE_KHR guard, for the same reason. (#1796)"
        );
        assert!(
            fb_guard_pos < call_site_pos && oode_guard_pos < call_site_pos,
            "record_skinned_blas_refit (which sets skin_dispatch_ran true) \
             must be called AFTER both early-return guards — calling it any \
             earlier would defeat the rollback signal entirely. (#1796)"
        );
    }
}

/// Regression for #3569 / D9-01. `bind_inverse_upload_failed` must be
/// reset `false` in lockstep with `skin_dispatch_ran` — both guard the
/// same rollback check in `app_frame.rs`, and a stale `true` surviving
/// from a previous frame's failure would force an unnecessary requeue
/// every frame after.
#[cfg(test)]
mod bind_inverse_upload_failed_reset_tests {
    #[test]
    fn bind_inverse_upload_failed_is_reset_alongside_skin_dispatch_ran() {
        let src = super::draw_frame_body();

        let skin_reset_pos = src
            .find("self.skin_dispatch_ran = false;")
            .expect("draw_frame must reset skin_dispatch_ran to false (#1796)");
        let upload_failed_reset_pos = src
            .find("self.bind_inverse_upload_failed = false;")
            .expect("draw_frame must reset bind_inverse_upload_failed to false (#3569)");
        let fb_guard_pos = src
            .find("if self.swapchain.framebuffers.is_empty() {")
            .expect("draw_frame must guard on empty framebuffers (#1211)");

        assert!(
            skin_reset_pos < upload_failed_reset_pos,
            "bind_inverse_upload_failed reset must come right after the \
             skin_dispatch_ran reset it mirrors. (#3569)"
        );
        assert!(
            upload_failed_reset_pos < fb_guard_pos,
            "bind_inverse_upload_failed reset must come BEFORE the \
             empty-framebuffers guard — same reasoning as \
             skin_dispatch_ran: an early return must not leak the \
             previous frame's failure into this frame's check. (#3569)"
        );
    }
}

/// Regression for D6-04 / #1811. `next_clean_skin_frames` /
/// `should_skip_skin_gpu_refresh` gate the bone_world upload + device

#[cfg(test)]
mod draw_frame_size_budget_tests {
    #[test]
    fn cpu_preparation_spans_bracket_the_work_and_do_not_overlap() {
        fn ordered(source: &str, needles: &[&str]) {
            let mut rest = source;
            for needle in needles {
                let pos = rest
                    .find(needle)
                    .unwrap_or_else(|| panic!("missing or out of order: {needle}"));
                rest = &rest[pos + needle.len()..];
            }
        }
        let upload = include_str!("build_and_upload_instances.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        ordered(
            upload,
            &[
                "t.ssbo_build_ns =",
                "let pipeline_t0 = Instant::now();",
                "let variant_t0 = Instant::now();",
                "self.get_or_create_blend_pipeline(",
                "variant_t0.elapsed()",
                "save_pipeline_cache_if_grown(",
                "t.pipeline_compile_ns = pipeline_t0.elapsed()",
                "let parameter_t0 = Instant::now();",
                "let composite_inputs =",
                "t.parameter_upload_ns = parameter_t0.elapsed()",
            ],
        );
        let fog = include_str!("../volumetrics.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        ordered(
            fog,
            &[
                "let fog_cluster_t0 = std::time::Instant::now();",
                "let build = build_fog_volume_clusters(",
                "self.param_buffers[frame].write_mapped(",
                "self.fog_volume_buffers[frame].write_mapped_prefix(",
                "self.fog_cluster_buffers[frame].write_mapped_at(",
                "*fog_cluster_ns = fog_cluster_t0.elapsed()",
            ],
        );
        ordered(
            super::draw_frame_body(),
            &[
                "let cmd_t0 = Instant::now();",
                "&mut t.fog_cluster_ns,",
                "t.cmd_record_ns =",
                ".saturating_sub(t.fog_cluster_ns)",
                "t.submit_present_ns =",
                "let post_present_t0 = Instant::now();",
                "self.shrink_frame_scratch()",
                "accel.shrink_tlas_scratch_to_fit(",
                "t.post_present_ns = post_present_t0.elapsed()",
                "*out = t;",
            ],
        );
    }

    /// Headroom over `draw_frame`'s length after #4767's extraction (704
    /// lines). Raise it only alongside a deliberate decision, not to absorb
    /// an inline addition.
    const DRAW_FRAME_LINE_BUDGET: usize = 720;

    /// #5087 (regression of #4767) — the function budget above could not see
    /// growth landing in the pure helpers above `draw_frame` (`build_composite_params`,
    /// the DoF/FSR builders, camera deltas), which is exactly how the file
    /// regrew to 2211 production lines after #4767 cut it to 1982. Those
    /// helpers now live in `frame_params.rs`; this pins BOTH files' production
    /// (non-test) line counts, so the next inline addition fails here and gets
    /// its own module instead of quietly regrowing one of the two.
    const DRAW_FILE_LINE_BUDGET: usize = 900;
    const FRAME_PARAMS_FILE_LINE_BUDGET: usize = 1500;

    /// Production lines of a context source file: everything except its
    /// `#[cfg(test)]` modules. `draw.rs` and `frame_params.rs` interleave
    /// test modules with production code, so every test module is stripped
    /// rather than cutting at the first one the way
    /// [`crate::source_scan::production_text`] does.
    fn production_lines(src: &str) -> usize {
        let mut out = 0usize;
        let mut depth: i32 = 0;
        for line in src.lines() {
            let trimmed = line.trim_start();
            if depth == 0 && (trimmed.starts_with("#[cfg(test)]") || trimmed.starts_with("#[cfg(all(test")) {
                // Skip the attribute line(s) and the module that follows.
                continue;
            }
            if depth > 0 {
                depth += line.matches('{').count() as i32;
                depth -= line.matches('}').count() as i32;
                continue;
            }
            if trimmed.starts_with("mod ") && trimmed.ends_with('{') {
                // A #[cfg(test)] mod opening — enter it (only reachable when
                // the attribute was consumed above on a prior line).
                depth = 1;
                continue;
            }
            out += 1;
        }
        out
    }

    #[test]
    fn draw_and_frame_params_stay_within_their_file_budgets() {
        let draw = production_lines(include_str!("draw.rs"));
        assert!(
            draw <= DRAW_FILE_LINE_BUDGET,
            "draw.rs carries {draw} production lines (budget {DRAW_FILE_LINE_BUDGET}); \
             move the new work into a context/<phase>.rs sibling like \
             sync_and_acquire_frame.rs, or into frame_params.rs if it is pure \
             parameter assembly, rather than raising the budget (#5087)"
        );
        let params = production_lines(include_str!("frame_params.rs"));
        assert!(
            params <= FRAME_PARAMS_FILE_LINE_BUDGET,
            "frame_params.rs carries {params} production lines (budget \
             {FRAME_PARAMS_FILE_LINE_BUDGET}); split the next addition into its own \
             module rather than raising the budget (#5089)"
        );
    }

    /// #4767 / TD1-2026-09-22-01 — `draw_frame` has been split and regrown
    /// past the file's 2000-production-LOC line at least six times (#1052,
    /// #1748, #1857, #2197, #2255, #3282, #4767): each fix moved a phase
    /// into a sibling, and the next features were inlined back. The file
    /// budget is only measured by the tech-debt audit; this budgets the
    /// orchestrator itself, so the next inline addition fails here and gets
    /// its own phase file (`context/<phase>.rs`) instead.
    #[test]
    fn draw_frame_stays_within_its_line_budget() {
        let lines = super::draw_frame_body().matches('\n').count() + 1;
        assert!(
            lines <= DRAW_FRAME_LINE_BUDGET,
            "draw_frame is {lines} lines (budget {DRAW_FRAME_LINE_BUDGET}); move the new \
             work into a context/<phase>.rs sibling like sync_and_acquire_frame.rs or \
             shrink_frame_scratch.rs rather than raising the budget (#4767)"
        );
    }
}
