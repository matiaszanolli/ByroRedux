//! Stateless helper functions used by VulkanContext::new(), recreate_swapchain(), and Drop.

use super::super::allocator::SharedAllocator;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;
use gpu_allocator::MemoryLocation;

/// Query the physical device for a supported depth format.
pub(super) fn find_depth_format(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> Result<vk::Format> {
    let candidates = [
        vk::Format::D32_SFLOAT,
        vk::Format::D32_SFLOAT_S8_UINT,
        vk::Format::D24_UNORM_S8_UINT,
        vk::Format::D16_UNORM,
    ];

    for &format in &candidates {
        let props =
            unsafe { instance.get_physical_device_format_properties(physical_device, format) };

        if props
            .optimal_tiling_features
            .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
        {
            log::info!("Depth format selected: {:?}", format);
            return Ok(format);
        }
    }

    anyhow::bail!("No supported depth format found (tried D32, D32S8, D24S8, D16)")
}

pub(super) fn create_render_pass(
    device: &ash::Device,
    color_format: vk::Format,
    normal_format: vk::Format,
    motion_format: vk::Format,
    mesh_id_format: vk::Format,
    raw_indirect_format: vk::Format,
    albedo_format: vk::Format,
    depth_format: vk::Format,
) -> Result<vk::RenderPass> {
    // Phase 2: main render pass writes to 6 color attachments + depth.
    // Formats are the authoritative constants in `vulkan/gbuffer.rs`; the
    // list below names them for orientation.
    //   0 — HDR color    (RGBA16F)        — direct lighting only
    //   1 — normal       (RG16_SNORM)     — octahedral-encoded world-space
    //                                       normal (#275, 4 B/px vs 8 B/px)
    //   2 — motion       (R16G16_SFLOAT)  — screen-space motion vector
    //   3 — mesh_id      (R16_UINT)       — per-instance ID + 1.
    //                                       Lower 15 bits = id + 1, bit 15
    //                                       (0x8000) is the ALPHA_BLEND_NO_HISTORY
    //                                       flag (`triangle.frag:712`), so the
    //                                       hard ceiling is 32767 distinct
    //                                       instances — guarded by the
    //                                       `debug_assert!` in
    //                                       `draw.rs::draw_frame` (#647 / RP-1).
    //                                       background = 0; shader writes id+1.
    //                                       See #318 / R34-02.
    //   4 — raw_indirect (B10G11R11_UFLOAT) — demodulated indirect light (for SVGF)
    //   5 — albedo       (B10G11R11_UFLOAT) — surface color (re-multiplied at composite)
    //   6 — depth        (D32)
    //
    // All color attachments use final_layout SHADER_READ_ONLY_OPTIMAL so
    // the composite pass and SVGF compute passes can sample them.
    let make_color = |fmt: vk::Format| -> vk::AttachmentDescription {
        vk::AttachmentDescription::default()
            .format(fmt)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
    };
    let color_attachment = make_color(color_format);
    let normal_attachment = make_color(normal_format);
    let motion_attachment = make_color(motion_format);
    let mesh_id_attachment = make_color(mesh_id_format);
    let raw_indirect_attachment = make_color(raw_indirect_format);
    let albedo_attachment = make_color(albedo_format);

    // Depth is STORED (not DONT_CARE) so the SSAO compute pass can read it
    // after the render pass. Final layout is READ_ONLY for shader sampling.
    let depth_attachment = vk::AttachmentDescription::default()
        .format(depth_format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL);

    // Attachments 0..=5 are color, attachment 6 is depth.
    let make_color_ref = |i: u32| vk::AttachmentReference {
        attachment: i,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    };
    let color_refs = [
        make_color_ref(0), // HDR
        make_color_ref(1), // normal
        make_color_ref(2), // motion
        make_color_ref(3), // mesh_id
        make_color_ref(4), // raw_indirect
        make_color_ref(5), // albedo
    ];

    let depth_ref = vk::AttachmentReference {
        attachment: 6,
        layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
    };

    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_refs)
        .depth_stencil_attachment(&depth_ref);

    // Include LATE_FRAGMENT_TESTS because the fragment shader uses `discard`,
    // which can defer depth writes to the late fragment test stage per spec.
    let dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
        )
        .src_access_mask(vk::AccessFlags::empty())
        .dst_stage_mask(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
        )
        .dst_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        );

    // Outgoing dependency: ensure color + depth writes are complete before
    // downstream passes read them.
    // - Composite fragment shader reads HDR color as a sampled texture.
    // - SSAO compute shader reads depth in READ_ONLY layout.
    let dependency_out = vk::SubpassDependency::default()
        .src_subpass(0)
        .dst_subpass(vk::SUBPASS_EXTERNAL)
        .src_stage_mask(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
        )
        .src_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        )
        .dst_stage_mask(
            vk::PipelineStageFlags::FRAGMENT_SHADER
                | vk::PipelineStageFlags::COMPUTE_SHADER
                | vk::PipelineStageFlags::BOTTOM_OF_PIPE,
        )
        .dst_access_mask(vk::AccessFlags::SHADER_READ);

    let attachments = [
        color_attachment,
        normal_attachment,
        motion_attachment,
        mesh_id_attachment,
        raw_indirect_attachment,
        albedo_attachment,
        depth_attachment,
    ];
    let subpasses = [subpass];
    let dependencies = [dependency, dependency_out];

    let create_info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(&subpasses)
        .dependencies(&dependencies);

    let render_pass = unsafe {
        device
            .create_render_pass(&create_info, None)
            .context("Failed to create render pass")?
    };

    log::info!("Render pass created (6 color + depth)");
    Ok(render_pass)
}

