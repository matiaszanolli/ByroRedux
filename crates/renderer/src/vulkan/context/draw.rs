//! Frame recording and submission — the per-frame hot path.

use super::super::pipeline::{gamebryo_to_vk_compare_op, PipelineKey};
use super::super::scene_buffer::{
    self, GpuInstance, GpuTerrainTile, INSTANCE_FLAG_ALPHA_BLEND, INSTANCE_FLAG_CAUSTIC_SOURCE,
    INSTANCE_FLAG_NON_UNIFORM_SCALE, INSTANCE_FLAG_TERRAIN_SPLAT, INSTANCE_TERRAIN_TILE_MASK,
    INSTANCE_TERRAIN_TILE_SHIFT,
};
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::{DrawCommand, FrameTimings, SkyParams, VulkanContext};
use anyhow::{Context, Result};
use ash::vk;
use byroredux_core::ecs::storage::EntityId;
use std::time::Instant;

/// Halton low-discrepancy sequence value at `index` (1-indexed) for `base`.
/// Returns a value in [0, 1).
fn halton(mut index: u32, base: u32) -> f32 {
    let mut result = 0.0_f32;
    let mut f = 1.0 / base as f32;
    while index > 0 {
        result += f * (index % base) as f32;
        index /= base;
        f /= base as f32;
    }
    result
}

/// A batch of instances sharing the same mesh + pipeline state.
/// Drawn with a single `cmd_draw_indexed` call.
///
/// `pub(super)` so the enclosing `VulkanContext` can hold a reusable
/// `Vec<DrawBatch>` scratch buffer as a field and amortize allocations
/// across frames. See issue #243.
pub(super) struct DrawBatch {
    pub mesh_handle: u32,
    /// Pipeline selector. `Opaque` uses one of two prebuilt pipelines;
    /// `Blended { src, dst, two_sided }` resolves through the lazy
    /// blend pipeline cache on `VulkanContext`. See #392.
    pub pipeline_key: PipelineKey,
    pub is_decal: bool,
    pub first_instance: u32,
    pub instance_count: u32,
    pub index_count: u32,
    /// Offset into the global index buffer (in indices). Used with the
    /// global geometry SSBO as `first_index` in `cmd_draw_indexed`. #294.
    pub global_index_offset: u32,
    /// Offset into the global vertex buffer (in vertices). Used with the
    /// global geometry SSBO as `vertex_offset` in `cmd_draw_indexed`. #294.
    pub global_vertex_offset: i32,
    /// `NiZBufferProperty.z_test` — fed to `vkCmdSetDepthTestEnable`
    /// before the batch (extended dynamic state, Vulkan 1.3 core).
    /// Batched into the merge key so consecutive draws sharing depth
    /// state pay zero state-change cost. See #398.
    pub z_test: bool,
    /// `NiZBufferProperty.z_write` — fed to `vkCmdSetDepthWriteEnable`.
    pub z_write: bool,
    /// `NiZBufferProperty.z_function` — fed to `vkCmdSetDepthCompareOp`
    /// (Gamebryo `TestFunction` enum mapped to `vk::CompareOp`).
    pub z_function: u8,
}

impl VulkanContext {
    /// Record and submit a frame.
    ///
    /// `view_proj`: combined view-projection matrix as column-major [f32; 16].
    /// `draw_commands`: per-object (mesh_handle, model_matrix) pairs.
    pub fn draw_frame(
        &mut self,
        clear_color: [f32; 4],
        view_proj: &[f32; 16],
        draw_commands: &[DrawCommand],
        lights: &[scene_buffer::GpuLight],
        bone_palette: &[[[f32; 4]; 4]],
        camera_pos: [f32; 3],
        ambient_color: [f32; 3],
        fog_color: [f32; 3],
        fog_near: f32,
        fog_far: f32,
        ui_texture_handle: Option<u32>,
        sky_params: &SkyParams,
        timings: Option<&mut FrameTimings>,
    ) -> Result<bool> {
        let frame = self.current_frame;
        // Use a local to avoid borrow complexity; copy out at end.
        let mut t = FrameTimings::default();

        // Wait for this frame-in-flight slot AND the previous slot to be
        // available. SVGF's temporal pass reads the previous slot's G-buffer
        // images (mesh_id, motion, raw_indirect) — without waiting on the
        // other slot's fence, a read-after-write hazard exists when the GPU
        // hasn't finished the other slot's render pass. See #282.
        //
        // Cost: zero in practice — the GPU is rarely more than 1 frame
        // behind the CPU, so the other fence is almost always signaled.
        let fence_t0 = Instant::now();
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

        // If a screenshot was captured last frame, the GPU is done — read it back.
        self.screenshot_finish_readback();

        // Acquire next swapchain image.
        let (image_index, suboptimal) = unsafe {
            match self.swapchain_state.swapchain_loader.acquire_next_image(
                self.swapchain_state.swapchain,
                u64::MAX,
                self.frame_sync.image_available[frame],
                vk::Fence::null(),
            ) {
                Ok((idx, suboptimal)) => (idx, suboptimal),
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(true),
                Err(e) => anyhow::bail!("acquire_next_image: {:?}", e),
            }
        };

        let img = image_index as usize;

        // If this swapchain image is still in use by a different frame, wait.
        let image_fence = self.frame_sync.images_in_flight[img];
        if image_fence != vk::Fence::null() && image_fence != self.frame_sync.in_flight[frame] {
            unsafe {
                self.device
                    .wait_for_fences(&[image_fence], true, u64::MAX)
                    .context("wait for image fence")?;
            }
        }
        self.frame_sync.images_in_flight[img] = self.frame_sync.in_flight[frame];

        unsafe {
            self.device
                .reset_fences(&[self.frame_sync.in_flight[frame]])
                .context("reset_fences")?;
        }

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

        // Record command buffer. Indexed by frame-in-flight (not swapchain
        // image) so the fence and command buffer share the same slot — #259.
        // Safe because in_flight[frame] was just waited on, guaranteeing
        // the GPU has finished with this cmd buffer's previous recording.
        let cmd = self.command_buffers[frame];
        unsafe {
            self.device
                .reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
                .context("reset_command_buffer")?;
        }

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .begin_command_buffer(cmd, &begin_info)
                .context("begin_command_buffer")?;
        }

