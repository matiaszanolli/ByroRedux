//! Construction stages of [`VulkanContext::build_pipelines_and_finish`] — each
//! optional pass (or tightly-coupled group) built by one function that takes
//! only the handles it needs and returns only what it makes. (#6143, split
//! out of the 1373-line `build_pipelines_and_finish`.)
//!
//! Every body is moved verbatim: construction only, no command recording, and
//! the call order in `build_pipelines_and_finish` is the original statement
//! order. Failure policy is unchanged — a pass that was soft-fail stays
//! soft-fail (`None` plus a warning), a hard requirement still returns `Err`.
//!
//! Source-scan tests that pin construction text read `INIT_SOURCES` (in `context/mod.rs`),
//! the concatenation of this file and `init.rs`.

use super::*;

/// Cluster-cull and skinning compute pipelines plus the per-pass GPU timers.
pub(super) struct ComputePasses {
    pub(super) cluster_cull: Option<ClusterCullPipeline>,
    pub(super) skin_compute: Option<super::super::skin_compute::SkinComputePipeline>,
    pub(super) skin_palette: Option<super::super::skin_compute::SkinPaletteComputePipeline>,
    pub(super) gpu_timers: Option<super::super::gpu_timers::GpuPerFrameTimers>,
}

/// The water pipeline, its caustic accumulator and the two placeholders.
pub(super) struct WaterPasses {
    pub(super) water: Option<WaterPipeline>,
    pub(super) placeholder_ao: Option<super::super::placeholder::PlaceholderImage>,
    pub(super) placeholder_caustic_sink: Option<super::super::placeholder::PlaceholderImage>,
    pub(super) water_caustic_accum: Option<super::super::water_caustic::WaterCausticAccum>,
}

/// SKYAL cloud noise + sky-cube bake and the exposure resource + meter.
pub(super) struct SkyAndExposure {
    pub(super) cloud_noise: super::super::cloud_noise::CloudNoiseVolumes,
    pub(super) sky_cube: Option<super::super::sky_cube::SkyCubePipeline>,
    pub(super) exposure: ExposureResource,
    pub(super) exposure_meter: Option<super::super::exposure_meter::ExposureMeterPipeline>,
}

/// The G-buffer and the per-frame view lists later passes bind.
pub(super) struct GBufferStage {
    pub(super) gbuffer: Option<GBuffer>,
    pub(super) raw_indirect_views: Vec<vk::ImageView>,
    pub(super) motion_views_seed: Vec<vk::ImageView>,
    pub(super) mesh_id_views_seed: Vec<vk::ImageView>,
    pub(super) normal_views_for_svgf: Vec<vk::ImageView>,
    pub(super) albedo_views: Vec<vk::ImageView>,
    pub(super) reactive_views: Vec<vk::ImageView>,
    pub(super) transparency_views: Vec<vk::ImageView>,
}

/// SVGF, the ReSTIR reservoirs and the direct-shadow filter.
pub(super) struct SvgfStage {
    pub(super) svgf: Option<SvgfPipeline>,
    pub(super) reservoir_buffers: super::super::restir::ReservoirBuffers,
    pub(super) shadow_filter: Option<super::super::shadow_filter::ShadowFilterPipeline>,
}

/// The caustic scatter pass and the views composite samples from it.
pub(super) struct CausticStage {
    pub(super) caustic: Option<CausticPipeline>,
    pub(super) caustic_views: Vec<vk::ImageView>,
    pub(super) normal_views_seed: Vec<vk::ImageView>,
}

/// The image-health counter buffers and the presentation pipeline.
pub(super) struct PresentationStage {
    pub(super) image_health_buffers: Vec<super::super::buffer::GpuBuffer>,
    pub(super) presentation: PresentationPipeline,
}

/// Cluster-cull and skinning compute pipelines plus the per-pass GPU timers (steps 12c–12d.5 and #1194).
pub(super) fn create_compute_passes(
    device: &ash::Device,
    gpu_allocator: &SharedAllocator,
    pipeline_cache: vk::PipelineCache,
    scene_buffers: &scene_buffer::SceneBuffers,
    device_caps: &device::DeviceCapabilities,
) -> ComputePasses {
    // 12c. Cluster cull compute pipeline (light culling)
    let cluster_cull = match ClusterCullPipeline::new(
        &device,
        &gpu_allocator,
        pipeline_cache,
        scene_buffers.light_buffers(),
        scene_buffers.camera_buffers(),
        scene_buffers.light_buffer_size(),
        scene_buffers.camera_buffer_size(),
    ) {
        Ok(cc) => {
            // Write cluster buffer references into scene descriptor sets.
            for f in 0..MAX_FRAMES_IN_FLIGHT {
                scene_buffers.write_cluster_buffers(
                    &device,
                    f,
                    cc.grid_buffer(f),
                    cc.grid_buffer_size(),
                    cc.index_buffer(f),
                    cc.index_buffer_size(),
                );
            }
            Some(cc)
        }
        Err(e) => {
            log::warn!(
                "Cluster cull pipeline creation failed: {e} — falling back to all-lights loop"
            );
            None
        }
    };

    // 12d. Skin compute pipeline (M29 Phase 2) — feeds the skinned
    // BLAS refit path. Created with the max slot ceiling `SKIN_MAX_SLOTS`
    // — the same ceiling the bone-palette upload path enforces in
    // `build_render_data`. Buffer bindings are
    // deferred to per-dispatch (cell-transition robustness).
    // See the module-level `SKIN_MAX_SLOTS` const in `context/mod.rs`
    // for the value and rationale (#5217: 32 was the pre-#900 ceiling;
    // the live value is `(MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1`).
    let mut skin_compute = match super::super::skin_compute::SkinComputePipeline::new(
        &device,
        pipeline_cache,
        SKIN_MAX_SLOTS,
    ) {
        Ok(sc) => Some(sc),
        Err(e) => {
            log::warn!(
                "Skin compute pipeline creation failed: {e} — \
                 skinned RT shadows disabled (raster inline-skinning unaffected)"
            );
            None
        }
    };

    // 12d.5. M29.5 — GPU bone-palette compute pipeline. Construction
    // failure logs but
    // doesn't abort; downstream `skin_palette.is_some()` checks
    // skip the dispatch (no CPU-multiply fallback exists — the
    // legacy `upload_bones` + staging-copy path is removed since
    // M29.5 cleanup, and the engine has no supported no-RT mode).
    let skin_palette = match super::super::skin_compute::SkinPaletteComputePipeline::new(
        &device,
        pipeline_cache,
    ) {
        Ok(sp) => Some(sp),
        Err(e) => {
            log::warn!(
                "Skin palette compute pipeline creation failed: {e} — \
                 GPU bone-palette dispatch disabled (M29.5)"
            );
            None
        }
    };
    // #1783 / CONC-D2-01 — couple the two pipelines. See
    // `couple_skin_compute_to_palette`'s doc for the full rationale.
    skin_compute = couple_skin_compute_to_palette(skin_compute, skin_palette.is_some());

    // #1194 — per-pass GPU timer. Best-effort: failure to create
    // the query pools (driver lacks timestamp_compute_and_graphics,
    // or pool allocation errored) leaves `gpu_timers = None`, the
    // brackets in `draw_frame` no-op, and `skin.coverage` shows
    // `gpu_timer: unavailable` instead of ms values.
    let gpu_timers =
        match super::super::gpu_timers::GpuPerFrameTimers::new(&device, &device_caps) {
            Ok(t) => t,
            Err(e) => {
                log::warn!(
                    "GPU per-pass timer creation failed: {e} — PERF-DIM7 \
                 instrumentation is unavailable"
                );
                None
            }
        };
    if gpu_timers.is_none() {
        log::warn!(
            "GPU timers unavailable; adaptive ray quality will run open-loop \
             and promote GI conservatively to its normal tier"
        );
    }
    ComputePasses { cluster_cull, skin_compute, skin_palette, gpu_timers }
}

