//! `VulkanContext` construction chain — split out of `mod.rs` (#1749 /
//! TD1-004): the single 1025+ LOC `new()` had grown to ~1325 lines as more
//! optional passes (SVGF, SSAO, TAA, bloom, volumetrics, ReSTIR, caustics,
//! water) were added over time, and `context/mod.rs` itself was large
//! mostly because of this constructor + the struct definition (TD1-003).
//!
//! Four ordered phases, each a `Self` method, assembled by `new()`:
//! 1. [`VulkanContext::build_core_device`] — entry → instance → debug →
//!    surface → physical device → logical device → GPU allocator.
//! 2. [`VulkanContext::build_swapchain_and_resources`] — swapchain, depth
//!    resources, render pass, command pools, texture registry, scene
//!    buffers, acceleration manager, pipeline cache (created before ANY
//!    pipeline so every compile shares the warm-start cache).
//! 3. [`VulkanContext::build_pipelines_and_finish`] — every pipeline +
//!    optional pass (cluster cull, skin compute, main/UI/water pipelines,
//!    SSAO, volumetrics, G-buffer, SVGF, ReSTIR, caustics, bloom,
//!    composite, TAA/FSR upscaler, presentation, framebuffers, sync
//!    objects) and the final struct assembly.
//!
//! Each phase's body is moved **verbatim** from the original single
//! function — see the git history for #1749 if you need to diff against
//! the pre-split version. Only the phase-boundary destructure/return
//! wrapping is new; no statement inside a phase was reordered. A further
//! split (e.g. separating "core pipelines" from "deferred/GI passes"
//! within phase 3) was evaluated and rejected: nearly every value built in
//! phase 3 (water, volumetrics, ssao, exposure, …) is still read by the
//! final struct-literal assembly at the very end of the same phase, so a
//! finer boundary would multiply the struct-threading surface (another
//! 15+-field struct) for no reduction in what actually crosses it.
//!
//! `impl Drop for VulkanContext` lives in `teardown.rs`, the mirror image
//! of this file.

use super::*;
/// Foundational Vulkan handles built by [`VulkanContext::build_core_device`]
/// — the first init phase. Destructured back into locals by `new()` so the
/// rest of the constructor reads unchanged. See #1749.
struct CoreDevice {
    entry: ash::Entry,
    vk_instance: ash::Instance,
    debug_messenger: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    surface_loader: ash::khr::surface::Instance,
    vk_surface: vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
    queue_indices: QueueFamilyIndices,
    device_caps: device::DeviceCapabilities,
    depth_format: vk::Format,
    device: ash::Device,
    graphics_queue: Arc<Mutex<vk::Queue>>,
    present_queue: Arc<Mutex<vk::Queue>>,
    gpu_allocator: SharedAllocator,
}

impl VulkanContext {
    /// Init phase 1 (#1749): load the loader, create the instance + debug
    /// messenger + surface, pick the physical device, build the logical
    /// device + queues, and create the GPU allocator. Body moved verbatim
    /// from `new()`.
    fn build_core_device(
        display_handle: RawDisplayHandle,
        window_handle: RawWindowHandle,
    ) -> Result<CoreDevice> {
        // 1. Entry
        // SAFETY: Loads the Vulkan shared library (libvulkan.so / vulkan-1.dll).
        // Must be called before any other Vulkan function. The Entry must
        // outlive all objects created through it (guaranteed by struct field order).
        let entry = unsafe { ash::Entry::load().context("Failed to load Vulkan loader")? };
        log::info!("Vulkan loader ready");

        // 2. Instance
        let vk_instance = instance::create_instance(&entry, display_handle)?;

        // 3. Debug messenger — created whenever validation is enabled
        // (debug build OR `BYRO_VALIDATION` set), so the layer's messages
        // route to the Rust `log` instead of vanishing on raw stderr.
        let debug_messenger = if instance::validation_enabled() {
            Some(debug::create_debug_messenger(&vk_instance, &entry)?)
        } else {
            None
        };

        // 4. Surface
        let surface_loader = ash::khr::surface::Instance::new(&entry, &vk_instance);
        let vk_surface =
            surface::create_surface(&entry, &vk_instance, display_handle, window_handle)?;

        // 5. Physical device + capability probe
        let (physical_device, queue_indices, device_caps) =
            device::pick_physical_device(&vk_instance, &surface_loader, vk_surface)?;

        // 6. Query supported depth format
        let depth_format = find_depth_format(&vk_instance, physical_device)?;

        // 6b. Every G-buffer color format is a hard-coded const (unlike
        // depth, which is queried above); assert up front that this device
        // actually supports COLOR_ATTACHMENT (+ COLOR_ATTACHMENT_BLEND
        // where blended) for all of them instead of failing later with a
        // generic "Failed to create gb_normal image" deep inside
        // `GBuffer::new`. #2502 / REN-D11-2026-08-07-05.
        check_gbuffer_color_formats(
            &vk_instance,
            physical_device,
            helpers::GBufferFormats {
                color_format: HDR_FORMAT,
                normal_format: NORMAL_FORMAT,
                motion_format: MOTION_FORMAT,
                mesh_id_format: MESH_ID_FORMAT,
                raw_indirect_format: RAW_INDIRECT_FORMAT,
                albedo_format: ALBEDO_FORMAT,
                fsr_mask_format: FSR_MASK_FORMAT,
                depth_format,
            },
        )?;

        // 7. Logical device + queues (enables RT extensions when available)
        let (device, raw_graphics_queue, raw_present_queue) = device::create_logical_device(
            &vk_instance,
            physical_device,
            queue_indices,
            &device_caps,
        )?;
        let graphics_queue = Arc::new(Mutex::new(raw_graphics_queue));
        // When graphics and present use the same queue family, share the
        // same Mutex to avoid two locks wrapping one VkQueue handle (#284).
        let present_queue = if queue_indices.graphics == queue_indices.present {
            Arc::clone(&graphics_queue)
        } else {
            Arc::new(Mutex::new(raw_present_queue))
        };

        // 7. GPU allocator (buffer_device_address required for RT acceleration structures)
        let gpu_allocator = allocator::create_allocator(
            &vk_instance,
            &device,
            physical_device,
            device_caps.ray_query_supported,
        )?;

        Ok(CoreDevice {
            entry,
            vk_instance,
            debug_messenger,
            surface_loader,
            vk_surface,
            physical_device,
            queue_indices,
            device_caps,
            depth_format,
            device,
            graphics_queue,
            present_queue,
            gpu_allocator,
        })
    }
}

