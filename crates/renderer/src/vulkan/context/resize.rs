//! Swapchain recreation after window resize or suboptimal present.

use super::super::composite::HDR_FORMAT;
use super::super::gbuffer::{
    ALBEDO_FORMAT, FSR_MASK_FORMAT, MESH_ID_FORMAT, MOTION_FORMAT, NORMAL_FORMAT,
    RAW_INDIRECT_FORMAT,
};
use super::super::ssao::SsaoPipeline;
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::super::{pipeline, swapchain};
use super::helpers::{
    create_depth_resources, create_main_framebuffers, create_render_pass, destroy_depth_resources,
    destroy_main_framebuffers, destroy_render_pass_pipelines, init_depth_history_layout,
    GBufferFormats, GBufferViews,
};
use super::VulkanContext;
use anyhow::{Context, Result};
use ash::vk;

impl VulkanContext {
    /// Recreate the swapchain after a resize or suboptimal present.
    /// Phase 1 of resize (#1671) — swapchain handoff + depth + format-gated
    /// render-pass / rasterization-pipeline rebuild. Captures the old surface
    /// format, destroys the old framebuffers / depth / image views, creates
    /// the new swapchain (atomic `oldSwapchain` handoff), recreates depth +
    /// depth-history, and rebuilds the render pass + raster/UI/water pipelines
    /// only when the surface format changed (#576). Extracted verbatim from
    /// the former 761-LOC `recreate_swapchain`; every local it computes
    /// (`old_swapchain_format` / `format_changed` / `old_image_views` /
    /// `old_swapchain`) is consumed before this phase returns, so nothing
    /// crosses into the later phases — they read the `self` state set here.
    fn recreate_swapchain_core(&mut self, window_size: [u32; 2]) -> Result<()> {
        unsafe {
            // SAFETY: `self.device` is the live logical device; `device_wait_idle`
            // is a pure host-side wait that only requires the device handle to be
            // valid, which it is for the lifetime of `self`.
            self.device.device_wait_idle().context("device_wait_idle")?;
        }

        // #1005 — shrink the BLAS build scratch buffer to fit. The
        // scratch is grow-only across the process lifetime by design
        // (#495); without this call a session that touched one heavy
        // mesh (Starfield `Saturn.nif`, FO4 LOD terrain) keeps the
        // ~80–200 MB scratch resident across every subsequent resize
        // until a cell unload triggers the shrink. Resize already
        // paid the `device_wait_idle` cost above, so the BLAS build
        // command buffer (fenced one-time submit) is guaranteed
        // complete and the scratch buffer is safe to destroy/realloc.
        // SAFETY: post-`device_wait_idle` — no in-flight GPU work
        // references the scratch. Skipped when `accel_manager` or
        // `allocator` are absent (pre-init / headless paths).
        if let (Some(accel), Some(allocator)) =
            (self.accel_manager.as_mut(), self.allocator.as_ref())
        {
            unsafe {
                // SAFETY: `accel`, `self.device` and `allocator` are live;
                // recreate already waited the frames-in-flight idle, so the
                // freed BLAS scratch is not referenced by any in-flight build.
                accel.shrink_blas_scratch_to_fit(&self.device, allocator);
            }
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
            // SAFETY: `device_wait_idle` above guarantees the device is idle, so
            // these framebuffers, depth image/view, and depth-history image/view —
            // all created by `self.device`/`self.allocator` and not yet destroyed —
            // are no longer referenced by any in-flight command buffer and can be
            // destroyed. Each handle is nulled by the helper so a later Drop is a
            // no-op on VK_NULL_HANDLE.
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

            // Soft-particle depth-history image follows the depth buffer's
            // extent — recreated below. The sampler is extent-independent so
            // it survives the resize untouched.
            destroy_depth_resources(
                &self.device,
                self.allocator
                    .as_ref()
                    .expect("allocator missing during resize"),
                &mut self.depth_history_view,
                &mut self.depth_history_image,
                &mut self.depth_history_allocation,
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
            swapchain::SwapchainSurfaceCtx {
                instance: &self.instance,
                device: &self.device,
                physical_device: self.physical_device,
                surface_loader: &self.surface_loader,
                surface: self.surface,
            },
            self.queue_indices,
            window_size,
            old_swapchain, // atomic handoff — avoids flicker during resize
        )?;
        let max_image_dimension_2d = unsafe {
            // SAFETY: `self.physical_device` was selected from `self.instance`
            // and both remain live for the context lifetime.
            self.instance
                .get_physical_device_properties(self.physical_device)
                .limits
                .max_image_dimension2_d
        };
        let frame_extents = super::super::upscaling::FrameExtentSet::for_output(
            self.swapchain_state.extent,
            self.renderer_config.upscaler,
            max_image_dimension_2d,
        )?;
        let fsr_temporal = match self.renderer_config.upscaler {
            super::super::upscaling::UpscalerMode::Taa => None,
            super::super::upscaling::UpscalerMode::Fsr3(_) => Some(
                super::super::upscaling::FsrTemporalState::new(frame_extents)
                    .context("query resized FSR temporal jitter sequence")?,
            ),
        };
        self.frame_extents = frame_extents;
        self.fsr_temporal = fsr_temporal;

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
                // SAFETY: the device is idle (device_wait_idle at entry), so the
                // old pipelines and render pass — all created by `self.device` and
                // not yet destroyed — are free of in-flight references. Pipelines
                // are torn down before the render pass they were built against, and
                // `render_pass` is nulled so a later Drop is a no-op.
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
                    &mut self.pipeline_wireframe,
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
            // SAFETY: the device is idle (device_wait_idle at entry) and the new
            // swapchain is already created, so these retired image views — created
            // by `self.device` from the old swapchain's images and not yet
            // destroyed — have no in-flight references and can be destroyed.
            for &view in &old_image_views {
                self.device.destroy_image_view(view, None);
            }
        }

        // Destroy the retired old swapchain now that the new one is active.
        if old_swapchain != vk::SwapchainKHR::null() {
            unsafe {
                // SAFETY: the device is idle (device_wait_idle at entry), its child
                // image views were destroyed just above, and the new swapchain is
                // already active, so the retired `old_swapchain` (non-null per the
                // guard) has no remaining references and can be destroyed by the
                // loader that created it.
                self.swapchain_state
                    .swapchain_loader
                    .destroy_swapchain(old_swapchain, None);
            }
        }

        let (depth_image, depth_image_view, depth_allocation) = create_depth_resources(
            &self.device,
            self.allocator.as_ref().expect("allocator missing"),
            self.frame_extents.render,
            self.depth_format,
            // TRANSFER_SRC: soft-particle depth-history copy source (#1583).
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC,
            "depth_buffer",
        )?;
        self.depth_image = depth_image;
        self.depth_image_view = depth_image_view;
        self.depth_allocation = Some(depth_allocation);

        // Re-create the soft-particle depth-history image at the new extent
        // and re-prime its layout (UNDEFINED → far-clear → SHADER_READ_ONLY).
        // Its descriptor (set 1, binding 15) is rewritten alongside the AO
        // texture further down (the view handle changed).
        let (depth_history_image, depth_history_view, depth_history_allocation) =
            create_depth_resources(
                &self.device,
                self.allocator.as_ref().expect("allocator missing"),
                self.frame_extents.render,
                self.depth_format,
                vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                "depth_history",
            )?;
        self.depth_history_image = depth_history_image;
        self.depth_history_view = depth_history_view;
        self.depth_history_allocation = Some(depth_history_allocation);
        init_depth_history_layout(
            &self.device,
            &self.graphics_queue,
            self.command_pool,
            self.depth_history_image,
        )?;
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            self.scene_buffers.write_depth_history(
                &self.device,
                f,
                self.depth_history_view,
                self.depth_history_sampler,
            );
        }

