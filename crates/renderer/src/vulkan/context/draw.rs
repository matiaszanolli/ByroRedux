//! Frame recording and submission — the per-frame hot path.

use super::super::scene_buffer;
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::{DrawCommand, VulkanContext};
use anyhow::{Context, Result};
use ash::vk;

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
        camera_pos: [f32; 3],
        ambient_color: [f32; 3],
        ui_texture_handle: Option<u32>,
    ) -> Result<bool> {
        let frame = self.current_frame;

        // Wait for this frame-in-flight slot to be available.
        unsafe {
            self.device
                .wait_for_fences(&[self.frame_sync.in_flight[frame]], true, u64::MAX)
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

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            self.device
                .begin_command_buffer(cmd, &begin_info)
                .context("begin_command_buffer")?;
        }

        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: clear_color,
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

        let render_pass_begin = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[image_index as usize])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.swapchain_state.extent,
            })
            .clear_values(&clear_values);

        // Build TLAS if RT is available (before render pass).
        unsafe {
            if let Some(ref mut accel) = self.accel_manager {
                if let Some(alloc) = self.allocator.as_ref() {
                    if let Err(e) = accel.build_tlas(&self.device, alloc, cmd, draw_commands) {
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
                        if let Some(tlas_handle) = accel.tlas_handle() {
                            self.scene_buffers
                                .write_tlas(&self.device, frame, tlas_handle);
                        }
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
        let camera = scene_buffer::GpuCamera {
            position: [camera_pos[0], camera_pos[1], camera_pos[2], 0.0],
            flags: [
                rt_flag,
                ambient_color[0],
                ambient_color[1],
                ambient_color[2],
            ],
        };
        self.scene_buffers
            .upload_camera(&self.device, frame, &camera)
            .unwrap_or_else(|e| log::warn!("Failed to upload camera: {e}"));

        unsafe {
            self.device
                .cmd_begin_render_pass(cmd, &render_pass_begin, vk::SubpassContents::INLINE);

            // Bind the graphics pipeline.
            self.device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);

            // Set dynamic viewport and scissor.
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

            // Push viewProj matrix (first 64 bytes of push constants).
            // SAFETY: [f32; 16] is 64 bytes, properly aligned, and the
            // reference is valid for the duration of cmd_push_constants.
            let view_proj_bytes: &[u8] =
                std::slice::from_raw_parts(view_proj.as_ptr() as *const u8, 64);
            self.device.cmd_push_constants(
                cmd,
                self.pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                view_proj_bytes,
            );

            // Bind scene descriptor set (lights + camera SSBOs/UBOs).
            // Indexed by `frame` (frame-in-flight, 0..MAX_FRAMES_IN_FLIGHT) because
            // scene data is double-buffered per frame, not per swapchain image.
            let scene_set = self.scene_buffers.descriptor_set(frame);
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                1, // set 1
                &[scene_set],
                &[],
            );

            // Draw: opaque first, then alpha-blended. Switch pipeline on mode change.
            let mut last_texture = u32::MAX;
            let mut last_pipeline_key = (false, false); // (alpha_blend, two_sided)

            for draw_cmd in draw_commands {
                if let Some(mesh) = self.mesh_registry.get(draw_cmd.mesh_handle) {
                    // Switch pipeline when rendering mode changes.
                    let pipeline_key = (draw_cmd.alpha_blend, draw_cmd.two_sided);
                    if pipeline_key != last_pipeline_key {
                        let pipe = match pipeline_key {
                            (false, false) => self.pipeline,
                            (true, false) => self.pipeline_alpha,
                            (false, true) => self.pipeline_two_sided,
                            (true, true) => self.pipeline_alpha_two_sided,
                        };
                        self.device
                            .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipe);
                        last_pipeline_key = pipeline_key;
                        // Descriptor set 0 (texture) is preserved across compatible
                        // pipeline switches per Vulkan spec 14.2.2 — no rebind needed.
                    }

                    // Bind texture descriptor set (skip if same as previous draw).
                    // Indexed by `image_index` (swapchain image) because texture sets
                    // are allocated per-image to avoid write-after-read hazards across
                    // frames that may reference different swapchain images.
                    if draw_cmd.texture_handle != last_texture {
                        let desc_set = self
                            .texture_registry
                            .descriptor_set(draw_cmd.texture_handle, image_index as usize);
                        self.device.cmd_bind_descriptor_sets(
                            cmd,
                            vk::PipelineBindPoint::GRAPHICS,
                            self.pipeline_layout,
                            0,
                            &[desc_set],
                            &[],
                        );
                        last_texture = draw_cmd.texture_handle;
                    }

                    // Depth bias for decal geometry.
                    let bias = if draw_cmd.is_decal { -8.0_f32 } else { 0.0 };
                    self.device.cmd_set_depth_bias(
                        cmd,
                        bias,
                        0.0,
                        if draw_cmd.is_decal { -2.0 } else { 0.0 },
                    );

                    // Push model matrix (bytes 64..128).
                    let model_bytes: &[u8] =
                        std::slice::from_raw_parts(draw_cmd.model_matrix.as_ptr() as *const u8, 64);
                    self.device.cmd_push_constants(
                        cmd,
                        self.pipeline_layout,
                        vk::ShaderStageFlags::VERTEX,
                        64,
                        model_bytes,
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
                        .cmd_draw_indexed(cmd, mesh.index_count, 1, 0, 0, 0);
                }
            }

            // UI overlay: draw a fullscreen quad with the Ruffle-rendered texture.
            if let (Some(ui_tex), Some(ui_quad)) = (ui_texture_handle, self.ui_quad_handle) {
                if let Some(mesh) = self.mesh_registry.get(ui_quad) {
                    self.device.cmd_bind_pipeline(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.pipeline_ui,
                    );

                    let desc_set = self
                        .texture_registry
                        .descriptor_set(ui_tex, image_index as usize);
                    self.device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.pipeline_layout,
                        0,
                        &[desc_set],
                        &[],
                    );

                    // Push identity matrices (required by pipeline layout, ignored by UI shader).
                    let identity: [f32; 16] = [
                        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
                        1.0,
                    ];
                    // SAFETY: [f32; 16] is 64 bytes, properly aligned, and the
                    // reference is valid for the duration of cmd_push_constants.
                    let identity_bytes: &[u8] =
                        std::slice::from_raw_parts(identity.as_ptr() as *const u8, 64);
                    self.device.cmd_push_constants(
                        cmd,
                        self.pipeline_layout,
                        vk::ShaderStageFlags::VERTEX,
                        0,
                        identity_bytes,
                    );
                    self.device.cmd_push_constants(
                        cmd,
                        self.pipeline_layout,
                        vk::ShaderStageFlags::VERTEX,
                        64,
                        identity_bytes,
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
                        .cmd_draw_indexed(cmd, mesh.index_count, 1, 0, 0, 0);
                }
            }

            self.device.cmd_end_render_pass(cmd);
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

        Ok(suboptimal || present_suboptimal)
    }
}
