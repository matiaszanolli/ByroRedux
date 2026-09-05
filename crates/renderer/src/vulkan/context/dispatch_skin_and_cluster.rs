//! Skin GPU upload + palette-build dispatch, skinned-BLAS refit, TLAS
//! build, and cluster light culling — extracted from `draw.rs` (#3282 /
//! TD1-2026-08-24-01) to shrink `draw_frame`. The TLAS build sits between
//! the skin dispatch and the cluster-cull dispatch in the pre-split
//! `draw_frame` (it was relocated there by M29 Phase 2 to pick up
//! same-frame skinned poses), so it stays bundled into this phase rather
//! than the command-buffer-open phase, to avoid reordering it relative to
//! its neighbors. The single `unsafe` scopes, barrier order, and recording
//! order are unchanged from the pre-split `draw_frame`.

use super::super::descriptors::memory_barrier;
use super::draw::{next_clean_skin_frames, should_skip_skin_gpu_refresh};
use super::{DrawCommand, FrameTimings, VulkanContext};
use ash::vk;
use byroredux_core::ecs::storage::EntityId;
use std::time::Instant;

impl VulkanContext {
    /// Upload bone_world + pending bind_inverses, dispatch the skin-palette
    /// compute pass, refit skinned BLASes, build the TLAS, and dispatch
    /// cluster light culling. Extracted verbatim from `draw_frame` — the
    /// recording order is unchanged.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn dispatch_skin_and_cluster(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        draw_commands: &[DrawCommand],
        bone_world: &[[[f32; 4]; 4]],
        skin_offsets: &rustc_hash::FxHashMap<EntityId, u32>,
        bind_inverse_pending_uploads: &[(u32, Vec<[[f32; 4]; 4]>)],
        pose_dirty: &rustc_hash::FxHashSet<EntityId>,
        instance_map: &[Option<u32>],
        tlas_t0: Instant,
        t: &mut FrameTimings,
    ) {
        // D6-04 / #1811 — track how many consecutive frames had no
        // skinned-pose change and no pending first-sight bind_inverses
        // upload. Any dirty signal resets the streak so the forthcoming
        // upload/copy/dispatch trio (below) always runs at least once
        // per change, and for the next `MAX_FRAMES_IN_FLIGHT` frames
        // after that so every per-frame `bone_world` buffer copy sees
        // the fresh value at least once (same safety margin as the
        // `MAX_FRAMES_IN_FLIGHT + 1` sweep threshold in
        // `SkinSlotPool::sweep` / `build_skinned_palettes`).
        let skin_state_dirty = !pose_dirty.is_empty() || !bind_inverse_pending_uploads.is_empty();
        self.clean_skin_frames = next_clean_skin_frames(self.clean_skin_frames, skin_state_dirty);
        let skip_skin_gpu_refresh = should_skip_skin_gpu_refresh(self.clean_skin_frames);

        // M29.5/M29.6 — upload bone_world (per-frame) and any pending
        // first-sight bind_inverses (write-once persistent SSBO). The
        // skin_palette dispatch below reads both:
        //   - bone_world from the per-frame DEVICE_LOCAL pair
        //   - bind_inverses from the persistent DEVICE_LOCAL SSBO
        // and writes the existing palette SSBO that raster +
        // skin_vertices.comp consume.
        //
        // D6-04 / #1811 — skipped entirely once `skip_skin_gpu_refresh`
        // is true: every live frame-in-flight buffer already holds
        // today's (unchanged) bone_world content, so the staging
        // memcpy + device copy would just rewrite identical bytes.
        if !skip_skin_gpu_refresh {
            let dirty_slot_offsets = pose_dirty
                .iter()
                .filter_map(|entity| skin_offsets.get(entity).copied());
            self.scene_buffers
                .upload_bone_worlds(&self.device, frame, bone_world, dirty_slot_offsets)
                .unwrap_or_else(|e| log::warn!("Failed to upload bone_world: {e}"));
        }

        // #3676 — the timestamp begins before the first transfer command in
        // this skin-preparation chain. The staging writes above are host work
        // and intentionally remain outside the GPU measurement; the bracket
        // covers the device-local copies, their transfer barriers, and the
        // palette compute dispatch below.
        let mut skin_palette_timer_started = false;
        let bone_world_copy_recorded = !skip_skin_gpu_refresh
            && self.scene_buffers.bone_input_upload_bytes(frame) > 0;
        if bone_world_copy_recorded {
            if let Some(ref mut timers) = self.gpu_timers {
                timers.cmd_skin_palette_start(&self.device, cmd, frame);
                skin_palette_timer_started = true;
            }
        }
        if !skip_skin_gpu_refresh {
            self.scene_buffers
                .record_bone_world_copy(&self.device, cmd, frame);
        }

        // M29.6 — drain pending bind_inverses first-sight uploads.
        // Two-stage: write into HOST_VISIBLE staging, then record
        // per-slot cmd_copy_buffer regions into the persistent SSBO,
        // followed by a single TRANSFER → COMPUTE_SHADER barrier.
        // No-op when the pending list is empty (steady-state).
        let pending_capped = if !bind_inverse_pending_uploads.is_empty() {
            self.scene_buffers
                .upload_pending_bind_inverses(&self.device, bind_inverse_pending_uploads)
                .unwrap_or_else(|e| {
                    log::warn!("Failed to upload pending bind_inverses: {e}");
                    // #3569 / D9-01 — `bind_inverse_pending_uploads` was
                    // already irrevocably drained from `SkinSlotPool` before
                    // this call. `record_skinned_blas_refit` (later this
                    // same frame) sets `skin_dispatch_ran = true`
                    // unconditionally, so without this latch the caller's
                    // `!skin_dispatch_ran` requeue check never fires and
                    // these entries are lost for good. Reset alongside
                    // `skin_dispatch_ran` at the top of `draw_frame`.
                    self.bind_inverse_upload_failed = true;
                    0
                })
        } else {
            0
        };
        if pending_capped > 0 {
            if !skin_palette_timer_started {
                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_skin_palette_start(&self.device, cmd, frame);
                    skin_palette_timer_started = true;
                }
            }
            let pending_slots: Vec<u32> = bind_inverse_pending_uploads
                .iter()
                .take(pending_capped)
                .map(|(s, _)| *s)
                .collect();
            self.scene_buffers.record_pending_bind_inverse_copies(
                &self.device,
                cmd,
                &pending_slots,
                pending_capped,
            );
        }

        // M29.5/M29.6 — dispatch the palette-build compute pass.
        // Writes the existing `bone_device_buffers[frame]` SSBO that
        // raster (`triangle.vert:147-204` inline-skinning, set 1
        // binding 3 + binding 12) and `skin_vertices.comp` (set 0
        // binding 1 in SkinComputePipeline) read. Emits the
        // COMPUTE_SHADER_WRITE → (COMPUTE_SHADER_READ | VERTEX_SHADER_READ)
        // barrier on the palette buffer after the dispatch so both
        // downstream consumers see well-defined data.
        if let Some(ref mut skin_palette) = self.skin_palette {
            let bone_dispatch_bytes = self.scene_buffers.bone_world_dispatch_bytes(frame);
            // Each palette slot is one mat4 = 64 B. Skip the dispatch
            // entirely when there are no skinned bones this frame —
            // the palette buffer retains its prior contents (slot 0
            // identity from a previous frame's write, or zero on
            // frame 0), so any raster sampling at `bone_offset = 0`
            // either reads identity (post-warm) or garbage that
            // never gets shaded (no entity points there).
            let bone_count = (bone_dispatch_bytes as usize
                / std::mem::size_of::<[[f32; 4]; 4]>())
                as u32;
            // D6-04 / #1811 — also skip once `skip_skin_gpu_refresh` is
            // true: the palette buffer already holds the correct output
            // for today's (unchanged) bone_world + bind_inverses, so a
            // full-range recompute would just rewrite identical data.
            if bone_count > 0
                && !skip_skin_gpu_refresh
                && (bone_world_copy_recorded || pending_capped > 0)
            {
                if !skin_palette_timer_started {
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_skin_palette_start(&self.device, cmd, frame);
                        skin_palette_timer_started = true;
                    }
                }
                let bone_world_buf = self.scene_buffers.bone_world_buffers()[frame].buffer;
                let bind_inverse_buf = self.scene_buffers.bind_inverses_persistent().buffer;
                let bind_inverse_size = self.scene_buffers.bone_buffer_size();
                let palette_buf = self.scene_buffers.bone_buffers()[frame].buffer;
                let palette_size = self.scene_buffers.bone_buffer_size();
                // SAFETY: `cmd` is recording (begin_command_buffer succeeded above); the bone-world / bind-inverse / palette buffers are live SSBOs for this frame and `bone_count > 0`. The COMPUTE_SHADER_WRITE -> SHADER_READ buffer barrier afterward sequences the palette write before its compute + vertex consumers; no concurrent recording of this buffer.
                unsafe {
                    skin_palette.dispatch(
                        &self.device,
                        cmd,
                        frame,
                        super::super::skin_compute::PaletteDispatchBuffers {
                            bone_world_buffer: bone_world_buf,
                            bone_world_buffer_size: bone_dispatch_bytes,
                            bind_inverse_buffer: bind_inverse_buf,
                            bind_inverse_buffer_size: bind_inverse_size,
                            palette_buffer: palette_buf,
                            palette_buffer_size: palette_size,
                        },
                        super::super::skin_compute::SkinPalettePushConstants { bone_count },
                    );
                    // COMPUTE_SHADER_WRITE → SHADER_READ barrier on the
                    // palette buffer covers both downstream consumers:
                    // `skin_vertices.comp` (compute read in this same
                    // command buffer below) and `triangle.vert` (vertex
                    // read during the raster pass).
                    let palette_barrier = vk::BufferMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .buffer(palette_buf)
                        .offset(0)
                        .size(palette_size);
                    self.device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::COMPUTE_SHADER,
                        vk::PipelineStageFlags::COMPUTE_SHADER
                            | vk::PipelineStageFlags::VERTEX_SHADER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[palette_barrier],
                        &[],
                    );
                }
            }
        }

        if skin_palette_timer_started {
            if let Some(ref mut timers) = self.gpu_timers {
                timers.cmd_skin_palette_end(&self.device, cmd, frame);
            }
        }

        self.record_skinned_blas_refit(cmd, frame, draw_commands, pose_dirty);

        // ── TLAS build (relocated from top of frame) ─────────────────
        // Picks up just-refit per-skinned-entity BLAS via the
        // `bone_offset != 0` override in `build_tlas`. Static draws
        // continue using the per-mesh `blas_entries` table.
        self.tlas_build_succeeded_last_frame = false;
        // SAFETY: `cmd` is recording; `accel` and `alloc` are live. `build_tlas`
        // records into `cmd`; the following barrier sequences ray-query reads.
        unsafe {
            if let Some(ref mut accel) = self.accel_manager {
                if let Some(alloc) = self.allocator.as_ref() {
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_tlas_build_start(&self.device, cmd, frame);
                    }
                    let tlas_build_failed = if let Err(e) = accel.build_tlas(
                        &self.device,
                        alloc,
                        cmd,
                        draw_commands,
                        &instance_map,
                        frame,
                    ) {
                        log::warn!("TLAS build failed: {e}");
                        // #2673 / CONC-D1-NEW-01 — defence in depth for
                        // the warn-only policy above. `tlas_written` is
                        // otherwise a one-way latch, so a slot that ever
                        // had a TLAS keeps `rt_flag = 1.0` forever and
                        // every RT path (shadows, reflections, GI, water
                        // refraction) keeps ray-querying binding 2 on a
                        // frame whose build never landed. Re-point the
                        // binding at whatever AS the manager still owns
                        // (post-#2673 a failed resize keeps the previous
                        // one alive), then clear the latch and drop
                        // `rt_flag` so this frame degrades to non-RT
                        // shading instead. The next successful build
                        // re-latches and re-patches it to 1.0 via the
                        // `first_tlas_this_slot` path below.
                        if let Some(stale_handle) = accel.tlas_handle(frame) {
                            self.scene_buffers
                                .write_tlas(&self.device, frame, stale_handle);
                        }
                        // Ordered after `write_tlas`, which latches the
                        // flag `true` as a side effect.
                        self.scene_buffers.tlas_written[frame] = false;
                        if let Err(e) =
                            self.scene_buffers
                                .patch_camera_rt_flag(&self.device, frame, 0.0)
                        {
                            log::warn!("Failed to clear rt_flag after TLAS build failure: {e}");
                        }
                        self.rt_flag_last_frame = false;
                        true
                    } else {
                        if let Some(ref mut timers) = self.gpu_timers {
                            timers.cmd_tlas_build_end(&self.device, cmd, frame);
                        }
                        false
                    };

                    // Memory barrier: AS writes → ray-query consumers
                    // (FRAGMENT_SHADER for main render pass +
                    // COMPUTE_SHADER for caustic_splat.comp and the
                    // volumetrics inject dispatch). See #415 for the
                    // COMPUTE_SHADER widening.
                    // AS_BUILD_KHR → FRAGMENT_SHADER|COMPUTE_SHADER
                    //
                    // #2931 / CON-D2-01 — this runs on BOTH arms, not just
                    // the success arm. It does not only publish the TLAS
                    // build: `record_skinned_blas_refit` ran earlier in this
                    // same command buffer, and this is the frame's ONLY
                    // AS_WRITE → AS_READ barrier, so it is what makes those
                    // refits visible too.
                    //
                    // Clearing `rt_flag` on the failure arm is not
                    // sufficient cover. `rt_flag` gates the FRAGMENT
                    // consumers; the volumetrics inject dispatch gates on
                    // `accel.tlas_handle(frame)` instead
                    // (`post_passes.rs::record_volumetrics_pass`), and
                    // post-#2673 a failed build deliberately keeps the
                    // previous AS alive — so `tlas_handle` is still `Some`,
                    // volumetrics still ray-queries from COMPUTE, and
                    // without this barrier it reads skinned BLAS whose
                    // refit writes were never made visible.
                    //
                    // An extra barrier on a path that only runs when a TLAS
                    // build has already failed costs nothing measurable;
                    // skipping it is a real RAW hazard.
                    memory_barrier(
                        &self.device,
                        cmd,
                        vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                        vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
                        vk::PipelineStageFlags::FRAGMENT_SHADER
                            | vk::PipelineStageFlags::COMPUTE_SHADER,
                        vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
                    );

                    if !tlas_build_failed {
                        if let Some(tlas_handle) = accel.tlas_handle(frame) {
                            self.tlas_build_succeeded_last_frame = true;
                            // Capture whether this is the first time the
                            // TLAS lands for this FIF slot — `write_tlas`
                            // flips `tlas_written[frame] = true`, but
                            // we want to know if it WAS false before.
                            let first_tlas_this_slot = !self.scene_buffers.tlas_written[frame];
                            self.scene_buffers
                                .write_tlas(&self.device, frame, tlas_handle);
                            // #1227 / REN-D8-NEW-21 — earlier in this
                            // frame `rt_flag` was uploaded as 0.0 because
                            // `tlas_written[frame]` was still false at
                            // camera-UBO upload time. Now that the TLAS
                            // exists and the descriptor is wired, patch
                            // `flags[0]` to 1.0 in-place so the upcoming
                            // render pass sees RT enabled on this very
                            // frame. Without this, frame 0 + frame 1
                            // (one per FIF slot) render with RT shading
                            // off and TAA dissolves the flash across
                            // ~5 frames on every cell-load. Only fires
                            // on RT-capable hardware AND only on the
                            // slot's first valid-TLAS frame — steady
                            // state pays nothing.
                            if first_tlas_this_slot && self.device_caps.ray_query_supported {
                                self.rt_flag_last_frame = match self
                                    .scene_buffers
                                    .patch_camera_rt_flag(&self.device, frame, 1.0)
                                {
                                    Ok(()) => true,
                                    Err(error) => {
                                        log::warn!("Failed to patch rt_flag post-TLAS: {error}");
                                        false
                                    }
                                };
                            }
                        }
                        // #1792 — `pending_bytes = 0`: no in-flight batch
                        // context at this per-frame call site.
                        accel.evict_unused_blas(&self.device, alloc, 0);
                    }
                }
            }
        }
        t.tlas_build_ns = tlas_t0.elapsed().as_nanos() as u64;

        // ── Cluster light culling (compute dispatch) ─────────────────
        //
        // Runs after light + camera uploads, before the render pass.
        // The compute shader reads lights/camera and writes cluster SSBOs
        // that the fragment shader reads during the render pass.
        // SAFETY: `cmd` is recording; `cc` (cluster-cull pipeline) and its per-frame cluster SSBOs are live. The leading HOST_WRITE -> COMPUTE barrier makes the host-written light/camera buffers visible before `dispatch`; the trailing COMPUTE_WRITE -> FRAGMENT_READ barrier sequences the cluster SSBO outputs before the render pass reads them.
        unsafe {
            if let Some(ref mut cc) = self.cluster_cull {
                // Barrier: host writes to light/camera SSBOs must be visible
                // to the compute shader before dispatch. Required by Vulkan
                // spec even for HOST_COHERENT memory. Instance data is NOT
                // uploaded yet — it is built and uploaded after this dispatch.
                // HOST → COMPUTE_SHADER (light/camera UBO flush)
                memory_barrier(
                    &self.device,
                    cmd,
                    vk::PipelineStageFlags::HOST,
                    vk::AccessFlags::HOST_WRITE,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::SHADER_READ | vk::AccessFlags::UNIFORM_READ,
                );

                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_cluster_cull_start(&self.device, cmd, frame);
                }
                cc.dispatch(&self.device, cmd, frame);
                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_cluster_cull_end(&self.device, cmd, frame);
                }
                // Barrier: compute writes → fragment reads on cluster SSBOs.
                // COMPUTE_SHADER → FRAGMENT_SHADER (cluster SSBO outputs)
                memory_barrier(
                    &self.device,
                    cmd,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::SHADER_WRITE,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::AccessFlags::SHADER_READ,
                );
            }
        }
    }
}