/// Create one main framebuffer per frame-in-flight slot. Each framebuffer
/// binds that slot's HDR + normal + motion + mesh_id + raw_indirect +
/// albedo views, plus the shared depth view.
///
/// All the color view slices must have the same length (MAX_FRAMES_IN_FLIGHT).
pub(super) fn create_main_framebuffers(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    hdr_views: &[vk::ImageView],
    normal_views: &[vk::ImageView],
    motion_views: &[vk::ImageView],
    mesh_id_views: &[vk::ImageView],
    raw_indirect_views: &[vk::ImageView],
    albedo_views: &[vk::ImageView],
    depth_view: vk::ImageView,
    extent: vk::Extent2D,
) -> Result<Vec<vk::Framebuffer>> {
    debug_assert_eq!(hdr_views.len(), normal_views.len());
    debug_assert_eq!(hdr_views.len(), motion_views.len());
    debug_assert_eq!(hdr_views.len(), mesh_id_views.len());
    debug_assert_eq!(hdr_views.len(), raw_indirect_views.len());
    debug_assert_eq!(hdr_views.len(), albedo_views.len());

    (0..hdr_views.len())
        .map(|i| {
            let attachments = [
                hdr_views[i],
                normal_views[i],
                motion_views[i],
                mesh_id_views[i],
                raw_indirect_views[i],
                albedo_views[i],
                depth_view,
            ];
            let create_info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            unsafe {
                device
                    .create_framebuffer(&create_info, None)
                    .context("Failed to create main framebuffer")
            }
        })
        .collect()
}