/// The EXAL ground-cover blade pipeline and its authored-model tier (#4054 / #4413 / #4888). Both are gated on the combined indirect-draw predicate (#4827).
pub(super) fn create_ground_cover(
    device: &ash::Device,
    gpu_allocator: &SharedAllocator,
    device_caps: &device::DeviceCapabilities,
    render_pass: vk::RenderPass,
    pipeline_cache: vk::PipelineCache,
    texture_registry: &TextureRegistry,
    scene_buffers: &scene_buffer::SceneBuffers,
) -> (Option<super::super::groundcover::GroundCoverPipeline>, Option<super::super::groundcover_models::GroundCoverModelTier>) {
    let groundcover = if !device_caps.indirect_draws_supported() {
        log::warn!(
            "Ground-cover blade pipeline disabled: the device lacks multiDrawIndirect \
             and/or drawIndirectFirstInstance (the blade draw multi-draws up to \
             GROUNDCOVER_MAX_CHUNKS chunks per LOD stream) — no ground cover"
        );
        None
    } else {
        match super::super::groundcover::GroundCoverPipeline::new(
            &device,
            &gpu_allocator,
            render_pass,
            pipeline_cache,
            texture_registry.descriptor_set_layout,
            scene_buffers.descriptor_set_layout,
        ) {
            Ok(gc) => Some(gc),
            Err(e) => {
                log::warn!("Ground-cover pipeline creation failed: {e} — no ground cover");
                None
            }
        }
    };

    // #4413 — the authored-model tier reads the scatter's chunk records,
    // so it exists only beside the blade pipeline; its placement traces
    // the same cover-test ray.
    //
    // #4827 — its draws are GPU-emitted `VkDrawIndexedIndirectCommand`s
    // whose `firstInstance` is the tier's slot in the instance-SSBO tail
    // (non-zero whenever the frame has any main instance), so it needs
    // `drawIndirectFirstInstance` and has no direct-draw fallback: on a
    // device without both indirect features the tier is simply not
    // created, exactly like a failed creation below. (The blade gate
    // above already implies this one today; it stays as its own gate
    // so the tier's firstInstance requirement is argued on its own.)
    let groundcover_models = if groundcover.is_some() && !device_caps.indirect_draws_supported()
    {
        log::warn!(
            "Ground-cover model tier disabled: the device lacks multiDrawIndirect and/or \
             drawIndirectFirstInstance (its indirect draws carry a non-zero firstInstance) \
             — no authored ground cover"
        );
        None
    } else if groundcover.is_some() {
        match super::super::groundcover_models::GroundCoverModelTier::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
        ) {
            Ok(tier) => Some(tier),
            Err(e) => {
                log::warn!(
                    "Ground-cover model tier creation failed: {e} — no authored ground cover"
                );
                None
            }
        }
    } else {
        None
    };
    (groundcover, groundcover_models)
}

