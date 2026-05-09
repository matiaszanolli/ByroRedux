//! Swapchain recreation after window resize or suboptimal present.

use super::super::composite::HDR_FORMAT;
use super::super::gbuffer::{
    ALBEDO_FORMAT, MESH_ID_FORMAT, MOTION_FORMAT, NORMAL_FORMAT, RAW_INDIRECT_FORMAT,
};
use super::super::ssao::SsaoPipeline;
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::super::{pipeline, swapchain};
use super::helpers::{
    create_depth_resources, create_main_framebuffers, create_render_pass,
    destroy_depth_resources, destroy_main_framebuffers, destroy_render_pass_pipelines,
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

        // Capture the old swapchain format BEFORE recreation so the
        // post-recreate comparison can decide whether to keep the
        // render pass + rasterization pipelines. They depend on
        // attachment formats (HDR_FORMAT / G-buffer / depth — all
        // stable across resize) but bind to the render pass, so the
        // rebuild is only required when the swapchain surface format
        // changes (HDR toggle, monitor swap, etc.). Pre-#576 every
        // resize destroyed and rebuilt them unconditionally — drag-
        // resize stalled on pipeline recompilation. See PIPE-2.
        let old_swapchain_format = self.swapchain_state.format;

        // Destroy old framebuffers, depth resources, swapchain views.
        // Handles are nulled after destruction so that if a later creation
        // step fails and Drop runs, the destroy calls are no-ops (Vulkan
        // spec: vkDestroy* on VK_NULL_HANDLE is always valid). The
        // framebuffer + depth steps are encoded once in helpers.rs so
        // Drop and resize stay in lockstep — see #33 / R-10.
        unsafe {
            destroy_main_framebuffers(&self.device, &mut self.framebuffers);

            destroy_depth_resources(
                &self.device,
                self.allocator
                    .as_ref()
                    .expect("allocator missing during resize"),
                &mut self.depth_image_view,
                &mut self.depth_image,
                &mut self.depth_allocation,
            );

            // NOTE: pipeline + render pass destruction is deferred
            // until after we know the new swapchain format. The
            // existing comment block below the recreate_swapchain call
            // explains the format-stable fast path that #576
            // introduced.
        }

        // #654 / LIFE-M1 — defer image-view destruction until AFTER the
        // new swapchain is created. Image views are children of the
        // old swapchain's images; destroying them BEFORE
        // `vkCreateSwapchainKHR(... oldSwapchain = old_swapchain ...)`
        // leaves the old swapchain in a state where validation
        // layers (and some IHV drivers) emit "swapchain image not in
        // expected state" warnings on the handoff.
        //
        // Take ownership of the old views before the assignment at
        // line ~80 overwrites `self.swapchain_state` — once the
        // assignment runs, `self.swapchain_state.image_views` points
        // at the new (just-created) views and would destroy the
        // wrong set. `mem::take` leaves a default-empty Vec in place
        // so the old struct is in a valid state through the
        // create_swapchain call (and the assignment immediately
        // replaces it anyway).
        let old_image_views: Vec<vk::ImageView> =
            std::mem::take(&mut self.swapchain_state.image_views);

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

        // Decide whether to rebuild the render pass + rasterization
        // pipelines. Both reference attachment formats only — extent
        // is dynamic state on the pipelines, and the framebuffers
        // (which bind the extent) are rebuilt unconditionally below.
        // The main render pass attachments are HDR_FORMAT,
        // NORMAL_FORMAT, MOTION_FORMAT, MESH_ID_FORMAT,
        // RAW_INDIRECT_FORMAT, ALBEDO_FORMAT (compile-time consts) +
        // self.depth_format (stable across the device's lifetime).
        // None of those depend on the swapchain surface format, so a
        // format-stable resize can keep every pipeline handle. See
        // PIPE-2 / #576.
        let format_changed = self.swapchain_state.format != old_swapchain_format;
        if format_changed {
            unsafe {
                // Destroy old pipelines before the render pass they
                // reference (Vulkan spec: pipelines must outlive their
                // render pass for a clean teardown). Helper drains the
                // blend cache — every pipeline in it is bound to the
                // old render pass and must be rebuilt against the new
                // one. Subsequent frames lazy-create on demand. See
                // #392 / #33.
                destroy_render_pass_pipelines(
                    &self.device,
                    &mut self.pipeline,
                    &mut self.pipeline_two_sided,
                    &mut self.blend_pipeline_cache,
                    &mut self.pipeline_ui,
                );

                self.device.destroy_render_pass(self.render_pass, None);
                self.render_pass = vk::RenderPass::null();
            }
        }

        // #654 / LIFE-M1 — destroy the old swapchain's image views NOW,
        // after the new swapchain has been created (so the handoff at
        // line ~78 saw the old swapchain in a consistent state with
        // its child views still alive) but before we destroy the old
        // swapchain itself. Vulkan spec allows destroying child views
        // either before or after the parent swapchain; this ordering
        // satisfies the strictest validation-layer interpretation
        // (VUID-VkSwapchainCreateInfoKHR-oldSwapchain-01933 + the
        // "swapchain image not in expected state" check).
        unsafe {
            for &view in &old_image_views {
                self.device.destroy_image_view(view, None);
            }
        }

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

        // Pair the destroy-side gate above: only rebuild render pass +
        // rasterization pipelines when the swapchain surface format
        // changed. Format-stable resizes keep their handles — the
        // rasterization pipelines have dynamic viewport + scissor
        // state, so an extent change doesn't invalidate them, and
        // every attachment format the render pass binds is constant
        // across resizes. See PIPE-2 / #576.
        if format_changed {
            // Main render pass: 6 color (HDR + G-buffer + raw_indirect
            // + albedo) + depth.
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

            // Recreate pipelines against the new render pass, reusing
            // existing layout.
            let pipelines = pipeline::recreate_triangle_pipelines(
                &self.device,
                self.render_pass,
                self.swapchain_state.extent,
                self.pipeline_cache,
                self.pipeline_layout,
            )?;
            self.pipeline = pipelines.opaque;
            self.pipeline_two_sided = pipelines.opaque_two_sided;

            self.pipeline_ui = pipeline::create_ui_pipeline(
                &self.device,
                self.render_pass,
                self.swapchain_state.extent,
                self.pipeline_layout,
                self.pipeline_cache,
            )?;
        }

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
                self.pipeline_cache,
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
        let (raw_indirect_views, motion_views_in, mesh_id_views_in, normal_views_in, albedo_views) = {
            let gbuffer_ref = self
                .gbuffer
                .as_ref()
                .expect("gbuffer must exist during resize");
            let n = MAX_FRAMES_IN_FLIGHT;
            let ri: Vec<vk::ImageView> = (0..n).map(|i| gbuffer_ref.raw_indirect_view(i)).collect();
            let mo: Vec<vk::ImageView> = (0..n).map(|i| gbuffer_ref.motion_view(i)).collect();
            let mi: Vec<vk::ImageView> = (0..n).map(|i| gbuffer_ref.mesh_id_view(i)).collect();
            // #650 / SH-5 — SVGF temporal needs the GBuffer normal
            // attachments for the 2×2 bilinear consistency loop. Same
            // ping-pong source as mesh_id; rebuilt on every resize so
            // the descriptor write picks up the new image views.
            let nm: Vec<vk::ImageView> = (0..n).map(|i| gbuffer_ref.normal_view(i)).collect();
            let ab: Vec<vk::ImageView> = (0..n).map(|i| gbuffer_ref.albedo_view(i)).collect();
            (ri, mo, mi, nm, ab)
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
                &normal_views_in,
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

        // Recreate caustic accumulator images + rewrite its descriptor sets
        // before composite (composite samples caustic's views).
        let normal_views_in: Vec<vk::ImageView> = {
            let gb = self
                .gbuffer
                .as_ref()
                .expect("gbuffer must exist during resize");
            (0..MAX_FRAMES_IN_FLIGHT)
                .map(|i| gb.normal_view(i))
                .collect()
        };
        if let Some(ref mut caustic) = self.caustic {
            caustic.recreate_on_resize(
                &self.device,
                self.allocator
                    .as_ref()
                    .expect("allocator missing during resize"),
                self.depth_image_view,
                &normal_views_in,
                &mesh_id_views_in,
                self.scene_buffers.light_buffers(),
                self.scene_buffers.light_buffer_size(),
                self.scene_buffers.camera_buffers(),
                self.scene_buffers.camera_buffer_size(),
                self.scene_buffers.instance_buffers(),
                self.scene_buffers.instance_buffer_size(),
                self.swapchain_state.extent.width,
                self.swapchain_state.extent.height,
            )?;
            if let Err(e) = unsafe {
                caustic.initialize_layouts(&self.device, &self.graphics_queue, self.transfer_pool)
            } {
                log::warn!("Caustic layout re-init after resize failed: {e}");
            }
        }
        let caustic_views: Vec<vk::ImageView> = match self.caustic {
            Some(ref c) => (0..MAX_FRAMES_IN_FLIGHT)
                .map(|i| c.sampled_view(i))
                .collect(),
            None => mesh_id_views_in.clone(),
        };

        // Recreate bloom pipeline (#905). Bloom's down/up mip pyramid
        // is sized from screen_extent; the old mips are stuck at the
        // pre-resize extent and would alias when sampled by composite.
        // Mirrors the SSAO destroy+new pattern above. Pipelines/layouts
        // /sampler/pool aren't extent-dependent but get rebuilt anyway
        // — this is the simpler path; recreate is rare. Failing closed:
        // composite needs SOME bloom view for binding 7, so a recreate
        // failure is fatal (matches init behaviour at mod.rs:1422-1426).
        if let Some(ref mut old_bloom) = self.bloom {
            let allocator = self
                .allocator
                .as_ref()
                .expect("allocator missing during resize");
            unsafe { old_bloom.destroy(&self.device, allocator) };
            self.bloom = None;
            match super::super::bloom::BloomPipeline::new(
                &self.device,
                allocator,
                self.pipeline_cache,
                self.swapchain_state.extent,
            ) {
                Ok(new_bloom) => {
                    if let Err(e) = unsafe {
                        new_bloom.initialize_layouts(
                            &self.device,
                            &self.graphics_queue,
                            self.transfer_pool,
                        )
                    } {
                        log::warn!("Bloom layout re-init after resize failed: {e}");
                    }
                    self.bloom = Some(new_bloom);
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Bloom pipeline re-creation failed during resize: {e} \
                         — composite binding 7 would dangle. See #905."
                    ));
                }
            }
        }

        // Snapshot bloom + volumetric output views — composite binding 6
        // (volumetric) and binding 7 (bloom) need to be (re-)written.
        // Volumetric is fixed froxel size (160×90×128 per volumetrics.rs)
        // so its views survive resize untouched; we still re-bind for
        // canonical lockstep with init. See #905.
        let bloom_views: Vec<vk::ImageView> = match self.bloom.as_ref() {
            Some(b) => b.output_views(),
            None => {
                return Err(anyhow::anyhow!(
                    "Bloom pipeline absent during resize — \
                     composite binding 7 cannot be bound. See #905."
                ));
            }
        };
        let volumetric_views: Vec<vk::ImageView> = match self.volumetrics.as_ref() {
            Some(v) => v.integrated_views(),
            None => {
                return Err(anyhow::anyhow!(
                    "Volumetrics pipeline absent during resize — \
                     composite binding 6 cannot be bound. See #905."
                ));
            }
        };

        // Recreate composite pipeline's HDR images + swapchain framebuffers
        // with the new extent. Also rewrites descriptor sets to point at
        // the new indirect + albedo + caustic + volumetric + bloom views.
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
                &caustic_views,
                &volumetric_views,
                &bloom_views,
                self.swapchain_state.extent.width,
                self.swapchain_state.extent.height,
            )?;
        }

        // Snapshot composite's HDR views (owned Vec) so subsequent &mut
        // borrows for TAA + composite don't conflict.
        let hdr_views_owned: Vec<vk::ImageView> = self
            .composite
            .as_ref()
            .expect("composite must exist during resize")
            .hdr_image_views
            .clone();

        // Recreate TAA history images + descriptor sets.
        if let Some(ref mut taa) = self.taa {
            taa.recreate_on_resize(
                &self.device,
                self.allocator
                    .as_ref()
                    .expect("allocator missing during resize"),
                &hdr_views_owned,
                &motion_views_in,
                &mesh_id_views_in,
                self.swapchain_state.extent.width,
                self.swapchain_state.extent.height,
            )?;
            if let Err(e) = unsafe {
                taa.initialize_layouts(&self.device, &self.graphics_queue, self.transfer_pool)
            } {
                log::warn!("TAA layout re-init after resize failed: {e}");
            }
        }
        // Rewire composite's HDR binding to TAA output (if TAA is active).
        if let (Some(ref t), Some(ref mut c)) = (&self.taa, &mut self.composite) {
            let n = MAX_FRAMES_IN_FLIGHT;
            let taa_views: Vec<vk::ImageView> = (0..n).map(|i| t.output_view(i)).collect();
            c.rebind_hdr_views(&self.device, &taa_views, vk::ImageLayout::GENERAL);
        }

        // Reset permanent-failure latches — every downstream pass has
        // just been recreated so any previous lost-device state is no
        // longer authoritative. See #479.
        self.taa_failed = false;
        self.svgf_failed = false;
        self.caustic_failed = false;

        // Main framebuffers bind the new HDR + G-buffer views + depth.
        let gbuffer_ref = self
            .gbuffer
            .as_ref()
            .expect("gbuffer must exist during resize");
        let hdr_views = &hdr_views_owned;
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

