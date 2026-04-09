//! Stateless helper functions used by VulkanContext::new(), recreate_swapchain(), and Drop.

use super::super::allocator::SharedAllocator;
use super::super::swapchain::SwapchainState;
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
    depth_format: vk::Format,
) -> Result<vk::RenderPass> {
    let color_attachment = vk::AttachmentDescription::default()
        .format(color_format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

    // Depth store DONT_CARE — cleared each frame, never sampled afterward.
    // Saves bandwidth on tile-based GPUs (skips depth writeback to memory).
    let depth_attachment = vk::AttachmentDescription::default()
        .format(depth_format)
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

pub(super) fn create_framebuffers(
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
        .format(depth_format)
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
        "Depth buffer created: {}x{} {:?}",
        extent.width,
        extent.height,
        depth_format
    );
    Ok((image, view, allocation))
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

const PIPELINE_CACHE_PATH: &str = "pipeline_cache.bin";

/// Load pipeline cache data from disk, or create an empty cache.
pub(super) fn load_or_create_pipeline_cache(device: &ash::Device) -> Result<vk::PipelineCache> {
    let initial_data = std::fs::read(PIPELINE_CACHE_PATH).unwrap_or_default();

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

    if let Err(e) = std::fs::write(PIPELINE_CACHE_PATH, &data) {
        log::warn!("Failed to save pipeline cache to disk: {}", e);
    } else {
        log::info!("Pipeline cache saved ({} bytes)", data.len());
    }
}
