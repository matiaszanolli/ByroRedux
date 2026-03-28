//! Physical device selection and logical device creation.

use anyhow::{Context, Result};
use ash::vk;
use std::ffi::CStr;

/// Holds the selected queue family indices.
#[derive(Debug, Clone, Copy)]
pub struct QueueFamilyIndices {
    pub graphics: u32,
    pub present: u32,
}

/// Required device extensions.
const DEVICE_EXTENSIONS: &[&CStr] = &[ash::khr::swapchain::NAME];

/// Picks a suitable physical device that supports our required extensions
/// and has graphics + present queue families for the given surface.
pub fn pick_physical_device(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<(vk::PhysicalDevice, QueueFamilyIndices)> {
    let devices = unsafe {
        instance
            .enumerate_physical_devices()
            .context("Failed to enumerate physical devices")?
    };

    if devices.is_empty() {
        anyhow::bail!("No Vulkan-capable GPU found");
    }

    for &device in &devices {
        if let Some(indices) = is_device_suitable(instance, surface_loader, surface, device)? {
            let props = unsafe { instance.get_physical_device_properties(device) };
            let name = unsafe { CStr::from_ptr(props.device_name.as_ptr()) };
            log::info!("Selected GPU: {:?}", name);
            return Ok((device, indices));
        }
    }

    anyhow::bail!("No suitable GPU found (need graphics + present queues and swapchain support)")
}

fn is_device_suitable(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    device: vk::PhysicalDevice,
) -> Result<Option<QueueFamilyIndices>> {
    // Check required extensions.
    let available_extensions = unsafe {
        instance
            .enumerate_device_extension_properties(device)
            .context("Failed to enumerate device extensions")?
    };

    for required in DEVICE_EXTENSIONS {
        let found = available_extensions.iter().any(|ext| {
            let name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
            name == *required
        });
        if !found {
            return Ok(None);
        }
    }

    // Find queue families.
    let queue_families =
        unsafe { instance.get_physical_device_queue_family_properties(device) };

    let mut graphics = None;
    let mut present = None;

    for (i, family) in queue_families.iter().enumerate() {
        let i = i as u32;

        if family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            graphics = Some(i);
        }

        let present_support = unsafe {
            surface_loader
                .get_physical_device_surface_support(device, i, surface)
                .unwrap_or(false)
        };

        if present_support {
            present = Some(i);
        }

        if graphics.is_some() && present.is_some() {
            break;
        }
    }

    match (graphics, present) {
        (Some(g), Some(p)) => Ok(Some(QueueFamilyIndices {
            graphics: g,
            present: p,
        })),
        _ => Ok(None),
    }
}

/// Creates a logical device with graphics and present queues.
pub fn create_logical_device(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    indices: QueueFamilyIndices,
) -> Result<(ash::Device, vk::Queue, vk::Queue)> {
    let unique_families: Vec<u32> = if indices.graphics == indices.present {
        vec![indices.graphics]
    } else {
        vec![indices.graphics, indices.present]
    };

    let queue_priorities = [1.0_f32];
    let queue_create_infos: Vec<vk::DeviceQueueCreateInfo> = unique_families
        .iter()
        .map(|&family| {
            vk::DeviceQueueCreateInfo::default()
                .queue_family_index(family)
                .queue_priorities(&queue_priorities)
        })
        .collect();

    let device_features = vk::PhysicalDeviceFeatures::default();

    let extension_names: Vec<*const i8> = DEVICE_EXTENSIONS.iter().map(|e| e.as_ptr()).collect();

    let create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_infos)
        .enabled_features(&device_features)
        .enabled_extension_names(&extension_names);

    let device = unsafe {
        instance
            .create_device(physical_device, &create_info, None)
            .context("Failed to create logical device")?
    };

    let graphics_queue = unsafe { device.get_device_queue(indices.graphics, 0) };
    let present_queue = unsafe { device.get_device_queue(indices.present, 0) };

    log::info!(
        "Logical device created (graphics queue family: {}, present: {})",
        indices.graphics,
        indices.present
    );

    Ok((device, graphics_queue, present_queue))
}
