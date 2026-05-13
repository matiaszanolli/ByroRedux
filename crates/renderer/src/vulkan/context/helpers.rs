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
    //   3 — mesh_id      (R32_UINT)       — per-instance ID + 1.
    //                                       Lower 31 bits = id + 1, bit 31
    //                                       (0x80000000) is the ALPHA_BLEND_NO_HISTORY
    //                                       flag (`triangle.frag:980`), so the
    //                                       encoding ceiling is 0x7FFFFFFF
    //                                       distinct instances — `MAX_INSTANCES`
    //                                       sits well below that to bound the
    //                                       persistent SSBO allocation, guarded
    //                                       by the `debug_assert!` in
    //                                       `draw.rs::draw_frame` (#647 / RP-1).
    //                                       background = 0; shader writes id+1.
    //                                       See #318 / R34-02 / #992.
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
        // #947 / REN-D4-NEW-01: include EARLY_FRAGMENT_TESTS in
        // symmetry with `dependency` above. Depth writes can complete
        // in either EARLY or LATE depending on whether the fragment
        // shader hits a `discard` / `gl_FragDepth` branch. LATE alone
        // is spec-legal (LATE is logically-later so the dep
        // transitively covers EARLY writes) but Synchronization2
        // validation treats the missing EARLY as an
        // under-synchronisation hint.
        .src_stage_mask(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
        )
        .src_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        )
        // BOTTOM_OF_PIPE is omitted intentionally (#573 / SY-2). Per
        // Vulkan spec the BOTTOM_OF_PIPE stage in `dst_stage_mask`
        // must be paired with a zero `dst_access_mask`; combining it
        // with `SHADER_READ` is rejected by Synchronization2
        // validation. The flag also provides no memory-ordering
        // guarantee on its own — the FRAGMENT_SHADER + COMPUTE_SHADER
        // pair is what actually gates the downstream reads (composite
        // fragment shader for HDR color, SSAO compute shader for
        // depth in READ_ONLY layout). composite.rs:408 and
        // screenshot.rs:164 also use BOTTOM_OF_PIPE in `dst_stage_mask`
        // but pair it with an empty `dst_access_mask`, which the spec
        // permits — so they're left alone.
        .dst_stage_mask(
            vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
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
    blend_pipeline_cache: &mut std::collections::HashMap<(u8, u8), vk::Pipeline>,
    pipeline_ui: &mut vk::Pipeline,
) {
    device.destroy_pipeline(*pipeline, None);
    *pipeline = vk::Pipeline::null();
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

/// Validate a Vulkan pipeline cache header against the running
/// device's properties. Per VK spec
/// [`VkPipelineCacheHeaderVersionOne`] the on-disk header is 32
/// bytes:
///
/// | bytes | field            |
/// |-------|------------------|
/// |  0–3  | u32 headerSize   |
/// |  4–7  | u32 headerVersion (= 1, `VK_PIPELINE_CACHE_HEADER_VERSION_ONE`) |
/// |  8–11 | u32 vendorID     |
/// | 12–15 | u32 deviceID     |
/// | 16–31 | u8[16] pipelineCacheUUID |
///
/// Returns `true` only when every field matches the running device.
/// Drivers re-validate independently, but defense-in-depth: a buggy
/// driver — or a malicious cache file dropped next to the binary by
/// a process with filesystem write access — could trip undefined
/// behaviour parsing partial bad data before its own validator runs.
/// Re-checking here means a bad header never reaches the driver.
/// See SAFE-11 / #91.
///
/// Pure helper, separated from the load path so the bit-shuffling
/// can be unit-tested without a Vulkan device.
pub(super) fn validate_pipeline_cache_header(
    initial_data: &[u8],
    expected_vendor_id: u32,
    expected_device_id: u32,
    expected_uuid: &[u8; 16],
) -> bool {
    // VK_PIPELINE_CACHE_HEADER_VERSION_ONE has exactly 32 bytes of
    // fixed prefix; anything shorter is corrupt or truncated.
    if initial_data.len() < 32 {
        return false;
    }
    let header_size = u32::from_le_bytes([
        initial_data[0],
        initial_data[1],
        initial_data[2],
        initial_data[3],
    ]);
    let header_version = u32::from_le_bytes([
        initial_data[4],
        initial_data[5],
        initial_data[6],
        initial_data[7],
    ]);
    let vendor_id = u32::from_le_bytes([
        initial_data[8],
        initial_data[9],
        initial_data[10],
        initial_data[11],
    ]);
    let device_id = u32::from_le_bytes([
        initial_data[12],
        initial_data[13],
        initial_data[14],
        initial_data[15],
    ]);
    // Indexing 16..32 is safe — we already early-returned on len < 32.
    let mut uuid = [0u8; 16];
    uuid.copy_from_slice(&initial_data[16..32]);

    // headerSize is the size of the fixed-prefix struct itself
    // (always 32 today). A driver that writes a longer prefix would
    // bump this; a value < 32 means the file lies about its own
    // shape — reject. We don't upper-bound: a future version might
    // legitimately grow the prefix, and the body bytes after the
    // prefix are driver-specific.
    if header_size < 32 {
        return false;
    }
    if header_version != 1 {
        return false;
    }
    if vendor_id != expected_vendor_id {
        return false;
    }
    if device_id != expected_device_id {
        return false;
    }
    if uuid != *expected_uuid {
        return false;
    }
    true
}

/// Load pipeline cache data from disk, or create an empty cache.
///
/// Validates the on-disk header against the running device's
/// `VkPhysicalDeviceProperties` (vendor / device / pipelineCacheUUID)
/// before handing the bytes to the driver. A mismatch — driver
/// upgrade, GPU swap, mod-installed bad file, malicious payload —
/// silently degrades to an empty cache with a one-line warn rather
/// than feeding suspect bytes through `vkCreatePipelineCache`. See
/// SAFE-11 / #91.
pub(super) fn load_or_create_pipeline_cache(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: &ash::Device,
) -> Result<vk::PipelineCache> {
    let initial_data = std::fs::read(pipeline_cache_path()).unwrap_or_default();

    // Header validation gate — drop the bytes when any field
    // disagrees with the running device. The driver would reject
    // these too, but pre-validating keeps untrusted bytes off the
    // driver's parse path entirely. SAFE-11 / #91.
    let validated_data = if !initial_data.is_empty() {
        // SAFETY: `instance` + `physical_device` are valid for the
        // lifetime of `VulkanContext::new` (the only caller); both
        // were minted by `pick_physical_device` immediately above
        // this call site and aren't destroyed until VulkanContext
        // shutdown.
        let props = unsafe { instance.get_physical_device_properties(physical_device) };
        if validate_pipeline_cache_header(
            &initial_data,
            props.vendor_id,
            props.device_id,
            &props.pipeline_cache_uuid,
        ) {
            log::info!(
                "Loading pipeline cache from disk ({} bytes, header validated)",
                initial_data.len()
            );
            initial_data
        } else {
            log::warn!(
                "Pipeline cache header mismatch — discarding {} stale bytes \
                 (driver upgrade, GPU swap, or tampered file). Starting with \
                 empty cache. SAFE-11 / #91.",
                initial_data.len()
            );
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let create_info = if validated_data.is_empty() {
        log::info!("Creating empty pipeline cache");
        vk::PipelineCacheCreateInfo::default()
    } else {
        vk::PipelineCacheCreateInfo::default().initial_data(&validated_data)
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
        log::info!(
            "Pipeline cache saved to {} ({} bytes)",
            path.display(),
            data.len()
        );
    }
}

#[cfg(test)]
mod pipeline_cache_header_tests {
    //! Regression tests for `validate_pipeline_cache_header` — issue
    //! #91 / SAFE-11. The header is the only part of the cache file
    //! we can sanity-check without a Vulkan device, so the unit tests
    //! pin all six rejection paths plus the happy path.
    use super::*;

    /// Build a synthetic `VkPipelineCacheHeaderVersionOne` prefix
    /// (32 bytes) with caller-controlled fields. Body bytes (driver-
    /// specific opaque payload) follow at offset 32 — the validator
    /// doesn't inspect them, so the body length is whatever the
    /// caller appends.
    fn make_header(
        header_size: u32,
        header_version: u32,
        vendor_id: u32,
        device_id: u32,
        uuid: [u8; 16],
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(32);
        out.extend_from_slice(&header_size.to_le_bytes());
        out.extend_from_slice(&header_version.to_le_bytes());
        out.extend_from_slice(&vendor_id.to_le_bytes());
        out.extend_from_slice(&device_id.to_le_bytes());
        out.extend_from_slice(&uuid);
        out
    }

    /// Canonical "matching device" header — the happy path. Plus a
    /// kilobyte of zero body bytes so the validator's prefix-only
    /// scope is verified (it must not touch the body).
    #[test]
    fn happy_path_valid_header_returns_true() {
        let uuid = [0x42u8; 16];
        let mut data = make_header(32, 1, 0x1002, 0x73BF, uuid);
        data.resize(1024, 0); // body bytes — must be ignored
        assert!(validate_pipeline_cache_header(&data, 0x1002, 0x73BF, &uuid));
    }

    #[test]
    fn empty_data_returns_false() {
        assert!(!validate_pipeline_cache_header(
            &[],
            0x1002,
            0x73BF,
            &[0u8; 16]
        ));
    }

    #[test]
    fn truncated_under_32_bytes_returns_false() {
        let data = vec![0u8; 31];
        assert!(!validate_pipeline_cache_header(
            &data, 0x1002, 0x73BF, &[0u8; 16]
        ));
    }

    /// `headerSize < 32` is a malformed file claiming a smaller
    /// fixed-prefix than VK_PIPELINE_CACHE_HEADER_VERSION_ONE — reject.
    #[test]
    fn header_size_below_32_returns_false() {
        let uuid = [0x42u8; 16];
        let data = make_header(16, 1, 0x1002, 0x73BF, uuid);
        assert!(!validate_pipeline_cache_header(
            &data, 0x1002, 0x73BF, &uuid
        ));
    }

    #[test]
    fn unknown_header_version_returns_false() {
        let uuid = [0x42u8; 16];
        let data = make_header(32, 99, 0x1002, 0x73BF, uuid);
        assert!(!validate_pipeline_cache_header(
            &data, 0x1002, 0x73BF, &uuid
        ));
    }

    /// Vendor ID mismatch — running on AMD (0x1002) with a cache
    /// written by NVIDIA (0x10DE). Must reject — feeding a foreign
    /// vendor's IR through the wrong driver is exactly the
    /// undefined-behaviour surface SAFE-11 guards against.
    #[test]
    fn vendor_id_mismatch_returns_false() {
        let uuid = [0x42u8; 16];
        let data = make_header(32, 1, 0x10DE, 0x73BF, uuid); // NVIDIA file
        assert!(!validate_pipeline_cache_header(
            &data, 0x1002, 0x73BF, &uuid
        )); // AMD device
    }

    #[test]
    fn device_id_mismatch_returns_false() {
        let uuid = [0x42u8; 16];
        let data = make_header(32, 1, 0x1002, 0x1234, uuid);
        assert!(!validate_pipeline_cache_header(
            &data, 0x1002, 0x73BF, &uuid
        ));
    }

    /// `pipelineCacheUUID` mismatch is the most common rejection
    /// path in practice — drivers update the UUID on every minor
    /// version bump, so a cache from yesterday's driver is rejected
    /// by tomorrow's. The happy `vendor + device` match means the
    /// UUID-only check is the regression guard for driver-upgrade
    /// staleness.
    #[test]
    fn pipeline_cache_uuid_mismatch_returns_false() {
        let device_uuid = [0x42u8; 16];
        let cache_uuid = [0x99u8; 16]; // different driver build
        let data = make_header(32, 1, 0x1002, 0x73BF, cache_uuid);
        assert!(!validate_pipeline_cache_header(
            &data,
            0x1002,
            0x73BF,
            &device_uuid
        ));
    }

    /// All-zero header — the on-disk shape `std::fs::read` returns
    /// after a corruption that zero-fills the file. Must reject.
    /// Zero `headerSize` fails the `< 32` gate; zero `headerVersion`
    /// fails the `!= 1` gate.
    #[test]
    fn all_zero_header_returns_false() {
        let data = vec![0u8; 32];
        assert!(!validate_pipeline_cache_header(
            &data, 0x1002, 0x73BF, &[0u8; 16]
        ));
    }

    /// Future-driver headers may legitimately grow the prefix
    /// (`headerSize > 32`). The validator must accept that — only
    /// the < 32 case is malformed.
    #[test]
    fn larger_header_size_returns_true_when_other_fields_match() {
        let uuid = [0x42u8; 16];
        let mut data = make_header(64, 1, 0x1002, 0x73BF, uuid);
        // 32 bytes of forward-compat extension prefix the validator
        // ignores (driver consumes them; we don't).
        data.resize(64, 0);
        assert!(validate_pipeline_cache_header(&data, 0x1002, 0x73BF, &uuid));
    }
}
