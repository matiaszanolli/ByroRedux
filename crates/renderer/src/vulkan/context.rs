//! Top-level Vulkan context that owns the entire graphics state.

use super::allocator::{self, SharedAllocator};
use super::debug;
use super::device::{self, QueueFamilyIndices};
use super::instance;
use super::pipeline;
use super::surface;
use super::swapchain::{self, SwapchainState};
use super::sync::{self, FrameSync, MAX_FRAMES_IN_FLIGHT};
use super::texture::Texture;
use crate::mesh::MeshRegistry;
use crate::texture_registry::TextureRegistry;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;
use gpu_allocator::MemoryLocation;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;

/// A single draw command: which mesh to draw, with what texture, and what model matrix.
pub struct DrawCommand {
    pub mesh_handle: u32,
    pub texture_handle: u32,
    pub model_matrix: [f32; 16],
    pub alpha_blend: bool,
    pub two_sided: bool,
}

pub struct VulkanContext {
    // Ordered for drop safety — later fields are destroyed first.
    pub current_frame: usize,

    frame_sync: FrameSync,
    command_buffers: Vec<vk::CommandBuffer>,
    pub command_pool: vk::CommandPool,
    framebuffers: Vec<vk::Framebuffer>,
    depth_image_view: vk::ImageView,
    depth_image: vk::Image,
    depth_allocation: Option<vk_alloc::Allocation>,
    pub mesh_registry: MeshRegistry,
    pub texture_registry: TextureRegistry,
    pipeline: vk::Pipeline,
    pipeline_alpha: vk::Pipeline,
    pipeline_two_sided: vk::Pipeline,
    pipeline_alpha_two_sided: vk::Pipeline,
    pipeline_ui: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    vert_module: vk::ShaderModule,
    frag_module: vk::ShaderModule,
    ui_vert_module: vk::ShaderModule,
    ui_frag_module: vk::ShaderModule,
    /// Mesh handle for the fullscreen quad used by UI overlay.
    pub ui_quad_handle: Option<u32>,
    render_pass: vk::RenderPass,
    swapchain_state: SwapchainState,

    pub allocator: Option<SharedAllocator>,

    pub graphics_queue: vk::Queue,
    pub present_queue: vk::Queue,
    pub queue_indices: QueueFamilyIndices,
    pub device: ash::Device,
    pub physical_device: vk::PhysicalDevice,

    surface: vk::SurfaceKHR,
    surface_loader: ash::khr::surface::Instance,

    debug_messenger: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,

    pub instance: ash::Instance,
    pub entry: ash::Entry,
}

