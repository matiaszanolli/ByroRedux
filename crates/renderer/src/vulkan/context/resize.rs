//! Swapchain recreation after window resize or suboptimal present.

use super::super::composite::HDR_FORMAT;
use super::super::gbuffer::{
    ALBEDO_FORMAT, MESH_ID_FORMAT, MOTION_FORMAT, NORMAL_FORMAT, RAW_INDIRECT_FORMAT,
};
use super::super::ssao::SsaoPipeline;
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::super::{pipeline, swapchain};
use super::helpers::{create_depth_resources, create_main_framebuffers, create_render_pass};
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
        // Handles are nulled after destruction so that if a later creation
        // step fails and Drop runs, the destroy calls are no-ops (Vulkan
        // spec: vkDestroy* on VK_NULL_HANDLE is always valid).
        unsafe {
            for &fb in &self.framebuffers {
                self.device.destroy_framebuffer(fb, None);
            }
            self.framebuffers.clear();

            // Depth: view → image → free allocation. The image must be
            // destroyed while its bound memory is still valid (Vulkan spec
            // VUID-vkFreeMemory-memory-00677).
            self.device.destroy_image_view(self.depth_image_view, None);
            self.depth_image_view = vk::ImageView::null();
            self.device.destroy_image(self.depth_image, None);
            self.depth_image = vk::Image::null();
            if let Some(alloc) = self.depth_allocation.take() {
                self.allocator
                    .as_ref()
                    .expect("allocator missing during resize")
                    .lock()
                    .expect("allocator lock poisoned")
                    .free(alloc)
                    .expect("Failed to free depth allocation");
            }

            // Destroy old pipelines before the render pass they reference.
            self.device.destroy_pipeline(self.pipeline, None);
            self.pipeline = vk::Pipeline::null();
            self.device.destroy_pipeline(self.pipeline_alpha, None);
            self.pipeline_alpha = vk::Pipeline::null();
            self.device.destroy_pipeline(self.pipeline_additive, None);
            self.pipeline_additive = vk::Pipeline::null();
            self.device.destroy_pipeline(self.pipeline_two_sided, None);
            self.pipeline_two_sided = vk::Pipeline::null();
            self.device
                .destroy_pipeline(self.pipeline_alpha_two_sided, None);
            self.pipeline_alpha_two_sided = vk::Pipeline::null();
            self.device
                .destroy_pipeline(self.pipeline_additive_two_sided, None);
            self.pipeline_additive_two_sided = vk::Pipeline::null();
            self.device.destroy_pipeline(self.pipeline_ui, None);
            self.pipeline_ui = vk::Pipeline::null();

            self.device.destroy_render_pass(self.render_pass, None);
            self.render_pass = vk::RenderPass::null();
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

        // Main render pass: 6 color (HDR + G-buffer + raw_indirect + albedo) + depth.
        self.render_pass = create_render_pass(
            &self.device,
            HDR_FORMAT,
            NORMAL_FORMAT,
            MOTION_FORMAT,
            MESH_ID_FORMAT,
            RAW_INDIRECT_FORMAT,
            ALBEDO_FORMAT,
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
        self.pipeline_additive = pipelines.additive;
        self.pipeline_two_sided = pipelines.opaque_two_sided;
        self.pipeline_alpha_two_sided = pipelines.alpha_two_sided;
        self.pipeline_additive_two_sided = pipelines.additive_two_sided;

        self.pipeline_ui = pipeline::create_ui_pipeline(
            &self.device,
            self.render_pass,
            self.swapchain_state.extent,
            self.pipeline_layout,
            self.pipeline_cache,
        )?;

        // Recreate descriptor sets for existing textures (new swapchain image count).
        self.texture_registry
            .recreate_descriptor_sets(&self.device, self.swapchain_state.images.len() as u32)?;

        // Recreate SSAO pipeline with the new depth image view and dimensions.
        // The old pipeline's descriptor sets still reference the destroyed depth
        // image view (VUID-VkDescriptorImageInfo-imageView-parameter), so we
        // must destroy and rebuild it. The scene descriptor set binding 7
        // (aoTexture) is also re-written to point at the new AO image.
        if let Some(ref mut old_ssao) = self.ssao {
            let allocator = self
                .allocator
                .as_ref()
                .expect("allocator missing during resize");
            unsafe { old_ssao.destroy(&self.device, allocator) };
            self.ssao = None;
            match SsaoPipeline::new(
                &self.device,
                allocator,
                self.depth_image_view,
                self.swapchain_state.extent.width,
                self.swapchain_state.extent.height,
            ) {
                Ok(new_ssao) => {
                    // Transition AO image to valid layout before first use.
                    if let Err(e) = unsafe {
                        new_ssao.initialize_ao_images(
                            &self.device,
                            &self.graphics_queue,
                            self.transfer_pool,
                        )
                    } {
                        log::warn!("SSAO AO image init failed after resize: {e}");
                    }
                    for f in 0..MAX_FRAMES_IN_FLIGHT {
                        self.scene_buffers.write_ao_texture(
                            &self.device,
                            f,
                            new_ssao.ao_image_views[f],
                            new_ssao.ao_sampler,
                        );
                    }
                    self.ssao = Some(new_ssao);
                }
                Err(e) => {
                    log::warn!("SSAO recreation failed after resize: {e} — no ambient occlusion");
                }
            }
        }

        // Recreate G-buffer images FIRST (they're referenced by composite
        // descriptor sets, which we'll rewrite during composite recreation).
        if let Some(ref mut gbuffer) = self.gbuffer {
            gbuffer.recreate_on_resize(
                &self.device,
                self.allocator
                    .as_ref()
                    .expect("allocator missing during resize"),
                self.swapchain_state.extent.width,
                self.swapchain_state.extent.height,
            )?;
            // New images start UNDEFINED — transition to SHADER_READ_ONLY so
            // the "prev" frame slot is valid on the first frame after resize.
            if let Err(e) = unsafe {
                gbuffer.initialize_layouts(&self.device, &self.graphics_queue, self.transfer_pool)
            } {
                log::warn!("G-buffer post-resize layout init failed: {e}");
            }
        }

        // Collect fresh G-buffer views before we borrow &mut self.svgf /
        // self.composite. Motion and mesh_id are needed by SVGF.
        let (raw_indirect_views, motion_views_in, mesh_id_views_in, albedo_views) = {
            let gbuffer_ref = self
                .gbuffer
                .as_ref()
                .expect("gbuffer must exist during resize");
            let n = MAX_FRAMES_IN_FLIGHT;
            let ri: Vec<vk::ImageView> = (0..n).map(|i| gbuffer_ref.raw_indirect_view(i)).collect();
            let mo: Vec<vk::ImageView> = (0..n).map(|i| gbuffer_ref.motion_view(i)).collect();
            let mi: Vec<vk::ImageView> = (0..n).map(|i| gbuffer_ref.mesh_id_view(i)).collect();
            let ab: Vec<vk::ImageView> = (0..n).map(|i| gbuffer_ref.albedo_view(i)).collect();
            (ri, mo, mi, ab)
        };

        // Recreate SVGF history images + rewrite its descriptor sets
        // against the new G-buffer views. Must happen before composite
        // (whose descriptor sets reference SVGF's indirect_view).
        if let Some(ref mut svgf) = self.svgf {
            svgf.recreate_on_resize(
                &self.device,
                self.allocator
                    .as_ref()
                    .expect("allocator missing during resize"),
                &raw_indirect_views,
                &motion_views_in,
                &mesh_id_views_in,
                self.swapchain_state.extent.width,
                self.swapchain_state.extent.height,
            )?;
            // Re-transition the fresh history images to GENERAL.
            if let Err(e) = unsafe {
                svgf.initialize_layouts(&self.device, &self.graphics_queue, self.transfer_pool)
            } {
                log::warn!("SVGF layout re-init after resize failed: {e}");
            }
        }

        // Choose the indirect source for composite: SVGF accumulated (in
        // GENERAL layout) if available, else raw G-buffer indirect.
        let (composite_indirect_views, indirect_is_general): (Vec<vk::ImageView>, bool) =
            if let Some(ref s) = self.svgf {
                let n = MAX_FRAMES_IN_FLIGHT;
                ((0..n).map(|i| s.indirect_view(i)).collect(), true)
            } else {
                (raw_indirect_views.clone(), false)
            };

        // Recreate composite pipeline's HDR images + swapchain framebuffers
        // with the new extent. Also rewrites descriptor sets to point at
        // the new indirect + albedo views.
        if let Some(ref mut composite) = self.composite {
            composite.recreate_on_resize(
                &self.device,
                self.allocator
                    .as_ref()
                    .expect("allocator missing during resize"),
                &self.swapchain_state.image_views,
                &composite_indirect_views,
                indirect_is_general,
                &albedo_views,
                self.depth_image_view,
                self.swapchain_state.extent.width,
                self.swapchain_state.extent.height,
            )?;
        }

        // Main framebuffers bind the new HDR + G-buffer views + depth.
        let composite_ref = self
            .composite
            .as_ref()
            .expect("composite must exist during resize");
        let gbuffer_ref = self
            .gbuffer
            .as_ref()
            .expect("gbuffer must exist during resize");
        let hdr_views = &composite_ref.hdr_image_views;
        let n = hdr_views.len();
        let normal_views: Vec<vk::ImageView> = (0..n).map(|i| gbuffer_ref.normal_view(i)).collect();
        let motion_views: Vec<vk::ImageView> = (0..n).map(|i| gbuffer_ref.motion_view(i)).collect();
        let mesh_id_views: Vec<vk::ImageView> =
            (0..n).map(|i| gbuffer_ref.mesh_id_view(i)).collect();
        self.framebuffers = create_main_framebuffers(
            &self.device,
            self.render_pass,
            hdr_views,
            &normal_views,
            &motion_views,
            &mesh_id_views,
            &raw_indirect_views,
            &albedo_views,
            self.depth_image_view,
            self.swapchain_state.extent,
        )?;

        // Command buffers are per frame-in-flight (fixed count), so they
        // don't need reallocation on swapchain resize. They'll be reset
        // before re-recording on the next draw_frame. See #259.

        // Recreate per-image semaphores and fence tracking for the new swapchain.
        unsafe {
            self.frame_sync
                .recreate_for_swapchain(&self.device, self.swapchain_state.images.len())?;
        }

        // Reset frame-in-flight counter so the first post-resize frame
        // starts from slot 0 with a clean fence/semaphore cycle.
        self.current_frame = 0;

        log::info!(
            "Swapchain recreated: {}x{}",
            self.swapchain_state.extent.width,
            self.swapchain_state.extent.height
        );
        Ok(())
    }
}