/// The water pipeline, the two 1×1 placeholders the optional passes fall back to, and the water-caustic accumulator (steps 15a–15b, #2141 / #2142).
pub(super) fn create_water_passes(
    device: &ash::Device,
    gpu_allocator: &SharedAllocator,
    graphics_queue: &Arc<Mutex<vk::Queue>>,
    transfer_pool: vk::CommandPool,
    render_pass: vk::RenderPass,
    pipeline_cache: vk::PipelineCache,
    texture_registry: &TextureRegistry,
    scene_buffers: &scene_buffer::SceneBuffers,
    render_extent: vk::Extent2D,
) -> WaterPasses {
    let mut water = match WaterPipeline::new(
        &device,
        &gpu_allocator,
        render_pass,
        pipeline_cache,
        texture_registry.descriptor_set_layout,
        scene_buffers.descriptor_set_layout,
    ) {
        Ok(w) => Some(w),
        Err(e) => {
            log::warn!("Water pipeline creation failed: {e} — water surfaces will not render");
            None
        }
    };

    // 15b. Water-caustic accumulator (#1255 / Phase C of #1210).
    // Per-FIF R32_UINT image, cleared pre-render-pass each frame,
    // written by `water.frag::imageAtomicAdd` during the main
    // pass (once Phase D activates the consumer), sampled by
    // `composite.frag` (Phase E) alongside the existing caustic
    // accumulator. Failure degrades gracefully — water still
    // renders, just without the caustic contribution path.
    // #2141 / #2142 — the two 1×1 placeholders, created BEFORE the
    // optional passes that fall back to them so the `None` arms below
    // have something live to rebind to. A failure here is itself
    // non-fatal: the affected binding just keeps its pre-fix behaviour.
    let placeholder_ao = match super::super::placeholder::PlaceholderImage::new_white_ao(
        &device,
        &gpu_allocator,
        &graphics_queue,
        transfer_pool,
    ) {
        Ok(p) => Some(p),
        Err(e) => {
            log::warn!(
                "AO placeholder creation failed: {e} — scene binding 7 has no fallback if SSAO drops out"
            );
            None
        }
    };
    let placeholder_caustic_sink =
        match super::super::placeholder::PlaceholderImage::new_storage_sink(
            &device,
            &gpu_allocator,
            &graphics_queue,
            transfer_pool,
            super::super::caustic::CAUSTIC_FORMAT,
        ) {
            Ok(p) => Some(p),
            Err(e) => {
                log::warn!(
                "Caustic-sink placeholder creation failed: {e} — water set 2 has no fallback if the accumulator drops out"
            );
                None
            }
        };

    let water_caustic_accum = match super::super::water_caustic::WaterCausticAccum::new(
        &device,
        &gpu_allocator,
        render_extent.width,
        render_extent.height,
    ) {
        Ok(a) => {
            // One-time UNDEFINED → GENERAL transition so the first
            // frame's `clear_pre_render_pass` doesn't trip
            // VUID-vkCmdDraw-None-09600 (the barrier assumes
            // `oldLayout = GENERAL`). Mirror of CausticPipeline's
            // initialize_layouts call in the caustic block below.
            if let Err(e) = unsafe {
                // SAFETY: `device` + `graphics_queue` are live and
                // `transfer_pool` is a command pool allocated from this
                // device; `a`'s caustic-accumulator images were just created
                // above by the same device, so recording their one-time
                // layout transition is sound.
                a.initialize_layouts(&device, &graphics_queue, transfer_pool)
            } {
                log::warn!(
                    "Water-caustic initialize_layouts failed: {e} — disabling for the rest of the session"
                );
                crate::vulkan::texture::release_after_failed_submit(a, &e, |mut a| unsafe {
                    // SAFETY: the layout submit never reached the queue (the
                    // helper leaks the accumulator otherwise, #6146) and no frame
                    // command buffer has referenced its images.
                    a.destroy(&device, &gpu_allocator)
                });
                None
            } else {
                Some(a)
            }
        }
        Err(e) => {
            log::warn!(
                "Water-caustic accumulator creation failed: {e} — water-side caustics disabled this session"
            );
            None
        }
    };

    // Wire the WaterPipeline's set 2 descriptors at the matching
    // WaterCausticAccum slot views, falling back to the 1×1 storage
    // sink when the accumulator failed to create.
    //
    // #2142 — the previous comment here claimed an unwritten set 2 was
    // "safe because Phase D's shader-side read is gated on
    // `sunDirection.w > 0` and won't fire during the scaffold-only
    // window". That window closed when Phase D and Phase E shipped
    // (#1255 / #1257): `record_draw` binds set 2 unconditionally and
    // the shader now *writes* it via `imageAtomicAdd`, so leaving the
    // descriptor unwritten (init) or pointing at a destroyed view
    // (resize failure) is an atomic write to freed memory, not a
    // harmless no-op.
    let water_was_enabled = water.is_some();
    water = couple_water_to_caustic_sink(
        water,
        water_caustic_accum.is_some(),
        placeholder_caustic_sink.is_some(),
    );
    if water_was_enabled && water.is_none() {
        log::error!(
            "Water pipeline has neither a caustic accumulator nor a placeholder storage sink; disabling water to preserve descriptor validity"
        );
    }
    if let Some(w) = water.as_ref() {
        let views: Option<Vec<vk::ImageView>> = match water_caustic_accum.as_ref() {
            Some(accum) => Some(
                (0..super::sync::MAX_FRAMES_IN_FLIGHT)
                    .map(|i| accum.storage_view(i))
                    .collect(),
            ),
            // Same view in every FIF slot: nothing reads the sink back,
            // so the slots have no reason to stay distinct.
            None => placeholder_caustic_sink
                .as_ref()
                .map(|p| vec![p.gpu.view; super::sync::MAX_FRAMES_IN_FLIGHT]),
        };
        let views = views.expect(
            "water/caustic coupling must disable water when both storage sinks are absent",
        );
        w.update_water_caustic_descriptors(&device, &views);
    }
    WaterPasses { water, placeholder_ao, placeholder_caustic_sink, water_caustic_accum }
}