/// Regression for #3569 / D9-01. A failed first-sight `bind_inverses`
/// upload must latch `bind_inverse_upload_failed = true` in the same
/// `unwrap_or_else` arm that logs the warning — otherwise the caller's
/// `!skin_dispatch_ran || bind_inverse_upload_failed` rollback check in
/// `app_frame.rs` never sees the failure (`record_skinned_blas_refit`,
/// later this same frame, unconditionally sets `skin_dispatch_ran =
/// true`), and the entries `SkinSlotPool::drain_pending` already removed
/// are lost for good.
#[cfg(test)]
mod bind_inverse_upload_failure_latch_tests {
    #[test]
    fn upload_pending_bind_inverses_failure_arm_sets_the_latch() {
        let src = include_str!("dispatch_skin_and_cluster.rs");

        let warn_pos = src
            .find("Failed to upload pending bind_inverses: {e}")
            .expect(
                "dispatch_skin_and_cluster must warn on upload_pending_bind_inverses failure (#3569)",
            );
        let latch_pos = src
            .find("self.bind_inverse_upload_failed = true;")
            .expect(
                "the upload_pending_bind_inverses failure arm must set \
                 bind_inverse_upload_failed = true, or the requeue signal \
                 is silently lost (#3569)",
            );

        assert!(
            warn_pos < latch_pos,
            "the latch must be set in the same error arm as the warning \
             log, not somewhere unrelated. (#3569)"
        );
        // Loose textual proximity check (source-scan, not AST) that the
        // latch sits in the same `unwrap_or_else` closure as the warning
        // rather than somewhere unrelated in the file.
        assert!(
            latch_pos - warn_pos < 1000,
            "the latch should be set immediately alongside the warning, \
             inside the same unwrap_or_else closure. (#3569)"
        );
    }
}