impl VulkanContext {
    /// Full Vulkan initialization chain:
    /// 1. Load Vulkan entry points
    /// 2. Create instance + validation layers
    /// 3. Set up debug messenger
    /// 4. Create surface
    /// 5. Pick physical device
    /// 6. Create logical device + queues
    /// 7. Create swapchain
    /// 8. Create render pass
    /// 9. Create framebuffers
    /// 10. Create command pool + command buffers
    /// 11. Create synchronization objects
    pub fn new(
        display_handle: RawDisplayHandle,
        window_handle: RawWindowHandle,
        window_size: [u32; 2],
    ) -> Result<Self> {
        // 1. Entry
        let entry = unsafe { ash::Entry::load().context("Failed to load Vulkan loader")? };
        log::info!("Vulkan loader ready");

        // 2. Instance
        let vk_instance = instance::create_instance(&entry, display_handle)?;

        // 3. Debug messenger
        let debug_messenger = if cfg!(debug_assertions) {
            Some(debug::create_debug_messenger(&vk_instance, &entry)?)
        } else {
            None
        };

        // 4. Surface
        let surface_loader = ash::khr::surface::Instance::new(&entry, &vk_instance);
        let vk_surface =
            surface::create_surface(&entry, &vk_instance, display_handle, window_handle)?;

        // 5. Physical device
        let (physical_device, queue_indices) =
            device::pick_physical_device(&vk_instance, &surface_loader, vk_surface)?;

        // 6. Logical device + queues
        let (device, graphics_queue, present_queue) =
            device::create_logical_device(&vk_instance, physical_device, queue_indices)?;

        // 7. GPU allocator
        let gpu_allocator =
            allocator::create_allocator(&vk_instance, &device, physical_device)?;

        // 8. Swapchain
        let swapchain_state = swapchain::create_swapchain(
            &vk_instance,
            &device,
            physical_device,
            &surface_loader,
            vk_surface,
            queue_indices,
            window_size,
        )?;

        // 8. Depth resources
        let (depth_image, depth_image_view, depth_allocation) =
            create_depth_resources(&device, &gpu_allocator, swapchain_state.extent)?;

        // 9. Render pass (color + depth)
        let render_pass = create_render_pass(&device, swapchain_state.format.format)?;

        // 10. Command pool (needed for texture upload one-time commands)
        let command_pool = create_command_pool(&device, queue_indices.graphics)?;

        // 11. Texture registry with checkerboard fallback
        let checkerboard = super::texture::generate_checkerboard(256, 256, 32);
        let fallback_texture = Texture::from_rgba(
            &device,
            &gpu_allocator,
            graphics_queue,
            command_pool,
            256,
            256,
            &checkerboard,
        )?;
        let texture_registry = TextureRegistry::new(
            &device,
            swapchain_state.images.len() as u32,
            1024,
            fallback_texture,
        )?;

        // 12. Graphics pipeline (with depth test + descriptor set layout)
        let pipelines = pipeline::create_triangle_pipeline(
                &device,
                render_pass,
                swapchain_state.extent,
                texture_registry.descriptor_set_layout,
            )?;

        // 13. UI overlay pipeline (no depth, alpha blend, passthrough shaders)
        let (pipeline_ui, ui_vert_module, ui_frag_module) = pipeline::create_ui_pipeline(
            &device,
            render_pass,
            swapchain_state.extent,
            pipelines.layout,
        )?;

        // 14. Mesh registry (empty — meshes uploaded by the application)
        let mesh_registry = MeshRegistry::new();

        // 15. Framebuffers (color + depth attachments)
        let framebuffers =
            create_framebuffers(&device, render_pass, &swapchain_state, depth_image_view)?;

        // 16. Command buffers
        let command_buffers =
            allocate_command_buffers(&device, command_pool, swapchain_state.images.len())?;

        // 17. Sync objects
        let frame_sync =
            sync::create_sync_objects(&device, swapchain_state.images.len())?;

        log::info!("Vulkan context fully initialized");

        Ok(Self {
            entry,
            instance: vk_instance,
            debug_messenger,
            surface_loader,
            surface: vk_surface,
            physical_device,
            device,
            queue_indices,
            graphics_queue,
            present_queue,
            swapchain_state,
            allocator: Some(gpu_allocator),
            render_pass,
            pipeline: pipelines.opaque,
            pipeline_alpha: pipelines.alpha,
            pipeline_two_sided: pipelines.opaque_two_sided,
            pipeline_alpha_two_sided: pipelines.alpha_two_sided,
            pipeline_ui,
            pipeline_layout: pipelines.layout,
            vert_module: pipelines.vert_module,
            frag_module: pipelines.frag_module,
            ui_vert_module,
            ui_frag_module,
            ui_quad_handle: None,
            mesh_registry,
            texture_registry,
            depth_allocation: Some(depth_allocation),
            depth_image,
            depth_image_view,
            framebuffers,
            command_pool,
            command_buffers,
            frame_sync,
            current_frame: 0,
        })
    }