        // Pair the destroy-side gate above: only rebuild render pass +
        // rasterization pipelines when the swapchain surface format
        // changed. Format-stable resizes keep their handles — the
        // rasterization pipelines have dynamic viewport + scissor
        // state, so an extent change doesn't invalidate them, and
        // every attachment format the render pass binds is constant
        // across resizes. See PIPE-2 / #576.
        if format_changed {
            // Main render pass: 7 color (HDR + G-buffer + raw_indirect
            // + albedo + reservoir) + depth.
            self.render_pass = create_render_pass(
                &self.device,
                GBufferFormats {
                    color_format: HDR_FORMAT,
                    normal_format: NORMAL_FORMAT,
                    motion_format: MOTION_FORMAT,
                    mesh_id_format: MESH_ID_FORMAT,
                    raw_indirect_format: RAW_INDIRECT_FORMAT,
                    albedo_format: ALBEDO_FORMAT,
                    fsr_mask_format: FSR_MASK_FORMAT,
                    depth_format: self.depth_format,
                },
            )?;

            // Recreate pipelines against the new render pass, reusing
            // existing layout.
            let pipelines = pipeline::recreate_triangle_pipelines(
                &self.device,
                self.render_pass,
                self.frame_extents.render,
                self.pipeline_cache,
                self.pipeline_layout,
                self.device_caps.fill_mode_non_solid_supported,
            )?;
            self.pipeline = pipelines.opaque;
            self.pipeline_wireframe = pipelines.opaque_wireframe;

            self.pipeline_ui = pipeline::create_ui_pipeline(
                &self.device,
                self.render_pass,
                self.frame_extents.render,
                self.pipeline_layout,
                self.pipeline_cache,
            )?;

            // Water pipeline depends on the render pass. Destroy +
            // recreate when the render pass changes; absorb failure
            // by falling back to no water rendering (same robustness
            // policy as the initial create site).
            if let Some(mut old) = self.water.take() {
                // SAFETY: the resize path called device_wait_idle before this
                // rebuild, so the old water pipeline — created by `self.device`
                // and not yet destroyed — has no in-flight references and is safe
                // to destroy.
                unsafe { old.destroy(&self.device) };
            }
            self.water = match super::super::water::WaterPipeline::new(
                &self.device,
                self.render_pass,
                self.pipeline_cache,
                self.texture_registry.descriptor_set_layout,
                self.scene_buffers.descriptor_set_layout,
            ) {
                Ok(w) => Some(w),
                Err(e) => {
                    log::warn!("Water pipeline recreate after resize failed: {e}");
                    None
                }
            };
        }