/// The SSAO pipeline (step 14a). On failure binding 7 is pointed at the white placeholder (#2141).
pub(super) fn create_ssao(
    device: &ash::Device,
    gpu_allocator: &SharedAllocator,
    graphics_queue: &Arc<Mutex<vk::Queue>>,
    transfer_pool: vk::CommandPool,
    pipeline_cache: vk::PipelineCache,
    depth_image_view: vk::ImageView,
    render_extent: vk::Extent2D,
    scene_buffers: &scene_buffer::SceneBuffers,
    placeholder_ao: &Option<super::super::placeholder::PlaceholderImage>,
) -> Option<SsaoPipeline> {
    // 14a. SSAO pipeline (reads depth buffer after render pass)
    let ssao = match SsaoPipeline::new(
        &device,
        &gpu_allocator,
        pipeline_cache,
        depth_image_view,
        render_extent.width,
        render_extent.height,
    ) {
        Ok(s) => {
            // Transition AO image from UNDEFINED to SHADER_READ_ONLY_OPTIMAL
            // so the first frame's fragment shader sees a valid layout (1.0 =
            // no occlusion). Without this, sampling UNDEFINED is UB.
            if let Err(e) = unsafe {
                // SAFETY: `device` + `graphics_queue` are live and
                // `transfer_pool` is a command pool from this device; the
                // SSAO pipeline `s`'s AO images were just created above by
                // the same device, so recording their UNDEFINED →
                // SHADER_READ_ONLY transition is sound.
                s.initialize_ao_images(&device, &graphics_queue, transfer_pool)
            } {
                log::warn!("SSAO AO image init failed: {e}");
            }
            for f in 0..MAX_FRAMES_IN_FLIGHT {
                scene_buffers.write_ao_texture(&device, f, s.ao_image_view(f), s.ao_sampler);
            }
            Some(s)
        }
        Err(e) => {
            // #2141 — binding 7 would otherwise be left entirely
            // unwritten here, and `triangle.frag` samples `aoTexture`
            // with no gate. Point it at the white placeholder so the
            // degraded path reads "no occlusion" instead of an
            // uninitialised descriptor.
            log::warn!("SSAO pipeline creation failed: {e} — no ambient occlusion");
            if let Some(p) = placeholder_ao.as_ref() {
                for f in 0..MAX_FRAMES_IN_FLIGHT {
                    scene_buffers.write_ao_texture(&device, f, p.gpu.view, p.sampler);
                }
            }
            None
        }
    };
    ssao
}

/// The SKYAL cloud-noise volumes and sky-cube bake, the exposure resource and its metering pass (steps 14a-noise – 14b-bis). Cloud noise and exposure are hard requirements.
pub(super) fn create_sky_and_exposure(
    device: &ash::Device,
    gpu_allocator: &SharedAllocator,
    graphics_queue: &Arc<Mutex<vk::Queue>>,
    transfer_pool: vk::CommandPool,
    pipeline_cache: vk::PipelineCache,
    texture_registry: &TextureRegistry,
    scene_buffers: &scene_buffer::SceneBuffers,
) -> Result<SkyAndExposure> {
    // 14a-noise. SKYAL cloud density volumes, shared by the sky-cube bake
    // below and composite further down. Created before both because both
    // write its views into their descriptor sets at construction.
    // Mandatory, unlike the bake: composite needs them and is mandatory.
    //
    // SAFETY: `device`, `graphics_queue` and `transfer_pool` are live and
    // the pool belongs to this device; the volumes are created inside the
    // call, so nothing can be in flight against them.
    let cloud_noise = unsafe {
        super::super::cloud_noise::CloudNoiseVolumes::new(
            &device,
            &gpu_allocator,
            &graphics_queue,
            transfer_pool,
        )
    }
    .map_err(|e| anyhow::anyhow!("SKYAL cloud noise volumes creation failed: {e}"))?;

    // 14a-bis. SKYAL sky-cubemap bake. Optional, like SSAO: on failure
    // the engine renders without it and `raytrace.glsl` falls back to
    // the pre-SKYAL flat sky blend, gated on the ready flag in
    // `GpuCamera::exterior_sky_tint`'s w lane.
    //
    // Unlike SSAO there is no placeholder arm: binding 20 is
    // PARTIALLY_BOUND and the shader reads it ONLY when that flag is
    // set, so leaving it unwritten on the failure path is safe — which
    // is the whole reason the flag exists rather than relying on the
    // binding being present.
    let sky_cube = match super::super::sky_cube::SkyCubePipeline::new(
        &device,
        &gpu_allocator,
        pipeline_cache,
        texture_registry.descriptor_set_layout,
        cloud_noise.views(),
        MAX_FRAMES_IN_FLIGHT,
    ) {
        Ok(s) => {
            for f in 0..MAX_FRAMES_IN_FLIGHT {
                scene_buffers.write_sky_cube(
                    &device,
                    f,
                    s.cube_view(f),
                    s.sampler,
                    s.irradiance_buffer(f),
                    s.cloud_shadow_view(f),
                );
            }
            Some(s)
        }
        Err(e) => {
            log::warn!(
                "sky cubemap pipeline creation failed: {e} — \
                 sky reflections fall back to the flat blend"
            );
            None
        }
    };

    // 14b. Exposure producer (per-frame-in-flight 1x1 R32_SFLOAT).
    // Hard-fail since Stage 1: presentation's set-0 binding 2 samples
    // `exposureTex` unconditionally, so — like bloom's #1081 policy —
    // there is no meaningful degraded mode without it. A 1x1 image
    // allocation failing means the device is not usable anyway.
    let exposure =
        match ExposureResource::new(&device, &gpu_allocator, &graphics_queue, transfer_pool) {
            Ok(e) => e,
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Exposure resource failed to initialize — presentation \
                     requires the exposure texel for binding 2 (Stage 1). \
                     Check earlier WARN logs. ({e})"
                ));
            }
        };

    // 14b-bis. Exposure metering pass (Stage 1). Soft-fail: the
    // exposure slots are cleared to DEFAULT_EXPOSURE, so a meter-less
    // session degrades to frozen fixed exposure rather than aborting.
    let exposure_meter = match super::super::exposure_meter::ExposureMeterPipeline::new(
        &device,
        &gpu_allocator,
        pipeline_cache,
    ) {
        Ok(m) => Some(m),
        Err(e) => {
            log::warn!(
                "Exposure meter pipeline creation failed: {e} — exposure is \
                 frozen at the fixed default for this session"
            );
            None
        }
    };
    Ok(SkyAndExposure { cloud_noise, sky_cube, exposure, exposure_meter })
}

