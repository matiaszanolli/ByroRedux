//! Swapchain creation and management.

use super::device::QueueFamilyIndices;
use anyhow::{Context, Result};
use ash::vk;

/// Everything needed to present frames.
pub struct SwapchainState {
    pub swapchain_loader: ash::khr::swapchain::Device,
    pub swapchain: vk::SwapchainKHR,
    pub images: Vec<vk::Image>,
    pub image_views: Vec<vk::ImageView>,
    pub format: vk::SurfaceFormatKHR,
    pub extent: vk::Extent2D,
}

/// Queries surface capabilities and picks the best format, present mode, and extent.
/// `old_swapchain`: pass the previous swapchain handle for atomic handoff
/// during recreation (avoids flicker on some platforms). Pass `null()` for
/// initial creation.
pub fn create_swapchain(
    instance: &ash::Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    indices: QueueFamilyIndices,
    window_size: [u32; 2],
    old_swapchain: vk::SwapchainKHR,
) -> Result<SwapchainState> {
    let capabilities = unsafe {
        surface_loader
            .get_physical_device_surface_capabilities(physical_device, surface)
            .context("Failed to get surface capabilities")?
    };

    let formats = unsafe {
        surface_loader
            .get_physical_device_surface_formats(physical_device, surface)
            .context("Failed to get surface formats")?
    };

    let present_modes = unsafe {
        surface_loader
            .get_physical_device_surface_present_modes(physical_device, surface)
            .context("Failed to get present modes")?
    };

    let format = choose_surface_format(&formats);
    let present_mode = choose_present_mode(&present_modes);
    let extent = choose_extent(&capabilities, window_size);

    // Request one more than the minimum to avoid stalling on the driver.
    let mut image_count = capabilities.min_image_count + 1;
    if capabilities.max_image_count > 0 && image_count > capabilities.max_image_count {
        image_count = capabilities.max_image_count;
    }

    // Switched from a raw struct initializer with `queue_family_indices.as_ptr()`
    // to the ash builder pattern so the borrow of the local `queue_family_indices`
    // array is checked by the compiler instead of trusted via a SAFETY comment.
    // The builder stores a `&[u32]` reference, which must outlive `create_info`;
    // both live in this function, so the constraint is automatic. See #93.
    let queue_family_indices = [indices.graphics, indices.present];
    let sharing_mode = if indices.graphics != indices.present {
        vk::SharingMode::CONCURRENT
    } else {
        vk::SharingMode::EXCLUSIVE
    };

    let mut create_info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(format.format)
        .image_color_space(format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
        .image_sharing_mode(sharing_mode)
        .pre_transform(capabilities.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode)
        .clipped(true)
        .old_swapchain(old_swapchain);

    // Only set queue family indices in CONCURRENT mode — the Vulkan spec
    // requires `queueFamilyIndexCount == 0` and `pQueueFamilyIndices == NULL`
    // under SHARING_MODE_EXCLUSIVE, which is the builder's default.
    if indices.graphics != indices.present {
        create_info = create_info.queue_family_indices(&queue_family_indices);
    }

    let swapchain_loader = ash::khr::swapchain::Device::new(instance, device);

    let swapchain = unsafe {
        swapchain_loader
            .create_swapchain(&create_info, None)
            .context("Failed to create swapchain")?
    };

    let images = unsafe {
        swapchain_loader
            .get_swapchain_images(swapchain)
            .context("Failed to get swapchain images")?
    };

    let image_views = create_image_views(device, &images, format.format)?;

    log::info!(
        "Swapchain created: {}x{}, {} images, format {:?}",
        extent.width,
        extent.height,
        images.len(),
        format.format,
    );

    Ok(SwapchainState {
        swapchain_loader,
        swapchain,
        images,
        image_views,
        format,
        extent,
    })
}

fn choose_surface_format(formats: &[vk::SurfaceFormatKHR]) -> vk::SurfaceFormatKHR {
    // Prefer sRGB B8G8R8A8.
    formats
        .iter()
        .copied()
        .find(|f| {
            f.format == vk::Format::B8G8R8A8_SRGB
                && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .unwrap_or(formats[0])
}

fn choose_present_mode(modes: &[vk::PresentModeKHR]) -> vk::PresentModeKHR {
    // Mailbox (triple-buffered) if available, otherwise FIFO (always available).
    if modes.contains(&vk::PresentModeKHR::MAILBOX) {
        vk::PresentModeKHR::MAILBOX
    } else {
        vk::PresentModeKHR::FIFO
    }
}

fn choose_extent(capabilities: &vk::SurfaceCapabilitiesKHR, window_size: [u32; 2]) -> vk::Extent2D {
    if capabilities.current_extent.width != u32::MAX {
        capabilities.current_extent
    } else {
        vk::Extent2D {
            width: window_size[0].clamp(
                capabilities.min_image_extent.width,
                capabilities.max_image_extent.width,
            ),
            height: window_size[1].clamp(
                capabilities.min_image_extent.height,
                capabilities.max_image_extent.height,
            ),
        }
    }
}

fn create_image_views(
    device: &ash::Device,
    images: &[vk::Image],
    format: vk::Format,
) -> Result<Vec<vk::ImageView>> {
    images
        .iter()
        .map(|&image| {
            let create_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .components(vk::ComponentMapping {
                    r: vk::ComponentSwizzle::IDENTITY,
                    g: vk::ComponentSwizzle::IDENTITY,
                    b: vk::ComponentSwizzle::IDENTITY,
                    a: vk::ComponentSwizzle::IDENTITY,
                })
                .subresource_range(super::descriptors::color_subresource_single_mip());

            unsafe {
                device
                    .create_image_view(&create_info, None)
                    .context("Failed to create image view")
            }
        })
        .collect()
}

impl SwapchainState {
    /// Destroy swapchain resources. Must be called before dropping.
    ///
    /// #655 — clears `image_views` and nulls `swapchain` after the
    /// destroy calls so a hypothetical second `destroy` (e.g. a future
    /// panic-cleanup path) is a no-op against `VK_NULL_HANDLE` rather
    /// than a double-free of every view + the swapchain itself.
    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        for &view in &self.image_views {
            device.destroy_image_view(view, None);
        }
        self.image_views.clear();
        if self.swapchain != vk::SwapchainKHR::null() {
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
            self.swapchain = vk::SwapchainKHR::null();
        }
    }
}
