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
        let bone_world_copy_recorded =
            !skip_skin_gpu_refresh && self.scene_buffers.bone_input_upload_bytes(frame) > 0;
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
                    // #4049 — `Once`-gated, matching `SkinSlotPool::
                    // overflow_warned` / `failed_skin_slots` / `failed_skin_blas`:
                    // the #3569 requeue retries this same upload every frame
                    // until it succeeds, so an un-gated `warn!` here floods the
                    // log once per frame for as long as the failure persists,
                    // and buries the diagnostically useful FIRST occurrence.
                    // `bind_inverse_upload_failure_count` keeps the magnitude
                    // (surfaced via `skin.coverage`) after the log goes silent.
                    self.bind_inverse_upload_failure_count =
                        self.bind_inverse_upload_failure_count.saturating_add(1);
                    if !self.bind_inverse_upload_warned {
                        self.bind_inverse_upload_warned = true;
                        log::warn!(
                            "Failed to upload pending bind_inverses: {e} (subsequent \
                             consecutive failures are counted, not logged — see \
                             skin.coverage's bind_inverse_upload_failures)"
                        );
                    }
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
        let pending_slots: Vec<u32> = bind_inverse_pending_uploads
            .iter()
            .take(pending_capped)
            .map(|(s, _)| *s)
            .collect();
        if pending_capped > 0 {
            if !skin_palette_timer_started {
                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_skin_palette_start(&self.device, cmd, frame);
                    skin_palette_timer_started = true;
                }
            }
            self.scene_buffers.record_pending_bind_inverse_copies(
                &self.device,
                cmd,
                &pending_slots,
                pending_capped,
            );
            // #4204 — a bind-inverse that lands now changes the palette of a
            // slot whose pose may already have been copied and computed. The
            // dense dispatch hid that by recomputing everything; the narrowed
            // one must be told, for this frame slot *and* the other one.
            self.scene_buffers.mark_palette_slots_dirty(&pending_slots);
        }

        // M29.5/M29.6 — dispatch the palette-build compute pass.
        // Writes the existing `bone_device_buffers[frame]` SSBO that
        // raster (`triangle.vert:147-204` inline-skinning, set 1
        // binding 3 + binding 12) and `skin_vertices.comp` (set 0
        // binding 1 in SkinComputePipeline) read. Emits the
        // COMPUTE_SHADER_WRITE → (COMPUTE | VERTEX | FRAGMENT) SHADER_READ
        // barrier on the palette buffer after the dispatch so all
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
            let bone_count =
                (bone_dispatch_bytes as usize / std::mem::size_of::<[[f32; 4]; 4]>()) as u32;
            // #4204 — dispatch only the palette slots whose inputs changed
            // for THIS frame slot: the bone-world strides the staging copy
            // just refreshed, plus the slots whose bind-inverse landed above.
            // The dense `[0, bone_count)` range is what this used to run
            // whenever anything moved, which made the pass scale with the
            // skinned population instead of with how much of it moved.
            // `plan_palette_dispatch` still picks the dense range when the
            // dirty set would cover it anyway.
            let stride = crate::shader_constants::MAX_BONES_PER_MESH;
            // #4611 — planned into the persistent scratch, restored below.
            let mut palette_plan = std::mem::take(&mut self.scratch.palette_plan_scratch);
            super::super::skin_compute::plan_palette_dispatch(
                self.scene_buffers.palette_dirty_bone_ranges(frame).chain(
                    pending_slots
                        .iter()
                        .map(|&slot| (slot * stride, (slot + 1) * stride)),
                ),
                bone_count,
                &mut palette_plan,
            );
            // D6-04 / #1811 — also skip once `skip_skin_gpu_refresh` is
            // true: the palette buffer already holds the correct output
            // for today's (unchanged) bone_world + bind_inverses, so any
            // recompute would just rewrite identical data. An empty plan is
            // the same statement reached per slot rather than per frame.
            if bone_count > 0
                && !skip_skin_gpu_refresh
                && !palette_plan.is_empty()
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
                // SAFETY: `cmd` is recording (begin_command_buffer succeeded above); the bone-world / bind-inverse / palette buffers are live SSBOs for this frame and `bone_count > 0`. The COMPUTE_SHADER_WRITE -> SHADER_READ buffer barrier afterward sequences the palette write before its compute, vertex and fragment consumers; no concurrent recording of this buffer.
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
                        &palette_plan,
                    );
                    // COMPUTE_SHADER_WRITE → SHADER_READ barrier on the
                    // palette buffer covers all downstream consumers:
                    // `skin_vertices.comp` (compute read in this same
                    // command buffer below), `triangle.vert` and the shared
                    // secondary-hit frame (fragment read during raster).
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
                            | vk::PipelineStageFlags::VERTEX_SHADER
                            | vk::PipelineStageFlags::FRAGMENT_SHADER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[palette_barrier],
                        &[],
                    );
                }
            }
            self.scratch.palette_plan_scratch = palette_plan;
        }

        if skin_palette_timer_started {
            if let Some(ref mut timers) = self.gpu_timers {
                timers.cmd_skin_palette_end(&self.device, cmd, frame);
            }
        }

        self.record_skinned_blas_refit(cmd, frame, draw_commands, pose_dirty);

        // #4179 / CONC-D1-02 — publish BLAS writes to the TLAS build's
        // reads unconditionally, at frame scope.
        //
        // `build_tlas` reads every referenced BLAS, static ones included.
        // The only AS barrier that used to precede it lives inside
        // `record_skinned_blas_refit`, nested behind `!dispatches
        // .is_empty()` (plus a live `skin_compute`, `accel_manager` and
        // bone buffer). A frame with no skinned dispatches — an actor-free
        // interior, a headless bench, or any early return — reached
        // `build_tlas` with no AS dependency at all, while the static BLAS
        // it traverses were written by a *different* submission:
        // `step_streaming`'s `build_blas_batched`, or
        // `restore_missing_static_blas_for_draws`, which since #4180 also
        // runs as a between-frames step rather than inside the render driver.
        //
        // Same cross-submission rule this crate already applies to the
        // shared AS scratch (#983 / #1140 / #1300, and #4177 for the static
        // build path): the host fence-wait `submit_one_time` performs is a
        // host-side dependency only, so a device-side edge is still
        // required. The consequence if the strict reading holds is a TLAS
        // traversing BLAS whose builds are not yet visible — missing or
        // corrupt RT shadows/reflections/GI on freshly-streamed meshes,
        // self-healing on the next frame that happens to have a skinned
        // actor, and never in a cell that has none.
        //
        // Purely additive: the refit site keeps its own emit rather than
        // having it moved here. That leaves two back-to-back identical
        // barriers on skinned frames, which is a no-op for the driver, and
        // it avoids disturbing the `cmd_blas_refit_end` GPU-timer bracket
        // that barrier sits inside (#1194).
        if self.accel_manager.is_some() {
            // SAFETY: `cmd` is recording. A memory barrier records no
            // resource access of its own; it only declares the dependency
            // the `build_tlas` below relies on.
            unsafe {
                memory_barrier(
                    &self.device,
                    cmd,
                    vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                    vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
                    vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                    vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
                );
            }
        }

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
                        instance_map,
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
                    // consumers; post-#2673 a failed build deliberately
                    // keeps the previous AS alive, so `tlas_handle` is still
                    // `Some`. #4779 routes the compute tracers (volumetrics
                    // inject, ground-cover scatter) through
                    // `ray_query_tlas`, which withholds that stale handle,
                    // but the barrier stays unconditional: any COMPUTE
                    // consumer that does read the AS this frame must see the
                    // skinned refit writes.
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
                // to the compute shader before dispatch. Defense-in-depth
                // rather than a spec requirement (#4182): host writes
                // flushed before `queue_submit` are already visible
                // (Vulkan 1.3 §7.9), and these are mapped writes made
                // earlier in `draw_frame` — the barrier guards a future
                // non-coherent memory type or a post-recording host write.
                // Instance data is NOT
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

        // EXAL ground cover (#4054). Outside the render pass and before it:
        // the scatter writes the blade buffer and the indirect draw list the
        // geometry pass then consumes.
        // The TLAS this frame's build just published (above). The scatter
        // traces it to keep ground cover off placed geometry — roads,
        // flagstones, rock bases — so it must read this slot's structure,
        // not a handle resolved before the build could resize it, and never
        // the stale one a failed build leaves behind (#4779).
        let tlas = self.ray_query_tlas(frame);
        if let Some(ref mut gc) = self.groundcover {
            gc.record_scatter(&self.device, cmd, frame, tlas, self.gpu_timers.as_mut());
        }

        self.record_groundcover_bench(cmd, frame);
    }

    /// This frame's TLAS for a compute pass that ray-queries it
    /// unconditionally, or `None` when this frame's `build_tlas` failed.
    ///
    /// #4779 / CONC-D1-2026-09-23-01 — post-#2673 a failed build keeps the
    /// slot's previous AS alive, so `tlas_handle(frame)` stays `Some`. That
    /// stale TLAS still references BLAS which eviction only protects for the
    /// *current* frame's draws and `deferred_destroy` frees two frames later,
    /// so tracing it can dereference a freed BLAS (GPU page fault / device
    /// loss). Fragment consumers are covered by `rt_flag = 0` and
    /// `caustic_splat.comp` by its `sceneFlags.x` early-out; the volumetrics
    /// inject and the ground-cover scatter trace without either gate, so they
    /// take their handle from here and skip the frame on `None` instead.
    pub(super) fn ray_query_tlas(&self, frame: usize) -> Option<vk::AccelerationStructureKHR> {
        self.accel_manager
            .as_ref()
            .and_then(|accel| accel.tlas_handle(frame))
            .filter(|_| self.tlas_build_succeeded_last_frame)
    }

    /// EXAL ground-cover §11.1 terrain-attribute sampling bench (#4052).
    ///
    /// Placed after the cluster-cull dispatch and before the main render
    /// pass, for the same reason cluster cull sits there: it is compute (or,
    /// for the raster variants, a self-contained render pass into its own
    /// throwaway target) that reads buffers already uploaded this frame and
    /// writes nothing anyone else reads.
    ///
    /// Returns immediately on every frame of a normal run — `groundcover_bench`
    /// is `None` unless `--bench-groundcover-sampling` created it.
    fn record_groundcover_bench(&mut self, cmd: vk::CommandBuffer, frame: usize) {
        if self.groundcover_bench.is_none() {
            return;
        }
        // The global vertex SSBO is path A's whole subject. It does not exist
        // until the first mesh upload, and `MeshRegistry` replaces the handle
        // when it grows — the bench rewrites its descriptors from whatever it
        // is handed here rather than caching it.
        let Some(vertex_buffer) = self
            .mesh_registry
            .global_vertex_buffer
            .as_ref()
            .map(|b| b.buffer)
        else {
            return;
        };
        // Resolve each published terrain cell against the live registry.
        // A cell whose mesh has been evicted is dropped rather than sampled
        // at a stale offset, which would read another mesh's vertices as
        // terrain and quietly change the numbers.
        let cells: Vec<super::super::groundcover_bench::BenchCellInput> = self
            .groundcover_bench_cells
            .iter()
            .filter_map(|cell| {
                self.mesh_registry.get(cell.mesh_id).map(|mesh| {
                    super::super::groundcover_bench::BenchCellInput {
                        origin_xz: cell.origin_xz,
                        vertex_offset: mesh.global_vertex_offset,
                    }
                })
            })
            .collect();

        let Some(bench) = self.groundcover_bench.as_mut() else {
            return;
        };
        bench.record(
            &self.device,
            cmd,
            frame,
            vertex_buffer,
            &cells,
            self.gpu_timers.as_mut(),
        );
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
        let latch_pos = src.find("self.bind_inverse_upload_failed = true;").expect(
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

/// Regression for #4049 (REN-2026-09-06-D9-03). The #3569 requeue retries a
/// failed `bind_inverses` upload every frame until it succeeds, so an
/// un-gated `warn!` in that arm floods the log once per frame for as long as
/// the failure persists — unlike every sibling failure path in this
/// subsystem (`SkinSlotPool::overflow_warned`, `failed_skin_slots` /
/// `failed_skin_blas`), all of which log once and count silently after.
#[cfg(test)]
mod bind_inverse_upload_failure_is_rate_limited_tests {
    #[test]
    fn upload_pending_bind_inverses_failure_arm_is_once_gated_and_counted() {
        let src = include_str!("dispatch_skin_and_cluster.rs");
        // Scoped to the production portion — an unscoped search would match
        // this very module's own literals, same hazard the sibling latch
        // test above documents.
        let module_start = src
            .find("mod bind_inverse_upload_failure_is_rate_limited_tests")
            .expect("this test module must still exist under its own name");
        let src = &src[..module_start];

        assert!(
            src.contains("if !self.bind_inverse_upload_warned {"),
            "the failure arm must gate its warn!() behind a one-shot latch \
             (bind_inverse_upload_warned), matching SkinSlotPool::overflow_warned \
             and the failed_skin_slots/failed_skin_blas convention — an \
             unconditional warn!() here floods the log for a persistent \
             failure (#4049)"
        );
        assert!(
            src.contains("bind_inverse_upload_failure_count.saturating_add(1)"),
            "the failure arm must still increment a cumulative counter even \
             after the log goes silent, or the magnitude of a persistent \
             failure becomes unobservable (#4049)"
        );
        // The counter must be incremented UNCONDITIONALLY (every failure),
        // not just inside the `if !warned` branch — a source-scan proxy for
        // that is: the increment line must appear textually BEFORE the
        // `if !self.bind_inverse_upload_warned {` gate that guards the warn.
        let inc_pos = src
            .find("self.bind_inverse_upload_failure_count.saturating_add(1);")
            .expect("increment must still exist under this exact spelling");
        let gate_pos = src
            .find("if !self.bind_inverse_upload_warned {")
            .expect("the one-shot gate must still exist under this exact spelling");
        assert!(
            inc_pos < gate_pos,
            "the failure counter must increment before (i.e. outside) the \
             one-shot warn gate, so every failure is counted, not just the \
             first (#4049)"
        );
    }
}

#[cfg(test)]
mod pre_tlas_acceleration_barrier_tests {
    /// Regression: #4179 / CONC-D1-02 — the AS_WRITE → AS_READ barrier
    /// publishing BLAS writes to `build_tlas`'s reads must be emitted at
    /// frame scope, not nested inside the skinned path's control flow.
    ///
    /// Before this, the only such barrier lived in
    /// `skinned_blas_refit.rs`, behind `!dispatches.is_empty()` plus a
    /// live `skin_compute`/`accel_manager`/bone buffer. Any frame without
    /// skinned dispatches — actor-free interior, headless bench, early
    /// return — reached `build_tlas` with no AS dependency while the
    /// static BLAS it traverses had been written by a *different*
    /// submission.
    ///
    /// A source-shape pin for the same reason as #4177's: no Vulkan
    /// device in unit tests, and a cross-submission dependency is not
    /// something a later frame's output can be asserted against.
    #[test]
    fn build_tlas_is_preceded_by_an_unconditional_as_write_to_as_read_barrier() {
        let src = include_str!("dispatch_skin_and_cluster.rs");
        // Scoped to the production portion — an unscoped search would match
        // this very module's own literals, same hazard the sibling latch
        // tests above document.
        let module_start = src
            .find("mod pre_tlas_acceleration_barrier_tests")
            .expect("this test module must still exist under its own name");
        let src = &src[..module_start];

        let barrier_at = src.find("ACCELERATION_STRUCTURE_WRITE_KHR").expect(
            "an AS_WRITE → AS_READ barrier must precede build_tlas at frame scope \
                 (#4179); leaving it only in the skinned refit path means a frame with \
                 no skinned actors builds the TLAS with no dependency on the static \
                 BLAS writes it traverses",
        );
        let build_at = src
            .find("accel.build_tlas(")
            .expect("the TLAS build call must still exist under this spelling");
        assert!(
            barrier_at < build_at,
            "the AS barrier must be recorded before build_tlas, not after \
             (barrier {barrier_at}, build {build_at})"
        );

        // It must NOT be nested behind the skinned-dispatch condition —
        // that nesting is the entire defect. The only gate allowed is the
        // `accel_manager` presence check, which is also what `build_tlas`
        // itself requires.
        let gate_at = src.find("if self.accel_manager.is_some() {").expect(
            "the barrier must be gated only on accel_manager presence (#4179) — \
                 any dispatch-list or skin-compute condition reintroduces the \
                 actor-free-frame hole",
        );
        assert!(
            gate_at < barrier_at && gate_at > src.find("self.record_skinned_blas_refit(").unwrap(),
            "the frame-scope barrier belongs after the refit call and before the \
             TLAS build, outside the skinned path"
        );
    }
}

#[cfg(test)]
mod palette_dirty_plan_tests {
    /// #4204 — the palette pass dispatches the dirty plan, not the dense
    /// high-water range, and it re-arms slots whose bind-inverse landed late.
    ///
    /// The first half is the perf fix. The second is what keeps it correct:
    /// `bind_inverses` first-sight uploads are capped per frame, so a slot's
    /// bind-inverse can arrive after its pose was already copied and its
    /// palette computed. The dense dispatch healed that by recomputing
    /// everything every frame; a narrowed one leaves that slot wrong in one or
    /// both frame-in-flight palettes unless the slot is marked dirty again.
    /// Dropping the re-arm would pass every other test and render a skinned
    /// mesh against a stale or zero bind pose — collapsed or exploded
    /// geometry, and only on frames where the upload cap bit.
    #[test]
    fn palette_dispatch_uses_the_dirty_plan_and_rearms_late_bind_inverses() {
        let src = include_str!("dispatch_skin_and_cluster.rs");
        let production = &src[..src.find("#[cfg(test)]").expect("test modules")];
        assert!(
            production.contains("plan_palette_dispatch("),
            "the palette dispatch must be planned from the dirty ranges (#4204)"
        );
        assert!(
            production.contains("&palette_plan,"),
            "the palette dispatch must receive the plan, not a dense bone count (#4204)"
        );
        assert!(
            !production.contains("SkinPalettePushConstants { bone_count }"),
            "a dense `bone_count` push constant is the pre-#4204 full-range dispatch"
        );
        let upload = production
            .find("record_pending_bind_inverse_copies(")
            .expect("bind-inverse upload site");
        let rearm = production
            .find("mark_palette_slots_dirty(&pending_slots)")
            .expect("late bind-inverse slots must be re-armed for both frame slots (#4204)");
        assert!(
            upload < rearm,
            "the re-arm belongs with the upload it compensates for"
        );
        // Whitespace-insensitive: rustfmt reflows this call depending on line
        // length, and the pin is about the data flow, not the layout.
        let compact: String = production.split_whitespace().collect();
        assert!(
            compact.contains(".chain(pending_slots.iter().map("),
            "this frame's palette plan must include the slots whose bind-inverse just landed"
        );
    }
}

#[cfg(test)]
mod stale_tlas_compute_gate_tests {
    /// Regression: #4779 / CONC-D1-2026-09-23-01 — after a failed
    /// `build_tlas`, the slot's previous AS stays alive (#2673), so
    /// `tlas_handle(frame)` is still `Some` while the BLAS it references can
    /// already be in `deferred_destroy`. The compute passes that trace
    /// without an `rt_flag` / `sceneFlags.x` gate — the volumetrics inject
    /// and the ground-cover scatter — must take their handle from
    /// `ray_query_tlas`, which withholds it on a failed-build frame.
    ///
    /// A source-shape pin for the same reason as the barrier test above: no
    /// Vulkan device in unit tests, and the hazard is a freed-BLAS read that
    /// no CPU-side assertion can observe.
    #[test]
    fn compute_ray_query_passes_take_the_build_gated_tlas() {
        let this = include_str!("dispatch_skin_and_cluster.rs");
        let this = &this[..this
            .find("mod stale_tlas_compute_gate_tests")
            .expect("this test module must still exist under its own name")];

        let helper_at = this
            .find("pub(super) fn ray_query_tlas(")
            .expect("the build-gated TLAS accessor must exist (#4779)");
        let helper = &this[helper_at..];
        let helper = &helper[..helper.find("\n    }\n").expect("helper body end")];
        assert!(
            helper.contains(".filter(|_| self.tlas_build_succeeded_last_frame)"),
            "ray_query_tlas must withhold the handle when this frame's build failed"
        );

        let scatter_at = this
            .find("gc.record_scatter(")
            .expect("the ground-cover scatter call must still exist under this spelling");
        let scatter_tlas_at = this[..scatter_at]
            .rfind("let tlas = self.ray_query_tlas(frame);")
            .expect("the ground-cover scatter must trace the build-gated TLAS (#4779)");
        assert!(
            !this[scatter_tlas_at..scatter_at].contains("tlas_handle("),
            "no raw tlas_handle between the gated resolve and the scatter call"
        );

        let post = include_str!("post_passes.rs");
        let vol_at = post
            .find("fn record_volumetrics_pass(")
            .expect("the volumetrics recorder must still exist under this name");
        let vol = &post[vol_at..];
        let vol = &vol[..vol[1..].find("\n    fn ").map_or(vol.len(), |i| i + 1)];
        assert!(
            vol.contains("let vol_tlas = self.ray_query_tlas(frame);"),
            "the volumetrics inject must trace the build-gated TLAS (#4779)"
        );
        assert!(
            !vol.contains("tlas_handle("),
            "the volumetrics recorder must not resolve the raw, possibly stale TLAS"
        );
    }
}