/// The procedural volumetrics pipeline and its one-time froxel layout transition (step 14a-bis, #5947).
pub(super) fn create_volumetrics(
    device: &ash::Device,
    gpu_allocator: &SharedAllocator,
    graphics_queue: &Arc<Mutex<vk::Queue>>,
    transfer_pool: vk::CommandPool,
    pipeline_cache: vk::PipelineCache,
    vk_instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    render_extent: vk::Extent2D,
    renderer_config: &RendererConfig,
) -> Option<VolumetricsPipeline> {
    // 14a-bis. Procedural volumetrics. Froxel XY derives from the render
    // extent after FSR sizing; Z/reach come from the validated renderer
    // config. Each FIF slot owns raw V-buffer + integrated RGBA16F volumes.
    let max_image_dimension_3d = unsafe {
        // SAFETY: `physical_device` was selected from `vk_instance` and
        // both remain live for the duration of context construction.
        vk_instance
            .get_physical_device_properties(physical_device)
            .limits
            .max_image_dimension3_d
    };
    let mut volumetrics = match VolumetricsPipeline::new(
        &device,
        &gpu_allocator,
        pipeline_cache,
        render_extent,
        renderer_config.volumetrics,
        max_image_dimension_3d,
    ) {
        Ok(v) => Some(v),
        Err(e) => {
            log::warn!("Volumetrics pipeline creation failed: {e} — no volumetric lighting");
            None
        }
    };
    if let Some(ref mut v) = volumetrics {
        if let Err(e) = unsafe {
            // SAFETY: `device` + `graphics_queue` are live and
            // `transfer_pool` is a command pool from this device; the
            // volumetrics pipeline `v`'s froxel images were just created
            // above by the same device, so recording their one-time layout
            // transition and immutable density uploads is sound.
            v.initialize_layouts(&device, &gpu_allocator, &graphics_queue, transfer_pool)
        } {
            log::warn!("Volumetrics froxel layout init failed: {e} — disabling volumetrics");
            if let Some(mut pipe) = volumetrics.take() {
                if crate::vulkan::texture::OneTimeCommandError::may_be_in_flight(&e) {
                    // #5947 — the layout/noise submit may still be
                    // executing against the froxel and noise images, so a
                    // host-side destroy could race it. Leak them, as the
                    // #4891 upload paths do.
                    std::mem::forget(pipe);
                } else {
                    unsafe {
                        // SAFETY: `pipe` was just created by `device`; the
                        // layout submit never reached the queue and no
                        // frame command buffer has referenced its images,
                        // so destroying it is sound.
                        pipe.destroy(&device, &gpu_allocator)
                    };
                }
            }
        }
    }
    volumetrics
}

/// The G-buffer and the per-frame view lists the later passes bind (step 14b). Created before composite because composite samples its raw_indirect and albedo views.
pub(super) fn create_gbuffer(
    device: &ash::Device,
    gpu_allocator: &SharedAllocator,
    graphics_queue: &Arc<Mutex<vk::Queue>>,
    transfer_pool: vk::CommandPool,
    render_extent: vk::Extent2D,
) -> Result<GBufferStage> {
    // 14b. G-buffer: all auxiliary attachments (normal, motion, mesh_id,
    // raw_indirect, albedo). Created BEFORE composite because composite's
    // descriptor sets reference the raw_indirect + albedo views.
    let mut gbuffer = Some(GBuffer::new(
        &device,
        &gpu_allocator,
        render_extent.width,
        render_extent.height,
    )?);
    let gbuffer_ref = gbuffer.as_ref().expect("gbuffer must exist");

    // Transition all G-buffer images from UNDEFINED to
    // SHADER_READ_ONLY_OPTIMAL so the "previous frame" slot is in a
    // valid layout on the very first frame (SVGF temporal pass binds
    // the previous frame's mesh_id/motion/raw_indirect for sampling).
    if let Err(e) = unsafe {
        // SAFETY: `device` + `graphics_queue` are live and `transfer_pool`
        // is a command pool from this device; `gbuffer_ref`'s attachment
        // images were just created above by the same device, so recording
        // their UNDEFINED → SHADER_READ_ONLY transition is sound.
        gbuffer_ref.initialize_layouts(&device, &graphics_queue, transfer_pool)
    } {
        // #6343 — a hard requirement, like the caustic block below: every
        // post pass samples these attachments as SHADER_READ_ONLY, so
        // running on with them still UNDEFINED is a layout-mismatch
        // validation error on every frame (the #1389 class).
        if let Some(pass) = gbuffer.take() {
            crate::vulkan::texture::release_after_failed_submit(pass, &e, |mut pass| unsafe {
                // SAFETY: `pass` was just created by `device`; the layout
                // submit never reached the queue (the helper leaks the pass
                // otherwise, #6146) and no frame has referenced its images.
                pass.destroy(&device, &gpu_allocator)
            });
        }
        return Err(anyhow::anyhow!(
            "G-buffer layout init failed: {e} — every post pass samples the G-buffer"
        ));
    }

    // Collect G-buffer views up-front so svgf, composite, and main
    // framebuffer creation can reference them.
    let n_frames = MAX_FRAMES_IN_FLIGHT;
    let raw_indirect_views: Vec<vk::ImageView> = (0..n_frames)
        .map(|i| gbuffer_ref.raw_indirect_view(i))
        .collect();
    let motion_views_seed: Vec<vk::ImageView> =
        (0..n_frames).map(|i| gbuffer_ref.motion_view(i)).collect();
    let mesh_id_views_seed: Vec<vk::ImageView> =
        (0..n_frames).map(|i| gbuffer_ref.mesh_id_view(i)).collect();
    // #650 / SH-5 — SVGF needs the GBuffer normal attachments too
    // for the 2×2 consistency loop's normal-cone rejection. Pulled
    // up from below the SVGF init so the new binding is wired at
    // pipeline-creation time.
    let normal_views_for_svgf: Vec<vk::ImageView> =
        (0..n_frames).map(|i| gbuffer_ref.normal_view(i)).collect();
    let albedo_views: Vec<vk::ImageView> =
        (0..n_frames).map(|i| gbuffer_ref.albedo_view(i)).collect();
    let reactive_views: Vec<vk::ImageView> = (0..n_frames)
        .map(|i| gbuffer_ref.reactive_view(i))
        .collect();
    let transparency_views: Vec<vk::ImageView> = (0..n_frames)
        .map(|i| gbuffer_ref.transparency_view(i))
        .collect();
    Ok(GBufferStage { gbuffer, raw_indirect_views, motion_views_seed, mesh_id_views_seed, normal_views_for_svgf, albedo_views, reactive_views, transparency_views })
}

