//! Top-level Vulkan context that owns the entire graphics state.

use super::allocator::{self, SharedAllocator};
use super::buffer::GpuBuffer;
use super::debug;
use super::device::{self, QueueFamilyIndices};
use super::instance;
use super::pipeline;
use super::surface;
use super::swapchain::{self, SwapchainState};
use super::sync::{self, FrameSync, MAX_FRAMES_IN_FLIGHT};
use crate::vertex::Vertex;
use anyhow::{Context, Result};
use ash::vk;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

/// Triangle vertex data — same RGB corners as Phase 1, now from Rust.
const TRIANGLE_VERTICES: [Vertex; 3] = [
    Vertex::new([0.0, -0.5, 0.0], [1.0, 0.0, 0.0]), // top — red
    Vertex::new([0.5, 0.5, 0.0], [0.0, 1.0, 0.0]),   // bottom-right — green
    Vertex::new([-0.5, 0.5, 0.0], [0.0, 0.0, 1.0]),  // bottom-left — blue
];

const TRIANGLE_INDICES: [u32; 3] = [0, 1, 2];

pub struct VulkanContext {
    // Ordered for drop safety — later fields are destroyed first.
    pub current_frame: usize,

    frame_sync: FrameSync,
    command_buffers: Vec<vk::CommandBuffer>,
    command_pool: vk::CommandPool,
    framebuffers: Vec<vk::Framebuffer>,
    index_buffer: GpuBuffer,
    vertex_buffer: GpuBuffer,
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    vert_module: vk::ShaderModule,
    frag_module: vk::ShaderModule,
    render_pass: vk::RenderPass,
    swapchain_state: SwapchainState,

    allocator: Option<SharedAllocator>,

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

        // 8. Render pass
        let render_pass = create_render_pass(&device, swapchain_state.format.format)?;

        // 9. Graphics pipeline
        let (pipeline_handle, pipeline_layout, vert_module, frag_module) =
            pipeline::create_triangle_pipeline(&device, render_pass, swapchain_state.extent)?;

        // 10. Vertex + index buffers
        let vertex_buffer =
            GpuBuffer::create_vertex_buffer(&device, &gpu_allocator, &TRIANGLE_VERTICES)?;
        let index_buffer =
            GpuBuffer::create_index_buffer(&device, &gpu_allocator, &TRIANGLE_INDICES)?;
        log::info!("Vertex + index buffers created");

        // 11. Framebuffers
        let framebuffers =
            create_framebuffers(&device, render_pass, &swapchain_state)?;

        // 12. Command pool + buffers
        let command_pool = create_command_pool(&device, queue_indices.graphics)?;
        let command_buffers =
            allocate_command_buffers(&device, command_pool, swapchain_state.images.len())?;

        // 13. Sync objects
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
            pipeline: pipeline_handle,
            pipeline_layout,
            vert_module,
            frag_module,
            vertex_buffer,
            index_buffer,
            framebuffers,
            command_pool,
            command_buffers,
            frame_sync,
            current_frame: 0,
        })
    }

    /// Record and submit a frame that clears to the given color.
    pub fn draw_clear_frame(&mut self, clear_color: [f32; 4]) -> Result<bool> {
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

        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: clear_color,
            },
        }];

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

            // Bind vertex and index buffers, draw indexed.
            self.device
                .cmd_bind_vertex_buffers(cmd, 0, &[self.vertex_buffer.buffer], &[0]);
            self.device.cmd_bind_index_buffer(
                cmd,
                self.index_buffer.buffer,
                0,
                vk::IndexType::UINT32,
            );
            self.device.cmd_draw_indexed(cmd, TRIANGLE_INDICES.len() as u32, 1, 0, 0, 0);

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

    /// Recreate the swapchain after a resize or suboptimal present.
    pub fn recreate_swapchain(&mut self, window_size: [u32; 2]) -> Result<()> {
        unsafe {
            self.device.device_wait_idle().context("device_wait_idle")?;
        }

        // Destroy old framebuffers, render pass, swapchain views.
        unsafe {
            for &fb in &self.framebuffers {
                self.device.destroy_framebuffer(fb, None);
            }
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

        self.render_pass =
            create_render_pass(&self.device, self.swapchain_state.format.format)?;
        self.framebuffers =
            create_framebuffers(&self.device, self.render_pass, &self.swapchain_state)?;

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
            if let Some(ref alloc) = self.allocator {
                self.vertex_buffer.destroy(&self.device, alloc);
                self.index_buffer.destroy(&self.device, alloc);
            }
            self.device.destroy_pipeline(self.pipeline, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_shader_module(self.vert_module, None);
            self.device.destroy_shader_module(self.frag_module, None);
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
    let attachment = vk::AttachmentDescription::default()
        .format(color_format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

    let color_ref = vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    };
    let color_refs = [color_ref];

    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_refs);

    let dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);

    let attachments = [attachment];
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

    log::info!("Render pass created");
    Ok(render_pass)
}

fn create_framebuffers(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    swapchain: &SwapchainState,
) -> Result<Vec<vk::Framebuffer>> {
    swapchain
        .image_views
        .iter()
        .map(|&view| {
            let attachments = [view];
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