    /// Record and submit a frame.
    ///
    /// `view_proj`: combined view-projection matrix as column-major [f32; 16].
    /// `draw_commands`: per-object (mesh_handle, model_matrix) pairs.
    pub fn draw_frame(
        &mut self,
        clear_color: [f32; 4],
        view_proj: &[f32; 16],
        draw_commands: &[DrawCommand],
        ui_texture_handle: Option<u32>,
    ) -> Result<bool> {
        let frame = self.current_frame;

        // Wait for this frame-in-flight slot to be available.
        unsafe {
            self.device
                .wait_for_fences(&[self.frame_sync.in_flight[frame]], true, u64::MAX)
                .context("wait_for_fences")?;
        }

        // Acquire next swapchain image. Use the image-indexed semaphore
        // for acquisition — we don't know the image index yet, so use
        // frame index for the acquire semaphore (it's only waited on by
        // our own submit, not by the present engine).
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

        unsafe {
            self.device.cmd_begin_render_pass(
                cmd,
                &render_pass_begin,
                vk::SubpassContents::INLINE,
            );

            // Bind the graphics pipeline.
            self.device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
            );

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
            let view_proj_bytes: &[u8] = std::slice::from_raw_parts(
                view_proj.as_ptr() as *const u8,
                64,
            );
            self.device.cmd_push_constants(
                cmd,
                self.pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                view_proj_bytes,
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
                            (true,  false) => self.pipeline_alpha,
                            (false, true)  => self.pipeline_two_sided,
                            (true,  true)  => self.pipeline_alpha_two_sided,
                        };
                        self.device.cmd_bind_pipeline(
                            cmd,
                            vk::PipelineBindPoint::GRAPHICS,
                            pipe,
                        );
                        last_pipeline_key = pipeline_key;
                        // Force rebind of texture after pipeline switch.
                        last_texture = u32::MAX;
                    }

