//! Swapchain recreation after window resize or suboptimal present.

use super::super::{pipeline, swapchain};
use super::helpers::{
    allocate_command_buffers, create_depth_resources, create_framebuffers, create_render_pass,
};
use super::VulkanContext;
use anyhow::{Context, Result};
use ash::vk;

impl VulkanContext {
    /// Recreate the swapchain after a resize or suboptimal present.
    pub fn recreate_swapchain(&mut self, window_size: [u32; 2]) -> Result<()> {
        unsafe {
            self.device.device_wait_idle().context("device_wait_idle")?;
        }

        // Destroy old framebuffers, depth resources, swapchain views.
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

            // Destroy old pipelines before the render pass they reference.
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline(self.pipeline_alpha, None);
            self.device.destroy_pipeline(self.pipeline_two_sided, None);
            self.device
                .destroy_pipeline(self.pipeline_alpha_two_sided, None);
            self.device.destroy_pipeline(self.pipeline_ui, None);

            self.device.destroy_render_pass(self.render_pass, None);
            // Destroy old image views (but keep the old swapchain handle for handoff).
            for &view in &self.swapchain_state.image_views {
                self.device.destroy_image_view(view, None);
            }
        }

        let old_swapchain = self.swapchain_state.swapchain;

        self.swapchain_state = swapchain::create_swapchain(
            &self.instance,
            &self.device,
            self.physical_device,
            &self.surface_loader,
            self.surface,
            self.queue_indices,
            window_size,
            old_swapchain, // atomic handoff — avoids flicker during resize
        )?;

        // Destroy the retired old swapchain now that the new one is active.
        if old_swapchain != vk::SwapchainKHR::null() {
            unsafe {
                self.swapchain_state
                    .swapchain_loader
                    .destroy_swapchain(old_swapchain, None);
            }
        }

        let (depth_image, depth_image_view, depth_allocation) = create_depth_resources(
            &self.device,
            self.allocator.as_ref().expect("allocator missing"),
            self.swapchain_state.extent,
            self.depth_format,
        )?;
        self.depth_image = depth_image;
        self.depth_image_view = depth_image_view;
        self.depth_allocation = Some(depth_allocation);

        self.render_pass = create_render_pass(
            &self.device,
            self.swapchain_state.format.format,
            self.depth_format,
        )?;

        // Recreate pipelines against the new render pass, reusing existing layout.
        let pipelines = pipeline::recreate_triangle_pipelines(
            &self.device,
            self.render_pass,
            self.swapchain_state.extent,
            self.pipeline_cache,
            self.pipeline_layout,
        )?;
        self.pipeline = pipelines.opaque;
        self.pipeline_alpha = pipelines.alpha;
        self.pipeline_two_sided = pipelines.opaque_two_sided;
        self.pipeline_alpha_two_sided = pipelines.alpha_two_sided;

        self.pipeline_ui = pipeline::recreate_ui_pipeline(
            &self.device,
            self.render_pass,
            self.swapchain_state.extent,
            self.pipeline_layout,
            self.pipeline_cache,
        )?;

        // Recreate descriptor sets for existing textures (new swapchain image count).
        self.texture_registry
            .recreate_descriptor_sets(&self.device, self.swapchain_state.images.len() as u32)?;

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

        // Recreate per-image semaphores and fence tracking for the new swapchain.
        unsafe {
            self.frame_sync
                .recreate_for_swapchain(&self.device, self.swapchain_state.images.len())?;
        }

        log::info!(
            "Swapchain recreated: {}x{}",
            self.swapchain_state.extent.width,
            self.swapchain_state.extent.height
        );
        Ok(())
    }
}
