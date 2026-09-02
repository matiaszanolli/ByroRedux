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
/// The instance/device/surface handles a swapchain build queries against.
/// Groups the five Vulkan handles that travel together into
/// [`create_swapchain`].
#[derive(Clone, Copy)]
pub struct SwapchainSurfaceCtx<'a> {
    /// Vulkan instance.
    pub instance: &'a ash::Instance,
    /// Logical device the swapchain is created on.
    pub device: &'a ash::Device,
    /// Physical device the surface capabilities are queried from.
    pub physical_device: vk::PhysicalDevice,
    /// Surface extension loader.
    pub surface_loader: &'a ash::khr::surface::Instance,
    /// The window surface the swapchain presents to.
    pub surface: vk::SurfaceKHR,
}

pub fn create_swapchain(
    ctx: SwapchainSurfaceCtx,
    indices: QueueFamilyIndices,
    window_size: [u32; 2],
    old_swapchain: vk::SwapchainKHR,
) -> Result<SwapchainState> {
    let SwapchainSurfaceCtx {
        instance,
        device,
        physical_device,
        surface_loader,
        surface,
    } = ctx;
    let capabilities = unsafe {
        // SAFETY: `surface_loader` is the live surface extension loader;
        // `physical_device` and `surface` are the live handles it was queried
        // against, both owned by the enclosing context.
        surface_loader
            .get_physical_device_surface_capabilities(physical_device, surface)
            .context("Failed to get surface capabilities")?
    };

    let formats = unsafe {
        // SAFETY: `surface_loader` is live; `physical_device` and `surface` are
        // the live, context-owned handles it queries.
        surface_loader
            .get_physical_device_surface_formats(physical_device, surface)
            .context("Failed to get surface formats")?
    };

    let present_modes = unsafe {
        // SAFETY: `surface_loader` is live; `physical_device` and `surface` are
        // the live, context-owned handles it queries.
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
        // SAFETY: `swapchain_loader` was just built from the live instance +
        // device; `create_info` is fully initialized, borrows `queue_family_indices`
        // (which outlives this call) only in CONCURRENT mode, and references the
        // live `surface` + `old_swapchain`; the returned swapchain is caller-owned.
        swapchain_loader
            .create_swapchain(&create_info, None)
            .context("Failed to create swapchain")?
    };

    let images = unsafe {
        // SAFETY: `swapchain_loader` is live and `swapchain` is the swapchain it
        // just created; the driver owns the returned images (freed with the
        // swapchain, not individually).
        swapchain_loader
            .get_swapchain_images(swapchain)
            .context("Failed to get swapchain images")?
    };

    let image_views = create_image_views(device, &images, format.format)?;

    log::info!(
        "Swapchain created: {}x{}, {} images, format {:?}, present_mode {:?} (available: {:?})",
        extent.width,
        extent.height,
        images.len(),
        format.format,
        present_mode,
        present_modes,
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
        .unwrap_or_else(|| {
            // #3595 — #3426's UI-overlay exact-colour-round-trip argument
            // (Ruffle's sRGB-encoded capture -> linearising sampler ->
            // linear-space blend -> hardware sRGB re-encode on write) is
            // premised on the swapchain attachment actually being
            // `B8G8R8A8_SRGB` + `SRGB_NONLINEAR`. That pair is present on
            // every desktop driver this project targets, so this arm has
            // no observed hit — but a silent fallback under a documented
            // invariant is still a gap: log it so a colour-shifted overlay
            // on some future surface has a paper trail instead of "it just
            // looks wrong".
            let fallback = formats[0];
            log::warn!(
                "No B8G8R8A8_SRGB + SRGB_NONLINEAR surface format available; \
                 falling back to {:?}/{:?}. The UI overlay's sRGB round-trip \
                 (linearise on sample, hardware re-encode on write) depends \
                 on an sRGB swapchain attachment — expect the overlay to \
                 render too dark or washed out on this surface.",
                fallback.format, fallback.color_space,
            );
            fallback
        })
}

fn choose_present_mode(modes: &[vk::PresentModeKHR]) -> vk::PresentModeKHR {
    // `BYROREDUX_PRESENT_MODE=fifo|mailbox|immediate|fifo_relaxed` lets
    // the operator override the default selection for perf testing.
    // Useful when a Wayland compositor is gating frame callbacks
    // tighter than the GPU+CPU budget needs — switching to
    // IMMEDIATE bypasses the compositor's pacing entirely (at the
    // cost of tearing). On X11 + 4070 Ti the default MAILBOX should
    // give uncapped triple-buffered behaviour. Phase 13 added the
    // override so the 18-FPS "stuck" ceiling could be diagnosed
    // without recompiling.
    if let Ok(requested) = std::env::var("BYROREDUX_PRESENT_MODE") {
        let candidate = match requested.to_ascii_lowercase().as_str() {
            "fifo" => Some(vk::PresentModeKHR::FIFO),
            "mailbox" => Some(vk::PresentModeKHR::MAILBOX),
            "immediate" => Some(vk::PresentModeKHR::IMMEDIATE),
            "fifo_relaxed" => Some(vk::PresentModeKHR::FIFO_RELAXED),
            _ => None,
        };
        if let Some(c) = candidate {
            if modes.contains(&c) {
                log::info!("BYROREDUX_PRESENT_MODE override → {:?}", c);
                return c;
            } else {
                log::warn!(
                    "BYROREDUX_PRESENT_MODE={:?} not supported by surface (have {:?}); \
                     falling back to default selection",
                    c,
                    modes,
                );
            }
        }
    }
    // Default: Mailbox (triple-buffered) if available, otherwise FIFO (always available).
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
                // SAFETY: `device` is the live logical device; `create_info`
                // references `image`, a live swapchain image owned by `device`,
                // with a valid 2D color view configuration; the returned view is
                // caller-owned and destroyed in `SwapchainState::destroy`.
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
    ///
    /// # Safety
    ///
    /// Caller must ensure `device` is valid and live, the device is not lost,
    /// and that none of the swapchain images or views are still in use by an
    /// in-flight command buffer or pending present.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(format: vk::Format, color_space: vk::ColorSpaceKHR) -> vk::SurfaceFormatKHR {
        vk::SurfaceFormatKHR {
            format,
            color_space,
        }
    }

    /// #3595 — the documented invariant (#3426's UI-overlay exact-colour-
    /// round-trip argument) depends on this exact pair being selected when
    /// the surface offers it, even if it isn't first in the list.
    #[test]
    fn choose_surface_format_prefers_srgb_b8g8r8a8_when_present() {
        let formats = [
            fmt(vk::Format::B8G8R8A8_UNORM, vk::ColorSpaceKHR::SRGB_NONLINEAR),
            fmt(vk::Format::R8G8B8A8_UNORM, vk::ColorSpaceKHR::SRGB_NONLINEAR),
            fmt(vk::Format::B8G8R8A8_SRGB, vk::ColorSpaceKHR::SRGB_NONLINEAR),
        ];
        let chosen = choose_surface_format(&formats);
        assert_eq!(chosen.format, vk::Format::B8G8R8A8_SRGB);
        assert_eq!(chosen.color_space, vk::ColorSpaceKHR::SRGB_NONLINEAR);
    }

    /// The fallback path #3595 adds a `log::warn!` to: when no
    /// `B8G8R8A8_SRGB` + `SRGB_NONLINEAR` entry exists, the first
    /// surface-advertised format is still returned (never a panic — a
    /// missing sRGB pair is a colour-accuracy problem, not a fatal one).
    #[test]
    fn choose_surface_format_falls_back_to_the_first_advertised_format() {
        let formats = [
            fmt(vk::Format::B8G8R8A8_UNORM, vk::ColorSpaceKHR::SRGB_NONLINEAR),
            fmt(vk::Format::R8G8B8A8_UNORM, vk::ColorSpaceKHR::SRGB_NONLINEAR),
        ];
        let chosen = choose_surface_format(&formats);
        assert_eq!(chosen.format, vk::Format::B8G8R8A8_UNORM);
        assert_eq!(chosen.color_space, vk::ColorSpaceKHR::SRGB_NONLINEAR);
    }

    /// The right format alone isn't sufficient — a `B8G8R8A8_SRGB` entry
    /// paired with a non-`SRGB_NONLINEAR` colour space must not match
    /// either (both halves of the `find` predicate are load-bearing).
    #[test]
    fn choose_surface_format_requires_both_the_format_and_the_color_space() {
        let formats = [
            fmt(vk::Format::B8G8R8A8_SRGB, vk::ColorSpaceKHR::EXTENDED_SRGB_LINEAR_EXT),
            fmt(vk::Format::B8G8R8A8_UNORM, vk::ColorSpaceKHR::SRGB_NONLINEAR),
        ];
        let chosen = choose_surface_format(&formats);
        assert_eq!(
            chosen.format,
            vk::Format::B8G8R8A8_SRGB,
            "falls back to formats[0] — the mismatched-color-space entry \
             must not have matched the preference"
        );
        assert_eq!(chosen.color_space, vk::ColorSpaceKHR::EXTENDED_SRGB_LINEAR_EXT);
    }
}