/// Output of init phase 2 (#1749): everything `build_core_device` produced
/// (re-threaded verbatim so phase 3 only needs one incoming struct) plus
/// the swapchain, depth resources, render pass, command pools, texture
/// registry, scene buffers, acceleration manager, and pipeline cache.
/// Destructured back into locals by `build_pipelines_and_finish` so that
/// function's body reads unchanged from the pre-split `new()`.
struct SwapchainResources {
    // Passed through from `CoreDevice` — phase 3 (and the final struct
    // literal it ends with) still needs every one of these.
    entry: ash::Entry,
    vk_instance: ash::Instance,
    debug_messenger: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    surface_loader: ash::khr::surface::Instance,
    vk_surface: vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
    queue_indices: QueueFamilyIndices,
    device_caps: device::DeviceCapabilities,
    depth_format: vk::Format,
    device: ash::Device,
    graphics_queue: Arc<Mutex<vk::Queue>>,
    present_queue: Arc<Mutex<vk::Queue>>,
    gpu_allocator: SharedAllocator,
    // Built by this phase.
    swapchain_state: SwapchainState,
    frame_extents: FrameExtentSet,
    render_extent: vk::Extent2D,
    fsr_temporal: Option<FsrTemporalState>,
    depth_image: vk::Image,
    depth_image_view: vk::ImageView,
    depth_allocation: vk_alloc::Allocation,
    depth_history_image: vk::Image,
    depth_history_view: vk::ImageView,
    depth_history_allocation: vk_alloc::Allocation,
    depth_history_sampler: vk::Sampler,
    render_pass: vk::RenderPass,
    command_pool: vk::CommandPool,
    transfer_pool: vk::CommandPool,
    transfer_fence: Arc<Mutex<vk::Fence>>,
    texture_registry: TextureRegistry,
    scene_buffers: scene_buffer::SceneBuffers,
    accel_manager: Option<AccelerationManager>,
    pipeline_cache: vk::PipelineCache,
}