/// The SVGF temporal denoiser, the ReSTIR-DI reservoir buffers and the direct-shadow filter (steps 14b2, ReSTIR, #6249).
pub(super) fn create_svgf_and_restir(
    device: &ash::Device,
    gpu_allocator: &SharedAllocator,
    graphics_queue: &Arc<Mutex<vk::Queue>>,
    transfer_pool: vk::CommandPool,
    pipeline_cache: vk::PipelineCache,
    depth_image_view: vk::ImageView,
    render_extent: vk::Extent2D,
    scene_buffers: &scene_buffer::SceneBuffers,
    raw_indirect_views: &[vk::ImageView],
    motion_views_seed: &[vk::ImageView],
    mesh_id_views_seed: &[vk::ImageView],
    normal_views_for_svgf: &[vk::ImageView],
    albedo_views: &[vk::ImageView],
) -> Result<SvgfStage> {
    let n_frames = MAX_FRAMES_IN_FLIGHT;
    // 14b2. SVGF temporal denoiser — reads raw_indirect + motion +
    // mesh_id from the G-buffer, writes accumulated_indirect images
    // that the composite pass will sample in place of raw_indirect.
    // Created before composite so composite's descriptor sets can
    // reference SVGF's indirect_history views.
    let mut svgf = match SvgfPipeline::new(
        &device,
        &gpu_allocator,
        pipeline_cache,
        super::super::svgf::SvgfInputViews {
            raw_indirect_views: &raw_indirect_views,
            motion_views: &motion_views_seed,
            mesh_id_views: &mesh_id_views_seed,
            normal_views: &normal_views_for_svgf,
            albedo_views: &albedo_views,
            depth_view: depth_image_view,
        },
        render_extent.width,
        render_extent.height,
    ) {
        Ok(s) => Some(s),
        Err(e) => {
            log::warn!("SVGF pipeline creation failed: {e} — falling back to raw indirect");
            None
        }
    };
    // Transition history images UNDEFINED → GENERAL so first dispatch
    // and first descriptor sampling see a valid layout.
    if let Some(ref s) = svgf {
        if let Err(e) = unsafe {
            // SAFETY: `device` + `graphics_queue` are live and
            // `transfer_pool` is a command pool from this device; the SVGF
            // pipeline `s`'s history images were just created above by the
            // same device, so recording their UNDEFINED → GENERAL
            // transition is sound.
            s.initialize_layouts(&device, &graphics_queue, transfer_pool)
        } {
            log::warn!("SVGF layout init failed: {e} — disabling SVGF");
            if let Some(pipe) = svgf.take() {
                crate::vulkan::texture::release_after_failed_submit(pipe, &e, |mut pipe| unsafe {
                    // SAFETY: `pipe` was just created by `device`; the layout
                    // submit never reached the queue (the helper leaks the pass
                    // otherwise, #6146) and no frame has referenced its images.
                    pipe.destroy(&device, &gpu_allocator)
                });
            }
        }
    }

    // ReSTIR-DI reservoir buffers (screen-sized, ping-pong per FIF).
    // Written into the scene descriptor set (bindings 16/17) here and
    // re-written after a resize recreates them. The fragment shader
    // gates use on `!DBG_DISABLE_RESTIR`. See `vulkan::restir`.
    let reservoir_buffers = super::super::restir::ReservoirBuffers::new(
        &device,
        &gpu_allocator,
        &graphics_queue,
        transfer_pool,
        render_extent.width,
        render_extent.height,
    )?;
    for i in 0..n_frames {
        scene_buffers.write_reservoir_buffers(&device, i, &reservoir_buffers);
    }
    // Direct-shadow spatial filter over the reservoirs' shadow ratio;
    // composite binds its correction image when it exists. The mesh-ID
    // views are the G-buffer's per-frame slots (also `mesh_id_views_seed`
    // above) — the filter's ownership gate samples them (#6249).
    let shadow_filter = match super::super::shadow_filter::ShadowFilterPipeline::new(
        super::super::GpuUploadCtx {
            device: &device,
            allocator: &gpu_allocator,
            queue: &graphics_queue,
            command_pool: transfer_pool,
        },
        pipeline_cache,
        &reservoir_buffers,
        &mesh_id_views_seed,
        render_extent.width,
        render_extent.height,
    ) {
        Ok(f) => Some(f),
        Err(e) => {
            log::warn!("shadow filter creation failed: {e} — direct shadows unfiltered");
            None
        }
    };
    Ok(SvgfStage { svgf, reservoir_buffers, shadow_filter })
}