/// Create the depth image, view, and allocation.
pub(super) fn create_depth_resources(
    device: &ash::Device,
    allocator: &SharedAllocator,
    extent: vk::Extent2D,
    depth_format: vk::Format,
) -> Result<(vk::Image, vk::ImageView, vk_alloc::Allocation)> {
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(depth_format)
        .extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);

    // The early-exit leak path described in #96: previously, if `allocate`,
    // `bind_image_memory`, or `create_image_view` returned `Err` after the
    // image (and possibly the allocation) already existed, those handles
    // were dropped on the floor and leaked until shutdown. Each fallible
    // step below now owns its cleanup. The inline pattern (rather than a
    // dedicated RAII guard) matches how the rest of this helpers.rs file
    // handles resource creation.

    let image = unsafe {
        device
            .create_image(&image_info, None)
            .context("Failed to create depth image")?
    };

    let requirements = unsafe { device.get_image_memory_requirements(image) };

    let allocation = match allocator.lock().expect("allocator lock poisoned").allocate(
        &vk_alloc::AllocationCreateDesc {
            name: "depth_buffer",
            requirements,
            location: MemoryLocation::GpuOnly,
            linear: false,
            allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
        },
    ) {
        Ok(a) => a,
        Err(e) => {
            // Allocation failed — only `image` needs cleanup.
            unsafe {
                // SAFETY: `image` was created by `device` on line above
                // and has not been destroyed or bound to memory yet.
                device.destroy_image(image, None);
            }
            return Err(anyhow::Error::from(e).context("Failed to allocate depth image memory"));
        }
    };

    if let Err(e) =
        unsafe { device.bind_image_memory(image, allocation.memory(), allocation.offset()) }
    {
        // Bind failed — destroy the image and release the allocation.
        // Order matters: destroy the image first so the allocator isn't
        // freeing memory that still has a live binding from the GPU's
        // point of view (even though the bind call itself failed, be
        // conservative).
        unsafe {
            // SAFETY: `image` was created above and never bound successfully.
            device.destroy_image(image, None);
        }
        let _ = allocator
            .lock()
            .expect("allocator lock poisoned")
            .free(allocation);
        return Err(anyhow::Error::from(e).context("Failed to bind depth image memory"));
    }

    let view_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(depth_format)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });

    let view = match unsafe { device.create_image_view(&view_info, None) } {
        Ok(v) => v,
        Err(e) => {
            // View creation failed — image exists, memory bound. Free
            // the allocation (gpu-allocator handles the Vulkan memory
            // lifetime) and destroy the image.
            unsafe {
                // SAFETY: `image` was created and bound above. Destroying
                // it before freeing the allocation is required by the
                // Vulkan spec (image must not outlive its memory).
                device.destroy_image(image, None);
            }
            let _ = allocator
                .lock()
                .expect("allocator lock poisoned")
                .free(allocation);
            return Err(anyhow::Error::from(e).context("Failed to create depth image view"));
        }
    };

    log::info!(
        "Depth buffer created: {}x{} {:?}",
        extent.width,
        extent.height,
        depth_format
    );
    Ok((image, view, allocation))
}

// --- Shared teardown helpers (#33 / R-10) -----------------------------------
//
// Both `recreate_swapchain` (resize.rs) and `Drop` (mod.rs) tear down the
// same three resource categories in the same relative order: framebuffers,
// depth attachment, and the rasterization pipelines bound to the main render
// pass. Pre-#33 each site inlined its own loops, which were correct but
// drifted from each other and made future additions (e.g., a new render-
// pass-bound pipeline) easy to miss in one location. The helpers below
// encode the correct order once.
//
// The helpers null out handles after destruction so callers can rebuild
// idempotently (resize) or fall through cleanly (Drop, where nulling is a
// no-op since the struct is being dropped anyway).

/// Destroy every entry in the main framebuffer set and clear the Vec.
/// Called by both `recreate_swapchain` (rebuild path) and `Drop` (final
/// teardown) — see #33 / R-10.
///
/// SAFETY: All framebuffers must have been created by `device` and not
/// already destroyed. `device_wait_idle` (or equivalent fence wait) must
/// have completed before the call so no in-flight command buffer is still
/// referencing them.
pub(super) unsafe fn destroy_main_framebuffers(
    device: &ash::Device,
    framebuffers: &mut Vec<vk::Framebuffer>,
) {
    for &fb in framebuffers.iter() {
        device.destroy_framebuffer(fb, None);
    }
    framebuffers.clear();
}

/// Destroy the depth attachment in the order Vulkan requires:
/// view → image → backing allocation. The image must outlive its memory
/// (VUID-vkFreeMemory-memory-00677). Handles are nulled so callers can
/// rebuild idempotently. Called by both `recreate_swapchain` and `Drop`
/// — see #33 / R-10.
///
/// SAFETY: All three handles must have been created by `device` /
/// `allocator` and not already destroyed. `device_wait_idle` must have
/// completed before the call.
pub(super) unsafe fn destroy_depth_resources(
    device: &ash::Device,
    allocator: &SharedAllocator,
    view: &mut vk::ImageView,
    image: &mut vk::Image,
    allocation: &mut Option<vk_alloc::Allocation>,
) {
    device.destroy_image_view(*view, None);
    *view = vk::ImageView::null();
    device.destroy_image(*image, None);
    *image = vk::Image::null();
    if let Some(alloc) = allocation.take() {
        allocator
            .lock()
            .expect("allocator lock poisoned")
            .free(alloc)
            .expect("Failed to free depth allocation");
    }
}