        Ok(())
    }

    /// Phase 2 of resize (#1671) — texture + SSAO descriptor rebind: refresh
    /// the per-texture descriptor sets for the new swapchain image count, then
    /// destroy+rebuild the SSAO pipeline (its descriptor sets referenced the
    /// destroyed depth view) and rewrite the AO texture binding. Extracted
    /// verbatim from `recreate_swapchain`; reads only `self` state.
    fn recreate_texture_ssao_bindings(&mut self) -> Result<()> {
        // Recreate descriptor sets for existing textures (new swapchain image count).
        let material_mip_bias = self
            .frame_extents
            .material_mip_bias(self.renderer_config.upscaler)
            .clamp(
                -self.device_caps.max_sampler_lod_bias,
                self.device_caps.max_sampler_lod_bias,
            );
        self.texture_registry.recreate_descriptor_sets(
            &self.device,
            self.swapchain_state.images.len() as u32,
            material_mip_bias,
        )?;

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
            // SAFETY: the resize path called device_wait_idle before this phase,
            // so the old SSAO pipeline — its images/views/descriptors created by
            // `self.device`/`allocator` and not yet destroyed — has no in-flight
            // references and is safe to destroy.
            unsafe { old_ssao.destroy(&self.device, allocator) };
            self.ssao = None;
            // Re-use `self.pipeline_cache` for the rebuilt SSAO
            // pipeline. The cache survives the destroy + recreate
            // by design — `pipeline_cache` is the engine-wide
            // VkPipelineCache handle owned by VulkanContext, not
            // the per-pipeline cache slot — so the rebuilt SSAO
            // pipeline reads any cached blob the prior session's
            // pipeline-cache file deposited at startup. Pre-fix
            // this looked like an opportunity to allocate a fresh
            // cache on resize (cosmetic finding REN-D7-NEW-08,
            // audit 2026-05-09); reusing it is the right call —
            // a fresh cache would warm-cold every resize and
            // negate the disk-cache savings.
            match SsaoPipeline::new(
                &self.device,
                allocator,
                self.pipeline_cache,
                self.depth_image_view,
                self.frame_extents.render.width,
                self.frame_extents.render.height,
            ) {
                Ok(new_ssao) => {
                    // Transition AO image to valid layout before first use.
                    if let Err(e) = unsafe {
                        // SAFETY: `self.device`, `self.graphics_queue` and
                        // `self.transfer_pool` are all live; the AO images being
                        // transitioned were just created by `new_ssao` and are
                        // owned by it, so recording + submitting the one-time
                        // layout-transition command buffer is valid.
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
                    // #2141 / RL-D6-01 — the old SSAO pipeline (and its
                    // per-FIF AO images + views) was destroyed above, but
                    // scene set 1 / binding 7 still names those views and
                    // `triangle.frag` samples `aoTexture` unconditionally.
                    // This arm only warned, and the failure doesn't
                    // propagate — `recreate_screen_passes` completes and
                    // the #1211 `framebuffers.is_empty()` bail-out doesn't
                    // catch it — so every subsequent frame bound a
                    // descriptor referencing freed GPU memory.
                    log::warn!("SSAO recreation failed after resize: {e} — no ambient occlusion");
                    match self.placeholder_ao.as_ref() {
                        Some(p) => {
                            for f in 0..MAX_FRAMES_IN_FLIGHT {
                                self.scene_buffers.write_ao_texture(
                                    &self.device,
                                    f,
                                    p.view,
                                    p.sampler,
                                );
                            }
                        }
                        None => log::error!(
                            "SSAO recreation failed and no AO placeholder exists — scene \
                             binding 7 still references the destroyed AO view (#2141)"
                        ),
                    }
                }
            }
        }

        Ok(())
    }

    /// Phase 3 of resize (#1671) — screen-sized pass recreation: G-buffer,
    /// SVGF, ReSTIR reservoirs, caustics, bloom, composite, egui framebuffers,
    /// TAA, main framebuffers, then per-image sync recreate + temporal-recovery
    /// reset. Holds the tangled fresh-`vk::ImageView`-vector data flow that the
    /// earlier phases don't touch (all such locals are created and consumed
    /// within this phase). Extracted verbatim from `recreate_swapchain`.
    fn recreate_screen_passes(&mut self) -> Result<()> {
        // Recreate G-buffer images FIRST (they're referenced by composite
        // descriptor sets, which we'll rewrite during composite recreation).
        if let Some(ref mut gbuffer) = self.gbuffer {
            gbuffer.recreate_on_resize(
                &self.device,
                self.allocator
                    .as_ref()
                    .expect("allocator missing during resize"),
                self.frame_extents.render.width,
                self.frame_extents.render.height,
            )?;
            // New images start UNDEFINED — transition to SHADER_READ_ONLY so
            // the "prev" frame slot is valid on the first frame after resize.
            if let Err(e) = unsafe {
                // SAFETY: `self.device`, `self.graphics_queue` and
                // `self.transfer_pool` are all live; the G-buffer images being
                // transitioned were just (re)created by `gbuffer` and are owned by
                // it, so recording + submitting the one-time layout-transition
                // command buffer is valid.
                gbuffer.initialize_layouts(&self.device, &self.graphics_queue, self.transfer_pool)
            } {
                log::warn!("G-buffer post-resize layout init failed: {e}");
            }
        }

        // Collect fresh G-buffer views before we borrow &mut self.svgf /
        // self.composite. Motion and mesh_id are needed by SVGF.
        let (
            raw_indirect_views,
            motion_views_in,
            mesh_id_views_in,
            normal_views_in,
            albedo_views,
            reactive_views,
            transparency_views,
        ) = {
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
            let re: Vec<vk::ImageView> = (0..n).map(|i| gbuffer_ref.reactive_view(i)).collect();
            let tr: Vec<vk::ImageView> = (0..n).map(|i| gbuffer_ref.transparency_view(i)).collect();
            (ri, mo, mi, nm, ab, re, tr)
        };

        // Recreate SVGF history images + rewrite its descriptor sets
        // against the new G-buffer views. Must happen before composite
        // (whose descriptor sets reference SVGF's indirect_view). The
        // post-recreate `initialize_layouts` call lives INSIDE
        // `recreate_on_resize` (#1031) — the function is self-contained
        // so a new caller can't forget to walk the fresh history
        // images to GENERAL.
        if let Some(ref mut svgf) = self.svgf {
            svgf.recreate_on_resize(
                crate::vulkan::GpuUploadCtx {
                    device: &self.device,
                    allocator: self
                        .allocator
                        .as_ref()
                        .expect("allocator missing during resize"),
                    queue: &self.graphics_queue,
                    command_pool: self.transfer_pool,
                },
                crate::vulkan::svgf::SvgfInputViews {
                    raw_indirect_views: &raw_indirect_views,
                    motion_views: &motion_views_in,
                    mesh_id_views: &mesh_id_views_in,
                    normal_views: &normal_views_in,
                },
                self.frame_extents.render.width,
                self.frame_extents.render.height,
            )?;
        }

        // ReSTIR reservoir buffers are screen-sized — recreate them at the
        // new extent and re-write the scene descriptor set (bindings 16/17)
        // so triangle.frag reads/writes the fresh buffers. History is
        // meaningless across a resize; the final visibility ray re-validates
        // every shaded sample, so stale contents are harmless.
        {
            let allocator = self
                .allocator
                .as_ref()
                .expect("allocator missing during resize");
            self.reservoir_buffers.recreate_on_resize(
                &self.device,
                allocator,
                self.frame_extents.render.width,
                self.frame_extents.render.height,
            )?;
            for i in 0..MAX_FRAMES_IN_FLIGHT {
                self.scene_buffers.write_reservoir_buffers(
                    &self.device,
                    i,
                    self.reservoir_buffers.curr_buffer(i),
                    self.reservoir_buffers.prev_buffer(i),
                    self.reservoir_buffers.buffer_size(),
                );
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
            // `recreate_on_resize` walks the new slots to GENERAL
            // internally (#1031).
            caustic.recreate_on_resize(
                &self.device,
                self.allocator
                    .as_ref()
                    .expect("allocator missing during resize"),
                &self.graphics_queue,
                self.transfer_pool,
                self.depth_image_view,
                &normal_views_in,
                &mesh_id_views_in,
                self.scene_buffers.light_buffers(),
                self.scene_buffers.light_buffer_size(),
                self.scene_buffers.camera_buffers(),
                self.scene_buffers.camera_buffer_size(),
                self.scene_buffers.instance_buffers(),
                self.scene_buffers.instance_buffer_size(),
                self.frame_extents.render.width,
                self.frame_extents.render.height,
            )?;
        }
        // #1255 / Phase C of #1210 — water-caustic accumulator
        // resizes alongside the existing caustic image. SAFETY:
        // `recreate_swapchain` paid `device_wait_idle` earlier so
        // no in-flight command buffer references the old slots.
        if let Some(ref mut wca) = self.water_caustic_accum {
            let allocator = self
                .allocator
                .as_ref()
                .expect("allocator missing during resize");
            unsafe {
                // SAFETY: `recreate_swapchain` paid device_wait_idle before
                // this, so the water-caustic accumulator's old slot handles are
                // unreferenced by any in-flight command buffer and can be
                // recreated in place.
                if let Err(e) = wca.recreate_on_resize(
                    &self.device,
                    allocator,
                    self.frame_extents.render.width,
                    self.frame_extents.render.height,
                ) {
                    log::warn!(
                        "Water-caustic accumulator resize failed: {e} — disabling for the rest of the session"
                    );
                    wca.destroy(&self.device, allocator);
                    self.water_caustic_accum = None;
                } else if let Err(e) =
                    wca.initialize_layouts(&self.device, &self.graphics_queue, self.transfer_pool)
                {
                    // Fresh per-FIF slot images come up UNDEFINED again
                    // after recreate; if the transition fails we can't
                    // safely use the accumulator this session.
                    log::warn!(
                        "Water-caustic initialize_layouts after resize failed: {e} — disabling for the rest of the session"
                    );
                    wca.destroy(&self.device, allocator);
                    self.water_caustic_accum = None;
                }
            }
        }
        // Rebind WaterPipeline's set 2 to the new accumulator views (the
        // recreate above produced fresh `vk::ImageView` handles per FIF
        // slot), or to the 1×1 storage sink if the accumulator dropped out.
        //
        // #2142 / RL-D6-02 — this used to be `if let (Some(w), Some(accum))`,
        // so both failure arms above (which destroy the accumulator and set
        // it to `None`) skipped the rebind entirely and left set 2 holding
        // the destroyed per-FIF storage view. `record_draw` binds set 2
        // unconditionally and the geometry pass gates the water draw only on
        // `self.water.is_some()` — never on the accumulator — so every
        // subsequent exterior/water frame issued an `imageAtomicAdd` against
        // freed memory. Strictly worse than the AO case above: a write, not
        // a read.
        if let Some(w) = self.water.as_ref() {
            let views: Option<Vec<vk::ImageView>> = match self.water_caustic_accum.as_ref() {
                Some(accum) => Some(
                    (0..MAX_FRAMES_IN_FLIGHT)
                        .map(|i| accum.storage_view(i))
                        .collect(),
                ),
                None => self
                    .placeholder_caustic_sink
                    .as_ref()
                    .map(|p| vec![p.view; MAX_FRAMES_IN_FLIGHT]),
            };
            match views {
                Some(views) => w.update_water_caustic_descriptors(&self.device, &views),
                None => log::error!(
                    "Water-caustic accumulator dropped out and no sink placeholder exists — \
                     WaterPipeline set 2 still references the destroyed storage view (#2142)"
                ),
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
            // SAFETY: the resize path called device_wait_idle before this phase,
            // so the old bloom pipeline — its mip images/views created by
            // `self.device`/`allocator` and not yet destroyed — has no in-flight
            // references and is safe to destroy.
            unsafe { old_bloom.destroy(&self.device, allocator) };
            self.bloom = None;
            match super::super::bloom::BloomPipeline::new(
                &self.device,
                allocator,
                self.pipeline_cache,
                self.frame_extents.render,
            ) {
                Ok(new_bloom) => {
                    if let Err(e) = unsafe {
                        // SAFETY: `self.device`, `self.graphics_queue` and
                        // `self.transfer_pool` are all live; the bloom mip images
                        // being transitioned were just created by `new_bloom` and
                        // are owned by it, so recording + submitting the one-time
                        // layout-transition command buffer is valid.
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

        // Froxel XY is derived from the new render resolution (after the FSR
        // preset query), so the volume cannot survive a render-extent change.
        // The resize path is device-idle here; rebuild the whole pass to keep
        // images, per-FIF history descriptors, and dispatch dimensions atomic.
        let volumetrics_config = self
            .volumetrics
            .as_ref()
            .map_or(self.renderer_config.volumetrics, |volume| volume.config());
        if let Some(mut old_volumetrics) = self.volumetrics.take() {
            let allocator = self
                .allocator
                .as_ref()
                .expect("allocator missing during resize");
            // SAFETY: `recreate_swapchain` waited for the device to become
            // idle before this phase, so no command buffer can reference the
            // old froxel images, descriptors, or pipelines.
            unsafe { old_volumetrics.destroy(&self.device, allocator) };
        }
        let allocator = self
            .allocator
            .as_ref()
            .expect("allocator missing during resize");
        let mut new_volumetrics = super::super::volumetrics::VolumetricsPipeline::new(
            &self.device,
            allocator,
            self.pipeline_cache,
            self.frame_extents.render,
            volumetrics_config,
        )
        .context("recreate render-resolution froxel volume")?;
        // SAFETY: `new_volumetrics` exclusively owns freshly-created images;
        // the device, queue, and transfer pool are live and device-idle here.
        if let Err(error) = unsafe {
            new_volumetrics.initialize_layouts(
                &self.device,
                allocator,
                &self.graphics_queue,
                self.transfer_pool,
            )
        } {
            // SAFETY: initialization failed before the pipeline was published
            // to `self` or submitted by a frame, so its owned handles have no
            // in-flight users and can be destroyed immediately.
            unsafe { new_volumetrics.destroy(&self.device, allocator) };
            return Err(error).context("initialize recreated froxel layouts");
        }
        self.volumetrics = Some(new_volumetrics);

        // Snapshot bloom + volumetric output views — composite binding 6
        // (volumetric) and binding 7 (bloom) need to be (re-)written.
        // Volumetrics was recreated immediately above because its XY follows
        // render resolution. See #905.
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

        // Recreate composite pipeline's raw + composed HDR images and
        // per-frame framebuffers. Also rewrites descriptor sets to point at
        // the new indirect + albedo + caustic + volumetric + bloom views.
        // #1257 / Phase E of #1210 — gather the resized water-caustic
        // sampled views. Same fall-back-to-existing-caustic shape as
        // the init path in context::new.
        let water_caustic_views: Vec<vk::ImageView> = match self.water_caustic_accum.as_ref() {
            Some(a) => (0..MAX_FRAMES_IN_FLIGHT)
                .map(|i| a.sampled_view(i))
                .collect(),
            None => caustic_views.clone(),
        };
        if let Some(ref mut composite) = self.composite {
            composite.recreate_on_resize(
                &self.device,
                self.allocator
                    .as_ref()
                    .expect("allocator missing during resize"),
                &composite_indirect_views,
                indirect_is_general,
                &albedo_views,
                self.depth_image_view,
                &caustic_views,
                &water_caustic_views,
                &volumetric_views,
                &bloom_views,
                &reactive_views,
                &transparency_views,
                self.frame_extents,
            )?;
        }

        // Phase 4 — recreate the egui overlay's framebuffers
        // against the fresh swapchain image views. The render pass
        // itself is preserved (swapchain format doesn't change on
        // resize); only the framebuffer attachments + extent need
        // to track the new images.
        if let Some(ref mut pass) = self.egui_pass {
            pass.recreate_framebuffers(
                &self.device,
                &self.swapchain_state.image_views,
                self.swapchain_state.extent,
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

        // Recreate TAA history images + descriptor sets. The
        // post-recreate layout walk to GENERAL lives inside
        // `recreate_on_resize` (#1031).
        if let Some(ref mut taa) = self.taa {
            taa.recreate_on_resize(
                crate::vulkan::GpuUploadCtx {
                    device: &self.device,
                    allocator: self
                        .allocator
                        .as_ref()
                        .expect("allocator missing during resize"),
                    queue: &self.graphics_queue,
                    command_pool: self.transfer_pool,
                },
                crate::vulkan::taa::TaaInputViews {
                    hdr_views: &hdr_views_owned,
                    motion_views: &motion_views_in,
                    mesh_id_views: &mesh_id_views_in,
                    normal_views: &normal_views_in,
                },
                self.frame_extents.render.width,
                self.frame_extents.render.height,
            )?;
        }
        // Rewire composite's HDR binding to TAA output (if TAA is active).
        if let (Some(ref t), Some(ref mut c)) = (&self.taa, &mut self.composite) {
            let n = MAX_FRAMES_IN_FLIGHT;
            let taa_views: Vec<vk::ImageView> = (0..n).map(|i| t.output_view(i)).collect();
            c.rebind_hdr_views(&self.device, &taa_views, vk::ImageLayout::GENERAL);
        }

        // Presentation descriptors reference the upscaler's output views, so
        // retire presentation before replacing those views. The resize entry
        // point paid device_wait_idle before reaching this method.
        if let Some(mut presentation) = self.presentation.take() {
            // SAFETY: the resize entry point called device_wait_idle before
            // reaching this method, so `presentation` — created by
            // `self.device` and not yet destroyed — has no in-flight
            // command-buffer references and is safe to destroy.
            unsafe { presentation.destroy(&self.device) };
        }
        let allocator = self
            .allocator
            .as_ref()
            .expect("allocator missing during resize");
        let upscaled_views = {
            let upscaler = self
                .frame_upscaler
                .as_mut()
                .expect("frame upscaler must exist during resize");
            upscaler.recreate(
                &self.instance,
                &self.device,
                self.physical_device,
                allocator,
                &self.graphics_queue,
                self.transfer_pool,
                self.renderer_config.upscaler,
                self.frame_extents,
            )?;
            upscaler.output_views().to_vec()
        };
        self.presentation = Some(
            super::super::presentation::PresentationPipeline::new(
                &self.device,
                self.pipeline_cache,
                self.swapchain_state.format.format,
                &self.swapchain_state.image_views,
                &upscaled_views,
                self.frame_extents.output,
            )
            .context("recreate presentation pipeline")?,
        );

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
        let reactive_views: Vec<vk::ImageView> =
            (0..n).map(|i| gbuffer_ref.reactive_view(i)).collect();
        let transparency_views: Vec<vk::ImageView> =
            (0..n).map(|i| gbuffer_ref.transparency_view(i)).collect();
        self.framebuffers = create_main_framebuffers(
            &self.device,
            self.render_pass,
            GBufferViews {
                hdr_views,
                normal_views: &normal_views,
                motion_views: &motion_views,
                mesh_id_views: &mesh_id_views,
                raw_indirect_views: &raw_indirect_views,
                albedo_views: &albedo_views,
                reactive_views: &reactive_views,
                transparency_views: &transparency_views,
            },
            self.depth_image_view,
            self.frame_extents.render,
        )?;

        // Command buffers are per frame-in-flight (fixed count), so they
        // don't need reallocation on swapchain resize. They'll be reset
        // before re-recording on the next draw_frame. See #259.

        // Recreate per-image semaphores and fence tracking for the new swapchain.
        unsafe {
            // SAFETY: the resize path called device_wait_idle before this phase,
            // so the retired per-image semaphores/fences owned by `frame_sync` are
            // no longer in use by any in-flight submission and can be destroyed and
            // recreated against `self.device` (which is live).
            self.frame_sync
                .recreate_for_swapchain(&self.device, self.swapchain_state.images.len())?;
        }

        // Reset frame-in-flight counter so the first post-resize frame
        // starts from slot 0 with a clean fence/semaphore cycle.
        self.current_frame = 0;

        // #913 / REN-D7-NEW-07 — reset the per-frame counter that
        // feeds the Halton TAA jitter sequence (`draw.rs:334`) and
        // the camera UBO (`:398`) so the first post-resize frame's
        // jitter aligns with the freshly-recreated TAA history image
        // (which TAA's force-history-reset gate below will treat as
        // pure current pixel). Without this reset the Halton index
        // continued from wherever it was pre-resize while the history
        // image was just allocated UNDEFINED — one frame of mis-aligned
        // reprojection visible as a ghost / smear on the first post-
        // resize frame.
        self.frame_counter = 0;

        // Force a few-frame TAA history reset + SVGF α-elevation
        // window so the first post-resize frames are clean
        // accumulations rather than reprojections against the
        // freshly-recreated (effectively undefined) history images.
        // 8 frames matches the cell-streaming discontinuity budget
        // (`SVGF_TAA_STREAMING_RECOVERY_FRAMES` at `byroredux/src/
        // main.rs:56`) — at 60 FPS that's ~130 ms of recovery, in
        // the same band as TAA's own first-frame reset gate. The
        // robust half of the #913 fix; option 2 from the audit body.
        const RESIZE_RECOVERY_FRAMES: u32 = 8;
        self.signal_temporal_discontinuity(RESIZE_RECOVERY_FRAMES);

        log::info!(
            "Swapchain recreated: render={}x{}, output={}x{}",
            self.frame_extents.render.width,
            self.frame_extents.render.height,
            self.frame_extents.output.width,
            self.frame_extents.output.height,
        );
        Ok(())
    }

    /// Recreate the swapchain and every extent-dependent GPU resource after a
    /// window resize. Thin orchestrator over the three phases extracted under
    /// #1671 (was a single 761-LOC function): each runs in sequence, and the
    /// later phases read the `self` state the earlier ones wrote — no locals
    /// cross a phase boundary, so the split is behaviour-identical.
    pub fn recreate_swapchain(&mut self, window_size: [u32; 2]) -> Result<()> {
        self.recreate_swapchain_core(window_size)?;
        self.recreate_texture_ssao_bindings()?;
        self.recreate_screen_passes()?;
        Ok(())
    }

    /// Switch the temporal reconstruction path at runtime.
    ///
    /// The render extent, the FSR context, the jitter sequence, the material
    /// mip bias, and every render-sized target are all derived from
    /// `renderer_config.upscaler`, so the switch is "set the mode, then take
    /// the resize path" — with the TAA pipeline as the one resource the
    /// resize path cannot decide about on its own, because it recreates a TAA
    /// that exists rather than creating or retiring one.
    ///
    /// The two paths are mutually exclusive by construction: leaving TAA
    /// destroys its history and hands composite's HDR binding back to the raw
    /// attachment, and entering TAA builds it only after the resize settled
    /// the new render extent. Neither upscaler ever sees the other's output.
    ///
    /// Runs at a frame boundary: `device_wait_idle` first, so no in-flight
    /// command buffer references the resources being replaced.
    ///
    /// `Ok(())` means the renderer is drawable — either the switch landed, or
    /// it failed and the rollback below restored the previous configuration.
    /// `Err` means neither the new nor the old configuration could be built,
    /// so there is no drawable state left to return to; the call site must
    /// treat it as fatal rather than continuing to spin the frame loop
    /// (#2156).
    pub fn set_upscaler_mode(
        &mut self,
        mode: super::super::upscaling::UpscalerMode,
        window_size: [u32; 2],
    ) -> Result<()> {
        if self.renderer_config.upscaler == mode {
            return Ok(());
        }
        let previous = self.renderer_config.upscaler;

        // SAFETY: the only wait strong enough for what follows — both frame
        // slots retire before any descriptor, image, or pipeline they
        // reference is destroyed below.
        unsafe { self.device.device_wait_idle() }.context("wait idle before upscaler switch")?;

        if let Some(mut taa) = self.taa.take() {
            let allocator = self
                .allocator
                .as_ref()
                .expect("allocator missing during upscaler switch")
                .clone();
            // SAFETY: the device is idle, so no submitted command buffer can
            // still reference the TAA history images or descriptor sets.
            unsafe { taa.destroy(&self.device, &allocator) };
            // Composite has been sampling TAA's output; hand it back to the
            // raw HDR attachment before that output disappears.
            if let Some(ref mut composite) = self.composite {
                let raw_hdr_views = composite.hdr_image_views.clone();
                composite.rebind_hdr_views(
                    &self.device,
                    &raw_hdr_views,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                );
            }
        }

        // Every history the old path accumulated describes a different render
        // extent, so none of it survives the switch.
        // Same window the resize path uses — the switch invalidates history
        // for exactly the same reason a resize does.
        const SWITCH_RECOVERY_FRAMES: u32 = 8;

        self.renderer_config.upscaler = mode;
        if let Err(error) = self
            .recreate_swapchain(window_size)
            .with_context(|| format!("rebuild render targets for upscaler {mode}"))
        {
            // #2156 — a `?` here would return with the renderer mid-rebuild:
            // `recreate_swapchain` destroys the framebuffers up front and only
            // rebuilds them at the very end, so an earlier failure (the FSR
            // `upscaler.recreate`, `PresentationPipeline::new`, …) leaves
            // `framebuffers` empty and `presentation` None. The #1211 guard
            // then converts that into "skip every frame, forever" — nothing in
            // the frame loop ever retries, so the window freezes until the user
            // happens to resize it. Roll the mode back and take the resize path
            // once more: the previous configuration rendered a frame ago, so it
            // is the best shot at landing in a drawable state. The switch
            // request itself is dropped, which is what the doc comment above
            // already promises. A failing rollback is unrecoverable and
            // propagates — the call site treats that as fatal.
            log::error!("upscaler switch {previous} -> {mode} failed: {error:#} — rolling back");
            self.renderer_config.upscaler = previous;
            self.recreate_swapchain(window_size).with_context(|| {
                format!("roll back render targets to upscaler {previous} after failed switch")
            })?;
            if previous == super::super::upscaling::UpscalerMode::Taa {
                // The TAA pipeline was destroyed on the way in; the rollback
                // has to rebuild it or `previous` is only nominally restored.
                self.build_taa_pipeline();
            }
            self.signal_temporal_discontinuity(SWITCH_RECOVERY_FRAMES);
            return Ok(());
        }

        if mode == super::super::upscaling::UpscalerMode::Taa {
            self.build_taa_pipeline();
        }

        self.signal_temporal_discontinuity(SWITCH_RECOVERY_FRAMES);
        log::info!(
            "Upscaler switched {previous} -> {mode} (render {}x{}, output {}x{})",
            self.frame_extents.render.width,
            self.frame_extents.render.height,
            self.frame_extents.output.width,
            self.frame_extents.output.height,
        );
        Ok(())
    }

    /// Build the TAA pipeline for the current render extent and point
    /// composite at its output. Mirrors the construction block in
    /// `VulkanContext::new`, which cannot be shared because that one runs
    /// before `self` exists. A failure here is non-fatal in exactly the same
    /// way it is at startup: composite keeps sampling raw HDR and the frame
    /// renders without temporal anti-aliasing.
    fn build_taa_pipeline(&mut self) {
        let Some(hdr_views) = self
            .composite
            .as_ref()
            .map(|composite| composite.hdr_image_views.clone())
        else {
            log::warn!("composite missing — TAA left disabled after upscaler switch");
            return;
        };
        let Some(gbuffer) = self.gbuffer.as_ref() else {
            log::warn!("G-buffer missing — TAA left disabled after upscaler switch");
            return;
        };
        let n = MAX_FRAMES_IN_FLIGHT;
        let motion_views: Vec<vk::ImageView> = (0..n).map(|i| gbuffer.motion_view(i)).collect();
        let mesh_id_views: Vec<vk::ImageView> = (0..n).map(|i| gbuffer.mesh_id_view(i)).collect();
        let normal_views: Vec<vk::ImageView> = (0..n).map(|i| gbuffer.normal_view(i)).collect();
        let allocator = self
            .allocator
            .as_ref()
            .expect("allocator missing during upscaler switch")
            .clone();

        let mut taa = match super::super::taa::TaaPipeline::new(
            &self.device,
            &allocator,
            self.pipeline_cache,
            super::super::taa::TaaInputViews {
                hdr_views: &hdr_views,
                motion_views: &motion_views,
                mesh_id_views: &mesh_id_views,
                normal_views: &normal_views,
            },
            self.frame_extents.render.width,
            self.frame_extents.render.height,
        ) {
            Ok(taa) => taa,
            Err(error) => {
                log::warn!("TAA pipeline creation failed: {error} — falling back to raw HDR");
                return;
            }
        };
        if let Err(error) = unsafe {
            // SAFETY: the pipeline's images were just created by this device
            // and no frame command buffer has referenced them yet.
            taa.initialize_layouts(&self.device, &self.graphics_queue, self.transfer_pool)
        } {
            log::warn!("TAA layout init failed: {error} — disabling TAA");
            // SAFETY: same as above — nothing has referenced these images.
            unsafe { taa.destroy(&self.device, &allocator) };
            return;
        }
        if let Some(ref mut composite) = self.composite {
            let taa_views: Vec<vk::ImageView> = (0..n).map(|i| taa.output_view(i)).collect();
            composite.rebind_hdr_views(&self.device, &taa_views, vk::ImageLayout::GENERAL);
        }
        self.taa = Some(taa);
        self.taa_failed = false;
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

    // ── #2141 / #2142 — failure-arm descriptor rebinds ──────────────
    //
    // Both bugs are failure-path-only and need a live device to reproduce
    // (`SsaoPipeline::new` / `WaterCausticAccum::recreate_on_resize` must
    // actually fail, realistically under VRAM exhaustion). Static source
    // checks, matching the #654 test above, are what this crate can pin.

    /// `resize.rs` up to (not including) this test module.
    ///
    /// These tests assert on the *absence* of certain code shapes, and
    /// `include_str!` pulls in the assertions themselves — a test that
    /// names the shape it forbids would match its own source and fail
    /// against correct code.
    fn production_src() -> &'static str {
        let src = include_str!("resize.rs");
        src.split("\nmod tests {")
            .next()
            .expect("split always yields a first segment")
    }

    /// #2141 / RL-D6-01 — the SSAO `Err` arm must rebind scene binding 7
    /// to the placeholder.
    ///
    /// The old pipeline's AO images/views are destroyed *before* the
    /// rebuild is attempted, and the failure doesn't propagate, so without
    /// a rebind every subsequent frame samples a destroyed image view.
    #[test]
    fn ssao_recreate_failure_rebinds_binding_7_to_the_placeholder() {
        let src = production_src();

        let destroy_pos = src
            .find("old_ssao.destroy(&self.device, allocator)")
            .expect("the old SSAO pipeline is destroyed before the rebuild");
        let err_arm_pos = src
            .find("SSAO recreation failed after resize")
            .expect("the SSAO Err arm must still exist");
        // Match the call name only, not its argument list — rustfmt breaks
        // the args across lines once the arm grows, and a whitespace-exact
        // needle would make this test fail on a pure reformat.
        let rebind_pos = src[err_arm_pos..]
            .find("write_ao_texture(")
            .map(|off| err_arm_pos + off)
            .expect(
                "the SSAO Err arm must rebind scene binding 7 to the AO placeholder — \
                 the old AO views were already destroyed, and `triangle.frag` samples \
                 `aoTexture` unconditionally (#2141 / RL-D6-01)",
            );

        assert!(
            destroy_pos < err_arm_pos && err_arm_pos < rebind_pos,
            "the rebind must live inside the Err arm that follows the destroy"
        );
        assert!(
            src[err_arm_pos..rebind_pos].contains("placeholder_ao"),
            "the Err arm must rebind to `self.placeholder_ao`, not to a fresh \
             allocation — the realistic trigger for this arm is VRAM exhaustion, \
             where allocating on the failure path is exactly what won't work"
        );
    }

    /// #2142 / RL-D6-02 — the water set-2 rebind must NOT be gated on the
    /// accumulator being present.
    ///
    /// The pre-fix `if let (Some(w), Some(accum))` skipped the rebind on
    /// exactly the two arms that had just destroyed the accumulator,
    /// leaving set 2 naming a freed storage view that the shader then
    /// wrote via `imageAtomicAdd`.
    #[test]
    fn water_caustic_rebind_is_not_gated_on_accumulator_presence() {
        let src = production_src();

        assert!(
            !src.contains("if let (Some(w), Some(accum)) = (self.water.as_ref()"),
            "the water set-2 rebind must not be gated on the accumulator being \
             Some — that guard skipped the rebind on the two failure arms that \
             had just destroyed it (#2142 / RL-D6-02)"
        );

        let rebind_pos = src
            .find("update_water_caustic_descriptors(&self.device, &views)")
            .expect("the water set-2 rebind must still exist");
        // Walk back to the enclosing `if let Some(w) = self.water` and
        // confirm the placeholder fallback sits between it and the rebind.
        let block_pos = src
            .find("if let Some(w) = self.water.as_ref()")
            .expect("the rebind must be gated on the water pipeline alone");
        assert!(block_pos < rebind_pos);
        assert!(
            src[block_pos..rebind_pos].contains("placeholder_caustic_sink"),
            "when the accumulator is absent, set 2 must be rebound to the 1×1 \
             storage sink — `record_draw` binds set 2 unconditionally and the \
             water draw is gated only on `self.water` (#2142)"
        );
    }

    /// #2156 / RL-D6-03 — a failed `recreate_swapchain` inside
    /// `set_upscaler_mode` must roll the mode back and rebuild, not
    /// `?`-propagate.
    ///
    /// `recreate_swapchain` drains the framebuffers up front and rebuilds them
    /// last, so a `?` from anything in between (`upscaler.recreate`,
    /// `PresentationPipeline::new`, …) leaves `framebuffers` empty and
    /// `presentation` None. The #1211 guard then turns every subsequent frame
    /// into a skip, and nothing retries — a frozen window with one log line.
    /// Static source check: the failing calls need a real allocation/SDK
    /// failure that `cargo test` cannot induce.
    #[test]
    fn upscaler_switch_failure_rolls_back_instead_of_propagating() {
        let src = production_src();

        let start = src
            .find("pub fn set_upscaler_mode")
            .expect("set_upscaler_mode disappeared");
        let end = start
            + src[start..]
                .find("fn build_taa_pipeline")
                .expect("build_taa_pipeline follows set_upscaler_mode");
        let body = &src[start..end];

        let first_rebuild = body
            .find("rebuild render targets for upscaler")
            .expect("the switch must still rebuild the render targets");
        let rollback = body
            .find("self.renderer_config.upscaler = previous")
            .expect(
                "the failure arm must roll `renderer_config.upscaler` back to \
                 `previous` — otherwise the config claims the new upscaler while \
                 no target for it exists (#2156)",
            );
        let retry = body.find("roll back render targets to upscaler").expect(
            "the failure arm must re-enter recreate_swapchain for the previous \
                 upscaler — a mode rollback without a rebuild still leaves the \
                 framebuffers drained (#2156)",
        );
        assert!(
            first_rebuild < rollback && rollback < retry,
            "expected rebuild -> mode rollback -> rebuild ordering in \
             set_upscaler_mode's failure arm (#2156)",
        );
        assert!(
            body[rollback..retry].contains("build_taa_pipeline")
                || body[retry..].contains("build_taa_pipeline"),
            "rolling back to a TAA `previous` must rebuild the TAA pipeline the \
             switch destroyed on the way in (#2156)",
        );
    }

    /// Both placeholders are created once at context init, never on the
    /// failure path. Pins the ordering that makes the `None` arms in
    /// `context/mod.rs` able to fall back at all.
    #[test]
    fn placeholders_are_created_before_the_passes_that_fall_back_to_them() {
        let src = include_str!("mod.rs");

        let ao_create = src
            .find("PlaceholderImage::new_white_ao")
            .expect("the AO placeholder must be created at init");
        let sink_create = src
            .find("PlaceholderImage::new_storage_sink")
            .expect("the caustic-sink placeholder must be created at init");
        let ssao_init = src
            .find("let ssao = match SsaoPipeline::new")
            .expect("SSAO init block");
        let accum_init = src
            .find("let water_caustic_accum = match")
            .expect("water-caustic accumulator init block");

        assert!(
            ao_create < ssao_init,
            "the AO placeholder must exist before the SSAO init block, or its \
             failure arm has nothing to rebind binding 7 to (#2141)"
        );
        assert!(
            sink_create < accum_init,
            "the caustic sink must exist before the accumulator init block (#2142)"
        );
    }

    /// #2142 — the init-path set-2 wiring must also fall back, and the
    /// stale "scaffold-only window" justification must be gone.
    #[test]
    fn init_path_water_set_2_falls_back_and_drops_the_stale_comment() {
        let src = include_str!("mod.rs");

        assert!(
            !src.contains("won't fire during the\n        // scaffold-only window"),
            "the `sunDirection.w > 0` / scaffold-only-window justification is stale — \
             Phase D and Phase E (#1255 / #1257) shipped, so set 2 is now bound \
             unconditionally AND written by the shader (#2142)"
        );

        let init_block = src
            .find("if let Some(w) = water.as_ref()")
            .expect("init-path set-2 wiring must be gated on the water pipeline alone");
        let rebind = src[init_block..]
            .find("update_water_caustic_descriptors(&device, &views)")
            .map(|off| init_block + off)
            .expect("init-path rebind must still exist");
        assert!(
            src[init_block..rebind].contains("placeholder_caustic_sink"),
            "when the accumulator fails to create, init must still write set 2 — \
             leaving it unwritten is an atomic write to an uninitialised \
             descriptor once the water draw runs (#2142)"
        );
    }
}
