//! Frame recording and submission — the per-frame hot path.

use super::super::scene_buffer::{self, GpuInstance};
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::{DrawCommand, SkyParams, VulkanContext};
use anyhow::{Context, Result};
use ash::vk;

/// A batch of instances sharing the same mesh + pipeline state.
/// Drawn with a single `cmd_draw_indexed` call.
///
/// `pub(super)` so the enclosing `VulkanContext` can hold a reusable
/// `Vec<DrawBatch>` scratch buffer as a field and amortize allocations
/// across frames. See issue #243.
pub(super) struct DrawBatch {
    pub mesh_handle: u32,
    pub pipeline_key: (bool, bool), // (alpha_blend, two_sided)
    pub is_decal: bool,
    pub first_instance: u32,
    pub instance_count: u32,
    pub index_count: u32,
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
    ) -> Result<bool> {
        let frame = self.current_frame;

        // Advance the texture registry's deferred-destroy frame counter.
        self.texture_registry.begin_frame();

        // Tick the mesh registry's deferred SSBO destroy list.
        if let Some(ref alloc) = self.allocator {
            self.mesh_registry.tick_deferred_destroy(&self.device, alloc);
        }

        // Wait for this frame-in-flight slot AND the previous slot to be
        // available. SVGF's temporal pass reads the previous slot's G-buffer
        // images (mesh_id, motion, raw_indirect) — without waiting on the
        // other slot's fence, a read-after-write hazard exists when the GPU
        // hasn't finished the other slot's render pass. See #282.
        //
        // Cost: zero in practice — the GPU is rarely more than 1 frame
        // behind the CPU, so the other fence is almost always signaled.
        unsafe {
            let prev = (frame + 1) % super::super::sync::MAX_FRAMES_IN_FLIGHT;
            self.device
                .wait_for_fences(
                    &[self.frame_sync.in_flight[frame], self.frame_sync.in_flight[prev]],
                    true,
                    u64::MAX,
                )
                .context("wait_for_fences")?;
        }

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

        // Record command buffer.
        let cmd = self.command_buffers[image_index as usize];
        unsafe {
            self.device
                .reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
                .context("reset_command_buffer")?;
        }

        let begin_info =
            vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
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

        // Build TLAS if RT is available (before render pass).
        unsafe {
            if let Some(ref mut accel) = self.accel_manager {
                if let Some(alloc) = self.allocator.as_ref() {
                    if let Err(e) =
                        accel.build_tlas(&self.device, alloc, cmd, draw_commands, frame)
                    {
                        log::warn!("TLAS build failed: {e}");
                    } else {
                        // Memory barrier: TLAS build → fragment shader read.
                        let barrier = vk::MemoryBarrier::default()
                            .src_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR)
                            .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR);
                        self.device.cmd_pipeline_barrier(
                            cmd,
                            vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                            vk::PipelineStageFlags::FRAGMENT_SHADER,
                            vk::DependencyFlags::empty(),
                            &[barrier],
                            &[],
                            &[],
                        );
                        if let Some(tlas_handle) = accel.tlas_handle(frame) {
                            self.scene_buffers
                                .write_tlas(&self.device, frame, tlas_handle);
                        }
                        // Evict unused BLAS entries if over budget.
                        accel.evict_unused_blas(&self.device, alloc);
                    }
                }
            }
        }

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
        let vp = view_proj;
        let pvp = &self.prev_view_proj;
        // Precompute inverse(viewProj) once on the CPU so shaders
        // (cluster culling, SSAO) can read it directly from the UBO
        // instead of computing a ~100 ALU-op matrix inverse per invocation.
        let vp_mat = byroredux_core::math::Mat4::from_cols_array(vp);
        let inv_vp = vp_mat.inverse();
        let inv_vp_cols = inv_vp.to_cols_array();
        let inv_vp_arr = [
            [inv_vp_cols[0], inv_vp_cols[1], inv_vp_cols[2], inv_vp_cols[3]],
            [inv_vp_cols[4], inv_vp_cols[5], inv_vp_cols[6], inv_vp_cols[7]],
            [inv_vp_cols[8], inv_vp_cols[9], inv_vp_cols[10], inv_vp_cols[11]],
            [inv_vp_cols[12], inv_vp_cols[13], inv_vp_cols[14], inv_vp_cols[15]],
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
            position: [camera_pos[0], camera_pos[1], camera_pos[2], self.frame_counter as f32],
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
                    .dst_access_mask(
                        vk::AccessFlags::SHADER_READ | vk::AccessFlags::UNIFORM_READ,
                    );
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
        let mut gpu_instances: Vec<GpuInstance> =
            std::mem::take(&mut self.gpu_instances_scratch);
        gpu_instances.clear();
        gpu_instances.reserve(draw_commands.len() + 1); // +1 for optional UI quad
        let mut batches: Vec<DrawBatch> = std::mem::take(&mut self.batches_scratch);
        batches.clear();
        batches.reserve(draw_commands.len());

        // Assert draw commands are sorted by pipeline key so the consecutive
        // batch merge below produces optimal results. #279 P1-08.
        debug_assert!(
            draw_commands.windows(2).all(|w| {
                let k0 = (w[0].alpha_blend, w[0].two_sided, w[0].is_decal);
                let k1 = (w[1].alpha_blend, w[1].two_sided, w[1].is_decal);
                k0 <= k1 || w[0].sort_depth <= w[1].sort_depth
            }),
            "draw_commands should be sorted before batch merge"
        );

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
                (col0_sq - col1_sq).abs() > tol
                    || (col0_sq - col2_sq).abs() > tol
            };
            let flags = if has_non_uniform_scale { 1u32 } else { 0u32 };

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
                _pad1: 0,
            });

            let pipeline_key = (draw_cmd.alpha_blend, draw_cmd.two_sided);

            // Extend the current batch if this draw shares the same state.
            if let Some(batch) = batches.last_mut() {
                if batch.mesh_handle == draw_cmd.mesh_handle
                    && batch.pipeline_key == pipeline_key
                    && batch.is_decal == draw_cmd.is_decal
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
            });
        }

        // Append UI instance (if needed) BEFORE the bulk upload so it's
        // included in the single flush. Avoids the need for a separate raw
        // pointer write + flush that was missing on non-coherent memory (#189).
        let ui_instance_idx = if let (Some(ui_tex), Some(_)) = (ui_texture_handle, self.ui_quad_handle) {
            let idx = gpu_instances.len() as u32;
            gpu_instances.push(GpuInstance {
                texture_index: ui_tex,
                ..GpuInstance::default()
            });
            Some(idx)
        } else {
            None
        };

        // Upload all instance data (scene + UI) to the SSBO in one flush.
        if !gpu_instances.is_empty() {
            self.scene_buffers
                .upload_instances(&self.device, frame, &gpu_instances)
                .unwrap_or_else(|e| log::warn!("Failed to upload instances: {e}"));
        }

        // Barrier: make the instance SSBO host write (and any remaining
        // light/camera/bone host writes) visible to the vertex + fragment
        // shaders in the upcoming render pass. Required by Vulkan spec
        // even for HOST_COHERENT memory.
        unsafe {
            let instance_barrier = vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::HOST_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::UNIFORM_READ);
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::HOST,
                vk::PipelineStageFlags::VERTEX_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[instance_barrier],
                &[],
                &[],
            );
        }

        unsafe {
            self.device
                .cmd_begin_render_pass(cmd, &render_pass_begin, vk::SubpassContents::INLINE);

            // Bind the default graphics pipeline.
            self.device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);

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

            // ── Instanced draw loop ───────────────────────────────────
            //
            // Each batch is a single cmd_draw_indexed call with instance_count > 1.
            // The vertex shader reads instances[gl_InstanceIndex] for the model matrix,
            // texture index, and bone offset — no push constants, no per-draw descriptor
            // set binds.
            let mut last_pipeline_key = (false, false);
            let mut last_mesh_handle = u32::MAX;
            let mut last_is_decal = false;

            // Set initial depth bias to zero before first draw — Vulkan
            // requires the dynamic state to be set before any draw call
            // when the pipeline declares VK_DYNAMIC_STATE_DEPTH_BIAS.
            self.device.cmd_set_depth_bias(cmd, 0.0, 0.0, 0.0);

            for batch in &batches {
                // Switch pipeline when rendering mode changes.
                if batch.pipeline_key != last_pipeline_key {
                    let pipe = match batch.pipeline_key {
                        (false, false) => self.pipeline,
                        (true, false) => self.pipeline_alpha,
                        (false, true) => self.pipeline_two_sided,
                        (true, true) => self.pipeline_alpha_two_sided,
                    };
                    self.device
                        .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipe);
                    last_pipeline_key = batch.pipeline_key;
                }

                // Depth bias for decal geometry — only emit when state changes.
                // Reduced from (-8, -2) to (-4, -1) to prevent decals from
                // floating visibly in front of their host surface at grazing
                // viewing angles. The slope factor scales with the surface's
                // depth gradient, so -2.0 was pulling decals too far forward
                // on oblique walls.
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

                // Rebind vertex + index buffers only when the mesh changes.
                if batch.mesh_handle != last_mesh_handle {
                    if let Some(mesh) = self.mesh_registry.get(batch.mesh_handle) {
                        self.device.cmd_bind_vertex_buffers(
                            cmd,
                            0,
                            &[mesh.vertex_buffer.buffer],
                            &[0],
                        );
                        self.device.cmd_bind_index_buffer(
                            cmd,
                            mesh.index_buffer.buffer,
                            0,
                            vk::IndexType::UINT32,
                        );
                        last_mesh_handle = batch.mesh_handle;
                    }
                }

                // Single instanced draw call for the entire batch.
                self.device.cmd_draw_indexed(
                    cmd,
                    batch.index_count,
                    batch.instance_count,
                    0,
                    0,
                    batch.first_instance,
                );
            }

            // UI overlay: draw a fullscreen quad with the Ruffle-rendered texture.
            // The UI instance was appended to gpu_instances before the bulk upload,
            // so it's already in the SSBO with a proper flush.
            if let (Some(idx), Some(ui_quad)) = (ui_instance_idx, self.ui_quad_handle) {
                if let Some(mesh) = self.mesh_registry.get(ui_quad) {
                    self.device.cmd_bind_pipeline(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.pipeline_ui,
                    );
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
            if let Some(ref mut svgf) = self.svgf {
                if let Err(e) = svgf.dispatch(&self.device, cmd, frame) {
                    log::warn!("SVGF dispatch failed: {e}");
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
                if let Err(e) = ssao.dispatch(&self.device, cmd, frame, &vp_arr, &inv_vp_arr, camera_pos) {
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
                        0.0, 0.0, 0.0,
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
                        0.0,
                    ],
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
                composite.dispatch(&self.device, cmd, frame, img);
            }

            self.device
                .end_command_buffer(cmd)
                .context("end_command_buffer")?;
        }

        // Submit.
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

        self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
        self.frame_counter = self.frame_counter.wrapping_add(1);

        // Restore the scratch buffers to the context so their capacity
        // amortizes across frames. See issue #243.
        self.gpu_instances_scratch = gpu_instances;
        self.batches_scratch = batches;

        Ok(suboptimal || present_suboptimal)
    }
}