/// Destroy the rasterization pipelines that bind the main render pass:
/// forward, two-sided variant, the on-demand blend pipeline cache, and the
/// UI overlay pipeline. The render pass itself is **not** destroyed here:
/// `recreate_swapchain` keeps it on a format-stable resize, and `Drop` has
/// additional dependencies (pipeline_layout, mesh_registry, pipeline_cache)
/// to walk in order before reaching it. Handles are nulled so callers can
/// rebuild idempotently. See #33 / R-10.
///
/// SAFETY: All pipeline handles must have been created by `device` and
/// must outlive any descriptor set / command buffer still referencing them.
/// `device_wait_idle` must have completed before the call.
pub(super) unsafe fn destroy_render_pass_pipelines(
    device: &ash::Device,
    pipeline: &mut vk::Pipeline,
    pipeline_two_sided: &mut vk::Pipeline,
    blend_pipeline_cache: &mut std::collections::HashMap<(u8, u8, bool), vk::Pipeline>,
    pipeline_ui: &mut vk::Pipeline,
) {
    device.destroy_pipeline(*pipeline, None);
    *pipeline = vk::Pipeline::null();
    device.destroy_pipeline(*pipeline_two_sided, None);
    *pipeline_two_sided = vk::Pipeline::null();
    for (_, pipe) in blend_pipeline_cache.drain() {
        device.destroy_pipeline(pipe, None);
    }
    device.destroy_pipeline(*pipeline_ui, None);
    *pipeline_ui = vk::Pipeline::null();
}

pub(super) fn create_command_pool(
    device: &ash::Device,
    queue_family: u32,
) -> Result<vk::CommandPool> {
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

/// Create a command pool for one-time upload/transfer commands.
///
/// Unlike the per-frame draw pool, this pool does NOT set
/// RESET_COMMAND_BUFFER because one-time commands are allocated, used
/// once, and freed — never reset. Using a separate pool avoids Vulkan
/// external-sync contention with draw command buffer operations
/// (VUID-vkAllocateCommandBuffers-commandPool-00044).
pub(super) fn create_transfer_pool(
    device: &ash::Device,
    queue_family: u32,
) -> Result<vk::CommandPool> {
    let create_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(queue_family)
        .flags(vk::CommandPoolCreateFlags::TRANSIENT);

    let pool = unsafe {
        device
            .create_command_pool(&create_info, None)
            .context("Failed to create transfer command pool")?
    };

    log::info!("Transfer command pool created");
    Ok(pool)
}

pub(super) fn allocate_command_buffers(
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

fn pipeline_cache_path() -> std::path::PathBuf {
    // Resolve next to the executable so the cache persists across working-directory
    // changes (launcher, read-only cwd, packaged distribution).
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("pipeline_cache.bin")))
        .unwrap_or_else(|| std::path::PathBuf::from("pipeline_cache.bin"))
}

/// Load pipeline cache data from disk, or create an empty cache.
pub(super) fn load_or_create_pipeline_cache(device: &ash::Device) -> Result<vk::PipelineCache> {
    let initial_data = std::fs::read(pipeline_cache_path()).unwrap_or_default();

    let create_info = if initial_data.is_empty() {
        log::info!("Creating empty pipeline cache");
        vk::PipelineCacheCreateInfo::default()
    } else {
        log::info!(
            "Loading pipeline cache from disk ({} bytes)",
            initial_data.len()
        );
        vk::PipelineCacheCreateInfo::default().initial_data(&initial_data)
    };

    let cache = unsafe {
        device
            .create_pipeline_cache(&create_info, None)
            .context("Failed to create pipeline cache")?
    };

    Ok(cache)
}

/// Save pipeline cache data to disk. Best-effort — logs warnings on failure.
pub(super) fn save_pipeline_cache(device: &ash::Device, cache: vk::PipelineCache) {
    // SAFETY: get_pipeline_cache_data returns a Vec<u8> copy of the cache.
    // The cache handle is valid (not yet destroyed).
    let data = unsafe {
        match device.get_pipeline_cache_data(cache) {
            Ok(data) => data,
            Err(e) => {
                log::warn!("Failed to get pipeline cache data: {:?}", e);
                return;
            }
        }
    };

    let path = pipeline_cache_path();
    if let Err(e) = std::fs::write(&path, &data) {
        log::error!("Failed to save pipeline cache to {}: {}", path.display(), e);
    } else {
        log::info!("Pipeline cache saved to {} ({} bytes)", path.display(), data.len());
    }
}