/// The caustic scatter pass (step 14b-bis, #321). Composite binding 5 needs its RGB array, so both creation and the layout transition are hard requirements.
pub(super) fn create_caustic(
    device: &ash::Device,
    gpu_allocator: &SharedAllocator,
    graphics_queue: &Arc<Mutex<vk::Queue>>,
    transfer_pool: vk::CommandPool,
    pipeline_cache: vk::PipelineCache,
    depth_image_view: vk::ImageView,
    render_extent: vk::Extent2D,
    scene_buffers: &scene_buffer::SceneBuffers,
    gbuffer: &Option<GBuffer>,
    mesh_id_views_seed: &[vk::ImageView],
) -> Result<CausticStage> {
    let n_frames = MAX_FRAMES_IN_FLIGHT;
    let gbuffer_ref = gbuffer.as_ref().expect("gbuffer must exist");
    // 14b-bis. Caustic scatter pass (#321). Sits between SVGF and
    // composite so composite's binding 5 can sample its R32_UINT
    // accumulator. The compute shader fires ray queries against the
    // TLAS and uses the full set of per-FIF scene buffers, so all of
    // those need to exist (they do — this runs after SceneBuffers and
    // AccelerationManager are built).
    let normal_views_seed: Vec<vk::ImageView> =
        (0..n_frames).map(|i| gbuffer_ref.normal_view(i)).collect();
    let mut caustic: Option<CausticPipeline> = match CausticPipeline::new(
        &device,
        &gpu_allocator,
        pipeline_cache,
        depth_image_view,
        &normal_views_seed,
        &mesh_id_views_seed,
        scene_buffers.light_buffers(),
        scene_buffers.light_buffer_size(),
        scene_buffers.camera_buffers(),
        scene_buffers.camera_buffer_size(),
        scene_buffers.instance_buffers(),
        &scene_buffers.instance_buffer_sizes(),
        render_extent.width,
        render_extent.height,
    ) {
        Ok(c) => Some(c),
        Err(e) => {
            return Err(anyhow::anyhow!("Caustic pipeline creation failed: {e}"));
        }
    };
    if let Some(ref c) = caustic {
        if let Err(e) = unsafe {
            // SAFETY: `device` + `graphics_queue` are live and
            // `transfer_pool` is a command pool from this device; the
            // caustic pipeline `c`'s images were just created above by the
            // same device, so recording their one-time layout transition
            // is sound.
            c.initialize_layouts(&device, &graphics_queue, transfer_pool)
        } {
            if let Some(pipe) = caustic.take() {
                crate::vulkan::texture::release_after_failed_submit(pipe, &e, |mut pipe| unsafe {
                    // SAFETY: `pipe` was just created by `device`; the layout
                    // submit never reached the queue (the helper leaks the pass
                    // otherwise, #6146) and no frame has referenced its images.
                    pipe.destroy(&device, &gpu_allocator)
                });
            }
            return Err(anyhow::anyhow!(
                "Caustic layout init failed: {e} — composite binding 5 requires the RGB array"
            ));
        }
    }
    // Composite binding 5 is a `usampler2DArray`; only the RGB caustic
    // views have the required dimensionality. Pipeline creation and
    // layout transition are therefore hard requirements above.
    let caustic_views: Vec<vk::ImageView> = match caustic {
        Some(ref c) => (0..n_frames).map(|i| c.sampled_view(i)).collect(),
        None => unreachable!("caustic initialization returned successfully without a pipeline"),
    };
    Ok(CausticStage { caustic, caustic_views, normal_views_seed })
}

/// The bloom pyramid and its output views (step 14b-bis, M58). Composite samples binding 7 unconditionally, so a missing bloom pass is a hard init error (#1081).
pub(super) fn create_bloom(
    device: &ash::Device,
    gpu_allocator: &SharedAllocator,
    graphics_queue: &Arc<Mutex<vk::Queue>>,
    transfer_pool: vk::CommandPool,
    pipeline_cache: vk::PipelineCache,
    render_extent: vk::Extent2D,
) -> Result<(Option<BloomPipeline>, Vec<vk::ImageView>)> {
    // 14b-bis. Bloom pipeline (M58 Phase 1). Allocates the down/up
    // mip pyramids — does NOT need any input views at this stage
    // because the scene HDR view is rebound per-frame in
    // `dispatch()`. Constructed before composite so we can pass
    // its output views into composite's binding 7.
    //
    // No soft-fail path: composite unconditionally samples binding 7
    // (`bloomTex`) and there is no specialisation-constant gate for
    // the bloom-absent case. A black-dummy image would require a
    // one-time command-buffer submit here; for now we treat bloom
    // allocation failure as a hard init error (image-pyramid
    // allocations are universally supported on all Vulkan 1.1+ GPUs).
    // Tracked: #1081 — if a real dummy is ever needed, implement it
    // in `CompositePipeline::new` with an optional `bloom_views`.
    let bloom = match BloomPipeline::new(&device, &gpu_allocator, pipeline_cache, render_extent)
    {
        Ok(b) => {
            if let Err(e) = unsafe {
                // SAFETY: `device` + `graphics_queue` are live and
                // `transfer_pool` is a command pool from this device; the
                // bloom pipeline `b`'s pyramid images were just created
                // above by the same device, so recording their one-time
                // layout transition is sound.
                b.initialize_layouts(&device, &graphics_queue, transfer_pool)
            } {
                // #6343 — the same hard requirement as the creation arm
                // below: bloom's per-dispatch barriers declare the walked
                // GENERAL layout, so keeping a pass whose images are still
                // UNDEFINED is a validation error on every frame.
                crate::vulkan::texture::release_after_failed_submit(b, &e, |mut b| unsafe {
                    // SAFETY: `b` was just created by `device`; the layout
                    // submit never reached the queue (the helper leaks the
                    // pass otherwise, #6146) and no frame has used it.
                    b.destroy(&device, &gpu_allocator)
                });
                return Err(anyhow::anyhow!(
                    "Bloom pyramid layout init failed: {e} — composite \
                     requires the bloom output view for binding 7 (M58)."
                ));
            }
            Some(b)
        }
        Err(e) => {
            log::warn!("Bloom pipeline creation failed: {e} — no bloom this session");
            None
        }
    };
    let bloom_views: Vec<vk::ImageView> = match bloom.as_ref() {
        Some(b) => b.output_views(),
        None => {
            return Err(anyhow::anyhow!(
                "Bloom pipeline failed to initialize — composite \
                 requires the bloom output view for binding 7 (M58). \
                 Check earlier 'bloom' WARN logs."
            ));
        }
    };
    Ok((bloom, bloom_views))
}