#[cfg(test)]
mod tests {
    /// Regression for #654 / LIFE-M1. The old swapchain's image-view
    /// destruction must happen AFTER `swapchain::create_swapchain`
    /// returns (so the `oldSwapchain` handoff sees the parent
    /// swapchain in a consistent state with its child views still
    /// alive) but BEFORE the old swapchain itself is destroyed.
    ///
    /// This is a static source check — no Vulkan context is
    /// available in unit tests. The check parses the file and
    /// asserts the byte offsets of three landmark strings appear in
    /// the right relative order, with the captured `old_image_views`
    /// `mem::take` happening before the create_swapchain call.
    #[test]
    fn old_image_views_destroyed_between_new_swapchain_creation_and_old_destroy() {
        let src = include_str!("resize.rs");

        // Find the four key landmarks in the source:
        //   1. The `mem::take` capture of old image views.
        //   2. The `swapchain::create_swapchain(` call (new swapchain alive).
        //   3. The `for &view in &old_image_views` destroy loop (#654 site).
        //   4. The `destroy_swapchain(old_swapchain` call (old parent gone).
        let take_pos = src
            .find("std::mem::take(&mut self.swapchain_state.image_views)")
            .expect("must capture old image_views via mem::take (#654)");
        let create_pos = src
            .find("swapchain::create_swapchain(")
            .expect("must call swapchain::create_swapchain");
        let destroy_views_pos = src
            .find("for &view in &old_image_views")
            .expect("must destroy old_image_views in a for-loop (#654)");
        let destroy_swapchain_pos = src
            .find("destroy_swapchain(old_swapchain")
            .expect("must call destroy_swapchain on old_swapchain");

        // mem::take precedes create_swapchain — so the old vec is
        // owned before the field gets overwritten.
        assert!(
            take_pos < create_pos,
            "old_image_views must be captured via mem::take BEFORE \
             create_swapchain overwrites self.swapchain_state (#654)"
        );
        // create_swapchain precedes the views-destroy loop — strict
        // validation requires the old swapchain still have its child
        // views alive at handoff time.
        assert!(
            create_pos < destroy_views_pos,
            "old image views must be destroyed AFTER create_swapchain \
             returns (the new one is alive). Pre-fix the loop ran \
             before create_swapchain, leaving the old swapchain in \
             an inconsistent state during handoff. See #654 / LIFE-M1."
        );
        // Views destroyed before the old swapchain itself.
        assert!(
            destroy_views_pos < destroy_swapchain_pos,
            "old image views must be destroyed BEFORE the old \
             swapchain (children-before-parent). #654."
        );
    }
}