                    // Bind texture descriptor set (skip if same as previous draw).
                    if draw_cmd.texture_handle != last_texture {
                        let desc_set = self.texture_registry.descriptor_set(
                            draw_cmd.texture_handle,
                            image_index as usize,
                        );
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

                    // Push model matrix (bytes 64..128).
                    let model_bytes: &[u8] = std::slice::from_raw_parts(
                        draw_cmd.model_matrix.as_ptr() as *const u8,
                        64,
                    );
                    self.device.cmd_push_constants(
                        cmd,
                        self.pipeline_layout,
                        vk::ShaderStageFlags::VERTEX,
                        64,
                        model_bytes,
                    );

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
                    self.device.cmd_draw_indexed(cmd, mesh.index_count, 1, 0, 0, 0);
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

                    let desc_set = self.texture_registry.descriptor_set(
                        ui_tex,
                        image_index as usize,
                    );
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
                        1.0, 0.0, 0.0, 0.0,
                        0.0, 1.0, 0.0, 0.0,
                        0.0, 0.0, 1.0, 0.0,
                        0.0, 0.0, 0.0, 1.0,
                    ];
                    let identity_bytes: &[u8] = std::slice::from_raw_parts(
                        identity.as_ptr() as *const u8,
                        64,
                    );
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
                    self.device.cmd_draw_indexed(cmd, mesh.index_count, 1, 0, 0, 0);
                }
            }

            self.device.cmd_end_render_pass(cmd);
            self.device
                .end_command_buffer(cmd)
                .context("end_command_buffer")?;
        }

        // Submit.
        // Wait on the frame-indexed acquire semaphore.
        // Signal the image-indexed render_finished semaphore — this is
        // what the present engine waits on, and it holds it until the
        // image is re-acquired. By indexing per image, we guarantee
        // the semaphore isn't reused until this specific image comes back.
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
            self.device
                .queue_submit(
                    self.graphics_queue,
                    &[submit_info],
                    self.frame_sync.in_flight[frame],
                )
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
            match self
                .swapchain_state
                .swapchain_loader
                .queue_present(self.present_queue, &present_info)
            {
                Ok(suboptimal) => suboptimal,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => true,
                Err(e) => anyhow::bail!("queue_present: {:?}", e),
            }
        };

        self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;

        Ok(suboptimal || present_suboptimal)
    }

    /// Register the fullscreen quad mesh for UI overlay rendering.
    /// Call this once after creating the context.
    pub fn register_ui_quad(&mut self) -> Result<()> {
        let (vertices, indices) = crate::mesh::fullscreen_quad_vertices();
        let allocator = self.allocator.as_ref().expect("allocator missing");
        let handle = self.mesh_registry.upload(
            &self.device,
            allocator,
            &vertices,
            &indices,
        )?;
        self.ui_quad_handle = Some(handle);
        log::info!("UI fullscreen quad registered (mesh handle {})", handle);
        Ok(())
    }

    /// Get the current swapchain extent (viewport dimensions).
    pub fn swapchain_extent(&self) -> (u32, u32) {
        (self.swapchain_state.extent.width, self.swapchain_state.extent.height)
    }

    /// Recreate the swapchain after a resize or suboptimal present.
    pub fn recreate_swapchain(&mut self, window_size: [u32; 2]) -> Result<()> {
        unsafe {
            self.device.device_wait_idle().context("device_wait_idle")?;
        }

        // Destroy old framebuffers, depth resources, render pass, swapchain views.
        unsafe {
            for &fb in &self.framebuffers {
                self.device.destroy_framebuffer(fb, None);
            }
            // Depth: view → free allocation → destroy image.
            self.device.destroy_image_view(self.depth_image_view, None);
            if let Some(alloc) = self.depth_allocation.take() {
                self.allocator
                    .as_ref()
                    .expect("allocator missing during resize")
                    .lock()
                    .expect("allocator lock poisoned")
                    .free(alloc)
                    .expect("Failed to free depth allocation");
            }
            self.device.destroy_image(self.depth_image, None);

            self.device.destroy_render_pass(self.render_pass, None);
            self.swapchain_state.destroy(&self.device);
        }

        self.swapchain_state = swapchain::create_swapchain(
            &self.instance,
            &self.device,
            self.physical_device,
            &self.surface_loader,
            self.surface,
            self.queue_indices,
            window_size,
        )?;

        let (depth_image, depth_image_view, depth_allocation) = create_depth_resources(
            &self.device,
            self.allocator.as_ref().expect("allocator missing"),
            self.swapchain_state.extent,
        )?;
        self.depth_image = depth_image;
        self.depth_image_view = depth_image_view;
        self.depth_allocation = Some(depth_allocation);

        self.render_pass =
            create_render_pass(&self.device, self.swapchain_state.format.format)?;

        // Recreate descriptor sets for existing textures (new swapchain image count).
        self.texture_registry.recreate_descriptor_sets(
            &self.device,
            self.swapchain_state.images.len() as u32,
        )?;

        self.framebuffers = create_framebuffers(
            &self.device,
            self.render_pass,
            &self.swapchain_state,
            self.depth_image_view,
        )?;

        // Reallocate command buffers if image count changed.
        unsafe {
            self.device
                .free_command_buffers(self.command_pool, &self.command_buffers);
        }
        self.command_buffers = allocate_command_buffers(
            &self.device,
            self.command_pool,
            self.swapchain_state.images.len(),
        )?;

        // Reset per-image fence tracking for the new swapchain.
        self.frame_sync
            .reset_image_fences(self.swapchain_state.images.len());

        log::info!(
            "Swapchain recreated: {}x{}",
            self.swapchain_state.extent.width,
            self.swapchain_state.extent.height
        );
        Ok(())
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();

            self.frame_sync.destroy(&self.device);
            self.device
                .free_command_buffers(self.command_pool, &self.command_buffers);
            self.device.destroy_command_pool(self.command_pool, None);
            for &fb in &self.framebuffers {
                self.device.destroy_framebuffer(fb, None);
            }
            // Destroy texture registry (all textures + descriptor pool/layout).
            if let Some(ref alloc) = self.allocator {
                self.texture_registry.destroy(&self.device, alloc);
            }

            // Destroy depth resources before the allocator.
            self.device.destroy_image_view(self.depth_image_view, None);
            if let Some(alloc) = self.depth_allocation.take() {
                if let Some(ref allocator) = self.allocator {
                    allocator
                        .lock()
                        .expect("allocator lock poisoned")
                        .free(alloc)
                        .expect("Failed to free depth allocation");
                }
            }
            self.device.destroy_image(self.depth_image, None);

            if let Some(ref alloc) = self.allocator {
                self.mesh_registry.destroy_all(&self.device, alloc);
            }
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline(self.pipeline_alpha, None);
            self.device.destroy_pipeline(self.pipeline_two_sided, None);
            self.device.destroy_pipeline(self.pipeline_alpha_two_sided, None);
            self.device.destroy_pipeline(self.pipeline_ui, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_shader_module(self.vert_module, None);
            self.device.destroy_shader_module(self.frag_module, None);
            self.device.destroy_shader_module(self.ui_vert_module, None);
            self.device.destroy_shader_module(self.ui_frag_module, None);
            self.device.destroy_render_pass(self.render_pass, None);
            self.swapchain_state.destroy(&self.device);
            // Drop the allocator before destroying the device.
            // take() extracts from Option, then try_unwrap gets the inner
            // Mutex if we hold the last Arc, then into_inner gives us the
            // Allocator which we drop — running its cleanup while the device
            // is still alive.
            if let Some(alloc_arc) = self.allocator.take() {
                if let Ok(mutex) = std::sync::Arc::try_unwrap(alloc_arc) {
                    drop(mutex.into_inner().expect("allocator lock poisoned"));
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

// ── Helper functions ────────────────────────────────────────────────────

fn create_render_pass(device: &ash::Device, color_format: vk::Format) -> Result<vk::RenderPass> {
    let color_attachment = vk::AttachmentDescription::default()
        .format(color_format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

    let depth_attachment = vk::AttachmentDescription::default()
        .format(DEPTH_FORMAT)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::DONT_CARE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);

    let color_ref = vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    };
    let color_refs = [color_ref];

    let depth_ref = vk::AttachmentReference {
        attachment: 1,
        layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
    };

    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_refs)
        .depth_stencil_attachment(&depth_ref);

    let dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
        )
        .src_access_mask(vk::AccessFlags::empty())
        .dst_stage_mask(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
        )
        .dst_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        );

    let attachments = [color_attachment, depth_attachment];
    let subpasses = [subpass];
    let dependencies = [dependency];

    let create_info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(&subpasses)
        .dependencies(&dependencies);

    let render_pass = unsafe {
        device
            .create_render_pass(&create_info, None)
            .context("Failed to create render pass")?
    };

    log::info!("Render pass created (color + depth)");
    Ok(render_pass)
}