/// The TAA resolve pass (step 14d, #3572), built only in TAA mode; FSR owns temporal reconstruction otherwise.
pub(super) fn create_taa(
    device: &ash::Device,
    gpu_allocator: &SharedAllocator,
    graphics_queue: &Arc<Mutex<vk::Queue>>,
    transfer_pool: vk::CommandPool,
    pipeline_cache: vk::PipelineCache,
    render_extent: vk::Extent2D,
    renderer_config: &RendererConfig,
    composite: &Option<CompositePipeline>,
    motion_views_seed: &[vk::ImageView],
    mesh_id_views_seed: &[vk::ImageView],
    normal_views_seed: &[vk::ImageView],
    reactive_views: &[vk::ImageView],
) -> Option<TaaPipeline> {
    // 14d. TAA resolve pass — #3572: the resolve input is composite's
    // POST-composite SCENE views (the same fully-assembled image FSR
    // resolves), so sky / denoised indirect / volumetrics / caustics /
    // bloom are inside the resolved image on both jitter phases.
    // Composite keeps sampling the raw HDR attachment directly; TAA's
    // output feeds the upscale/presentation tap instead. Plus per-FIF
    // motion, mesh_id, and normal for surface-valid history
    // reprojection.
    let mut taa = if renderer_config.upscaler == UpscalerMode::Taa {
        let n_frames = sync::MAX_FRAMES_IN_FLIGHT;
        let scene_views: Vec<vk::ImageView> = (0..n_frames)
            .map(|i| {
                composite
                    .as_ref()
                    .expect("composite must exist after construction")
                    .scene_view(i)
            })
            .collect();
        match TaaPipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
            super::super::taa::TaaInputViews {
                src_color_views: &scene_views,
                motion_views: &motion_views_seed,
                mesh_id_views: &mesh_id_views_seed,
                normal_views: &normal_views_seed,
                reactive_views: &reactive_views,
            },
            render_extent.width,
            render_extent.height,
        ) {
            Ok(t) => Some(t),
            Err(e) => {
                log::warn!("TAA pipeline creation failed: {e} — falling back to raw HDR");
                None
            }
        }
    } else {
        log::info!(
            "FSR mode active: TAA history/resolve disabled; FSR owns temporal reconstruction"
        );
        None
    };
    if let Some(ref t) = taa {
        if let Err(e) = unsafe {
            // SAFETY: `device` + `graphics_queue` are live and
            // `transfer_pool` is a command pool from this device; the TAA
            // pipeline `t`'s history/output images were just created above
            // by the same device, so recording their one-time layout
            // transition is sound.
            t.initialize_layouts(&device, &graphics_queue, transfer_pool)
        } {
            log::warn!("TAA layout init failed: {e} — disabling TAA");
            if let Some(pipe) = taa.take() {
                crate::vulkan::texture::release_after_failed_submit(pipe, &e, |mut pipe| unsafe {
                    // SAFETY: `pipe` was just created by `device`; the layout
                    // submit never reached the queue (the helper leaks the pass
                    // otherwise, #6146) and no frame has referenced its images.
                    pipe.destroy(&device, &gpu_allocator)
                });
            }
        }
    }
    taa
}

/// The per-frame image-health counter buffers (EX-05 / #2736) and the presentation pipeline that reads them.
pub(super) fn create_health_buffers_and_presentation(
    device: &ash::Device,
    gpu_allocator: &SharedAllocator,
    pipeline_cache: vk::PipelineCache,
    swapchain_state: &SwapchainState,
    frame_extents: FrameExtentSet,
    frame_upscaler: &FrameUpscaler,
    exposure: &ExposureResource,
    pipeline_layout: vk::PipelineLayout,
) -> Result<PresentationStage> {
    // EX-05 / #2736 — one small host-visible counter buffer per frame slot.
    //
    // #2752 / REN-D4-05 — `create_host_readback` (`GpuToCpu`), not
    // `create_host_visible` (`CpuToGpu`). The shader `atomicAdd`s into
    // these and the HOST drains them every frame right after the fence
    // wait, so they are readback buffers; allocating them from the upload
    // preset put a per-frame host-read on memory gpu-allocator steers
    // toward uncached write-combined BAR on a discrete card. `GpuToCpu`
    // additionally prefers `HOST_CACHED` — which is exactly why
    // `collect_image_health` must now invalidate before reading.
    let mut image_health_buffers = Vec::with_capacity(sync::MAX_FRAMES_IN_FLIGHT);
    for _ in 0..sync::MAX_FRAMES_IN_FLIGHT {
        image_health_buffers.push(
            super::super::buffer::GpuBuffer::create_host_readback(
                &device,
                &gpu_allocator,
                IMAGE_HEALTH_BUFFER_BYTES,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )
            .context("create image-health counter buffer")?,
        );
    }
    // gpu-allocator makes no zero-init guarantee, so the very first frame
    // would otherwise read whatever was in the suballocation and report a
    // phantom NaN count before the shader had written anything.
    //
    // This is a host WRITE, so it needs the flush half of the pair — on a
    // non-coherent readback type the zeroing would otherwise sit in a
    // dirty cache line the shader's first `atomicAdd` never sees.
    for buffer in image_health_buffers.iter_mut() {
        if let Ok(bytes) = buffer.mapped_slice_mut() {
            bytes.fill(0);
        }
        if let Err(e) = buffer.flush_if_needed(&device) {
            log::warn!("image-health buffer zero-init flush failed: {e}");
        }
    }
    let health_handles: Vec<vk::Buffer> =
        image_health_buffers.iter().map(|b| b.buffer).collect();
    let presentation = PresentationPipeline::new(
        &device,
        pipeline_cache,
        crate::vulkan::presentation::PresentationTargets {
            swapchain_format: swapchain_state.format.format,
            swapchain_views: &swapchain_state.image_views,
            upscaled_views: &frame_upscaler.output_views(),
            exposure_views: &exposure.views(),
            health_buffers: &health_handles,
            extent: frame_extents.output,
        },
        pipeline_layout,
    )
    .context("create presentation pipeline")?;
    Ok(PresentationStage { image_health_buffers, presentation })
}