impl VulkanContext {
    /// Init phase 2 (#1749): swapchain, depth resources, render pass,
    /// command pools, texture registry (+ fallbacks), scene buffers,
    /// acceleration manager, pipeline cache. Body moved verbatim from
    /// `new()` — everything between `build_core_device`'s call site and
    /// the first pipeline-create call (`pipeline_cache` must exist before
    /// any of those, per the comment at its creation below).
    fn build_swapchain_and_resources(
        core: CoreDevice,
        window_size: [u32; 2],
        renderer_config: &RendererConfig,
    ) -> Result<SwapchainResources> {
        let CoreDevice {
            entry,
            vk_instance,
            debug_messenger,
            surface_loader,
            vk_surface,
            physical_device,
            queue_indices,
            device_caps,
            depth_format,
            device,
            graphics_queue,
            present_queue,
            gpu_allocator,
        } = core;

        // 8. Swapchain
        let swapchain_state = swapchain::create_swapchain(
            swapchain::SwapchainSurfaceCtx {
                instance: &vk_instance,
                device: &device,
                physical_device,
                surface_loader: &surface_loader,
                surface: vk_surface,
            },
            queue_indices,
            window_size,
            vk::SwapchainKHR::null(), // no old swapchain on initial creation
        )?;
        let max_image_dimension_2d = unsafe {
            // SAFETY: `physical_device` was selected from `vk_instance` and
            // both remain live for the duration of context construction.
            vk_instance
                .get_physical_device_properties(physical_device)
                .limits
                .max_image_dimension2_d
        };
        let frame_extents = FrameExtentSet::for_output(
            swapchain_state.extent,
            renderer_config.upscaler,
            max_image_dimension_2d,
        )?;
        log::info!(
            "Frame extents: render={}x{}, output={}x{} ({})",
            frame_extents.render.width,
            frame_extents.render.height,
            frame_extents.output.width,
            frame_extents.output.height,
            renderer_config.upscaler,
        );
        let render_extent = frame_extents.render;
        let fsr_temporal = match renderer_config.upscaler {
            UpscalerMode::Taa => None,
            UpscalerMode::Fsr3(_) => Some(
                FsrTemporalState::new(frame_extents)
                    .context("query FSR temporal jitter sequence")?,
            ),
        };

        // 9. Depth resources
        let (depth_image, depth_image_view, depth_allocation) = create_depth_resources(
            &device,
            &gpu_allocator,
            render_extent,
            depth_format,
            // TRANSFER_SRC: the soft-particle depth-history copy uses the depth
            // buffer as a `vkCmdCopyImage` source each frame (#1583 validation).
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC,
            "depth_buffer",
        )?;

        // Soft-particle depth-fade history: a sampleable copy of the prior
        // frame's opaque depth. Effect-shader (kind 101) FX read it to
        // feather alpha as they approach geometry behind them — the authored
        // `BSEffectShaderProperty.soft_falloff_depth` / BGEM `soft_depth`.
        // Separate from the live depth image because that one is the active
        // attachment during the transparent pass (can't be sampled while
        // bound) and is cleared every frame. Initialized to far (1.0) so the
        // first frame reads "no occluder near" → full alpha (benign).
        let (depth_history_image, depth_history_view, depth_history_allocation) =
            create_depth_resources(
                &device,
                &gpu_allocator,
                render_extent,
                depth_format,
                vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                "depth_history",
            )?;
        let depth_history_sampler = create_depth_history_sampler(&device)?;

        // 10. Main render pass: 8 color attachments (HDR + G-buffer +
        // raw_indirect + albedo + fsr_reactive + fsr_transparency) + depth.
        // (The ReSTIR reservoir attachment this used to name was removed
        // under #1583; slots 6/7 are now the FSR reactive and
        // transparency-and-composition masks.)
        let render_pass = create_render_pass(
            &device,
            helpers::GBufferFormats {
                color_format: HDR_FORMAT,
                normal_format: NORMAL_FORMAT,
                motion_format: MOTION_FORMAT,
                mesh_id_format: MESH_ID_FORMAT,
                raw_indirect_format: RAW_INDIRECT_FORMAT,
                albedo_format: ALBEDO_FORMAT,
                fsr_mask_format: FSR_MASK_FORMAT,
                depth_format,
            },
        )?;

        // 10. Command pools: one for per-frame draw commands (RESET_COMMAND_BUFFER),
        //     one for one-time upload/transfer commands (separate pool to avoid
        //     contention — Vulkan requires external sync on VkCommandPool).
        let command_pool = create_command_pool(&device, queue_indices.graphics)?;
        let transfer_pool = create_transfer_pool(&device, queue_indices.graphics)?;

        // One-time transition of the depth-history image UNDEFINED → clear to
        // far (1.0) → SHADER_READ_ONLY so the very first frame's effect-shader
        // FX sample a valid layout before any per-frame depth copy has run.
        init_depth_history_layout(&device, &graphics_queue, command_pool, depth_history_image)?;

        // Persistent fence for one-time submits (#302). Created unsignaled;
        // every use calls reset_fences then wait_for_fences.
        let transfer_fence = Arc::new(Mutex::new(unsafe {
            // SAFETY: `device` is this context's live logical device; the
            // `FenceCreateInfo` is a stack temporary valid for the call and the
            // returned fence is owned here (stored in the struct) and destroyed
            // in `Drop`.
            device
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .context("create transfer fence")?
        }));

        // 11. Texture registry with checkerboard fallback.
        // Bindless capacity is split evenly across the 2D and cubemap
        // bindings so their combined sampled-image descriptor count stays
        // within the device's update-after-bind per-stage limit.
        let max_textures_per_binding = (device_caps.max_bindless_sampled_images / 2).max(2);
        let mut texture_registry = TextureRegistry::new(
            &device,
            &gpu_allocator,
            max_textures_per_binding,
            device_caps.max_sampler_anisotropy,
            frame_extents
                .material_mip_bias(renderer_config.upscaler)
                .clamp(
                    -device_caps.max_sampler_lod_bias,
                    device_caps.max_sampler_lod_bias,
                ),
        )?;
        let checkerboard = super::super::texture::generate_checkerboard(256, 256, 32);
        // One-shot 256×256 fallback — `None` pool skips the overhead of
        // the first pool entry that would otherwise linger for the rest
        // of the session.
        let fallback_texture = Texture::from_rgba(
            super::super::GpuUploadCtx {
                device: &device,
                allocator: &gpu_allocator,
                queue: &graphics_queue,
                command_pool: transfer_pool,
            },
            256,
            256,
            &checkerboard,
            texture_registry.shared_sampler,
            None,
        )?;
        texture_registry.set_fallback(&device, fallback_texture)?;

        // F2 (2026-05-26 Fallout sweep) — separate neutral fallback for
        // NIF-authored textureless surfaces (alpha-blend overlays,
        // emissive halos, vertex-color shapes). 1×1 white RGBA so the
        // shader's material × vertex-color × emissive multiply
        // collapses to the artist-intended look instead of magenta
        // checker × those terms.
        let white_pixel: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
        let neutral_texture = Texture::from_rgba(
            super::super::GpuUploadCtx {
                device: &device,
                allocator: &gpu_allocator,
                queue: &graphics_queue,
                command_pool: transfer_pool,
            },
            1,
            1,
            &white_pixel,
            texture_registry.shared_sampler,
            None,
        )?;
        texture_registry.set_neutral_fallback(&device, neutral_texture)?;

        // 12. Scene buffers (light SSBO + camera UBO + optional TLAS, descriptor set 1)
        let scene_buffers = scene_buffer::SceneBuffers::new(
            &device,
            &gpu_allocator,
            device_caps.ray_query_supported,
        )?;
        // M29.5 cleanup — the pre-#921 startup seed of slot-0 identity
        // into the palette buffer (`bone_device_buffers`) is no longer
        // needed. The per-frame `skin_palette.comp` dispatch writes
        // the palette unconditionally.
        //
        // M29.6 hotfix (#1191 / SAFE-D7-NEW-01) — but the persistent
        // `bind_inverses_persistent` SSBO that the dispatch READS from
        // DOES need a slot-0 seed: the slot pool reserves slot 0 for
        // the global identity slot, never pushes a pending upload for
        // it, and pool-overflowed skinned entities fall through to
        // `bone_offset = 0`. Without this seed, `palette[0..MBPM] =
        // identity × UNDEFINED = UB`. With the seed,
        // `palette[0..MBPM] = identity × identity = identity` and the
        // overflow case falls back to bind pose (pre-M29.6 behaviour).
        scene_buffers
            .seed_persistent_bind_inverses_identity(&device, &graphics_queue, transfer_pool)
            .context("seed bind_inverses_persistent slot 0 identity (M29.6 / #1191)")?;

        // 12b. Acceleration manager (RT only) — build empty TLAS so descriptors are valid
        let mut scene_buffers = scene_buffers;
        let accel_manager = if device_caps.ray_query_supported {
            let mut accel = AccelerationManager::new(
                &vk_instance,
                &device,
                physical_device,
                device_caps.min_accel_struct_scratch_offset_alignment,
                renderer_config.rt_test_blas_budget_bytes,
            )?;
            // Build an empty TLAS per frame-in-flight slot via one-time command
            // buffers so all descriptor sets have a valid acceleration structure
            // from frame 0. Each build blocks until complete (fence wait inside
            // with_one_time_commands), so no overlap between builds.
            let empty_draws: Vec<DrawCommand> = Vec::new();
            let empty_map: Vec<Option<u32>> = Vec::new();
            for f in 0..MAX_FRAMES_IN_FLIGHT {
                super::super::texture::with_one_time_commands_reuse_fence(
                    &device,
                    &graphics_queue,
                    transfer_pool,
                    &transfer_fence,
                    |cmd| unsafe {
                        // SAFETY: `cmd` is a command buffer that
                        // `with_one_time_commands_reuse_fence` has already begun
                        // recording; `device`/`gpu_allocator` are live and own
                        // the acceleration structures `accel` builds into for
                        // frame index `f` (< MAX_FRAMES_IN_FLIGHT).
                        accel
                            .build_tlas(&device, &gpu_allocator, cmd, &empty_draws, &empty_map, f)
                            .context("initial empty TLAS build")
                    },
                )?;
                if let Some(tlas_handle) = accel.tlas_handle(f) {
                    scene_buffers.write_tlas(&device, f, tlas_handle);
                }
            }
            Some(accel)
        } else {
            None
        };

        // 12b. Pipeline cache (load from disk if available).
        // Created before ANY pipeline-create call so every compile
        // writes into the shared cache — warm-start second-launch
        // skips most driver IR compilation (#426). The on-disk
        // header is validated against the running device's
        // vendorID / deviceID / pipelineCacheUUID before the bytes
        // reach the driver — defense in depth against tampered or
        // post-upgrade-stale files (SAFE-11 / #91).
        let pipeline_cache = load_or_create_pipeline_cache(&vk_instance, physical_device, &device)?;

        Ok(SwapchainResources {
            entry,
            vk_instance,
            debug_messenger,
            surface_loader,
            vk_surface,
            physical_device,
            queue_indices,
            device_caps,
            depth_format,
            device,
            graphics_queue,
            present_queue,
            gpu_allocator,
            swapchain_state,
            frame_extents,
            render_extent,
            fsr_temporal,
            depth_image,
            depth_image_view,
            depth_allocation,
            depth_history_image,
            depth_history_view,
            depth_history_allocation,
            depth_history_sampler,
            render_pass,
            command_pool,
            transfer_pool,
            transfer_fence,
            texture_registry,
            scene_buffers,
            accel_manager,
            pipeline_cache,
        })
    }