fn create_framebuffers(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    swapchain: &SwapchainState,
    depth_view: vk::ImageView,
) -> Result<Vec<vk::Framebuffer>> {
    swapchain
        .image_views
        .iter()
        .map(|&view| {
            let attachments = [view, depth_view];
            let create_info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&attachments)
                .width(swapchain.extent.width)
                .height(swapchain.extent.height)
                .layers(1);

            unsafe {
                device
                    .create_framebuffer(&create_info, None)
                    .context("Failed to create framebuffer")
            }
        })
        .collect()
}

/// Create the depth image, view, and allocation.
fn create_depth_resources(
    device: &ash::Device,
    allocator: &SharedAllocator,
    extent: vk::Extent2D,
) -> Result<(vk::Image, vk::ImageView, vk_alloc::Allocation)> {
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(DEPTH_FORMAT)
        .extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);

    let image = unsafe {
        device
            .create_image(&image_info, None)
            .context("Failed to create depth image")?
    };

    let requirements = unsafe { device.get_image_memory_requirements(image) };

    let allocation = allocator
        .lock()
        .expect("allocator lock poisoned")
        .allocate(&vk_alloc::AllocationCreateDesc {
            name: "depth_buffer",
            requirements,
            location: MemoryLocation::GpuOnly,
            linear: false,
            allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
        })
        .context("Failed to allocate depth image memory")?;

    unsafe {
        device
            .bind_image_memory(image, allocation.memory(), allocation.offset())
            .context("Failed to bind depth image memory")?;
    }

    let view_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(DEPTH_FORMAT)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });

    let view = unsafe {
        device
            .create_image_view(&view_info, None)
            .context("Failed to create depth image view")?
    };

    log::info!(
        "Depth buffer created: {}x{} D32_SFLOAT",
        extent.width,
        extent.height
    );
    Ok((image, view, allocation))
}

fn create_command_pool(device: &ash::Device, queue_family: u32) -> Result<vk::CommandPool> {
    let create_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(queue_family)
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);

    let pool = unsafe {
        device
            .create_command_pool(&create_info, None)
            .context("Failed to create command pool")?
    };

    log::info!("Command pool created");
    Ok(pool)
}

fn allocate_command_buffers(
    device: &ash::Device,
    pool: vk::CommandPool,
    count: usize,
) -> Result<Vec<vk::CommandBuffer>> {
    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(count as u32);

    let buffers = unsafe {
        device
            .allocate_command_buffers(&alloc_info)
            .context("Failed to allocate command buffers")?
    };

    log::info!("{} command buffers allocated", count);
    Ok(buffers)
}