        // 6 color attachments + depth. Order must match the render pass:
        //   0 HDR, 1 normal, 2 motion, 3 mesh_id, 4 raw_indirect, 5 albedo, 6 depth.
        let zero_f = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 0.0],
            },
        };
        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: clear_color,
                },
            },
            zero_f, // normal
            zero_f, // motion
            vk::ClearValue {
                // Mesh ID: 0 reserved for background (shader writes id + 1).
                color: vk::ClearColorValue {
                    uint32: [0, 0, 0, 0],
                },
            },
            zero_f, // raw_indirect (background: no light)
            zero_f, // albedo (background: no color)
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

        // Main framebuffer is now per-frame-in-flight (not per-swapchain-image).
        // Each frame slot has its own HDR color image, so no read-after-write
        // hazard across overlapping frames.
        let render_pass_begin = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[frame])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.swapchain_state.extent,
            })
            .clear_values(&clear_values);

        // Pre-compute the shared `draw_idx → ssbo_idx` map once so the
        // TLAS `instance_custom_index` values stay in lockstep with the
        // compacted SSBO positions regardless of which filter rejects a
        // draw_cmd. Before #419 the TLAS path used the raw enumerate
        // index while the SSBO builder used `gpu_instances.len()` —
        // identical only while `mesh_registry.get()` never returned None
        // for a submitted command. A single evicted mesh would shift
        // every subsequent SSBO entry by one while TLAS custom indices
        // stayed put, producing silently-wrong material/transform reads
        // on every RT hit downstream (shadows / reflections / GI /
        // caustics / primary-hit fallback in `triangle.frag`). See
        // `crates/renderer/src/vulkan/acceleration.rs::build_tlas` and
        // the SSBO builder below — both must honour this map.
        let tlas_t0 = Instant::now();
        let instance_map: Vec<Option<u32>> =
            super::super::acceleration::build_instance_map(draw_commands.len(), |i| {
                self.mesh_registry
                    .get(draw_commands[i].mesh_handle)
                    .is_some()
            });
        // M29 Phase 2: TLAS build moved to AFTER bone upload + skin
        // chain (compute dispatch + BLAS refit) so the TLAS sees this
        // frame's skinned poses with zero lag. instance_map computed
        // here stays valid through the move — it's a pure function of
        // draw_commands + mesh_registry state.

        // Upload scene data (lights + camera) BEFORE the render pass begins.
        self.scene_buffers
            .upload_lights(&self.device, frame, lights)
            .unwrap_or_else(|e| log::warn!("Failed to upload lights: {e}"));
        let rt_flag =
            if self.device_caps.ray_query_supported && self.scene_buffers.tlas_written[frame] {
                1.0
            } else {
                0.0
            };

        // TAA sub-pixel jitter via Halton(2,3) sequence. Each frame shifts
        // the projection by a different sub-pixel offset in NDC so that
        // temporal blending over ~8 frames reconstructs a super-sampled
        // result. The offset is applied in the vertex shader AFTER motion
        // vector computation so reprojection is jitter-free.
        let (jx, jy) = if self.taa.is_some() {
            let idx = (self.frame_counter % 8) as u32 + 1; // 1-indexed
            let hx = halton(idx, 2);
            let hy = halton(idx, 3);
            // Map [0,1] → [-0.5, 0.5] pixels, then to NDC.
            let w = self.swapchain_state.extent.width as f32;
            let h = self.swapchain_state.extent.height as f32;
            ((hx - 0.5) * 2.0 / w, (hy - 0.5) * 2.0 / h)
        } else {
            (0.0, 0.0)
        };

        let vp = view_proj;
        let pvp = &self.prev_view_proj;
        // Precompute inverse(viewProj) once on the CPU so shaders
        // (cluster culling, SSAO) can read it directly from the UBO
        // instead of computing a ~100 ALU-op matrix inverse per invocation.
        let vp_mat = byroredux_core::math::Mat4::from_cols_array(vp);
        let inv_vp = vp_mat.inverse();
        let inv_vp_cols = inv_vp.to_cols_array();
        let inv_vp_arr = [
            [
                inv_vp_cols[0],
                inv_vp_cols[1],
                inv_vp_cols[2],
                inv_vp_cols[3],
            ],
            [
                inv_vp_cols[4],
                inv_vp_cols[5],
                inv_vp_cols[6],
                inv_vp_cols[7],
            ],
            [
                inv_vp_cols[8],
                inv_vp_cols[9],
                inv_vp_cols[10],
                inv_vp_cols[11],
            ],
            [
                inv_vp_cols[12],
                inv_vp_cols[13],
                inv_vp_cols[14],
                inv_vp_cols[15],
            ],
        ];
        let camera = scene_buffer::GpuCamera {
            view_proj: [
                [vp[0], vp[1], vp[2], vp[3]],
                [vp[4], vp[5], vp[6], vp[7]],
                [vp[8], vp[9], vp[10], vp[11]],
                [vp[12], vp[13], vp[14], vp[15]],
            ],
            prev_view_proj: [
                [pvp[0], pvp[1], pvp[2], pvp[3]],
                [pvp[4], pvp[5], pvp[6], pvp[7]],
                [pvp[8], pvp[9], pvp[10], pvp[11]],
                [pvp[12], pvp[13], pvp[14], pvp[15]],
            ],
            inv_view_proj: inv_vp_arr,
            // w = monotonic frame counter for temporal jitter seed in shadow rays.
            position: [
                camera_pos[0],
                camera_pos[1],
                camera_pos[2],
                self.frame_counter as f32,
            ],
            flags: [
                rt_flag,
                ambient_color[0],
                ambient_color[1],
                ambient_color[2],
            ],
            screen: [
                self.swapchain_state.extent.width as f32,
                self.swapchain_state.extent.height as f32,
                fog_near,
                fog_far,
            ],
            fog: [
                fog_color[0],
                fog_color[1],
                fog_color[2],
                if fog_far > fog_near { 1.0 } else { 0.0 }, // fog enabled flag
            ],
            // jitter[2] carries the debug-bypass bitmask for the
            // fragment shader (see `parse_render_debug_flags_env` and
            // `triangle.frag`'s `floatBitsToUint(jitter.z)` branches).
            // Zero-bits → free no-op; non-zero → debug paths active.
            jitter: [jx, jy, f32::from_bits(self.render_debug_flags), 0.0],
        };
        self.scene_buffers
            .upload_camera(&self.device, frame, &camera)
            .unwrap_or_else(|e| log::warn!("Failed to upload camera: {e}"));
        // Store this frame's viewProj as next frame's "previous" for motion vectors.
        self.prev_view_proj = *vp;
        if !bone_palette.is_empty() {
            self.scene_buffers
                .upload_bones(&self.device, frame, bone_palette)
                .unwrap_or_else(|e| log::warn!("Failed to upload bone palette: {e}"));
        }

        // ── M29 Phase 2: GPU pre-skin + per-skinned-entity BLAS refit ─
        //
        // Runs AFTER bone palette upload (compute reads it) and BEFORE
        // TLAS build (TLAS picks up the freshly-refit BLAS, zero-lag
        // RT). For each draw with `bone_offset != 0`:
        //   - First sight: synchronous compute prime + synchronous BLAS
        //     BUILD (with `ALLOW_UPDATE`) via two one-time command
        //     buffers. Brief stall on the very first frame an NPC
        //     appears; M40 cell streaming will eventually preload.
        //   - Steady state: dispatch compute into the frame cmd buffer,
        //     barrier (COMPUTE_WRITE → AS_BUILD_INPUT_READ), then
        //     refit the per-entity BLAS (UPDATE mode, src == dst).
        //     Final AS_BUILD_WRITE → AS_BUILD_INPUT_READ barrier hands
        //     fresh BLAS to TLAS below.
        //
        // Skips entirely when `skin_compute` / `accel_manager` are None
        // (no RT) or no draws are skinned.
        let skin_t0 = Instant::now();
        if let (Some(ref skin_pipeline), Some(ref mut accel)) =
            (self.skin_compute.as_ref(), self.accel_manager.as_mut())
        {
            if let Some(ref alloc) = self.allocator {
                // Sub-block: limit borrow scope on `mesh_registry` /
                // `scene_buffers`. Skin-chain reads are immutable
                // through this block.
                let global_vert_buf = self
                    .mesh_registry
                    .global_vertex_buffer
                    .as_ref()
                    .map(|b| (b.buffer, b.size));
                let bone_buffer = self
                    .scene_buffers
                    .bone_buffers()
                    .get(frame)
                    .map(|b| b.buffer);
                let bone_buffer_size = self.scene_buffers.bone_buffer_size();

                if let (Some((input_buffer, input_size)), Some(bone_buf)) =
                    (global_vert_buf, bone_buffer)
                {
                    // Walk draw_commands once — collect unique skinned
                    // entities + their per-mesh metadata. Multiple
                    // draws of the same entity (rare; instanced rendering
                    // would hit this) coalesce on entity_id.
                    use std::collections::HashSet;
                    let mut seen: HashSet<EntityId> = HashSet::new();
                    let mut dispatches: Vec<(
                        EntityId,
                        super::super::skin_compute::SkinPushConstants,
                        vk::Buffer,
                        u32,
                        u32,
                    )> = Vec::new();
                    for dc in draw_commands.iter() {
                        if dc.bone_offset == 0 {
                            continue;
                        }
                        if !seen.insert(dc.entity_id) {
                            continue;
                        }
                        let Some(mesh) = self.mesh_registry.get(dc.mesh_handle) else {
                            continue;
                        };
                        let push = super::super::skin_compute::SkinPushConstants {
                            vertex_offset: mesh.global_vertex_offset,
                            vertex_count: mesh.vertex_count,
                            bone_offset: dc.bone_offset,
                        };
                        dispatches.push((
                            dc.entity_id,
                            push,
                            mesh.index_buffer.buffer,
                            mesh.index_count,
                            mesh.vertex_count,
                        ));
                    }

                    // First-sight setup: for each entity that doesn't
                    // yet have a SkinSlot OR a skinned BLAS, perform
                    // sync compute prime + sync BLAS BUILD.
                    for &(entity_id, push, idx_buffer, idx_count, vertex_count) in &dispatches {
                        let needs_slot = !self.skin_slots.contains_key(&entity_id);
                        let needs_blas = accel.skinned_blas_entry(entity_id).is_none();
                        if !needs_slot && !needs_blas {
                            continue;
                        }
                        // Create slot if missing.
                        if needs_slot {
                            match skin_pipeline.create_slot(&self.device, alloc, vertex_count) {
                                Ok(slot) => {
                                    self.skin_slots.insert(entity_id, slot);
                                }
                                Err(e) => {
                                    log::warn!(
                                        "skin_compute create_slot failed for entity {entity_id}: {e} \
                                         — skinned RT shadow disabled for this entity (raster unaffected)"
                                    );
                                    continue;
                                }
                            }
                        }
                        let Some(slot) = self.skin_slots.get(&entity_id) else {
                            continue;
                        };
                        // Sync compute prime — write current pose into
                        // the slot's output buffer via a one-time
                        // command buffer + fence wait. This is the only
                        // path before the slot's first BLAS build, so
                        // BUILD has valid vertex data to read.
                        let prime_result =
                            super::super::texture::with_one_time_commands_reuse_fence(
                                &self.device,
                                &self.graphics_queue,
                                self.transfer_pool,
                                &self.transfer_fence,
                                |prime_cmd| unsafe {
                                    skin_pipeline.dispatch(
                                        &self.device,
                                        prime_cmd,
                                        slot,
                                        frame,
                                        input_buffer,
                                        input_size,
                                        bone_buf,
                                        bone_buffer_size,
                                        push,
                                    );
                                    Ok(())
                                },
                            );
                        if let Err(e) = prime_result {
                            log::warn!(
                                "skin_compute first-sight prime failed for entity {entity_id}: {e}"
                            );
                            continue;
                        }
                        // Sync BLAS BUILD against the just-primed
                        // output buffer.
                        if let Err(e) = accel.build_skinned_blas(
                            &self.device,
                            alloc,
                            &self.graphics_queue,
                            self.transfer_pool,
                            Some(&self.transfer_fence),
                            entity_id,
                            slot.output_buffer.buffer,
                            vertex_count,
                            idx_buffer,
                            idx_count,
                        ) {
                            log::warn!(
                                "skin_compute first-sight BLAS build failed for entity {entity_id}: {e}"
                            );
                        }
                    }

                    // Per-frame steady-state: dispatch compute for
                    // every registered skinned slot (refresh output
                    // buffer with current pose), then barrier, then
                    // refit BLAS.
                    if !dispatches.is_empty() {
                        unsafe {
                            for &(entity_id, push, _, _, _) in &dispatches {
                                let Some(slot) = self.skin_slots.get_mut(&entity_id) else {
                                    continue;
                                };
                                // #643 / MEM-2-1 — bump LRU before the
                                // dispatch so the eviction sweep below
                                // sees this entity as "active this
                                // frame" and won't drop it. Mirrors
                                // the BLAS-side `last_used_frame` bump
                                // in `acceleration.rs::build_tlas`.
                                slot.last_used_frame = self.frame_counter as u64;
                                skin_pipeline.dispatch(
                                    &self.device,
                                    cmd,
                                    slot,
                                    frame,
                                    input_buffer,
                                    input_size,
                                    bone_buf,
                                    bone_buffer_size,
                                    push,
                                );
                            }
                            // Compute writes (skinned vertex output
                            // buffers) → AS build input reads.
                            let compute_to_blas = vk::MemoryBarrier::default()
                                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                                .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR);
                            self.device.cmd_pipeline_barrier(
                                cmd,
                                vk::PipelineStageFlags::COMPUTE_SHADER,
                                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                                vk::DependencyFlags::empty(),
                                &[compute_to_blas],
                                &[],
                                &[],
                            );
                            // Each `refit_skinned_blas` call shares
                            // `blas_scratch_buffer` with every other
                            // refit in this loop AND with any sync
                            // BUILD that ran earlier this frame
                            // (`build_skinned_blas` first-sight,
                            // `build_blas_batched` cell-load) — Vulkan
                            // spec on `scratchData` requires an
                            // AS_WRITE → AS_WRITE serialise barrier
                            // between every pair of AS-builds that
                            // share scratch, regardless of submission
                            // boundary (the host fence-wait is a
                            // host-side dependency only and does NOT
                            // establish device-side memory ordering
                            // for the next submission). Emitting the
                            // barrier before EVERY iteration covers
                            // both refit→refit (#642) and the
                            // cross-submission BUILD→first-refit case
                            // (#644 / MEM-2-2). The redundant
                            // first-iteration barrier is essentially
                            // free when the cmd has no prior AS-build
                            // — same-stage AS_WRITE↔AS_WRITE on a
                            // queue with no in-flight build work.
                            for &(entity_id, _, idx_buffer, idx_count, vertex_count) in &dispatches
                            {
                                let Some(slot) = self.skin_slots.get(&entity_id) else {
                                    continue;
                                };
                                accel.record_scratch_serialize_barrier(&self.device, cmd);
                                if let Err(e) = accel.refit_skinned_blas(
                                    &self.device,
                                    cmd,
                                    entity_id,
                                    slot.output_buffer.buffer,
                                    vertex_count,
                                    idx_buffer,
                                    idx_count,
                                ) {
                                    log::warn!(
                                        "skin_compute BLAS refit failed for entity {entity_id}: {e}"
                                    );
                                    continue;
                                }
                            }
                            // BLAS refit writes → TLAS build reads.
                            let blas_to_tlas = vk::MemoryBarrier::default()
                                .src_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR)
                                .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR);
                            self.device.cmd_pipeline_barrier(
                                cmd,
                                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                                vk::DependencyFlags::empty(),
                                &[blas_to_tlas],
                                &[],
                                &[],
                            );
                        }
                    }

                    // #643 / MEM-2-1 — drop SkinSlots (and the matching
                    // skinned BLAS) for entities whose `last_used_frame`
                    // trails the current draw by more than
                    // `MAX_FRAMES_IN_FLIGHT` frames. Mirrors
                    // `evict_unused_blas`'s LRU pattern: the threshold
                    // guarantees no in-flight command buffer still
                    // references the descriptor sets / output buffer /
                    // BLAS, so synchronous destroy is safe — no
                    // deferred-destroy queue needed.
                    //
                    // Pre-fix the `skin_slots` HashMap and the
                    // `skinned_blas` map only ever had entries
                    // *inserted* (draw.rs first-sight loop) or *drained
                    // wholesale on Drop* (context/mod.rs). On long
                    // sessions that streamed through several
                    // worldspaces, every NPC ever rendered stayed
                    // resident; the FREE_DESCRIPTOR_SET pool would
                    // exhaust well before the player's exterior
                    // population caught up.
                    let min_idle = MAX_FRAMES_IN_FLIGHT as u64 + 1;
                    let now = self.frame_counter as u64;
                    let evictees: Vec<EntityId> = self
                        .skin_slots
                        .iter()
                        .filter_map(|(&eid, slot)| {
                            super::super::skin_compute::should_evict_skin_slot(
                                slot.last_used_frame,
                                now,
                                min_idle,
                            )
                            .then_some(eid)
                        })
                        .collect();
                    if !evictees.is_empty() {
                        log::debug!(
                            "skin_slots eviction: dropping {} idle SkinSlot(s) and matching skinned BLAS",
                            evictees.len()
                        );
                        for eid in evictees {
                            if let Some(slot) = self.skin_slots.remove(&eid) {
                                skin_pipeline.destroy_slot(&self.device, alloc, slot);
                            }
                            accel.drop_skinned_blas(eid);
                        }
                    }
                }
            }
        }
        let _skin_chain_ns = skin_t0.elapsed().as_nanos() as u64;

        // ── TLAS build (relocated from top of frame) ─────────────────
        // Picks up just-refit per-skinned-entity BLAS via the
        // `bone_offset != 0` override in `build_tlas`. Static draws
        // continue using the per-mesh `blas_entries` table.
        unsafe {
            if let Some(ref mut accel) = self.accel_manager {
                if let Some(alloc) = self.allocator.as_ref() {
                    if let Err(e) = accel.build_tlas(
                        &self.device,
                        alloc,
                        cmd,
                        draw_commands,
                        &instance_map,
                        frame,
                    ) {
                        log::warn!("TLAS build failed: {e}");
                    } else {
                        // Memory barrier: TLAS build → ray-query consumers
                        // (FRAGMENT_SHADER for main render pass +
                        // COMPUTE_SHADER for caustic_splat.comp). See
                        // #415 for the COMPUTE_SHADER widening.
                        let barrier = vk::MemoryBarrier::default()
                            .src_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR)
                            .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR);
                        self.device.cmd_pipeline_barrier(
                            cmd,
                            vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                            vk::PipelineStageFlags::FRAGMENT_SHADER
                                | vk::PipelineStageFlags::COMPUTE_SHADER,
                            vk::DependencyFlags::empty(),
                            &[barrier],
                            &[],
                            &[],
                        );
                        if let Some(tlas_handle) = accel.tlas_handle(frame) {
                            self.scene_buffers
                                .write_tlas(&self.device, frame, tlas_handle);
                        }
                        accel.evict_unused_blas(&self.device, alloc);
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
        unsafe {
            if let Some(ref cc) = self.cluster_cull {
                // Barrier: host writes to light/camera SSBOs must be visible
                // to the compute shader before dispatch. Required by Vulkan
                // spec even for HOST_COHERENT memory. Instance data is NOT
                // uploaded yet — it is built and uploaded after this dispatch.
                let host_barrier = vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::HOST_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::UNIFORM_READ);
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::HOST,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[host_barrier],
                    &[],
                    &[],
                );

                cc.dispatch(&self.device, cmd, frame);
                // Barrier: compute writes → fragment reads on cluster SSBOs.
                let barrier = vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ);
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[barrier],
                    &[],
                    &[],
                );
            }
        }

        // ── Build instance SSBO + draw batches ────────────────────────
        //
        // Each DrawCommand becomes one GpuInstance in the SSBO. Consecutive
        // commands with the same (pipeline_key, is_decal, mesh_handle) are
        // merged into a single instanced draw call.
        //
        // The two working vectors are held on `self` as scratch buffers
        // (`gpu_instances_scratch`, `batches_scratch`). `mem::take` moves
        // them out so the rest of draw_frame can continue borrowing other
        // fields of `self` without fighting the borrow checker; at the
        // bottom of the function they are moved back, amortizing their
        // capacity across frames. Error-path early returns lose the
        // amortization for one frame only — acceptable since the draw
        // has already failed. See issue #243.
        let ssbo_t0 = Instant::now();
        let mut gpu_instances: Vec<GpuInstance> = std::mem::take(&mut self.gpu_instances_scratch);
        gpu_instances.clear();
        gpu_instances.reserve(draw_commands.len() + 1); // +1 for optional UI quad
        let mut batches: Vec<DrawBatch> = std::mem::take(&mut self.batches_scratch);
        batches.clear();
        batches.reserve(draw_commands.len());

        // Sort contract for draw_commands is owned by render.rs
        // `build_render_data`. The per-field cluster order is covered
        // by the unit test `render::sort_key_clusters_by_alpha_decal_twosided`
        // (#500 D3-M2). A duplicate debug_assert here drifted out of
        // sync with the real key and was removed rather than kept in
        // lockstep across two crates.
        for draw_cmd in draw_commands {
            let Some(mesh) = self.mesh_registry.get(draw_cmd.mesh_handle) else {
                continue;
            };

            let instance_idx = gpu_instances.len() as u32;
            let m = &draw_cmd.model_matrix;

            // Detect non-uniform scale from the model matrix column lengths.
            // If the 3 column vectors of the upper-3x3 have different lengths,
            // the vertex shader must use inverse-transpose for normals.
            // Otherwise it can skip the expensive inverse (~40 ALU ops).
            // Three dot products is trivial compared to the per-vertex savings.
            let col0_sq = m[0] * m[0] + m[1] * m[1] + m[2] * m[2];
            let col1_sq = m[4] * m[4] + m[5] * m[5] + m[6] * m[6];
            let col2_sq = m[8] * m[8] + m[9] * m[9] + m[10] * m[10];
            let has_non_uniform_scale = {
                let tol = 0.001;
                (col0_sq - col1_sq).abs() > tol || (col0_sq - col2_sq).abs() > tol
            };
            // Per-instance flags — see INSTANCE_FLAG_* constants in
            // scene_buffer.rs. CPU-side assembly must stay in lockstep
            // with the fragment shader's `flags & N` checks.
            //   bit 0 = non-uniform scale
            //   bit 1 = NiAlphaProperty blend bit
            //   bit 2 = caustic source (alpha-blend + metalness < 0.3). See #321.
            //   bit 3 = terrain splat (set in cell_loader for LAND entities, #470).
            let mut flags = if has_non_uniform_scale {
                INSTANCE_FLAG_NON_UNIFORM_SCALE
            } else {
                0u32
            };
            if draw_cmd.alpha_blend {
                flags |= INSTANCE_FLAG_ALPHA_BLEND;
                if draw_cmd.metalness < 0.3 {
                    flags |= INSTANCE_FLAG_CAUSTIC_SOURCE;
                }
            }
            if let Some(tile_idx) = draw_cmd.terrain_tile_index {
                flags |= INSTANCE_FLAG_TERRAIN_SPLAT;
                flags |= (tile_idx & INSTANCE_TERRAIN_TILE_MASK) << INSTANCE_TERRAIN_TILE_SHIFT;
            }

            gpu_instances.push(GpuInstance {
                model: [
                    [m[0], m[1], m[2], m[3]],
                    [m[4], m[5], m[6], m[7]],
                    [m[8], m[9], m[10], m[11]],
                    [m[12], m[13], m[14], m[15]],
                ],
                texture_index: draw_cmd.texture_handle,
                bone_offset: draw_cmd.bone_offset,
                normal_map_index: draw_cmd.normal_map_index,
                roughness: draw_cmd.roughness,
                metalness: draw_cmd.metalness,
                emissive_mult: draw_cmd.emissive_mult,
                emissive_r: draw_cmd.emissive_color[0],
                emissive_g: draw_cmd.emissive_color[1],
                emissive_b: draw_cmd.emissive_color[2],
                specular_strength: draw_cmd.specular_strength,
                specular_r: draw_cmd.specular_color[0],
                specular_g: draw_cmd.specular_color[1],
                specular_b: draw_cmd.specular_color[2],
                vertex_offset: mesh.global_vertex_offset,
                index_offset: mesh.global_index_offset,
                vertex_count: mesh.vertex_count,
                alpha_threshold: draw_cmd.alpha_threshold,
                alpha_test_func: draw_cmd.alpha_test_func,
                dark_map_index: draw_cmd.dark_map_index,
                avg_albedo_r: draw_cmd.avg_albedo[0],
                avg_albedo_g: draw_cmd.avg_albedo[1],
                avg_albedo_b: draw_cmd.avg_albedo[2],
                flags,
                material_kind: draw_cmd.material_kind,
                glow_map_index: draw_cmd.glow_map_index,
                detail_map_index: draw_cmd.detail_map_index,
                gloss_map_index: draw_cmd.gloss_map_index,
                parallax_map_index: draw_cmd.parallax_map_index,
                parallax_height_scale: draw_cmd.parallax_height_scale,
                parallax_max_passes: draw_cmd.parallax_max_passes,
                env_map_index: draw_cmd.env_map_index,
                env_mask_index: draw_cmd.env_mask_index,
                // #492 — plumb FO4 BGSM UV + material_alpha through
                // to the SSBO. Fragment shader consumers follow in
                // #494. Padding slots stay at Default (0.0) so the
                // 224-byte stride matches the shader-side layout.
                uv_offset_u: draw_cmd.uv_offset[0],
                uv_offset_v: draw_cmd.uv_offset[1],
                uv_scale_u: draw_cmd.uv_scale[0],
                uv_scale_v: draw_cmd.uv_scale[1],
                material_alpha: draw_cmd.material_alpha,
                _uv_pad0: 0.0,
                _uv_pad1: 0.0,
                _uv_pad2: 0.0,
                // #562 — Skyrim+ BSLightingShaderProperty variant
                // payloads. Each field is zero on meshes whose
                // `material_kind` doesn't use it; the fragment
                // shader's variant ladder gates reads accordingly.
                skin_tint_r: draw_cmd.skin_tint_rgba[0],
                skin_tint_g: draw_cmd.skin_tint_rgba[1],
                skin_tint_b: draw_cmd.skin_tint_rgba[2],
                skin_tint_a: draw_cmd.skin_tint_rgba[3],
                hair_tint_r: draw_cmd.hair_tint_rgb[0],
                hair_tint_g: draw_cmd.hair_tint_rgb[1],
                hair_tint_b: draw_cmd.hair_tint_rgb[2],
                multi_layer_envmap_strength: draw_cmd.multi_layer_envmap_strength,
                eye_left_center_x: draw_cmd.eye_left_center[0],
                eye_left_center_y: draw_cmd.eye_left_center[1],
                eye_left_center_z: draw_cmd.eye_left_center[2],
                eye_cubemap_scale: draw_cmd.eye_cubemap_scale,
                eye_right_center_x: draw_cmd.eye_right_center[0],
                eye_right_center_y: draw_cmd.eye_right_center[1],
                eye_right_center_z: draw_cmd.eye_right_center[2],
                _eye_pad: 0.0,
                multi_layer_inner_thickness: draw_cmd.multi_layer_inner_thickness,
                multi_layer_refraction_scale: draw_cmd.multi_layer_refraction_scale,
                multi_layer_inner_scale_u: draw_cmd.multi_layer_inner_scale[0],
                multi_layer_inner_scale_v: draw_cmd.multi_layer_inner_scale[1],
                sparkle_r: draw_cmd.sparkle_rgba[0],
                sparkle_g: draw_cmd.sparkle_rgba[1],
                sparkle_b: draw_cmd.sparkle_rgba[2],
                sparkle_intensity: draw_cmd.sparkle_rgba[3],
                // #221 — `NiMaterialProperty.diffuse` + `.ambient`. Per
                // material tint multiplier on sampled albedo + per
                // material ambient modulator. Identity (`[1.0; 3]`)
                // for every BSShader-only Skyrim+/FO4 mesh.
                diffuse_r: draw_cmd.diffuse_color[0],
                diffuse_g: draw_cmd.diffuse_color[1],
                diffuse_b: draw_cmd.diffuse_color[2],
                _diffuse_pad: 0.0,
                ambient_r: draw_cmd.ambient_color[0],
                ambient_g: draw_cmd.ambient_color[1],
                ambient_b: draw_cmd.ambient_color[2],
                _ambient_pad: 0.0,
            });

            // Frustum-culled draws still need an SSBO entry so RT hit
            // shaders that land on their TLAS instance read the right
            // material / transform (#516). Skip batch formation — they
            // have no rasterized pixels this frame. Breaking the batch
            // chain here also avoids accidentally extending a previous
            // batch across a gap in the SSBO layout (`first_instance +
            // instance_count` would point past an off-screen draw).
            if !draw_cmd.in_raster {
                continue;
            }

            let pipeline_key = if draw_cmd.alpha_blend {
                PipelineKey::Blended {
                    src: draw_cmd.src_blend,
                    dst: draw_cmd.dst_blend,
                    two_sided: draw_cmd.two_sided,
                }
            } else {
                PipelineKey::Opaque {
                    two_sided: draw_cmd.two_sided,
                }
            };

            // Extend the current batch if this draw shares the same
            // state AND is contiguous in the SSBO (no culled draws in
            // the gap). The contiguity check is new with #516 — before
            // the in_raster split the SSBO idx always advanced 1:1
            // with the batch-eligible iterations, so contiguity was
            // implicit. Now an off-screen draw pushes an SSBO entry
            // but skips batch formation, so the next rasterized draw
            // might land at a non-contiguous `instance_idx`.
            if let Some(batch) = batches.last_mut() {
                if batch.mesh_handle == draw_cmd.mesh_handle
                    && batch.pipeline_key == pipeline_key
                    && batch.is_decal == draw_cmd.is_decal
                    && batch.z_test == draw_cmd.z_test
                    && batch.z_write == draw_cmd.z_write
                    && batch.z_function == draw_cmd.z_function
                    && batch.first_instance + batch.instance_count == instance_idx
                {
                    batch.instance_count += 1;
                    continue;
                }
            }

            // Start a new batch.
            batches.push(DrawBatch {
                mesh_handle: draw_cmd.mesh_handle,
                pipeline_key,
                is_decal: draw_cmd.is_decal,
                first_instance: instance_idx,
                instance_count: 1,
                index_count: mesh.index_count,
                global_index_offset: mesh.global_index_offset,
                global_vertex_offset: mesh.global_vertex_offset as i32,
                z_test: draw_cmd.z_test,
                z_write: draw_cmd.z_write,
                z_function: draw_cmd.z_function,
            });
        }

        // Append UI instance (if needed) BEFORE the bulk upload so it's
        // included in the single flush. Avoids the need for a separate raw
        // pointer write + flush that was missing on non-coherent memory (#189).
        let ui_instance_idx =
            if let (Some(ui_tex), Some(_)) = (ui_texture_handle, self.ui_quad_handle) {
                let idx = gpu_instances.len() as u32;
                gpu_instances.push(GpuInstance {
                    texture_index: ui_tex,
                    ..GpuInstance::default()
                });
                Some(idx)
            } else {
                None
            };

        // #647 / RP-1 — guard against `gl_InstanceIndex` overflow into
        // the R16_UINT mesh_id attachment. `triangle.frag:712` packs
        // `(instance_index + 1) & 0x7FFF` into the low 15 bits and
        // reserves bit 15 (0x8000) for the ALPHA_BLEND_NO_HISTORY
        // flag, so the per-frame ceiling is 32766 distinct instances
        // (index 0..=32766 → mesh_ids 1..=32767). Past that, two
        // distinct meshes silently collapse to the same id and SVGF
        // disocclusion accepts stale samples from the wrong mesh —
        // visible as cross-instance ghosting on dense city cells.
        // Skyrim/FO4 city REFR counts (~50K) can plausibly hit this;
        // the right fix is bumping `MESH_ID_FORMAT` to R32_UINT (+8 MB
        // at 1080p), but the assert catches the silent failure mode
        // until that lands. Debug-only — release builds keep the
        // wrap behaviour rather than panicking on a busy frame.
        debug_assert!(
            gpu_instances.len() <= 0x7FFF,
            "RP-1: visible instance count {} exceeds the R16_UINT mesh_id \
             ceiling (0x7FFF = 32767 with the alpha-blend bit). \
             Bump MESH_ID_FORMAT to R32_UINT or partition draws.",
            gpu_instances.len(),
        );
        // Upload all instance data (scene + UI) to the SSBO in one flush.
        if !gpu_instances.is_empty() {
            self.scene_buffers
                .upload_instances(&self.device, frame, &gpu_instances)
                .unwrap_or_else(|e| log::warn!("Failed to upload instances: {e}"));
        }

        // Zero the ray budget counter so the fragment shader starts each
        // frame with a fresh allowance of Phase-3 IOR glass rays.
        self.scene_buffers
            .reset_ray_budget(&self.device, frame)
            .unwrap_or_else(|e| log::warn!("Failed to reset ray budget: {e}"));

        // Reupload the terrain tile SSBO when cell load mutated it.
        // The slab is static until the next cell transition — #497
        // moved it to a single DEVICE_LOCAL buffer uploaded via a
        // transient staging copy, so one upload per dirty transition
        // is enough. The scratch Vec lives on self so its 32 KB
        // capacity amortizes across cell loads — `mem::take` moves it
        // out so the fill can run while `&self.scene_buffers` consumes
        // the slice. #496.
        let mut tile_scratch: Vec<GpuTerrainTile> = std::mem::take(&mut self.terrain_tile_scratch);
        if self.fill_terrain_tile_scratch_if_dirty(&mut tile_scratch) {
            let allocator = self.allocator.as_ref().expect("allocator missing");
            self.scene_buffers
                .upload_terrain_tiles(
                    &self.device,
                    allocator,
                    &self.graphics_queue,
                    self.transfer_pool,
                    &tile_scratch,
                )
                .unwrap_or_else(|e| log::warn!("Failed to upload terrain tiles: {e}"));
        }
        self.terrain_tile_scratch = tile_scratch;

        // Build + upload indirect-draw commands for this frame (#309).
        // One `VkDrawIndexedIndirectCommand` per DrawBatch, laid out in
        // the same order as `batches` so the draw loop can reference a
        // contiguous range of the buffer for each pipeline group.
        // Populated regardless of `device_caps.multi_draw_indirect_supported`
        // — the upload is ~N × 20 B for small N, and this keeps the
        // indirect path always ready when it is enabled.
        if !batches.is_empty() && self.device_caps.multi_draw_indirect_supported {
            let indirect_scratch = &mut self.indirect_draws_scratch;
            indirect_scratch.clear();
            indirect_scratch.extend(batches.iter().map(|b| vk::DrawIndexedIndirectCommand {
                index_count: b.index_count,
                instance_count: b.instance_count,
                first_index: b.global_index_offset,
                vertex_offset: b.global_vertex_offset,
                first_instance: b.first_instance,
            }));
            self.scene_buffers
                .upload_indirect_draws(&self.device, frame, indirect_scratch)
                .unwrap_or_else(|e| log::warn!("Failed to upload indirect draws: {e}"));
        }
        t.ssbo_build_ns = ssbo_t0.elapsed().as_nanos() as u64;

        // Pre-populate the blend pipeline cache for any new (src, dst,
        // two_sided) combos this frame. Resolved up-front because the
        // hot draw loop only takes `&self.device` for `cmd_bind_pipeline`
        // and can't reborrow `&mut self` to lazy-create. After this loop
        // every `PipelineKey::Blended` has a corresponding cache entry.
        // See #392.
        for batch in &batches {
            if let PipelineKey::Blended {
                src,
                dst,
                two_sided,
            } = batch.pipeline_key
            {
                if !self
                    .blend_pipeline_cache
                    .contains_key(&(src, dst, two_sided))
                {
                    if let Err(e) = self.get_or_create_blend_pipeline(src, dst, two_sided) {
                        log::error!(
                            "Failed to create blend pipeline (src={src}, dst={dst}, two_sided={two_sided}): {e}; \
                             draws using this combo will fall back to opaque pipeline"
                        );
                    }
                }
            }
        }

        // Barrier: make the instance SSBO host write (and any remaining
        // light/camera/bone host writes) visible to the vertex + fragment
        // shaders in the upcoming render pass. Required by Vulkan spec
        // even for HOST_COHERENT memory.
        unsafe {
            // Host-to-device visibility barrier. Covers the instance
            // SSBO (read by VS/FS), camera/light/bone buffers (read by
            // VS/FS), and — when `multiDrawIndirect` is enabled —
            // the per-frame indirect buffer whose contents
            // `cmd_draw_indexed_indirect` reads at `DRAW_INDIRECT`
            // stage. Without the extra stage mask, Vulkan validation
            // flags the indirect fetch as racing the host write.
            let instance_barrier = vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::HOST_WRITE)
                .dst_access_mask(
                    vk::AccessFlags::SHADER_READ
                        | vk::AccessFlags::UNIFORM_READ
                        | vk::AccessFlags::INDIRECT_COMMAND_READ,
                );
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::HOST,
                vk::PipelineStageFlags::VERTEX_SHADER
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::DRAW_INDIRECT,
                vk::DependencyFlags::empty(),
                &[instance_barrier],
                &[],
                &[],
            );
        }

        let cmd_t0 = Instant::now();
        unsafe {
            self.device
                .cmd_begin_render_pass(cmd, &render_pass_begin, vk::SubpassContents::INLINE);

            // No unconditional pipeline bind here — the batch loop below
            // initializes `last_pipeline_key` to a sentinel Blended value
            // so the first real batch always rebinds to its own pipeline,
            // and the UI overlay rebinds `pipeline_ui` regardless. An
            // opaque bind at this point would always be discarded. #507.

            // Dynamic viewport + scissor.
            let viewports = [vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.swapchain_state.extent.width as f32,
                height: self.swapchain_state.extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }];
            self.device.cmd_set_viewport(cmd, 0, &viewports);

            let scissors = [vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.swapchain_state.extent,
            }];
            self.device.cmd_set_scissor(cmd, 0, &scissors);

            // Bind the bindless texture descriptor set (set 0) — once per frame.
            let texture_set = self.texture_registry.descriptor_set(frame);
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[texture_set],
                &[],
            );

            // Bind the scene descriptor set (set 1) — once per frame.
            let scene_set = self.scene_buffers.descriptor_set(frame);
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                1,
                &[scene_set],
                &[],
            );

            // ── Draw loop ─────────────────────────────────────────────
            //
            // Two paths depending on what the device supports:
            //
            // 1. **Multi-draw indirect** (#309) — when the device
            //    exposes `multiDrawIndirect` (universally supported on
            //    desktop Vulkan 1.0+) and the global VB/IB is bound,
            //    we group consecutive batches sharing
            //    `(pipeline_key, is_decal)` into one
            //    `cmd_draw_indexed_indirect` call reading N
            //    `VkDrawIndexedIndirectCommand` entries from the
            //    per-frame indirect buffer. Pipeline / depth-bias
            //    state transitions still split groups (necessary —
            //    dynamic state changes between draws).
            //
            // 2. **Per-batch fallback** — used when the device doesn't
            //    expose `multiDrawIndirect` or when the global VB/IB
            //    isn't bound (e.g. the spinning-cube demo before the
            //    scene SSBO is built). One `cmd_draw_indexed` per
            //    batch, same behavior as pre-#309.
            //
            // The indirect buffer has already been filled + flushed
            // above when `gpu_instances.upload_instances(...)` ran —
            // see the `indirect_draws` build-up where each batch
            // pushes one `VkDrawIndexedIndirectCommand` entry.
            let mut last_pipeline_key = PipelineKey::Blended {
                src: u8::MAX,
                dst: u8::MAX,
                two_sided: false,
            };
            let mut last_is_decal = false;
            // #398 — extended dynamic depth state. Vulkan requires the
            // dynamic state to be set BEFORE any draw call when the
            // pipeline declares the corresponding `vk::DynamicState`.
            // Initialise with the Gamebryo runtime defaults so the
            // first batch's "did this change?" check sees a sensible
            // baseline. Sentinel `last_z_function = u8::MAX` forces an
            // explicit set on the first batch regardless of value.
            let mut last_z_test = true;
            let mut last_z_write = true;
            let mut last_z_function: u8 = u8::MAX;
            // Dynamic cull mode only affects blend pipelines (they declare
            // VK_DYNAMIC_STATE_CULL_MODE in pipeline.rs). Opaque pipelines
            // bake a static BACK / NONE cull; emitting cmd_set_cull_mode
            // on them is harmless host-side state the static pipeline
            // ignores. Track the last value to elide redundant commands.
            let mut last_cull_mode = vk::CullModeFlags::BACK;
            // #664 — per-mesh-fallback VB/IB bind cache. Only consulted
            // on the `global_bound == false` path (early-startup or any
            // future failure mode). The two-sided alpha-blend split at
            // line ~1442 calls `dispatch_direct` twice for the same
            // batch, so without this cache the per-mesh fallback issued
            // two redundant binds per split batch. `u32::MAX` is the
            // never-bound sentinel — `MeshHandle` is `u32` and 0 is a
            // valid handle.
            let mut last_bound_mesh_handle: u32 = u32::MAX;

            // Set initial depth bias to zero before first draw — Vulkan
            // requires the dynamic state to be set before any draw call
            // when the pipeline declares VK_DYNAMIC_STATE_DEPTH_BIAS.
            self.device.cmd_set_depth_bias(cmd, 0.0, 0.0, 0.0);
            // Same requirement for the new dynamic depth state.
            self.device.cmd_set_depth_test_enable(cmd, true);
            self.device.cmd_set_depth_write_enable(cmd, true);
            self.device
                .cmd_set_depth_compare_op(cmd, vk::CompareOp::LESS_OR_EQUAL);
            self.device.cmd_set_cull_mode(cmd, last_cull_mode);

            // Bind the global geometry buffer once for all scene draws.
            // Each batch uses global_index_offset / global_vertex_offset
            // to index into this single buffer, eliminating per-mesh
            // vertex/index buffer rebinding (~200 rebinds/frame → 1). #294.
            let global_bound = if let (Some(gvb), Some(gib)) = (
                self.mesh_registry.global_vertex_buffer.as_ref(),
                self.mesh_registry.global_index_buffer.as_ref(),
            ) {
                self.device
                    .cmd_bind_vertex_buffers(cmd, 0, &[gvb.buffer], &[0]);
                self.device
                    .cmd_bind_index_buffer(cmd, gib.buffer, 0, vk::IndexType::UINT32);
                true
            } else {
                false
            };

            let use_indirect = global_bound && self.device_caps.multi_draw_indirect_supported;
            let indirect_buffer = self.scene_buffers.indirect_buffer(frame);
            let indirect_stride = std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u32;

            // Precompute indirect-buffer state for batch `i`. Returns
            // `(pipe, is_decal)` — consecutive batches sharing the
            // tuple form one indirect group.
            let batch_state = |b: &DrawBatch| (b.pipeline_key, b.is_decal);

            let mut i = 0;
            while i < batches.len() {
                let batch = &batches[i];

                // Switch pipeline when rendering mode changes.
                if batch.pipeline_key != last_pipeline_key {
                    let pipe = match batch.pipeline_key {
                        PipelineKey::Opaque { two_sided: false } => self.pipeline,
                        PipelineKey::Opaque { two_sided: true } => self.pipeline_two_sided,
                        PipelineKey::Blended {
                            src,
                            dst,
                            two_sided,
                        } => {
                            // Always present after the pre-population
                            // pass above. If creation failed earlier we
                            // fall back to the opaque pipeline rather
                            // than skipping the draw entirely — better
                            // a wrong-blend visible mesh than a vanished
                            // one. See #392.
                            *self
                                .blend_pipeline_cache
                                .get(&(src, dst, two_sided))
                                .unwrap_or(if two_sided {
                                    &self.pipeline_two_sided
                                } else {
                                    &self.pipeline
                                })
                        }
                    };
                    self.device
                        .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipe);
                    last_pipeline_key = batch.pipeline_key;
                }

                // Depth bias for decal geometry — only emit when state changes.
                if batch.is_decal != last_is_decal {
                    let bias = if batch.is_decal { -4.0_f32 } else { 0.0 };
                    self.device.cmd_set_depth_bias(
                        cmd,
                        bias,
                        0.0,
                        if batch.is_decal { -1.0 } else { 0.0 },
                    );
                    last_is_decal = batch.is_decal;
                }

                // #398 — extended dynamic depth state. Emit only on
                // change so consecutive batches sharing depth state pay
                // zero state-change cost. Sky domes / viewmodels / glow
                // halos that author `z_write=0` now actually skip the
                // depth write instead of z-fighting world geometry.
                if batch.z_test != last_z_test {
                    self.device.cmd_set_depth_test_enable(cmd, batch.z_test);
                    last_z_test = batch.z_test;
                }
                if batch.z_write != last_z_write {
                    self.device.cmd_set_depth_write_enable(cmd, batch.z_write);
                    last_z_write = batch.z_write;
                }
                if batch.z_function != last_z_function {
                    self.device
                        .cmd_set_depth_compare_op(cmd, gamebryo_to_vk_compare_op(batch.z_function));
                    last_z_function = batch.z_function;
                }

                // Classify the batch's cull-mode requirement.
                //
                // Every pipeline declares CULL_MODE as dynamic (so the
                // state persists across pipeline transitions — per
                // Vulkan spec a bind to a pipeline without the dynamic
                // state would invalidate prior cmd_set_cull_mode), so
                // we must emit the target cull per-batch even for
                // opaque draws. The per-batch cost is a single u32
                // host command.
                //
                // Two-sided alpha-blend batches are rendered in two
                // passes — FRONT cull first (draws back faces, which
                // write depth), then BACK cull (draws front faces,
                // which blend on top). Without the split, a single
                // CULL_NONE draw would put front and back triangles in
                // arbitrary index order; TAA subpixel jitter then
                // flips the depth winner per frame, producing
                // cross-hatch moiré on glass. See Phase 1 of Tier C
                // glass plan + `docs/issues/glass-investigation/`.
                let (is_blend, two_sided) = match batch.pipeline_key {
                    PipelineKey::Blended { two_sided, .. } => (true, two_sided),
                    PipelineKey::Opaque { two_sided } => (false, two_sided),
                };
                let needs_split = is_blend && two_sided;
                // Opaque & single-sided-blend cull target — used by
                // every branch below except the split two-sided blend.
                let default_cull = if two_sided {
                    vk::CullModeFlags::NONE
                } else {
                    vk::CullModeFlags::BACK
                };

                let set_cull = |target: vk::CullModeFlags, last: &mut vk::CullModeFlags| {
                    if *last != target {
                        self.device.cmd_set_cull_mode(cmd, target);
                        *last = target;
                    }
                };

                // Dispatch helper — one direct draw of `batch`. Factored
                // so we can call it twice for the two-sided alpha-blend
                // split without duplicating the global-bound / per-mesh
                // fallback paths.
                //
                // #664 — `last_bound` threads through so the per-mesh
                // fallback elides VB/IB rebinds when consecutive
                // dispatches share `mesh_handle` (the two-sided
                // alpha-blend split is the dominant case).
                let dispatch_direct = |this: &Self, last_bound: &mut u32| {
                    if global_bound {
                        this.device.cmd_draw_indexed(
                            cmd,
                            batch.index_count,
                            batch.instance_count,
                            batch.global_index_offset,
                            batch.global_vertex_offset,
                            batch.first_instance,
                        );
                    } else {
                        if batch.mesh_handle != *last_bound {
                            if let Some(mesh) = this.mesh_registry.get(batch.mesh_handle) {
                                this.device.cmd_bind_vertex_buffers(
                                    cmd,
                                    0,
                                    &[mesh.vertex_buffer.buffer],
                                    &[0],
                                );
                                this.device.cmd_bind_index_buffer(
                                    cmd,
                                    mesh.index_buffer.buffer,
                                    0,
                                    vk::IndexType::UINT32,
                                );
                                *last_bound = batch.mesh_handle;
                            }
                        }
                        this.device.cmd_draw_indexed(
                            cmd,
                            batch.index_count,
                            batch.instance_count,
                            0,
                            0,
                            batch.first_instance,
                        );
                    }
                };

                if needs_split {
                    // Two-sided alpha-blend: back faces first, then
                    // front faces. Fall out of indirect grouping —
                    // two-sided blend batches must draw each mesh
                    // back+front adjacently, which
                    // `cmd_draw_indexed_indirect` over a group can't
                    // express without interleaving meshes.
                    set_cull(vk::CullModeFlags::FRONT, &mut last_cull_mode);
                    dispatch_direct(self, &mut last_bound_mesh_handle);
                    set_cull(vk::CullModeFlags::BACK, &mut last_cull_mode);
                    dispatch_direct(self, &mut last_bound_mesh_handle);
                    i += 1;
                } else if use_indirect {
                    set_cull(default_cull, &mut last_cull_mode);
                    // Gather consecutive batches that share the current
                    // `(pipeline_key, is_decal)` tuple — each one is
                    // already represented in the indirect buffer as one
                    // VkDrawIndexedIndirectCommand. A single
                    // `cmd_draw_indexed_indirect` call dispatches all N.
                    //
                    // Two-sided blend batches are excluded above and
                    // can't reach this branch, so grouping is safe.
                    let key = batch_state(batch);
                    let mut end = i + 1;
                    while end < batches.len()
                        && batch_state(&batches[end]) == key
                        && !matches!(
                            batches[end].pipeline_key,
                            PipelineKey::Blended {
                                two_sided: true,
                                ..
                            }
                        )
                    {
                        end += 1;
                    }
                    let group_size = (end - i) as u32;
                    let byte_offset = (i * indirect_stride as usize) as vk::DeviceSize;
                    self.device.cmd_draw_indexed_indirect(
                        cmd,
                        indirect_buffer,
                        byte_offset,
                        group_size,
                        indirect_stride,
                    );
                    i = end;
                } else {
                    // Direct-draw fallback: global VB/IB bound or
                    // per-mesh fallback inside `dispatch_direct`.
                    set_cull(default_cull, &mut last_cull_mode);
                    dispatch_direct(self, &mut last_bound_mesh_handle);
                    i += 1;
                }
            }

            // UI overlay: draw a fullscreen quad with the Ruffle-rendered texture.
            // The UI instance was appended to gpu_instances before the bulk upload,
            // so it's already in the SSBO with a proper flush.
            //
            // CONTRACT (#663). Defensive `cmd_set_*` calls below cover
            // every state in `UI_PIPELINE_DYNAMIC_STATES` so the UI
            // overlay is decoupled from whatever dynamic-state values
            // the last main-batch pipeline left set. Depth / cull /
            // depth-bias state on `pipeline_ui` is STATIC and applied
            // by the pipeline bind itself — no `cmd_set_*` is legal
            // for those (validation would reject it). If you grow
            // `UI_PIPELINE_DYNAMIC_STATES`, the const assertion below
            // fires and you must add the matching `cmd_set_*` here
            // before the draw.
            if let (Some(idx), Some(ui_quad)) = (ui_instance_idx, self.ui_quad_handle) {
                if let Some(mesh) = self.mesh_registry.get(ui_quad) {
                    use super::super::pipeline::UI_PIPELINE_DYNAMIC_STATES;
                    const _UI_OVERLAY_DEFENSIVE_STATE_INVARIANT: () = {
                        // Update the explicit cmd_set_* calls below to cover
                        // every state in this list when the count changes.
                        assert!(
                            UI_PIPELINE_DYNAMIC_STATES.len() == 2,
                            "UI overlay path covers VIEWPORT + SCISSOR only — \
                             extend it before growing UI_PIPELINE_DYNAMIC_STATES",
                        );
                    };
                    self.device.cmd_bind_pipeline(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.pipeline_ui,
                    );
                    // Defensive re-set of dynamic viewport/scissor after the
                    // UI pipeline bind (#133). The opaque/blend pipelines
                    // all declare both as VK_DYNAMIC_STATE, so the state set
                    // at the start of the render pass is inherited —
                    // today. A future UI variant that rendered at a
                    // different extent (e.g. scaled Scaleform overlay on
                    // a non-native resolution) would silently use the
                    // inherited values. Cheap two-command insurance.
                    let viewports = [vk::Viewport {
                        x: 0.0,
                        y: 0.0,
                        width: self.swapchain_state.extent.width as f32,
                        height: self.swapchain_state.extent.height as f32,
                        min_depth: 0.0,
                        max_depth: 1.0,
                    }];
                    self.device.cmd_set_viewport(cmd, 0, &viewports);
                    let scissors = [vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: self.swapchain_state.extent,
                    }];
                    self.device.cmd_set_scissor(cmd, 0, &scissors);
                    self.device
                        .cmd_bind_vertex_buffers(cmd, 0, &[mesh.vertex_buffer.buffer], &[0]);
                    self.device.cmd_bind_index_buffer(
                        cmd,
                        mesh.index_buffer.buffer,
                        0,
                        vk::IndexType::UINT32,
                    );
                    self.device
                        .cmd_draw_indexed(cmd, mesh.index_count, 1, 0, 0, idx);
                }
            }

            self.device.cmd_end_render_pass(cmd);

            // SVGF temporal accumulation (Phase 3): reprojects previous
            // frame's accumulated indirect, blends with raw 1-SPP indirect
            // at α=0.2. Reads G-buffer raw_indirect/motion/mesh_id (now in
            // SHADER_READ_ONLY_OPTIMAL via render pass final_layout) +
            // history from previous frame's SVGF output slot, writes this
            // frame's accumulated indirect + moments. Composite samples
            // the output below.
            // SVGF permanent-failure latch: after the first dispatch
            // error, skip all further attempts and leave the warn-log
            // behind (escalated to `error!` so the once-per-session
            // signal stands out). Composite's `indirectTex` descriptor
            // keeps pointing at the stale denoised image until the
            // next `recreate_swapchain` resets the latch. Rebinding to
            // the raw-indirect G-buffer view would give a live (noisy)
            // picture but requires composite-side plumbing deferred
            // until a real lost-device repro. See #479.
            if !self.svgf_failed {
                if let Some(ref mut svgf) = self.svgf {
                    // #674 — temporal α state machine. 0.2 steady-state
                    // (Schied 2017 §4 floor), 0.5 in the recovery
                    // window after a discontinuity. Bumped via
                    // `signal_temporal_discontinuity`; consumes one
                    // frame from the window each draw_frame.
                    let (alpha_color, alpha_moments, next_frames) =
                        crate::vulkan::svgf::next_svgf_temporal_alpha(self.svgf_recovery_frames);
                    self.svgf_recovery_frames = next_frames;
                    if let Err(e) =
                        svgf.dispatch(&self.device, cmd, frame, alpha_color, alpha_moments)
                    {
                        log::error!(
                            "SVGF dispatch failed — pass disabled for the rest of the session: {e}"
                        );
                        self.svgf_failed = true;
                    }
                }
            }

            // Caustic scatter (#321): per-refractive-pixel refracted-light
            // splat. Runs after SVGF (reads the same G-buffer slots that
            // are now in SHADER_READ_ONLY_OPTIMAL) and before composite
            // (which samples the caustic accumulator). Writes binding 5
            // of the composite descriptor set.
            // Caustic permanent-failure latch — same shape as SVGF.
            // Composite's `causticTex` sampler keeps reading the
            // accumulator's last valid contents, so at worst one
            // stale caustic frame hangs around until resize. See #479.
            if !self.caustic_failed {
                if let Some(ref mut caustic) = self.caustic {
                    // Bind this frame's TLAS before dispatch — the AccelerationManager
                    // rebuilds/refits per frame but the handle is stable across frames
                    // once created, so we write it once and then again defensively.
                    // Skip the dispatch entirely when no TLAS is available
                    // for this frame (RT unsupported or scene-load not yet
                    // settled). Mirrors the shader's `sceneFlags.x < 0.5`
                    // early-out — pre-#640 the dispatch ran every frame
                    // regardless and the shader paid full ray-query cost
                    // against unwritten / stale TLAS state.
                    let tlas_handle = self
                        .accel_manager
                        .as_ref()
                        .and_then(|accel| accel.tlas_handle(frame));
                    if let Some(tlas) = tlas_handle {
                        caustic.write_tlas(&self.device, frame, tlas);
                        if let Err(e) = caustic.dispatch(&self.device, cmd, frame) {
                            log::error!(
                                "Caustic dispatch failed — pass disabled for the rest of the session: {e}"
                            );
                            self.caustic_failed = true;
                        }
                    }
                }
            }

            // TAA resolve: reprojects previous frame's history via motion
            // vectors, neighborhood-clamps in YCoCg, and writes the anti-
            // aliased HDR result for composite to sample. Runs after SVGF
            // (which denoises the indirect term) and before SSAO/composite.
            // TAA permanent-failure recovery: on the first dispatch
            // error the composite's binding 0 (which currently points
            // at TAA's output) gets rebound to the raw HDR render-pass
            // attachments so the screen keeps updating — without the
            // fallback the last TAA-written HDR frame would freeze on
            // screen for the rest of the session with only a `warn!`
            // log hinting at the cause. See #479.
            if !self.taa_failed {
                if let Some(ref mut taa) = self.taa {
                    if let Err(e) = taa.dispatch(&self.device, cmd, frame) {
                        log::error!(
                            "TAA dispatch failed — falling back to raw HDR for the rest of the session: {e}"
                        );
                        self.taa_failed = true;
                        if let Some(ref mut composite) = self.composite {
                            composite.fall_back_to_raw_hdr(&self.device);
                        }
                    }
                }
            }

            // SSAO compute pass: reads depth buffer (now in READ_ONLY layout
            // after render pass), writes AO texture for this frame's fragment
            // shader. Runs before composite so AO is current-frame (no lag).
            if let Some(ref mut ssao) = self.ssao {
                let vp_arr = [
                    [vp[0], vp[1], vp[2], vp[3]],
                    [vp[4], vp[5], vp[6], vp[7]],
                    [vp[8], vp[9], vp[10], vp[11]],
                    [vp[12], vp[13], vp[14], vp[15]],
                ];
                if let Err(e) =
                    ssao.dispatch(&self.device, cmd, frame, &vp_arr, &inv_vp_arr, camera_pos)
                {
                    log::warn!("SSAO dispatch failed: {e}");
                }
            }

            // Upload composite params (fog + sky) before the composite pass.
            if let Some(ref mut composite) = self.composite {
                let composite_params = super::super::composite::CompositeParams {
                    fog_color: [
                        fog_color[0],
                        fog_color[1],
                        fog_color[2],
                        if fog_far > fog_near { 1.0 } else { 0.0 },
                    ],
                    fog_params: [fog_near, fog_far, 0.0, 0.0],
                    depth_params: [
                        if sky_params.is_exterior { 1.0 } else { 0.0 },
                        0.85, // exposure — default Bethesda-era HDR target; promote to WTHR field (#743)
                        0.0,
                        0.0,
                    ],
                    sky_zenith: [
                        sky_params.zenith_color[0],
                        sky_params.zenith_color[1],
                        sky_params.zenith_color[2],
                        sky_params.sun_size,
                    ],
                    sky_horizon: [
                        sky_params.horizon_color[0],
                        sky_params.horizon_color[1],
                        sky_params.horizon_color[2],
                        0.0,
                    ],
                    // #541 — WTHR `SKY_LOWER` group. Pre-fix the
                    // shader faked this as `sky_horizon * 0.3`,
                    // dropping the authored colour entirely.
                    sky_lower: [
                        sky_params.lower_color[0],
                        sky_params.lower_color[1],
                        sky_params.lower_color[2],
                        0.0,
                    ],
                    sun_dir: [
                        sky_params.sun_direction[0],
                        sky_params.sun_direction[1],
                        sky_params.sun_direction[2],
                        sky_params.sun_intensity,
                    ],
                    sun_color: [
                        sky_params.sun_color[0],
                        sky_params.sun_color[1],
                        sky_params.sun_color[2],
                        // #478 — pack the CLMT FNAM sun sprite handle
                        // into the previously-unused w slot via
                        // `from_bits`. The shader reinterprets with
                        // `floatBitsToUint`; `0` keeps the procedural
                        // disc (pre-fix behaviour).
                        f32::from_bits(sky_params.sun_texture_index),
                    ],
                    cloud_params: [
                        sky_params.cloud_scroll[0],
                        sky_params.cloud_scroll[1],
                        sky_params.cloud_tile_scale,
                        f32::from_bits(sky_params.cloud_texture_index),
                    ],
                    cloud_params_1: [
                        sky_params.cloud_scroll_1[0],
                        sky_params.cloud_scroll_1[1],
                        sky_params.cloud_tile_scale_1,
                        f32::from_bits(sky_params.cloud_texture_index_1),
                    ],
                    cloud_params_2: [
                        sky_params.cloud_scroll_2[0],
                        sky_params.cloud_scroll_2[1],
                        sky_params.cloud_tile_scale_2,
                        f32::from_bits(sky_params.cloud_texture_index_2),
                    ],
                    cloud_params_3: [
                        sky_params.cloud_scroll_3[0],
                        sky_params.cloud_scroll_3[1],
                        sky_params.cloud_tile_scale_3,
                        f32::from_bits(sky_params.cloud_texture_index_3),
                    ],
                    // #428 — composite-pass fog needs the camera origin to
                    // compute per-pixel world-space distance from a depth
                    // sample. `w` is unused padding.
                    camera_pos: [camera_pos[0], camera_pos[1], camera_pos[2], 0.0],
                    inv_view_proj: inv_vp_arr,
                };
                if let Err(e) = composite.upload_params(&self.device, frame, &composite_params) {
                    log::warn!("composite upload_params failed: {e}");
                }
            }

            // HOST→FRAGMENT barrier: the composite UBO was host-written by
            // upload_params above. Per Vulkan spec, host writes require an
            // explicit barrier even for HOST_COHERENT memory (the execution
            // dependency ensures ordering). SVGF and SSAO correctly emit
            // HOST→COMPUTE barriers for their UBOs; composite was missing
            // this. See #281.
            {
                let barrier = vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::HOST_WRITE)
                    .dst_access_mask(vk::AccessFlags::UNIFORM_READ);
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::HOST,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[barrier],
                    &[],
                    &[],
                );
            }

            // Composite pass: sample HDR + indirect + albedo, combine, ACES
            // tone map, write to swapchain. Runs in its own render pass.
            // The main render pass's outgoing subpass dependency handles
            // the layout transitions of all input attachments to
            // SHADER_READ_ONLY_OPTIMAL.
            if let Some(ref composite) = self.composite {
                let bindless_set = self.texture_registry.descriptor_set(frame);
                composite.dispatch(&self.device, cmd, frame, img, bindless_set);
            }

            // Screenshot capture: copy swapchain image to staging buffer
            // if requested. Must happen after composite (image has content)
            // and before end_command_buffer (still recording).
            let swapchain_image = self.swapchain_state.images[img];
            self.screenshot_record_copy(cmd, swapchain_image);

            self.device
                .end_command_buffer(cmd)
                .context("end_command_buffer")?;
        }
        t.cmd_record_ns = cmd_t0.elapsed().as_nanos() as u64;

        // Submit.
        let submit_t0 = Instant::now();
        let wait_semaphores = [self.frame_sync.image_available[frame]];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let signal_semaphores = [self.frame_sync.render_finished[img]];
        let command_buffers_to_submit = [cmd];

        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers_to_submit)
            .signal_semaphores(&signal_semaphores);

        unsafe {
            let queue = *self
                .graphics_queue
                .lock()
                .expect("graphics queue lock poisoned");
            self.device
                .queue_submit(queue, &[submit_info], self.frame_sync.in_flight[frame])
                .context("queue_submit")?;
        }

        // Present.
        let swapchains = [self.swapchain_state.swapchain];
        let image_indices = [image_index];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        let present_suboptimal = unsafe {
            let pq = self
                .present_queue
                .lock()
                .expect("present queue lock poisoned");
            match self
                .swapchain_state
                .swapchain_loader
                .queue_present(*pq, &present_info)
            {
                Ok(suboptimal) => suboptimal,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => true,
                Err(e) => anyhow::bail!("queue_present: {:?}", e),
            }
        };

        t.submit_present_ns = submit_t0.elapsed().as_nanos() as u64;
        if let Some(out) = timings {
            *out = t;
        }

        self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
        self.frame_counter = self.frame_counter.wrapping_add(1);

        // Restore the scratch buffers to the context so their capacity
        // amortizes across frames (#243), then shrink them back toward
        // the working set after a past peak frame. Same policy as the
        // `tlas_instances_scratch` in #504 — scratch Vecs behave as
        // "grow fast, shrink on pressure": working-set × 2 keeps a
        // slack band against frame-to-frame variance, and the 512
        // floor avoids reallocations on common-case small scenes.
        let working_instances = gpu_instances.len();
        let working_batches = batches.len();
        self.gpu_instances_scratch = gpu_instances;
        self.batches_scratch = batches;
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.gpu_instances_scratch,
            working_instances,
            512,
        );
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.batches_scratch,
            working_batches,
            512,
        );

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
                    accel.shrink_tlas_to_fit(
                        slot_to_shrink,
                        working_instances as u32,
                        &self.device,
                        allocator,
                    );
                }
            }
        }

        Ok(suboptimal || present_suboptimal)
    }
}