    /// Init phase 3 (#1749): every pipeline + optional pass (cluster cull,
    /// skin compute, main/UI/water pipelines, SSAO, volumetrics, G-buffer,
    /// SVGF, ReSTIR, caustics, bloom, composite, TAA/FSR upscaler,
    /// presentation, framebuffers, sync objects), then the final struct
    /// assembly. Body moved verbatim from `new()` — this is everything
    /// from the `pipeline_cache`-creation comment's next statement through
    /// the constructor's final return.
    fn build_pipelines_and_finish(
        swapchain: SwapchainResources,
        window_size: [u32; 2],
        renderer_config: RendererConfig,
    ) -> Result<Self> {
        let SwapchainResources {
            entry,
            vk_instance,
            debug_messenger,
            surface_loader,
            vk_surface,
            physical_device,
            queue_indices,
            device_caps,
            depth_format,
            device,
            graphics_queue,
            present_queue,
            gpu_allocator,
            swapchain_state,
            frame_extents,
            render_extent,
            fsr_temporal,
            depth_image,
            depth_image_view,
            depth_allocation,
            depth_history_image,
            depth_history_view,
            depth_history_allocation,
            depth_history_sampler,
            render_pass,
            command_pool,
            transfer_pool,
            transfer_fence,
            texture_registry,
            scene_buffers,
            mut accel_manager,
            pipeline_cache,
        } = swapchain;

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

        // 12d. Skin compute pipeline (M29 Phase 2). RT-required: when
        // ray queries aren't supported there's no BLAS refit path to
        // feed, so the pipeline is dead weight. Created with the max
        // slot ceiling matching `MAX_TOTAL_BONES / MAX_BONES_PER_MESH
        // = 32` skinned meshes — same ceiling the bone-palette upload
        // path enforces in `build_render_data`. Buffer bindings are
        // deferred to per-dispatch (cell-transition robustness).
        let mut skin_compute = if device_caps.ray_query_supported {
            // See module-level `SKIN_MAX_SLOTS` const for the rationale.
            match super::super::skin_compute::SkinComputePipeline::new(
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
            }
        } else {
            None
        };

        // 12d.5. M29.5 — GPU bone-palette compute pipeline. Same RT
        // gate as `skin_compute` — the engine is RT-required per
        // VRAM-baseline policy, so this branch is the production path
        // on every supported config. Construction failure logs but
        // doesn't abort; downstream `skin_palette.is_some()` checks
        // skip the dispatch (no CPU-multiply fallback exists — the
        // legacy `upload_bones` + staging-copy path is removed since
        // M29.5 cleanup, and the engine has no supported no-RT mode).
        let skin_palette = if device_caps.ray_query_supported {
            match super::super::skin_compute::SkinPaletteComputePipeline::new(
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
            }
        } else {
            None
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

        // 14. Graphics pipeline (with depth test + descriptor set layouts for set 0 + set 1).
        // `fill_mode_non_solid_supported` gates the wireframe variant
        // (#869) — when false, only the FILL opaque pipeline is built
        // and `NiWireframeProperty` content silently renders filled.
        let pipelines = pipeline::create_triangle_pipeline(
            &device,
            render_pass,
            render_extent,
            texture_registry.descriptor_set_layout,
            scene_buffers.descriptor_set_layout,
            pipeline_cache,
            device_caps.fill_mode_non_solid_supported,
        )?;

        // 15. The UI overlay pipeline used to be created here, against the
        // geometry render pass. #3426 moved it into `PresentationPipeline`
        // (built below with `pipelines.layout`) so the Scaleform overlay
        // composites onto the tone-mapped, upscaled swapchain image instead
        // of being blended into the render-resolution HDR G-buffer.

        // 15a. Water pipeline (transparent, RT reflection/refraction,
        // SRC_ALPHA blend on HDR only — G-buffer attachments masked
        // off so SVGF / motion-vector reprojection ignore water).
        // Reuses set 0 + set 1 descriptor layouts for compatibility
        // with the bound triangle-pipeline descriptor sets at draw
        // time; the water pipeline layout adds a 112-byte push
        // constant range for per-plane material params.
        // #1561 — gate water pipeline creation on RT support, mirroring
        // `accel_manager` / `skin_compute` / `skin_palette` above. `water.frag`
        // uses set=1 binding=2 (TLAS) unconditionally — unlike `triangle.frag`
        // it has no `sceneFlags.x` runtime guard — and on a non-RT device
        // binding 2 is omitted from the bound layout while the SPIR-V still
        // carries the `RayQueryKHR` capability with the `rayQuery` feature
        // disabled. Creating it there risks a pipeline-creation failure or
        // (driver-dependent) an undefined ray query against an absent binding.
        // RT-capable hardware (the only configuration this engine targets —
        // RT is mandatory) is unaffected: the pipeline is created exactly as
        // before. The matching draw-side skip lives in `draw.rs`.
        let mut water = if device_caps.ray_query_supported {
            match WaterPipeline::new(
                &device,
                &gpu_allocator,
                render_pass,
                pipeline_cache,
                texture_registry.descriptor_set_layout,
                scene_buffers.descriptor_set_layout,
            ) {
                Ok(w) => Some(w),
                Err(e) => {
                    log::warn!(
                        "Water pipeline creation failed: {e} — water surfaces will not render"
                    );
                    None
                }
            }
        } else {
            log::info!(
                "Water pipeline skipped: device lacks ray_query support (water.frag traces \
                 RT rays unconditionally). See #1561."
            );
            None
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
                    let mut a_mut = a;
                    unsafe {
                        // SAFETY: `a_mut`'s images/buffers were made by `device`
                        // and are destroyed on this init-failure path before any
                        // frame command buffer could reference them.
                        a_mut.destroy(&device, &gpu_allocator)
                    };
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
                    .map(|p| vec![p.view; super::sync::MAX_FRAMES_IN_FLIGHT]),
            };
            let views = views.expect(
                "water/caustic coupling must disable water when both storage sinks are absent",
            );
            w.update_water_caustic_descriptors(&device, &views);
        }

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
                    scene_buffers.write_ao_texture(&device, f, s.ao_image_views[f], s.ao_sampler);
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
                        scene_buffers.write_ao_texture(&device, f, p.view, p.sampler);
                    }
                }
                None
            }
        };

        // 14b. Exposure producer (1x1 R32_SFLOAT). Cleared to the fixed HDR
        // exposure so presentation and the FSR dispatch share one value.
        let exposure =
            match ExposureResource::new(&device, &gpu_allocator, &graphics_queue, transfer_pool) {
                Ok(e) => Some(e),
                Err(e) => {
                    log::warn!(
                        "Exposure resource creation failed: {e} — presentation falls back to the \
                         default exposure constant"
                    );
                    None
                }
            };

        // Soft-particle depth-history descriptor (set 1, binding 15). The
        // image view is stable per swapchain generation, so it's written once
        // here (and again on resize) rather than per-frame — only the image
        // contents change each frame via the post-pass copy.
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            scene_buffers.write_depth_history(
                &device,
                f,
                depth_history_view,
                depth_history_sampler,
            );
        }

        // 14a-bis. Procedural volumetrics. Froxel XY derives from the render
        // extent after FSR sizing; Z/reach come from the validated renderer
        // config. Each FIF slot owns raw V-buffer + integrated RGBA16F volumes.
        let mut volumetrics = match VolumetricsPipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
            render_extent,
            renderer_config.volumetrics,
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
                    unsafe {
                        // SAFETY: `pipe` was just created by `device`; on this
                        // init-failure path no frame command buffer has yet
                        // referenced its images, so destroying it is sound.
                        pipe.destroy(&device, &gpu_allocator)
                    };
                }
            }
        }

        // #3839 — the BLAS budget is constructed before any render extent
        // exists, so it starts unreserved. Now that the froxel grid (and the
        // other resolution-scaled passes) are sized, re-derive it against what
        // they actually hold, so the very first frame evicts on real headroom
        // rather than on the whole heap. The resize path re-derives again at
        // each new extent.
        if let Some(accel) = accel_manager.as_mut() {
            accel.recompute_blas_budget(render_extent, renderer_config.volumetrics);
        }

        // 14. Mesh registry (empty — meshes uploaded by the application)
        let mesh_registry = MeshRegistry::new();

        // 14b. G-buffer: all auxiliary attachments (normal, motion, mesh_id,
        // raw_indirect, albedo). Created BEFORE composite because composite's
        // descriptor sets reference the raw_indirect + albedo views.
        let gbuffer = Some(GBuffer::new(
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
            log::warn!("G-buffer layout init failed: {e}");
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
                // Destroy partially-initialized pipeline.
                if let Some(mut pipe) = svgf.take() {
                    unsafe {
                        // SAFETY: `pipe` was just created by `device`; on this
                        // init-failure path no frame command buffer has yet
                        // referenced its images, so destroying it is sound.
                        pipe.destroy(&device, &gpu_allocator)
                    };
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
            scene_buffers.write_reservoir_buffers(
                &device,
                i,
                reservoir_buffers.curr_buffer(i),
                reservoir_buffers.prev_buffer(i),
                reservoir_buffers.buffer_size(),
            );
        }

        // Composite samples SVGF's accumulated indirect (GENERAL layout)
        // when SVGF is available, else falls back to raw G-buffer indirect
        // (SHADER_READ_ONLY_OPTIMAL layout).
        let (composite_indirect_views, indirect_is_general): (Vec<vk::ImageView>, bool) =
            if let Some(ref s) = svgf {
                ((0..n_frames).map(|i| s.indirect_view(i)).collect(), true)
            } else {
                (raw_indirect_views.clone(), false)
            };

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
            scene_buffers.instance_buffer_size(),
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
                if let Some(mut pipe) = caustic.take() {
                    unsafe {
                        // SAFETY: `pipe` was just created by `device`; on this
                        // init-failure path no frame command buffer has yet
                        // referenced its images, so destroying it is sound.
                        pipe.destroy(&device, &gpu_allocator)
                    };
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

        // 14c. Composite pipeline: owns HDR intermediates + scene-composition
        // pass. Tone mapping now happens at output resolution in presentation.
        // Its descriptor sets sample HDR (owned by composite), indirect
        // (from SVGF or raw G-buffer), and albedo (G-buffer).
        // Volumetric views (M55 Phase 3) — composite samples the
        // pre-integrated `(∫inscatter, T_cum)` volume per fragment
        // with one sampler3D tap. Hard requirement: composite's
        // binding 6 is `sampler3D`, so a None volumetrics pipeline
        // can't be papered over with a 2D fallback view. If pipeline
        // creation failed earlier, refuse to build composite. The
        // 14 MiB × 2 / slot 3D-image allocation is universally
        // supported on RT-class GPUs, so this only fires under exotic
        // hardware / driver pathologies.
        let volumetric_views: Vec<vk::ImageView> = match volumetrics.as_ref() {
            Some(v) => v.integrated_views(),
            None => {
                return Err(anyhow::anyhow!(
                    "Volumetric pipeline failed to initialize — composite \
                     requires the integrated 3D froxel volume for binding 6 \
                     (M55 Phase 3). Check earlier 'volumetrics' WARN logs."
                ));
            }
        };

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
                    log::warn!("Bloom pyramid layout init failed: {e}");
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
        // #1257 / Phase E of #1210 — water-side caustic sampled views.
        // None on init failure → bind the existing 1×1 R32_UINT sink. It is
        // a 2D sampled/storage view, matching binding 8; the RGB glass view
        // is now a 2D array and cannot be used as this fallback. The host flag
        // still gates the read so accumulated sink writes never contribute.
        let water_caustic_views: Vec<vk::ImageView> = match water_caustic_accum {
            Some(ref a) => (0..super::sync::MAX_FRAMES_IN_FLIGHT)
                .map(|i| a.sampled_view(i))
                .collect(),
            None => match placeholder_caustic_sink.as_ref() {
                Some(p) => vec![p.view; super::sync::MAX_FRAMES_IN_FLIGHT],
                None => {
                    return Err(anyhow::anyhow!(
                        "Water-caustic accumulator and its sampled placeholder are both absent"
                    ));
                }
            },
        };
        let mut composite = match CompositePipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
            &composite_indirect_views,
            indirect_is_general,
            &albedo_views,
            depth_image_view,
            &caustic_views,
            &water_caustic_views,
            &volumetric_views,
            &bloom_views,
            &reactive_views,
            &transparency_views,
            texture_registry.descriptor_set_layout,
            frame_extents,
        ) {
            Ok(c) => Some(c),
            Err(e) => {
                return Err(anyhow::anyhow!("Composite pipeline creation failed: {e}"));
            }
        };
        // Snapshot composite's HDR image views into an owned Vec so the
        // subsequent &mut borrow of `composite` (for TAA rewire) doesn't
        // conflict with the main-framebuffer creation below.
        let hdr_views_owned: Vec<vk::ImageView> = composite
            .as_ref()
            .expect("composite must exist after construction")
            .hdr_image_views
            .clone();

        // 14d. TAA resolve pass — needs the composite's HDR views (created
        // above) as its "current HDR" input, plus per-FIF motion, mesh_id,
        // and normal for surface-valid history reprojection.
        // If creation succeeds, composite's HDR descriptor is rewired to
        // sample TAA's output; otherwise we keep the raw HDR path.
        let mut taa = if renderer_config.upscaler == UpscalerMode::Taa {
            match TaaPipeline::new(
                &device,
                &gpu_allocator,
                pipeline_cache,
                super::super::taa::TaaInputViews {
                    hdr_views: &hdr_views_owned,
                    motion_views: &motion_views_seed,
                    mesh_id_views: &mesh_id_views_seed,
                    normal_views: &normal_views_seed,
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
                if let Some(mut pipe) = taa.take() {
                    unsafe {
                        // SAFETY: `pipe` was just created by `device`; on this
                        // init-failure path no frame command buffer has yet
                        // referenced its images, so destroying it is sound.
                        pipe.destroy(&device, &gpu_allocator)
                    };
                }
            }
        }
        // Swap composite's HDR binding to TAA output so scene composition
        // samples the anti-aliased image. When TAA is disabled composite keeps
        // its original raw-HDR descriptors.
        if let (Some(t), Some(ref mut c)) = (taa.as_ref(), composite.as_mut()) {
            let taa_views: Vec<vk::ImageView> = (0..n_frames).map(|i| t.output_view(i)).collect();
            c.rebind_hdr_views(&device, &taa_views, vk::ImageLayout::GENERAL);
        }

        let frame_upscaler = FrameUpscaler::new(
            &vk_instance,
            &device,
            physical_device,
            &gpu_allocator,
            &graphics_queue,
            transfer_pool,
            renderer_config.upscaler,
            frame_extents,
            device_caps.shader_float16_supported,
        )
        .context("create frame upscaler")?;
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
                upscaled_views: frame_upscaler.output_views(),
                health_buffers: &health_handles,
                extent: frame_extents.output,
            },
            pipelines.layout,
        )
        .context("create presentation pipeline")?;
        let frame_upscaler = Some(frame_upscaler);
        let presentation = Some(presentation);

        // 15. Main framebuffers: one per frame-in-flight slot, binding that
        // slot's HDR + normal + motion + mesh_id + raw_indirect + albedo
        // views + shared depth view.
        let hdr_views: &[vk::ImageView] = &hdr_views_owned;
        let normal_views: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.normal_view(i)).collect();
        let motion_views: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.motion_view(i)).collect();
        let mesh_id_views: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.mesh_id_view(i)).collect();
        let framebuffers = create_main_framebuffers(
            &device,
            render_pass,
            helpers::GBufferViews {
                hdr_views,
                normal_views: &normal_views,
                motion_views: &motion_views,
                mesh_id_views: &mesh_id_views,
                raw_indirect_views: &raw_indirect_views,
                albedo_views: &albedo_views,
                reactive_views: &reactive_views,
                transparency_views: &transparency_views,
            },
            depth_image_view,
            render_extent,
        )?;

        // 16. Command buffers — one per frame-in-flight (NOT per swapchain
        // image). The in_flight fence is per-frame, so tying command buffer
        // reuse to the same index makes the fence → cmd-buf relationship
        // direct and obvious. See #259.
        let command_buffers =
            allocate_command_buffers(&device, command_pool, sync::MAX_FRAMES_IN_FLIGHT)?;

        // 17. Sync objects
        let frame_sync = sync::create_sync_objects(&device, swapchain_state.images.len())?;

        log::info!("Vulkan context fully initialized");

        let mut context = Self {
            entry,
            instance: vk_instance,
            debug_messenger,
            surface_loader,
            surface: vk_surface,
            physical_device,
            depth_format,
            device,
            device_caps,
            queue_indices,
            graphics_queue,
            present_queue,
            swapchain_state,
            allocator: Some(gpu_allocator),
            memory_warning_once: Once::new(),
            egui_pass: None,
            egui_pending_output: None,
            render_pass,
            pipeline_cache,
            pipeline: pipelines.opaque,
            pipeline_wireframe: pipelines.opaque_wireframe,
            blend_pipeline_cache: FxHashMap::default(),
            blend_seen_scratch: FxHashSet::default(),
            pipeline_layout: pipelines.layout,
            ui_quad_handle: None,
            particle_quad_handle: None,
            terrain_tiles: vec![None; scene_buffer::MAX_TERRAIN_TILES],
            // Free list seeded with every slot in reverse order so
            // `pop()` returns slots in ascending order (deterministic
            // test behaviour).
            terrain_tile_free_list: (0..scene_buffer::MAX_TERRAIN_TILES as u32).rev().collect(),
            terrain_tiles_dirty: false,
            terrain_tile_scratch: Vec::new(),
            mesh_registry,
            texture_registry,
            scene_buffers,
            accel_manager,
            cluster_cull,
            skin_compute,
            gpu_timers,
            skin_palette,
            skin_slots: FxHashMap::default(),
            morph_slots: FxHashMap::default(),
            morph_delta_cache: FxHashMap::default(),
            failed_skin_slots: FxHashSet::default(),
            failed_skin_blas: FxHashSet::default(),
            pending_skin_unload_victims: Vec::new(),
            pending_morph_unload_victims: Vec::new(),
            last_skin_coverage_frame: super::super::skin_compute::SkinCoverageFrame::default(),
            last_draw_call_stats: DrawCallStats::default(),
            skin_dispatch_ran: false,
            bind_inverse_upload_failed: false,
            clean_skin_frames: 0,
            ssao,
            placeholder_ao,
            placeholder_caustic_sink,
            exposure,
            composite,
            frame_upscaler,
            presentation,
            gbuffer,
            svgf,
            reservoir_buffers,
            taa,
            caustic,
            volumetrics,
            bloom,
            water,
            water_caustic_accum,
            taa_failed: false,
            svgf_failed: false,
            svgf_recovery_frames: 0,
            caustic_failed: false,
            caustic_cleared_on_skip: [false; MAX_FRAMES_IN_FLIGHT],
            volumetrics_cleared_on_skip: [false; MAX_FRAMES_IN_FLIGHT],
            depth_allocation: Some(depth_allocation),
            depth_image,
            depth_image_view,
            depth_history_image,
            depth_history_view,
            depth_history_allocation: Some(depth_history_allocation),
            depth_history_sampler,
            framebuffers,
            command_pool,
            transfer_pool,
            transfer_fence,
            command_buffers,
            frame_sync,
            image_health_buffers,
            image_health_last: (0, 0),
            image_health_total: (0, 0),
            current_frame: 0,
            renderer_config,
            frame_extents,
            frame_counter: 0,
            rt_flag_last_frame: false,
            tlas_build_succeeded_last_frame: false,
            volumetric_time_seconds: 0.0,
            fsr_temporal,
            render_debug_flags: parse_render_debug_flags_env(),
            render_debug_mode: parse_render_debug_mode_env(),
            pending_selected_ray_probe: None,
            selected_ray_probe_result: None,
            next_selected_ray_probe_generation: 1,
            // REND-#1451 — default knee = 0.5 (authored radius at half
            // the cull radius). `light_atten_legacy` starts false; the
            // env path (`BYROREDUX_RENDER_DEBUG=0x1000`) can still force
            // the legacy formula at launch via `render_debug_flags`.
            light_atten_knee: 0.5,
            light_atten_legacy: false,
            // Initialize to identity; first frame will overwrite with current
            // viewProj so motion vector is zero on the first frame.
            prev_view_proj: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            prev_camera_position: [0.0; 3],
            prev_render_origin: [0.0; 3],
            prev_cam_forward: [0.0, 0.0, -1.0],
            gpu_instances_scratch: Vec::new(),
            frame_lights_scratch: Vec::new(),
            previous_rigid_models: FxHashMap::default(),
            prev_caustic_scene_key: 0,
            current_rigid_models_scratch: FxHashMap::default(),
            previous_models_scratch: Vec::new(),
            batches_scratch: Vec::new(),
            indirect_draws_scratch: Vec::new(),
            indirect_upload_ok: true,
            skin_dispatch_seen_scratch: FxHashSet::default(),
            skin_dispatches_scratch: Vec::new(),
            skin_first_sight_builds_scratch: Vec::new(),
            skin_built_this_frame_scratch: FxHashSet::default(),
            screenshot_requested: Arc::new(AtomicBool::new(false)),
            screenshot_result: Arc::new(Mutex::new(None)),
            screenshot_generation: Arc::new(AtomicU64::new(0)),
            screenshot_staging: None,
            screenshot_pending_readback: None,
            depth_capture_requested: Arc::new(AtomicBool::new(false)),
            depth_capture_result: Arc::new(Mutex::new(None)),
            depth_capture_staging: None,
            depth_capture_pending_readback: None,
        };

        // #2480 / REN-D23-2026-08-07-01 — `UpscalerMode::Taa` documents
        // itself as "the compatibility fallback taken whenever FSR context
        // creation ... fails", but nothing enforced that: on FSR context-
        // creation failure `frame_upscaler` stays alive with `context: None`
        // (native-blit degrade) while `renderer_config.upscaler` and
        // `frame_extents.render` stayed pinned to the FSR preset's REDUCED
        // extent and no TAA pipeline was ever built (it's only constructed
        // above when the mode is already `Taa`). The user silently got a
        // permanent, un-anti-aliased bilinear stretch of a sub-native
        // render — FSR Quality is the default, so this was the landing
        // state for every machine with a non-working FSR provider.
        //
        // Promote to native TAA using the same rollback-safe runtime
        // switch `set_upscaler_mode` already provides for the user-facing
        // `--upscaler` toggle, rather than duplicating its extent/resource
        // rebuild here. Safe to call at this exact point: `context` is
        // fully constructed and no frame has been submitted yet, so
        // `set_upscaler_mode`'s `device_wait_idle` precondition holds
        // trivially.
        if matches!(context.renderer_config.upscaler, UpscalerMode::Fsr3(_))
            && !context
                .frame_upscaler
                .as_ref()
                .is_some_and(FrameUpscaler::is_fsr_dispatch_active)
        {
            log::error!(
                "FSR context creation failed at startup; promoting to native-resolution \
                 TAA instead of silently staying at the reduced FSR render extent"
            );
            if let Err(e) = context.set_upscaler_mode(UpscalerMode::Taa, window_size) {
                log::error!(
                    "Failed to promote to TAA after FSR startup failure; the reduced-extent \
                     native-blit fallback stays active: {e:#}"
                );
            }
        }

        Ok(context)
    }

    /// Full Vulkan initialization chain, now three phases (#1749):
    /// 1. [`Self::build_core_device`] — entry → instance → debug → surface
    ///    → physical device → logical device → GPU allocator.
    /// 2. [`Self::build_swapchain_and_resources`] — swapchain → render pass
    ///    → command pools → texture registry → scene buffers → pipeline
    ///    cache.
    /// 3. [`Self::build_pipelines_and_finish`] — every pipeline + optional
    ///    pass, then the final struct assembly.
    pub fn new(
        display_handle: RawDisplayHandle,
        window_handle: RawWindowHandle,
        window_size: [u32; 2],
        renderer_config: RendererConfig,
    ) -> Result<Self> {
        let core = Self::build_core_device(display_handle, window_handle)?;
        let swapchain = Self::build_swapchain_and_resources(core, window_size, &renderer_config)?;
        Self::build_pipelines_and_finish(swapchain, window_size, renderer_config)
    }
}
